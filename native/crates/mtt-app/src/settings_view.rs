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

#[derive(Default)]
pub struct SettingsView {
    open: bool,
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

    /// The empty-state "LaMa 인페인트 준비" card (`WorkspaceEmptyState`), shown until Python,
    /// the runtime and the model are all ready. Returns whether it was drawn.
    pub fn lama_empty_card(&mut self, ui: &mut egui::Ui, services: &Services, worker: &Worker<AppEvent>) -> bool {
        if !self.lama_requested {
            self.lama_requested = true;
            self.refresh_lama_status(services, worker);
        }
        let Some(status) = self.lama_status.clone() else { return false };
        if status.python_available && status.runtime_ready && status.model_exists {
            return false;
        }
        egui::Frame::group(ui.style()).inner_margin(12.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("LaMa 인페인트 준비");
                if ui.button("새로고침").clicked() {
                    self.refresh_lama_status(services, worker);
                }
            });
            ui.horizontal(|ui| {
                runtime_state(ui, "Python", status.python_available, false);
                runtime_state(ui, "런타임", status.runtime_ready, self.lama_preparing);
                runtime_state(ui, "모델", status.model_exists, self.lama_downloading);
            });
            if !status.python_available {
                python_help(ui);
            }
            ui.horizontal(|ui| {
                let label = if self.lama_preparing { "환경 준비 중" } else { "환경 준비" };
                if ui.add_enabled(!self.lama_preparing, egui::Button::new(label)).clicked() {
                    self.prepare_lama(services, worker);
                }
                let label = if self.lama_downloading { "모델 다운로드 중" } else { "모델 다운로드" };
                if ui.add_enabled(!self.lama_downloading, egui::Button::new(label)).clicked() {
                    self.download_lama_model(services, worker);
                }
            });
            ui.label(RichText::new(format!("모델 경로: {}", status.model_path.display())).weak().small());
            if let Some(progress) = self.lama_progress.as_ref().filter(|_| self.lama_preparing || self.lama_downloading)
            {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(RichText::new(progress).small());
                });
            }
            if let Some((_, message)) = &self.lama_action {
                ui.label(RichText::new(message).weak());
            }
        });
        true
    }

    fn lama_section(&mut self, ui: &mut egui::Ui, services: &Services, worker: &Worker<AppEvent>) {
        ui.label(RichText::new("LaMa 인페인트").strong());
        let status = self.lama_status.clone();
        ui.horizontal(|ui| {
            runtime_state(ui, "Python", status.as_ref().is_some_and(|s| s.python_available), false);
            runtime_state(ui, "런타임", status.as_ref().is_some_and(|s| s.runtime_ready), self.lama_preparing);
            runtime_state(ui, "모델", status.as_ref().is_some_and(|s| s.model_exists), self.lama_downloading);
        });
        ui.horizontal(|ui| {
            let prepare_label = if self.lama_preparing { "준비 중..." } else { "환경 준비" };
            if ui.add_enabled(!self.lama_preparing, egui::Button::new(prepare_label)).clicked() {
                self.prepare_lama(services, worker);
            }
            let download_label = if self.lama_downloading { "다운로드 중..." } else { "모델 다운로드" };
            if ui.add_enabled(!self.lama_downloading, egui::Button::new(download_label)).clicked() {
                self.download_lama_model(services, worker);
            }
            if ui.button("새로고침").clicked() {
                self.refresh_lama_status(services, worker);
            }
        });
        if let Some(status) = &status {
            if !status.python_available {
                python_help(ui);
            }
            ui.label(RichText::new(format!("모델 경로: {}", status.model_path.display())).weak().small());
        }
        if let Some(error) = &self.lama_last_error {
            ui.label(RichText::new(format!("최근 오류: {error}")).weak().small());
        }
        if let Some(progress) = self.lama_progress.as_ref().filter(|_| self.lama_preparing || self.lama_downloading) {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new(progress).small());
            });
        }
        if let Some((state, message)) = &self.lama_action {
            let color = match state {
                ActionState::Success => egui::Color32::LIGHT_GREEN,
                ActionState::Error => egui::Color32::LIGHT_RED,
            };
            ui.label(RichText::new(message).strong().color(color));
        }
    }

    fn version_bar(&mut self, ui: &mut egui::Ui, worker: &Worker<AppEvent>) {
        let update_available = self.update.as_ref().is_some_and(|status| status.update_available);
        let fill = if update_available { egui::Color32::from_rgb(54, 46, 18) } else { ui.visuals().faint_bg_color };
        egui::Frame::new().fill(fill).corner_radius(4.0).inner_margin(6.0).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(format!("MangaTranslationTools v{}", update_check::CURRENT_VERSION)).strong(),
                    );
                    ui.label(RichText::new(update_check::status_text(self.update.as_ref(), self.update_busy)).small());
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
        egui::Window::new("설정").open(&mut open).collapsible(false).resizable(false).default_width(540.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().max_height(ctx.content_rect().height() - 120.0).show(ui, |ui| {
                ui.label(RichText::new("모델과 번역 옵션은 다음 번 번역 실행부터 적용됩니다.").weak().small());
                ui.add_space(4.0);
                toggle_row(ui, "한손모드", &mut draft.one_hand_mode);
                ui.label(
                    RichText::new("켜두면 편집 화면에서 Q 키가 Delete/Backspace와 같은 삭제 동작으로 작동합니다. 텍스트 입력 중에는 적용되지 않습니다.")
                        .weak()
                        .small(),
                );
                ui.add_space(6.0);

                ui.label("번역 모드");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut draft.translation_mode, TranslationMode::Fast, "빠름");
                    ui.selectable_value(&mut draft.translation_mode, TranslationMode::Accuracy, "정확성");
                });
                ui.label(RichText::new(translation_mode_description(draft.translation_mode)).weak().small());
                ui.add_space(6.0);

                ui.label("번역 엔진");
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut draft.model_provider, ModelProvider::OpenAiCodex, "OpenAI API");
                    ui.selectable_value(&mut draft.model_provider, ModelProvider::OpenAiCompatible, "커스텀");
                });
                ui.label(RichText::new(provider_description(draft.model_provider)).weak().small());
                ui.add_space(6.0);

                toggle_row(ui, "AI 번역 병렬 처리", &mut draft.translation_parallel.enabled);
                ui.horizontal(|ui| {
                    ui.label("최대 동시 번역 수");
                    ui.add_enabled(
                        draft.translation_parallel.enabled,
                        egui::DragValue::new(&mut draft.translation_parallel.max_concurrency)
                            .range(TRANSLATION_PARALLEL_MIN..=TRANSLATION_PARALLEL_MAX),
                    );
                });
                ui.label(RichText::new("켜두면 여러 페이지를 동시에 AI 번역합니다. API/로컬 모델 한도에 맞춰 조절하세요.").weak().small());

                ui.separator();
                self.lama_section(ui, services, worker);
                ui.separator();

                match draft.model_provider {
                    ModelProvider::OpenAiCodex => {
                        ui.label("Codex 모델");
                        ui.add(egui::TextEdit::singleline(&mut draft.codex.model).hint_text("gpt-5.5"));
                        ui.label("생각");
                        ui.horizontal(|ui| {
                            for effort in [
                                ReasoningEffort::None,
                                ReasoningEffort::Low,
                                ReasoningEffort::Medium,
                                ReasoningEffort::High,
                                ReasoningEffort::Xhigh,
                            ] {
                                ui.selectable_value(&mut draft.codex.reasoning_effort, effort, effort_label(effort));
                            }
                        });
                        ui.label(RichText::new(effort_description(draft.codex.reasoning_effort)).weak().small());
                        ui.horizontal(|ui| {
                            ui.label("openai-oauth 포트");
                            ui.add(egui::DragValue::new(&mut draft.codex.oauth_port).range(0..=65535));
                        });
                        ui.label(
                            RichText::new("네이티브 앱은 openai-oauth를 직접 띄우지 않습니다. 터미널에서 `npx openai-oauth`를 실행해 두세요.")
                                .weak()
                                .small(),
                        );
                    }
                    ModelProvider::OpenAiCompatible => {
                        ui.label("Base URL");
                        ui.add(
                            egui::TextEdit::singleline(&mut draft.openai_compatible.base_url)
                                .hint_text("http://127.0.0.1:1234/v1"),
                        );
                        ui.label("API Key");
                        ui.horizontal(|ui| {
                            ui.add(
                                egui::TextEdit::singleline(&mut draft.openai_compatible.api_key)
                                    .password(!self.show_key)
                                    .hint_text("로컬 서버면 비워둘 수 있습니다"),
                            );
                            ui.checkbox(&mut self.show_key, "보기");
                        });
                        ui.label("모델");
                        ui.add(egui::TextEdit::singleline(&mut draft.openai_compatible.model).hint_text("gpt-4o-mini"));
                    }
                }

                ui.add_space(6.0);
                ui.label("모델 테스트");
                let label = if self.testing { "테스트 중..." } else { "잘 작동되나 확인" };
                if ui.add_enabled(!self.testing && can_submit && !job_active, egui::Button::new(label)).clicked() {
                    self.testing = true;
                    self.test_result = None;
                    let mut settings = draft.normalized();
                    settings.openai_compatible.base_url = base_url.clone();
                    let config = mtt_translate::TranslationConfig::from_settings(&settings);
                    self.test_endpoint = Some(config.endpoint.base_url.clone());
                    worker.spawn(move || {
                        AppEvent::ModelTest(
                            mtt_translate::test_model(&config).map(|reply| format!("모델 응답 확인 완료: {reply}")),
                        )
                    });
                }
                ui.label(
                    RichText::new("서버가 뜨고 간단한 텍스트 요청에 응답하는지만 확인합니다. 실제 이미지 번역 가능 여부와는 다를 수 있습니다.")
                        .weak()
                        .small(),
                );
                if job_active {
                    ui.label(RichText::new("번역 작업 중에는 모델 테스트를 실행할 수 없습니다.").weak().small());
                }
                if self.testing {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(RichText::new("모델을 불러오고 간단한 텍스트 응답을 확인하는 중입니다...").strong());
                    });
                    ui.label(RichText::new("이 테스트는 모델 로드와 텍스트 응답만 확인합니다.").small());
                }
                match &self.test_result {
                    Some(Ok(message)) => {
                        ui.label(RichText::new(message).strong().color(egui::Color32::LIGHT_GREEN));
                        if let Some(endpoint) = &self.test_endpoint {
                            ui.label(RichText::new(format!("엔드포인트: {endpoint}")).small());
                        }
                    }
                    Some(Err(error)) => {
                        ui.label(RichText::new(error).strong().color(egui::Color32::LIGHT_RED));
                    }
                    None => {}
                }
                if draft.model_provider == ModelProvider::OpenAiCompatible && !base_url_valid {
                    ui.label(RichText::new("Base URL은 http:// 또는 https://로 시작해야 합니다.").weak().small());
                }
            });
            ui.separator();
            self.version_bar(ui, worker);
            if let Some(error) = &self.save_error {
                ui.colored_label(egui::Color32::LIGHT_RED, format!("저장 실패: {error}"));
            }
            if let Some(message) = &self.reset_message {
                ui.label(RichText::new(message).small());
            }
            ui.horizontal(|ui| {
                if ui.button("기본값 복원").clicked() {
                    reset = true;
                }
                if ui.button("취소").clicked() {
                    cancel = true;
                }
                if ui.add_enabled(can_submit, egui::Button::new("저장")).clicked() {
                    save = true;
                }
            });
        });
        if reset {
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

fn toggle_row(ui: &mut egui::Ui, label: &str, value: &mut bool) {
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.selectable_label(*value, if *value { "켜짐" } else { "꺼짐" }).clicked() {
            *value = !*value;
        }
    });
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
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.label(RichText::new("Python 설치가 필요합니다.").strong());
        let command = mtt_inpaint::lama_runtime::python_install_command();
        ui.horizontal(|ui| {
            ui.label(RichText::new(command).monospace());
            if ui.small_button("복사").clicked() {
                ui.ctx().copy_text(command.to_owned());
            }
        });
        for line in mtt_inpaint::lama_runtime::python_install_help() {
            ui.label(RichText::new(*line).small());
        }
    });
}

fn runtime_state(ui: &mut egui::Ui, label: &str, ready: bool, busy: bool) {
    let (text, color) = if ready {
        ("준비됨", egui::Color32::LIGHT_GREEN)
    } else if busy {
        ("진행 중", egui::Color32::from_rgb(240, 200, 80))
    } else {
        ("필요", egui::Color32::LIGHT_RED)
    };
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_width(96.0);
        ui.vertical(|ui| {
            ui.label(RichText::new(label).small());
            ui.label(RichText::new(text).strong().color(color));
        });
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
