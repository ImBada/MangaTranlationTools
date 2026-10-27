//! Status feedback like the TS app: a 4-second toast for each new status line, the
//! "기록" history (last 16 lines), recoverable failure cards with 재시도/닫기, the page
//! inpaint notice and the "저장 완료" flash (`useStatusFeedback.ts`,
//! `useRecoverableFailures.ts`, `NotificationDock`, `StatusHistoryPanel`).

use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, RichText};
use mtt_core::model::InpaintStatus;

use super::{Editor, InpaintScope};
use crate::app::AppEvent;
use crate::services::Services;
use crate::worker::Worker;

const TOAST_DURATION: Duration = Duration::from_secs(4);
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
    toast: Option<(String, Instant)>,
    pub history_open: bool,
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

    /// `signalSaveComplete`.
    pub fn flash_saved(&mut self) {
        self.save_flash = Some(Instant::now());
    }

    pub fn save_flash_active(&self) -> bool {
        self.save_flash.is_some_and(|at| at.elapsed() < SAVE_FLASH)
    }

    /// `appendStatusLine`: a changed status becomes the toast and the newest history line.
    fn observe(&mut self, status: &str) {
        let line = status.trim();
        if line.is_empty() || line == self.last_status {
            return;
        }
        self.last_status = line.to_owned();
        self.toast = Some((line.to_owned(), Instant::now()));
        if self.lines.first().map(String::as_str) != Some(line) {
            self.lines.insert(0, line.to_owned());
            self.lines.truncate(HISTORY_LIMIT);
        }
    }
}

impl Editor {
    /// Saved/unsaved/flash label for the status dot (`saveStatusLabel`).
    pub fn save_state_label(&self) -> &'static str {
        if self.notifications.save_flash_active() {
            "저장 완료"
        } else if self.has_unsaved_changes() {
            "저장되지 않은 변경 있음"
        } else {
            "최신 상태"
        }
    }

    /// Draws the notification dock and the history panel over the stage.
    pub fn notifications_ui(
        &mut self,
        ctx: &egui::Context,
        stage: egui::Rect,
        services: &Services,
        worker: &Worker<AppEvent>,
    ) {
        let status = self.status.clone();
        self.notifications.observe(&status);
        if let Some((_, at)) = &self.notifications.toast {
            if at.elapsed() >= TOAST_DURATION {
                self.notifications.toast = None;
            } else {
                ctx.request_repaint_after(TOAST_DURATION.saturating_sub(at.elapsed()));
            }
        }
        if self.notifications.save_flash_active() {
            ctx.request_repaint_after(SAVE_FLASH);
        }

        let mut retry = None;
        let mut dismiss = None;
        let mut rerun_inpaint = false;
        let page_notice = self.page().and_then(|page| match page.inpaint_status {
            Some(InpaintStatus::Running) => Some(("인페인트 중".to_owned(), page.name.clone(), false)),
            Some(InpaintStatus::Failed) => {
                Some(("인페인트 실패".to_owned(), format!("{} - 마스크와 레이어 상태는 유지됨", page.name), true))
            }
            _ => None,
        });
        let has_cards =
            page_notice.is_some() || self.notifications.has_failures() || self.notifications.toast.is_some();
        if has_cards {
            egui::Area::new(egui::Id::unique("notification-dock"))
                .anchor(
                    egui::Align2::RIGHT_BOTTOM,
                    [
                        -(ctx.content_rect().right() - stage.right()) - 12.0,
                        -(ctx.content_rect().bottom() - stage.bottom()) - 52.0,
                    ],
                )
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    ui.set_max_width(340.0);
                    if let Some((title, message, failed)) = &page_notice {
                        card(ui, *failed, title, message, |ui| {
                            if *failed && ui.button("다시 실행").clicked() {
                                rerun_inpaint = true;
                            }
                        });
                    }
                    for failure in &self.notifications.failures {
                        card(ui, true, &failure.title, &failure.message, |ui| {
                            if ui.button("재시도").clicked() {
                                retry = Some(failure.id);
                            }
                            if ui.button("닫기").clicked() {
                                dismiss = Some(failure.id);
                            }
                        });
                    }
                    if let Some((line, _)) = &self.notifications.toast {
                        let failed = line.contains("실패");
                        card(ui, failed, "알림", line, |_| {});
                    }
                });
        }

        egui::Area::new(egui::Id::unique("status-history-button"))
            .fixed_pos(stage.left_bottom() + egui::vec2(10.0, -40.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let failed = self.notifications.has_failures();
                let text =
                    RichText::new("기록").color(if failed { Color32::LIGHT_RED } else { ui.visuals().text_color() });
                if ui.selectable_label(self.notifications.history_open, text).on_hover_text("상태 기록").clicked() {
                    self.notifications.history_open = !self.notifications.history_open;
                }
            });
        if self.notifications.history_open {
            let job = self
                .translation
                .as_ref()
                .map(|job| format!("{} {}/{}", job.phase_label(), job.done, job.total))
                .unwrap_or_else(|| "대기 중".into());
            egui::Area::new(egui::Id::unique("status-history-panel"))
                .fixed_pos(stage.left_bottom() + egui::vec2(10.0, -330.0))
                .order(egui::Order::Foreground)
                .show(ctx, |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.set_width(320.0);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("상태 기록").strong());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("×").on_hover_text("상태 기록 닫기").clicked() {
                                    self.notifications.history_open = false;
                                }
                            });
                        });
                        ui.label(RichText::new(job).small().background_color(ui.visuals().faint_bg_color));
                        egui::ScrollArea::vertical().max_height(230.0).show(ui, |ui| {
                            if self.notifications.lines.is_empty() {
                                ui.label(RichText::new("아직 표시할 상태가 없습니다.").weak());
                            }
                            for line in &self.notifications.lines {
                                ui.label(RichText::new(line).small());
                            }
                        });
                    });
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

fn card(ui: &mut egui::Ui, failed: bool, title: &str, message: &str, actions: impl FnOnce(&mut egui::Ui)) {
    let stroke =
        if failed { Color32::from_rgb(239, 68, 68) } else { ui.visuals().widgets.noninteractive.bg_stroke.color };
    egui::Frame::popup(ui.style()).stroke((1.0, stroke)).show(ui, |ui| {
        ui.set_width(320.0);
        ui.label(RichText::new(title).strong());
        ui.label(message);
        ui.horizontal(actions);
    });
    ui.add_space(4.0);
}
