//! Settings window (`SettingsModal.tsx`): model provider, endpoints, translation mode,
//! parallelism, one-hand mode, a model connectivity test, LaMa runtime preparation
//! (`LamaRuntimeSettingsSection.tsx`) and the version/update bar (`SettingsVersionBar.tsx`).

use std::path::PathBuf;

use eframe::egui::{self, RichText};
use mtt_core::settings::{
    AppSettings, ModelProvider, ReasoningEffort, TRANSLATION_PARALLEL_MAX, TRANSLATION_PARALLEL_MIN, TranslationMode,
};

use crate::app::AppEvent;
use crate::services::Services;
use crate::theme;
use crate::update_check::{self, UpdateStatus};
use crate::worker::Worker;

/// `LamaRuntimeStatus` without the job flags (those live in [`SettingsView`]).
#[derive(Debug, Clone)]
pub struct LamaRuntimeStatus {
    python_available: bool,
    runtime_ready: bool,
    model_exists: bool,
    model_path: PathBuf,
}

impl LamaRuntimeStatus {
    fn detect(paths: &mtt_core::paths::AppPaths) -> Self {
        let status = mtt_inpaint::lama::status(paths);
        Self {
            python_available: mtt_inpaint::lama_runtime::resolve_python().is_some(),
            runtime_ready: status.python_ready && status.code_ready,
            model_exists: status.model_exists,
            model_path: status.model_path,
        }
    }
}

pub enum LamaEvent {
    Status(LamaRuntimeStatus),
    Progress(String),
    PrepareFinished(Result<(), String>),
    DownloadFinished(Result<(), String>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ActionState {
    Success,
    Error,
}

#[derive(Clone, Copy, PartialEq, Eq, Default)]
enum SettingsTab {
    #[default]
    Translation,
    Inpaint,
    Editing,
    About,
}

#[derive(Default)]
pub struct SettingsView {
    open: bool,
    tab: SettingsTab,
    draft: Option<AppSettings>,
    testing: bool,
    test_result: Option<Result<String, String>>,
    save_error: Option<String>,
    show_key: bool,
    lama_status: Option<LamaRuntimeStatus>,
    lama_preparing: bool,
    lama_downloading: bool,
    lama_progress: Option<String>,
    lama_last_error: Option<String>,
    lama_action: Option<(ActionState, String)>,
    update: Option<UpdateStatus>,
    update_busy: bool,
    test_endpoint: Option<String>,
    reset_message: Option<String>,
    lama_requested: bool,
    /// The library's LaMa notice was closed (until the next launch).
    lama_banner_dismissed: bool,
}

impl SettingsView {
    pub fn open(&mut self, services: &Services, worker: &Worker<AppEvent>) {
        self.open = true;
        self.draft = Some(services.settings.clone());
        self.test_result = None;
        self.save_error = None;
        self.lama_action = None;
        self.reset_message = None;
        self.refresh_lama_status(services, worker);
        if !self.update.as_ref().is_some_and(UpdateStatus::fresh) {
            self.refresh_update(worker);
        }
    }

    fn refresh_lama_status(&self, services: &Services, worker: &Worker<AppEvent>) {
        let paths = services.paths.clone();
        worker.spawn(move || AppEvent::Lama(LamaEvent::Status(LamaRuntimeStatus::detect(&paths))));
    }

    fn refresh_update(&mut self, worker: &Worker<AppEvent>) {
        if self.update_busy {
            return;
        }
        self.update_busy = true;
        worker.spawn(|| AppEvent::UpdateChecked(update_check::check()));
    }

    pub fn on_update_checked(&mut self, status: UpdateStatus) {
        self.update_busy = false;
        self.update = Some(status);
    }

    /// Returns true when a LaMa job finished, so the editor should re-detect the runtime.
    pub fn on_lama_event(&mut self, event: LamaEvent, services: &Services, worker: &Worker<AppEvent>) -> bool {
        match event {
            LamaEvent::Status(status) => {
                self.lama_status = Some(status);
                false
            }
            LamaEvent::Progress(line) => {
                self.lama_progress = Some(line);
                false
            }
            LamaEvent::PrepareFinished(result) => {
                self.lama_preparing = false;
                self.finish_lama_job(result, "LaMa 환경 준비가 끝났습니다.", services, worker);
                true
            }
            LamaEvent::DownloadFinished(result) => {
                self.lama_downloading = false;
                self.finish_lama_job(result, "LaMa 모델 다운로드가 끝났습니다.", services, worker);
                true
            }
        }
    }

    fn finish_lama_job(
        &mut self,
        result: Result<(), String>,
        done: &str,
        services: &Services,
        worker: &Worker<AppEvent>,
    ) {
        if !self.lama_preparing && !self.lama_downloading {
            self.lama_progress = None;
        }
        match result {
            Ok(()) => {
                self.lama_last_error = None;
                self.lama_action = Some((ActionState::Success, done.into()));
            }
            Err(error) => {
                log::error!("LaMa job failed: {error}");
                self.lama_action = Some((ActionState::Error, error.clone()));
                self.lama_last_error = Some(error);
            }
        }
        self.refresh_lama_status(services, worker);
    }

    /// `prepareLamaRuntime`: venv + packages + Er0mangaInpaint on a dedicated thread.
    fn prepare_lama(&mut self, services: &Services, worker: &Worker<AppEvent>) {
        if self.lama_status.as_ref().is_some_and(|status| !status.python_available) {
            let command = mtt_inpaint::lama_runtime::python_install_command();
            self.lama_last_error = Some(format!("Python 3을 찾을 수 없습니다. 설치 명령: {command}"));
            self.lama_action = Some((ActionState::Error, format!("Python 설치가 필요합니다: {command}")));
            return;
        }
        self.lama_preparing = true;
        self.lama_action = Some((ActionState::Success, "LaMa 환경 준비를 시작했습니다.".into()));
        let paths = services.paths.clone();
        let emitter = worker.emitter();
        std::thread::spawn(move || {
            let log_path = mtt_inpaint::lama_runtime::log_path(&paths);
            let result = mtt_inpaint::lama_runtime::prepare(&paths, |line| {
                emitter.emit(AppEvent::Lama(LamaEvent::Progress(line.to_owned())))
            })
            .map_err(|error| format!("LaMa 환경 준비가 실패했습니다: {error} (로그: {})", log_path.display()));
            emitter.emit(AppEvent::Lama(LamaEvent::PrepareFinished(result)));
        });
    }

    /// `downloadLamaModel`.
    fn download_lama_model(&mut self, services: &Services, worker: &Worker<AppEvent>) {
        if self.lama_status.as_ref().is_some_and(|status| status.model_exists) {
            self.lama_action = Some((ActionState::Success, "LaMa 모델이 이미 준비되어 있습니다.".into()));
            return;
        }
        self.lama_downloading = true;
        self.lama_action = Some((ActionState::Success, "LaMa 모델 다운로드를 시작했습니다.".into()));
        let paths = services.paths.clone();
        let emitter = worker.emitter();
        std::thread::spawn(move || {
            let mut last_percent = None;
            let result = mtt_inpaint::lama_runtime::download_model(&paths, |written, total| {
                let mb = |bytes: u64| bytes as f64 / 1_048_576.0;
                let line = match total {
                    Some(total) if total > 0 => {
                        let percent = written * 100 / total;
                        if last_percent == Some(percent) {
                            return;
                        }
                        last_percent = Some(percent);
                        format!("모델 다운로드 {percent}% ({:.0}/{:.0}MB)", mb(written), mb(total))
                    }
                    _ => format!("모델 다운로드 {:.0}MB", mb(written)),
                };
                emitter.emit(AppEvent::Lama(LamaEvent::Progress(line)));
            });
            emitter.emit(AppEvent::Lama(LamaEvent::DownloadFinished(result)));
        });
    }

    /// One-line notice on the library screen while LaMa isn't ready (local fill is used
    /// meanwhile): opens the 인페인트 settings tab, or can be dismissed for the session.
    /// Returns whether it was drawn.
    pub fn lama_empty_card(&mut self, ui: &mut egui::Ui, services: &Services, worker: &Worker<AppEvent>) -> bool {
        if !self.lama_requested {
            self.lama_requested = true;
            self.refresh_lama_status(services, worker);
        }
        let Some(status) = self.lama_status.clone() else { return false };
        if self.lama_banner_dismissed || (status.python_available && status.runtime_ready && status.model_exists) {
            return false;
        }
        let busy = self.lama_preparing || self.lama_downloading;
        let mut open_settings = false;
        egui::Frame::new()
            .fill(theme::CARD_BG)
            .stroke(egui::Stroke::new(1.0, theme::BORDER))
            .corner_radius(8)
            .inner_margin(egui::Margin::symmetric(12, 8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(theme::icon(theme::icon::INFO).color(theme::ACCENT));
                    if busy {
                        ui.spinner();
                        let progress = self.lama_progress.clone().unwrap_or_else(|| "LaMa 준비 중".into());
                        ui.add(egui::Label::new(RichText::new(progress).small()).truncate());
                    } else {
                        ui.add(
                            egui::Label::new(
                                RichText::new("LaMa 인페인트가 아직 준비되지 않아 로컬 채우기로 지웁니다.").small(),
                            )
                            .truncate(),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(theme::ghost_icon(theme::icon::X)).on_hover_text("이번 실행 동안 숨기기").clicked()
                        {
                            self.lama_banner_dismissed = true;
                        }
                        if ui
                            .add(
                                egui::Button::new(RichText::new("준비하기…").color(theme::ACCENT))
                                    .frame_when_inactive(false),
                            )
                            .on_hover_text("설정 › 인페인트에서 환경 준비와 모델 다운로드")
                            .clicked()
                        {
                            open_settings = true;
                        }
                    });
                });
            });
        if open_settings {
            self.open(services, worker);
            self.tab = SettingsTab::Inpaint;
        }
        true
    }

    fn lama_section(&mut self, ui: &mut egui::Ui, services: &Services, worker: &Worker<AppEvent>) {
        ui.label(theme::semibold("LaMa 인페인트"));
        theme::hint(ui, "원문 글자를 더 자연스럽게 지우는 인페인트 모델입니다. 준비 전에는 로컬 채우기를 씁니다.");
        ui.add_space(8.0);
        let status = self.lama_status.clone();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 18.0;
            runtime_state(ui, "Python", status.as_ref().is_some_and(|s| s.python_available), false);
            runtime_state(ui, "런타임", status.as_ref().is_some_and(|s| s.runtime_ready), self.lama_preparing);
            runtime_state(ui, "모델", status.as_ref().is_some_and(|s| s.model_exists), self.lama_downloading);
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let prepare_label = if self.lama_preparing { "준비 중…" } else { "환경 준비" };
            if ui.add_enabled(!self.lama_preparing, egui::Button::new(prepare_label)).clicked() {
                self.prepare_lama(services, worker);
            }
            let download_label = if self.lama_downloading { "다운로드 중…" } else { "모델 다운로드" };
            if ui.add_enabled(!self.lama_downloading, egui::Button::new(download_label)).clicked() {
                self.download_lama_model(services, worker);
            }
            if ui.add(egui::Button::new("상태 새로고침").frame_when_inactive(false)).clicked() {
                self.refresh_lama_status(services, worker);
            }
        });
        if let Some(progress) = self.lama_progress.as_ref().filter(|_| self.lama_preparing || self.lama_downloading) {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(12.0));
                ui.label(RichText::new(progress).small());
            });
        }
        if let Some((state, message)) = &self.lama_action {
            let color = match state {
                ActionState::Success => theme::SUCCESS,
                ActionState::Error => theme::DANGER,
            };
            ui.label(RichText::new(message).small().color(color));
        }
        if let Some(status) = &status {
            if !status.python_available {
                ui.add_space(6.0);
                python_help(ui);
            }
            ui.add_space(6.0);
            theme::hint(ui, &format!("모델 경로: {}", status.model_path.display()));
        }
        if let Some(error) = &self.lama_last_error {
            theme::hint(ui, &format!("최근 오류: {error}"));
        }
    }

    fn version_bar(&mut self, ui: &mut egui::Ui, worker: &Worker<AppEvent>) {
        let update_available = self.update.as_ref().is_some_and(|status| status.update_available);
        let (fill, stroke) = if update_available {
            (theme::WARNING.gamma_multiply(0.12), theme::WARNING.gamma_multiply(0.4))
        } else {
            (theme::INPUT_BG, theme::BORDER)
        };
        egui::Frame::new().fill(fill).stroke(egui::Stroke::new(1.0, stroke)).corner_radius(8).inner_margin(12).show(
            ui,
            |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(theme::semibold(format!("MangaTranslationTools {}", update_check::CURRENT_VERSION)));
                        theme::hint(ui, &update_check::status_text(self.update.as_ref(), self.update_busy));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let label = if self.update_busy { "확인 중" } else { "다시 확인" };
                        if ui.add_enabled(!self.update_busy, egui::Button::new(label)).clicked() {
                            self.refresh_update(worker);
                        }
                        if let Some(url) =
                            self.update.as_ref().filter(|s| s.update_available).and_then(|s| s.release_url.as_ref())
                        {
                            ui.hyperlink_to("업데이트 열기", url);
                        }
                    });
                });
            },
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn translation_tab(
        &mut self,
        ui: &mut egui::Ui,
        draft: &mut AppSettings,
        worker: &Worker<AppEvent>,
        job_active: bool,
        can_submit: bool,
        base_url_valid: bool,
        base_url: &str,
    ) {
        let label_width = 92.0;
        let row = |ui: &mut egui::Ui, label: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
            ui.horizontal(|ui| {
                form_label(ui, label, label_width);
                ui.vertical(|ui| add(ui));
            });
            ui.add_space(4.0);
        };
        row(ui, "번역 모드", &mut |ui| {
            if let Some(mode) = theme::segmented(
                ui,
                draft.translation_mode,
                &[(TranslationMode::Fast, "빠르게", ""), (TranslationMode::Accuracy, "정확하게", "")],
            ) {
                draft.translation_mode = mode;
            }
            theme::hint(ui, translation_mode_description(draft.translation_mode));
        });
        row(ui, "번역 엔진", &mut |ui| {
            if let Some(provider) = theme::segmented(
                ui,
                draft.model_provider,
                &[
                    (ModelProvider::OpenAiCodex, "OpenAI (Codex 로그인)", ""),
                    (ModelProvider::OpenAiCompatible, "OpenAI 호환 서버", ""),
                ],
            ) {
                draft.model_provider = provider;
            }
            theme::hint(ui, provider_description(draft.model_provider));
        });
        match draft.model_provider {
            ModelProvider::OpenAiCodex => {
                row(ui, "모델", &mut |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut draft.codex.model).hint_text("gpt-5.5").desired_width(300.0),
                    );
                });
                row(ui, "생각 수준", &mut |ui| {
                    let efforts = [
                        ReasoningEffort::None,
                        ReasoningEffort::Low,
                        ReasoningEffort::Medium,
                        ReasoningEffort::High,
                        ReasoningEffort::Xhigh,
                    ]
                    .map(|effort| (effort, effort_label(effort), effort_description(effort)));
                    if let Some(effort) = theme::segmented(ui, draft.codex.reasoning_effort, &efforts) {
                        draft.codex.reasoning_effort = effort;
                    }
                    theme::hint(ui, effort_description(draft.codex.reasoning_effort));
                });
                row(ui, "oauth 포트", &mut |ui| {
                    ui.add(egui::DragValue::new(&mut draft.codex.oauth_port).range(0..=65535));
                });
            }
            ModelProvider::OpenAiCompatible => {
                let show_key = &mut self.show_key;
                row(ui, "Base URL", &mut |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut draft.openai_compatible.base_url)
                            .hint_text("http://127.0.0.1:1234/v1")
                            .desired_width(300.0),
                    );
                });
                row(ui, "API Key", &mut |ui| {
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut draft.openai_compatible.api_key)
                                .password(!*show_key)
                                .hint_text("로컬 서버면 비워 두세요")
                                .desired_width(238.0),
                        );
                        ui.checkbox(show_key, "보기");
                    });
                });
                row(ui, "모델", &mut |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut draft.openai_compatible.model)
                            .hint_text("gpt-4o-mini")
                            .desired_width(300.0),
                    );
                });
            }
        }
        let note = match draft.model_provider {
            ModelProvider::OpenAiCodex => {
                Some(("터미널에서 `npx openai-oauth`를 실행해 두세요. 앱이 직접 띄우지 않습니다.", theme::TEXT_WEAK))
            }
            ModelProvider::OpenAiCompatible if !base_url_valid => {
                Some(("Base URL은 http:// 또는 https://로 시작해야 합니다.", theme::WARNING))
            }
            ModelProvider::OpenAiCompatible => None,
        };
        if let Some((text, color)) = note {
            ui.horizontal(|ui| {
                ui.add_space(label_width + ui.spacing().item_spacing.x);
                ui.label(RichText::new(text).small().color(color));
            });
        }
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);
        row(ui, "동시 번역", &mut |ui| {
            ui.horizontal(|ui| {
                toggle_row(ui, "여러 페이지 동시에", &mut draft.translation_parallel.enabled);
                ui.add_space(8.0);
                ui.label(RichText::new("최대").color(theme::TEXT_WEAK));
                ui.add_enabled(
                    draft.translation_parallel.enabled,
                    egui::DragValue::new(&mut draft.translation_parallel.max_concurrency)
                        .range(TRANSLATION_PARALLEL_MIN..=TRANSLATION_PARALLEL_MAX),
                )
                .on_hover_text("API·로컬 모델 한도에 맞춰 조절하세요");
            });
        });
        ui.add_space(6.0);
        ui.separator();
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            form_label(ui, "연결 확인", label_width);
            let label = if self.testing { "확인 중…" } else { "모델 응답 확인" };
            if ui.add_enabled(!self.testing && can_submit && !job_active, egui::Button::new(label)).clicked() {
                self.testing = true;
                self.test_result = None;
                let mut settings = draft.normalized();
                settings.openai_compatible.base_url = base_url.to_owned();
                let config = mtt_translate::TranslationConfig::from_settings(&settings);
                self.test_endpoint = Some(config.endpoint.base_url.clone());
                worker.spawn(move || {
                    AppEvent::ModelTest(
                        mtt_translate::test_model(&config).map(|reply| format!("모델 응답 확인 완료: {reply}")),
                    )
                });
            }
            if self.testing {
                ui.add(egui::Spinner::new().size(12.0));
            }
        });
        ui.horizontal(|ui| {
            ui.add_space(label_width + ui.spacing().item_spacing.x);
            ui.vertical(|ui| {
                if job_active {
                    theme::hint(ui, "번역 작업 중에는 확인할 수 없습니다.");
                }
                match &self.test_result {
                    Some(Ok(message)) => {
                        ui.label(RichText::new(message).small().color(theme::SUCCESS));
                        if let Some(endpoint) = &self.test_endpoint {
                            theme::hint(ui, &format!("엔드포인트: {endpoint}"));
                        }
                    }
                    Some(Err(error)) => {
                        ui.label(RichText::new(error).small().color(theme::DANGER));
                    }
                    None => theme::hint(
                        ui,
                        "간단한 텍스트 요청에 응답하는지만 봅니다. 이미지 번역 가능 여부와는 다를 수 있습니다.",
                    ),
                }
            });
        });
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn on_test_result(&mut self, result: Result<String, String>) {
        self.testing = false;
        self.test_result = Some(result);
    }

    /// `job_active` disables the model test like the TS modal does during a translation run.
    pub fn ui(&mut self, ctx: &egui::Context, services: &mut Services, worker: &Worker<AppEvent>, job_active: bool) {
        if !self.open {
            return;
        }
        let Some(mut draft) = self.draft.take() else {
            self.open = false;
            return;
        };
        let mut open = true;
        let mut save = false;
        let mut cancel = false;
        let mut reset = false;
        // `canSubmit` / `buildSettings` validation.
        let base_url = draft.openai_compatible.base_url.trim().trim_end_matches('/').to_owned();
        let lower = base_url.to_ascii_lowercase();
        let base_url_valid = lower
            .strip_prefix("http://")
            .or_else(|| lower.strip_prefix("https://"))
            .is_some_and(|rest| !rest.is_empty());
        let can_submit = match draft.model_provider {
            ModelProvider::OpenAiCodex => !draft.codex.model.trim().is_empty(),
            ModelProvider::OpenAiCompatible => base_url_valid && !draft.openai_compatible.model.trim().is_empty(),
        };
        let max_height = (ctx.content_rect().height() - 220.0).clamp(240.0, 470.0);
        theme::modal(
            ctx,
            egui::Window::new("설정")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .fixed_size([560.0, max_height + 110.0]),
            |ui| {
                if let Some(tab) = theme::segmented(
                    ui,
                    self.tab,
                    &[
                        (SettingsTab::Translation, "번역", "번역 엔진과 모델"),
                        (SettingsTab::Inpaint, "인페인트", "LaMa 준비"),
                        (SettingsTab::Editing, "편집", "단축키 옵션"),
                        (SettingsTab::About, "정보", "버전과 데이터 폴더"),
                    ],
                ) {
                    self.tab = tab;
                }
                ui.add_space(12.0);
                egui::ScrollArea::vertical().max_height(max_height).auto_shrink([false, false]).show(
                    ui,
                    |ui| match self.tab {
                        SettingsTab::Translation => self.translation_tab(
                            ui,
                            &mut draft,
                            worker,
                            job_active,
                            can_submit,
                            base_url_valid,
                            &base_url,
                        ),
                        SettingsTab::Inpaint => self.lama_section(ui, services, worker),
                        SettingsTab::Editing => {
                            toggle_row(ui, "한손모드", &mut draft.one_hand_mode);
                            theme::hint(
                                ui,
                                "편집 화면에서 Q 키를 삭제 키로 씁니다. 텍스트 입력 중에는 적용되지 않습니다.",
                            );
                            ui.add_space(14.0);
                            ui.label(theme::semibold("단축키"));
                            theme::hint(ui, "편집 화면에서 ?를 눌러도 볼 수 있습니다.");
                            ui.add_space(6.0);
                            crate::shortcuts::sheet(ui, 1);
                        }
                        SettingsTab::About => {
                            self.version_bar(ui, worker);
                            ui.add_space(14.0);
                            ui.label(theme::semibold("데이터 폴더"));
                            ui.label(
                                RichText::new(services.paths.data_root.display().to_string())
                                    .monospace()
                                    .color(theme::TEXT_WEAK),
                            );
                            ui.add_space(4.0);
                            if ui.button("폴더 열기").clicked()
                                && let Err(error) = crate::editor::report::open_with_system(&services.paths.data_root)
                            {
                                self.save_error = Some(format!("폴더를 열 수 없습니다: {error}"));
                            }
                        }
                    },
                );
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);
                if let Some(error) = &self.save_error {
                    ui.colored_label(theme::DANGER, format!("저장 실패: {error}"));
                }
                if let Some(message) = &self.reset_message {
                    theme::hint(ui, message);
                }
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::Button::new(RichText::new("기본값 복원").color(theme::TEXT_WEAK))
                                .frame_when_inactive(false),
                        )
                        .on_hover_text("모든 설정을 기본값으로 되돌리고 바로 저장합니다")
                        .clicked()
                    {
                        reset = true;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(can_submit, theme::primary("저장"))
                            .on_hover_text("모델과 번역 옵션은 다음 번역부터 적용됩니다")
                            .clicked()
                        {
                            save = true;
                        }
                        if ui.button("취소").clicked() {
                            cancel = true;
                        }
                        if !can_submit {
                            theme::hint(ui, "번역 탭의 모델·주소를 확인하세요");
                        }
                    });
                });
            },
        );
        if reset
            && theme::confirm(
                "기본값 복원",
                "모든 설정을 기본값으로 되돌릴까요?",
                "Base URL, API Key, 모델 이름도 지워지고 바로 저장됩니다. 취소로 되돌릴 수 없습니다.",
            )
        {
            // `resetAppSettings` writes the defaults right away.
            let defaults = AppSettings::env_defaults();
            match services.save_settings(defaults) {
                Ok(()) => {
                    draft = services.settings.clone();
                    self.reset_message = Some(
                        "설정을 기본값으로 복원했습니다. 모델/번역 옵션은 다음 번 번역 실행부터 적용됩니다.".into(),
                    );
                }
                Err(error) => {
                    log::error!("settings reset failed: {error}");
                    self.reset_message = Some("기본 설정을 복원하지 못했습니다.".into());
                }
            }
        }
        // The window's close button acts like the TS header "닫기" (submit).
        if (save || !open) && can_submit {
            draft.openai_compatible.base_url = base_url;
            draft.codex.model = draft.codex.model.trim().to_owned();
            draft.openai_compatible.model = draft.openai_compatible.model.trim().to_owned();
            draft.openai_compatible.api_key = draft.openai_compatible.api_key.trim().to_owned();
            match services.save_settings(draft.clone()) {
                Ok(()) => {
                    self.open = false;
                    return;
                }
                Err(error) => self.save_error = Some(error),
            }
        }
        if cancel {
            self.open = false;
            return;
        }
        self.draft = Some(draft);
    }
}

/// Left-aligned label of a form row, `width` wide.
fn form_label(ui: &mut egui::Ui, label: &str, width: f32) {
    ui.allocate_ui_with_layout(
        egui::vec2(width, ui.spacing().interact_size.y),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_min_width(width);
            ui.label(RichText::new(label).color(theme::TEXT_WEAK));
        },
    );
}

fn toggle_row(ui: &mut egui::Ui, label: &str, value: &mut bool) {
    ui.checkbox(value, label);
}

/// `TRANSLATION_MODE_OPTIONS` descriptions.
fn translation_mode_description(mode: TranslationMode) -> &'static str {
    match mode {
        TranslationMode::Fast => "원본 이미지만 보내고 토큰 예산을 줄여 더 빠르게 처리합니다.",
        TranslationMode::Accuracy => "고대비 보조 이미지를 함께 보내고 더 넉넉한 토큰 예산을 사용합니다.",
    }
}

/// `MODEL_PROVIDER_OPTIONS` descriptions.
fn provider_description(provider: ModelProvider) -> &'static str {
    match provider {
        ModelProvider::OpenAiCodex => "Codex 로그인 토큰을 쓰는 openai-oauth 엔드포인트로 요청합니다.",
        ModelProvider::OpenAiCompatible => "OpenAI 호환 /v1/chat/completions 엔드포인트로 요청합니다.",
    }
}

/// `CODEX_REASONING_OPTIONS` descriptions.
fn effort_description(effort: ReasoningEffort) -> &'static str {
    match effort {
        ReasoningEffort::None => "생각 예산을 쓰지 않고 가장 빠르게 응답합니다.",
        ReasoningEffort::Low => "가벼운 추론으로 처리합니다.",
        ReasoningEffort::Medium => "기본 균형 설정입니다.",
        ReasoningEffort::High => "더 오래 생각해서 까다로운 페이지를 처리합니다.",
        ReasoningEffort::Xhigh => "가장 넉넉한 생각 예산을 사용합니다.",
    }
}

/// `PythonInstallHelp`.
fn python_help(ui: &mut egui::Ui) {
    egui::Frame::new().fill(theme::INPUT_BG).corner_radius(6).inner_margin(10).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(theme::semibold("Python 설치가 필요합니다"));
        let command = mtt_inpaint::lama_runtime::python_install_command();
        ui.horizontal(|ui| {
            ui.label(RichText::new(command).monospace());
            if ui.small_button("복사").clicked() {
                ui.ctx().copy_text(command.to_owned());
            }
        });
        for line in mtt_inpaint::lama_runtime::python_install_help() {
            theme::hint(ui, line);
        }
    });
}

fn runtime_state(ui: &mut egui::Ui, label: &str, ready: bool, busy: bool) {
    let (text, color) = if ready {
        ("준비됨", theme::SUCCESS)
    } else if busy {
        ("진행 중", theme::WARNING)
    } else {
        ("필요", theme::DANGER)
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        theme::status_dot(ui, color);
        ui.label(label);
        ui.label(RichText::new(text).small().color(theme::TEXT_WEAK));
    });
}

fn effort_label(effort: ReasoningEffort) -> &'static str {
    match effort {
        ReasoningEffort::None => "없음",
        ReasoningEffort::Low => "낮음",
        ReasoningEffort::Medium => "보통",
        ReasoningEffort::High => "높음",
        ReasoningEffort::Xhigh => "최고",
    }
}
