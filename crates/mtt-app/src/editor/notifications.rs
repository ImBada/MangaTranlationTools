//! Status feedback (`useStatusFeedback.ts`, `useRecoverableFailures.ts`): the latest
//! status line and its history (last 16) for the status bar, recoverable failure cards
//! with 재시도/닫기 over the canvas, the failed-inpaint notice and the "저장 완료" flash.

use std::time::{Duration, Instant};

use eframe::egui;
use mtt_core::model::InpaintStatus;

use super::{Editor, InpaintScope};
use crate::app::AppEvent;
use crate::services::Services;
use crate::theme;
use crate::worker::Worker;

/// How long a new status line stays highlighted in the status bar.
const FRESH_DURATION: Duration = Duration::from_secs(4);
const SAVE_FLASH: Duration = Duration::from_millis(1200);
const HISTORY_LIMIT: usize = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureId {
    ChapterSave,
    LayerSave,
    InpaintRun,
    AnalysisRun,
}

struct Failure {
    id: FailureId,
    title: String,
    message: String,
}

#[derive(Default)]
pub struct Notifications {
    lines: Vec<String>,
    last_status: String,
    fresh: Option<Instant>,
    failures: Vec<Failure>,
    save_flash: Option<Instant>,
}

impl Notifications {
    pub fn report(&mut self, id: FailureId, title: &str, message: &str) {
        self.failures.retain(|failure| failure.id != id);
        self.failures.push(Failure { id, title: title.into(), message: message.into() });
    }

    pub fn clear(&mut self, id: FailureId) {
        self.failures.retain(|failure| failure.id != id);
    }

    pub fn has_failures(&self) -> bool {
        !self.failures.is_empty()
    }

    /// A chapter or layer write failed (the status bar's save state).
    pub fn has_save_failure(&self) -> bool {
        self.failures.iter().any(|failure| matches!(failure.id, FailureId::ChapterSave | FailureId::LayerSave))
    }

    /// `signalSaveComplete`.
    pub fn flash_saved(&mut self) {
        self.save_flash = Some(Instant::now());
    }

    pub fn save_flash_active(&self) -> bool {
        self.save_flash.is_some_and(|at| at.elapsed() < SAVE_FLASH)
    }

    /// Newest status line.
    pub fn latest(&self) -> Option<&str> {
        self.lines.first().map(String::as_str)
    }

    /// Whether the newest line arrived moments ago.
    pub fn latest_is_fresh(&self) -> bool {
        self.fresh.is_some_and(|at| at.elapsed() < FRESH_DURATION)
    }

    /// History, newest first.
    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    /// `appendStatusLine`: a changed status becomes the newest history line.
    fn observe(&mut self, status: &str) {
        let line = status.trim();
        if line.is_empty() || line == self.last_status {
            return;
        }
        self.last_status = line.to_owned();
        self.fresh = Some(Instant::now());
        if self.lines.first().map(String::as_str) != Some(line) {
            self.lines.insert(0, line.to_owned());
            self.lines.truncate(HISTORY_LIMIT);
        }
    }
}

impl Editor {
    /// Records status changes and draws the failure cards in the canvas corner.
    pub fn notifications_ui(
        &mut self,
        ctx: &egui::Context,
        stage: egui::Rect,
        services: &Services,
        worker: &Worker<AppEvent>,
    ) {
        let status = self.status.clone();
        self.notifications.observe(&status);
        if let Some(at) = self.notifications.fresh
            && at.elapsed() < FRESH_DURATION
        {
            ctx.request_repaint_after(FRESH_DURATION.saturating_sub(at.elapsed()));
        }
        if self.notifications.save_flash_active() {
            ctx.request_repaint_after(SAVE_FLASH);
        }

        let mut retry = None;
        let mut dismiss = None;
        let mut rerun_inpaint = false;
        let failed_inpaint = self.page().and_then(|page| {
            (page.inpaint_status == Some(InpaintStatus::Failed))
                .then(|| format!("{} - 마스크와 레이어 상태는 유지됨", page.name))
        });
        if failed_inpaint.is_some() || self.notifications.has_failures() {
            egui::Area::new(egui::Id::unique("notification-dock"))
                .pivot(egui::Align2::RIGHT_BOTTOM)
                .fixed_pos(stage.right_bottom() - egui::vec2(12.0, 12.0))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    ui.set_max_width(340.0);
                    if let Some(message) = &failed_inpaint {
                        card(ui, "인페인트 실패", message, |ui| {
                            if ui.button("다시 실행").clicked() {
                                rerun_inpaint = true;
                            }
                        });
                    }
                    for failure in &self.notifications.failures {
                        card(ui, &failure.title, &failure.message, |ui| {
                            if ui.button("재시도").clicked() {
                                retry = Some(failure.id);
                            }
                            if ui.button("닫기").clicked() {
                                dismiss = Some(failure.id);
                            }
                        });
                    }
                });
        }

        if let Some(id) = dismiss {
            self.notifications.clear(id);
        }
        if rerun_inpaint {
            self.start_inpaint(InpaintScope::WholeMask, worker);
        }
        if let Some(id) = retry {
            self.retry_failure(id, services, worker);
        }
    }

    /// `useAppActions` retry mapping.
    fn retry_failure(&mut self, id: FailureId, services: &Services, worker: &Worker<AppEvent>) {
        self.notifications.clear(id);
        match id {
            FailureId::ChapterSave => {
                self.chapter_dirty = true;
                self.last_change = Some(Instant::now() - super::AUTOSAVE_DELAY);
            }
            FailureId::LayerSave => self.retry_layer_saves(),
            FailureId::InpaintRun => match self.last_inpaint {
                Some(super::LastInpaint::AllPages) => self.start_inpaint_all_pages(),
                Some(super::LastInpaint::Page(scope)) => self.start_inpaint(scope, worker),
                None => {}
            },
            FailureId::AnalysisRun => {
                if let Some(mode) = self.last_run_mode {
                    self.start_translation(mode, services, worker);
                }
            }
        }
    }
}

fn card(ui: &mut egui::Ui, title: &str, message: &str, actions: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::popup(ui.style()).stroke((1.0, theme::DANGER)).show(ui, |ui| {
        ui.set_width(320.0);
        ui.label(theme::semibold(title).color(theme::DANGER));
        ui.label(message);
        ui.horizontal(actions);
    });
    ui.add_space(4.0);
}
