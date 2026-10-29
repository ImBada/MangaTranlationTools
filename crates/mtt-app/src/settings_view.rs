//! Settings window (`SettingsModal.tsx`): model provider, endpoints, translation mode,
//! parallelism, one-hand mode, a model connectivity test, the LaMa download
//! (`LamaRuntimeSettingsSection.tsx`), the text segmentation model's download and the
//! version/update bar (`SettingsVersionBar.tsx`).

use eframe::egui::{self, RichText};
use mtt_core::settings::{
    AppSettings, ModelProvider, ReasoningEffort, TRANSLATION_PARALLEL_MAX, TRANSLATION_PARALLEL_MIN, TranslationMode,
};
use mtt_inpaint::torch_runtime::{Flavor, Preference, RuntimeStatus};

use crate::app::AppEvent;
use crate::services::Services;
use crate::theme;
use crate::update_check::{self, UpdateStatus};
use crate::worker::Worker;

/// `LamaRuntimeStatus` without the job flags (those live in [`SettingsView`]), with the
/// text segmentation model's. Both carry the LibTorch runtime they share.
#[derive(Debug, Clone)]
pub struct LamaRuntimeStatus {
    lama: mtt_inpaint::lama::LamaStatus,
    segmentation: mtt_inpaint::text_segmentation::SegmentationStatus,
}

impl LamaRuntimeStatus {
    /// Asks the NVIDIA driver: on a worker thread.
    fn detect(paths: &mtt_core::paths::AppPaths) -> Self {
        Self { lama: mtt_inpaint::lama::status(paths), segmentation: mtt_inpaint::text_segmentation::status(paths) }
    }

    fn runtime(&self) -> &RuntimeStatus {
        &self.lama.runtime
    }
}

/// Which download a [`LamaEvent`] is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelJob {
    Lama,
    Segmentation,
}

pub enum LamaEvent {
    Status(Box<LamaRuntimeStatus>),
    Progress(ModelJob, String),
    DownloadFinished(ModelJob, Result<(), String>),
}

/// A model download's progress and outcome in the settings.
#[derive(Default)]
struct DownloadState {
    running: bool,
    progress: Option<String>,
    last_error: Option<String>,
    action: Option<(ActionState, String)>,
}

/// ChatGPT sign-in progress from its worker thread.
pub enum CodexLoginEvent {
    /// Enter `user_code` at `url` (opened in the browser already).
    Code {
        user_code: String,
        url: String,
    },
    Finished(Result<mtt_translate::codex::Account, String>),
}

/// A sign-in waiting for the user to approve the code in the browser.
struct CodexLogin {
    code: Option<(String, String)>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
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

/// The models one engine and address offer.
struct ModelList {
    /// `codex:{account}`, or the base URL and key they were asked of.
    source: String,
    /// `None` while loading.
    result: Option<Result<Vec<mtt_translate::ModelOption>, String>>,
}

/// How long the engine and address must stay the same before their models load, so an
/// address isn't asked at every keystroke.
const MODEL_LIST_SETTLE: std::time::Duration = std::time::Duration::from_millis(700);

#[derive(Default)]
pub struct SettingsView {
    open: bool,
    tab: SettingsTab,
    draft: Option<AppSettings>,
    testing: bool,
    test_result: Option<Result<String, String>>,
    save_error: Option<String>,
    /// The window's close button was pressed while the model or address is invalid.
    close_blocked: bool,
    show_key: bool,
    lama_status: Option<LamaRuntimeStatus>,
    lama: DownloadState,
    segmentation: DownloadState,
    /// Saving the CPU/CUDA choice failed.
    runtime_error: Option<String>,
    update: Option<UpdateStatus>,
    update_busy: bool,
    test_endpoint: Option<String>,
    reset_message: Option<String>,
    lama_requested: bool,
    /// The library's LaMa notice was closed (until the next launch).
    lama_banner_dismissed: bool,
    /// The signed-in ChatGPT account (read when the settings open).
    codex_account: Option<mtt_translate::codex::Account>,
    codex_login: Option<CodexLogin>,
    codex_message: Option<Result<String, String>>,
    /// The selected engine's models, for the model field's list.
    models: Option<ModelList>,
    /// The engine and address the model field is for, and since when.
    model_source: Option<(String, std::time::Instant)>,
}

impl SettingsView {
    pub fn open(&mut self, services: &Services, worker: &Worker<AppEvent>) {
        self.open = true;
        self.codex_account = mtt_translate::codex::CodexAuth::new(&services.paths.codex_auth_path).account();
        self.codex_message = None;
        self.models = None;
        self.model_source = None;
        self.draft = Some(services.settings.clone());
        self.test_result = None;
        self.save_error = None;
        self.close_blocked = false;
        self.lama.action = None;
        self.segmentation.action = None;
        self.runtime_error = None;
        self.reset_message = None;
        self.refresh_lama_status(services, worker);
        if !self.update.as_ref().is_some_and(UpdateStatus::fresh) {
            self.refresh_update(worker);
        }
    }

    fn refresh_lama_status(&self, services: &Services, worker: &Worker<AppEvent>) {
        let paths = services.paths.clone();
        worker.spawn(move || AppEvent::Lama(LamaEvent::Status(Box::new(LamaRuntimeStatus::detect(&paths)))));
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

    /// Returns true when a model download finished, so the editor should re-detect the
    /// models.
    pub fn on_lama_event(&mut self, event: LamaEvent, services: &Services, worker: &Worker<AppEvent>) -> bool {
        match event {
            LamaEvent::Status(status) => {
                self.lama_status = Some(*status);
                false
            }
            LamaEvent::Progress(job, line) => {
                self.download_state(job).progress = Some(line);
                false
            }
            LamaEvent::DownloadFinished(job, result) => {
                let done = match job {
                    ModelJob::Lama => "LaMa 준비가 끝났습니다.",
                    ModelJob::Segmentation => "글자 분할 모델 준비가 끝났습니다.",
                };
                let state = self.download_state(job);
                state.running = false;
                state.progress = None;
                match result {
                    Ok(()) => {
                        state.last_error = None;
                        state.action = Some((ActionState::Success, done.into()));
                    }
                    Err(error) => {
                        log::error!("{job:?} download failed: {error}");
                        state.action = Some((ActionState::Error, error.clone()));
                        state.last_error = Some(error);
                    }
                }
                self.refresh_lama_status(services, worker);
                true
            }
        }
    }

    fn download_state(&mut self, job: ModelJob) -> &mut DownloadState {
        match job {
            ModelJob::Lama => &mut self.lama,
            ModelJob::Segmentation => &mut self.segmentation,
        }
    }

    /// `downloadLamaModel`: the model and the LibTorch runtime, whichever are missing.
    fn download_lama(&mut self, services: &Services, worker: &Worker<AppEvent>) {
        if let Some(status) = self.lama_status.as_ref().map(|status| &status.lama) {
            if status.model_exists && status.runtime.installed {
                self.lama.action = Some((ActionState::Success, "LaMa가 이미 준비되어 있습니다.".into()));
                return;
            }
            if status.runtime.path.is_none() {
                self.lama.action = Some((ActionState::Error, "이 플랫폼용 LibTorch가 없습니다.".into()));
                return;
            }
        }
        self.lama.running = true;
        self.lama.action = Some((ActionState::Success, "LaMa 다운로드를 시작했습니다.".into()));
        let paths = services.paths.clone();
        spawn_download(worker, ModelJob::Lama, move |progress| mtt_inpaint::lama::download_lama(&paths, progress));
    }

    /// The text segmentation model and the LibTorch runtime, whichever are missing.
    fn download_segmentation(&mut self, services: &Services, worker: &Worker<AppEvent>) {
        if let Some(status) = self.lama_status.as_ref().map(|status| &status.segmentation) {
            if status.model_exists && status.runtime.installed {
                self.segmentation.action =
                    Some((ActionState::Success, "글자 분할 모델이 이미 준비되어 있습니다.".into()));
                return;
            }
            if status.runtime.path.is_none() {
                self.segmentation.action = Some((ActionState::Error, "이 플랫폼용 LibTorch가 없습니다.".into()));
                return;
            }
        }
        self.segmentation.running = true;
        self.segmentation.action = Some((ActionState::Success, "글자 분할 모델 다운로드를 시작했습니다.".into()));
        let paths = services.paths.clone();
        spawn_download(worker, ModelJob::Segmentation, move |progress| {
            mtt_inpaint::text_segmentation::download_segmentation(&paths, progress)
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
        if self.lama_banner_dismissed || status.lama.ready() {
            return false;
        }
        let busy = self.lama.running;
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
                        let progress = self.lama.progress.clone().unwrap_or_else(|| "LaMa 준비 중".into());
                        ui.add(egui::Label::new(RichText::new(progress).small()).truncate());
                    } else {
                        ui.add(
                            egui::Label::new(
                                RichText::new("LaMa가 아직 준비되지 않아 로컬 채우기로 지웁니다.").small(),
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
                                egui::Button::new(RichText::new("LaMa 받기…").color(theme::ACCENT))
                                    .frame_when_inactive(false),
                            )
                            .on_hover_text("설정 › 인페인트에서 모델과 LibTorch 다운로드")
                            .clicked()
                        {
                            open_settings = true;
                        }
                    });
                });
            });
        if open_settings {
            self.open(services, worker);
            self.show_inpaint_tab();
        }
        true
    }

    pub fn show_inpaint_tab(&mut self) {
        self.tab = SettingsTab::Inpaint;
    }

    /// The 인페인트 tab: on Windows and Linux the runtime's CUDA/CPU choice, then LaMa and
    /// the text segmentation model.
    fn inpaint_tab(&mut self, ui: &mut egui::Ui, services: &Services, worker: &Worker<AppEvent>) {
        let status = self.lama_status.clone();
        let runtime = status.as_ref().map(LamaRuntimeStatus::runtime);
        if let Some(runtime) = runtime.filter(|runtime| runtime.cuda_offered) {
            self.runtime_section(ui, runtime, services, worker);
            ui.add_space(18.0);
        }
        let runtime_note = |path: Option<&std::path::Path>, missing: &str| match path {
            Some(path) => format!("LibTorch 경로: {}", path.display()),
            None => missing.to_owned(),
        };
        let cuda = runtime.is_some_and(|runtime| runtime.wanted == Flavor::Cuda);
        let runtime_label = if cuda { "LibTorch (CUDA)" } else { "LibTorch" };
        let runtime_size = |cpu: &str| match runtime.filter(|_| cuda) {
            Some(runtime) => format!("CUDA용 LibTorch와 CUDA 라이브러리 약 {}", gigabytes(runtime.size)),
            None => cpu.to_owned(),
        };
        let lama = status.as_ref().map(|status| &status.lama);
        let mut notes = Vec::new();
        if let Some(command) = lama.and_then(|lama| lama.command.as_ref()) {
            notes.push(format!("MANGA_TRANSLATOR_LAMA_COMMAND로 지정한 명령을 씁니다: {}", command.display()));
        }
        if let Some(lama) = lama {
            notes.push(format!("모델 경로: {}", lama.model_path.display()));
            notes.push(runtime_note(
                lama.runtime.path.as_deref(),
                "이 플랫폼용 LibTorch가 없어 LaMa 모델을 앱 안에서 돌릴 수 없습니다.",
            ));
        }
        let sizes = format!("모델 약 200MB, {} (압축)", runtime_size("LibTorch 약 70~120MB"));
        let (download, refresh) = model_section(
            ui,
            &self.lama,
            &ModelSection {
                title: "LaMa 인페인트",
                description: "원문 글자를 더 자연스럽게 지우는 인페인트 모델입니다. 모델과 실행용 LibTorch를 받기 전에는 로컬 채우기를 씁니다.",
                button: "LaMa 받기",
                sizes: &sizes,
                model_exists: lama.is_some_and(|lama| lama.model_exists),
                runtime_label,
                runtime_exists: lama.is_some_and(|lama| lama.runtime.installed),
                runtime_usable: lama.is_some_and(|lama| lama.runtime.usable),
                device: runtime.map(|runtime| runtime.device.as_str()),
                notes,
            },
        );
        if download {
            self.download_lama(services, worker);
        }
        if refresh {
            self.refresh_lama_status(services, worker);
        }
        ui.add_space(18.0);
        let segmentation = status.as_ref().map(|status| &status.segmentation);
        let notes = segmentation
            .map(|segmentation| {
                vec![
                    format!("모델 경로: {}", segmentation.model_path.display()),
                    runtime_note(
                        segmentation.runtime.path.as_deref(),
                        "이 플랫폼용 LibTorch가 없어 글자 분할 모델을 앱 안에서 돌릴 수 없습니다.",
                    ),
                ]
            })
            .unwrap_or_default();
        let sizes = format!("모델 약 210MB, {} (압축, LaMa와 함께 씀)", runtime_size("LibTorch 약 70~120MB"));
        let (download, refresh) = model_section(
            ui,
            &self.segmentation,
            &ModelSection {
                title: "글자 분할 모델",
                description: "블록을 지울 때 글자 픽셀을 찾아 마스크를 만드는 모델입니다(Manga109로 학습, 라이선스 표기 없음). 받기 전이거나 실행에 실패하면 밝기로 글자를 찾습니다.",
                button: "글자 분할 모델 받기",
                sizes: &sizes,
                model_exists: segmentation.is_some_and(|segmentation| segmentation.model_exists),
                runtime_label,
                runtime_exists: segmentation.is_some_and(|segmentation| segmentation.runtime.installed),
                runtime_usable: segmentation.is_some_and(|segmentation| segmentation.runtime.usable),
                device: runtime.map(|runtime| runtime.device.as_str()),
                notes,
            },
        );
        if download {
            self.download_segmentation(services, worker);
        }
        if refresh {
            self.refresh_lama_status(services, worker);
        }
    }

    /// Windows and Linux: the GPU (CUDA) or the CPU for the LibTorch runtime both models
    /// share, what the driver reports and why CUDA isn't used.
    fn runtime_section(
        &mut self,
        ui: &mut egui::Ui,
        runtime: &RuntimeStatus,
        services: &Services,
        worker: &Worker<AppEvent>,
    ) {
        ui.label(theme::semibold("실행 장치"));
        theme::hint(
            ui,
            "LaMa와 글자 분할 모델이 함께 쓰는 LibTorch입니다. NVIDIA GPU가 있으면 CUDA용을 받아 GPU에서 돌립니다.",
        );
        ui.add_space(8.0);
        match &runtime.gpu {
            Ok(gpu) => {
                let current = if runtime.wanted == Flavor::Cuda { Preference::Gpu } else { Preference::Cpu };
                let gpu_hint = format!(
                    "{gpu}에서 실행합니다. 받기 버튼이 CUDA용 LibTorch와 CUDA 라이브러리(약 {})를 받습니다.",
                    gigabytes(runtime.cuda_size)
                );
                let options = [
                    (Preference::Gpu, "GPU (CUDA)", gpu_hint.as_str()),
                    (Preference::Cpu, "CPU", "CPU용 LibTorch(약 70~120MB)로 실행합니다."),
                ];
                if let Some(choice) = theme::segmented(ui, current, &options) {
                    self.runtime_error = mtt_inpaint::torch_runtime::set_preference(&services.paths, choice).err();
                    self.refresh_lama_status(services, worker);
                }
                theme::hint(ui, &format!("NVIDIA GPU: {gpu}"));
            }
            // An installed CUDA build says the same in its note.
            Err(reason) if runtime.note.is_none() => {
                theme::hint(ui, &format!("GPU(CUDA)를 쓸 수 없어 CPU로 실행합니다: {reason}"))
            }
            Err(_) => {}
        }
        if let Some(note) = &runtime.note {
            ui.label(RichText::new(note).small().color(theme::WARNING));
        }
        if let Some(error) = &self.runtime_error {
            ui.label(RichText::new(format!("설정을 저장하지 못했습니다: {error}")).small().color(theme::DANGER));
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
        codex_auth_path: &std::path::Path,
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
                    (ModelProvider::OpenAiCodex, "ChatGPT 계정 (Codex)", ""),
                    (ModelProvider::OpenAiCompatible, "OpenAI 호환 서버", ""),
                ],
            ) {
                draft.model_provider = provider;
            }
            theme::hint(ui, provider_description(draft.model_provider));
        });
        if draft.model_provider == ModelProvider::OpenAiCodex {
            ui.horizontal(|ui| {
                form_label(ui, "계정", label_width);
                ui.vertical(|ui| self.codex_account_ui(ui, codex_auth_path, worker));
            });
            ui.add_space(4.0);
        }
        let source = match draft.model_provider {
            ModelProvider::OpenAiCodex => self.codex_account.as_ref().map(|account| format!("codex:{}", account.id)),
            ModelProvider::OpenAiCompatible => {
                base_url_valid.then(|| format!("{base_url}\n{}", draft.openai_compatible.api_key.trim()))
            }
        };
        self.load_models(ui.ctx(), source.clone(), draft, base_url, worker);
        let mut reload = false;
        let models = self.models.as_ref().filter(|list| source.as_ref() == Some(&list.source));
        match draft.model_provider {
            ModelProvider::OpenAiCodex => {
                row(ui, "모델", &mut |ui| {
                    model_field(
                        ui,
                        "codex-models",
                        &mut draft.codex.model,
                        mtt_core::settings::DEFAULT_CODEX_MODEL,
                        models,
                        &mut reload,
                    );
                    if models.is_none() && self.codex_account.is_none() {
                        theme::hint(ui, "로그인하면 쓸 수 있는 모델 목록을 불러옵니다.");
                    }
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
                    // The listed model says which efforts it takes.
                    let supported = models
                        .and_then(|list| list.result.as_ref()?.as_ref().ok())
                        .and_then(|options| options.iter().find(|option| option.id == draft.codex.model.trim()))
                        .map(|option| &option.efforts)
                        .filter(|efforts| !efforts.is_empty());
                    let effort = draft.codex.reasoning_effort;
                    if let Some(supported) = supported
                        && !supported.iter().any(|name| name == mtt_translate::effort_name(effort))
                    {
                        ui.label(
                            RichText::new(format!("이 모델은 '{}'을 지원하지 않습니다.", effort_label(effort)))
                                .small()
                                .color(theme::WARNING),
                        );
                    }
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
                    model_field(
                        ui,
                        "compatible-models",
                        &mut draft.openai_compatible.model,
                        "gpt-4o-mini",
                        models,
                        &mut reload,
                    );
                });
            }
        }
        if reload && let Some(source) = source {
            self.fetch_models(source, draft, base_url, worker);
        }
        let note = match draft.model_provider {
            ModelProvider::OpenAiCodex => None,
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
        row(ui, "추가 지시문", &mut |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut draft.translation_instructions)
                    .hint_text("예: 반말은 반말로, 의성어는 짧게")
                    .desired_rows(3)
                    .desired_width(300.0),
            );
            theme::hint(
                ui,
                "모든 번역 요청 끝에 붙습니다. 인물 이름·용어는 작품마다 작품 › 화 메뉴의 번역 메모에 적으세요.",
            );
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

    /// Loads the models of `source` (see [`ModelList::source`]) once it has stayed the same
    /// for [`MODEL_LIST_SETTLE`], unless they are loaded or loading.
    fn load_models(
        &mut self,
        ctx: &egui::Context,
        source: Option<String>,
        draft: &AppSettings,
        base_url: &str,
        worker: &Worker<AppEvent>,
    ) {
        let Some(source) = source else { return };
        if self.models.as_ref().is_some_and(|list| list.source == source) {
            return;
        }
        match &self.model_source {
            Some((seen, since)) if *seen == source && since.elapsed() >= MODEL_LIST_SETTLE => {
                self.fetch_models(source, draft, base_url, worker);
            }
            Some((seen, since)) if *seen == source => ctx.request_repaint_after(MODEL_LIST_SETTLE - since.elapsed()),
            _ => {
                self.model_source = Some((source, std::time::Instant::now()));
                ctx.request_repaint_after(MODEL_LIST_SETTLE);
            }
        }
    }

    fn fetch_models(&mut self, source: String, draft: &AppSettings, base_url: &str, worker: &Worker<AppEvent>) {
        let mut settings = draft.normalized();
        settings.openai_compatible.base_url = base_url.to_owned();
        let config = mtt_translate::TranslationConfig::from_settings(&settings);
        self.models = Some(ModelList { source: source.clone(), result: None });
        worker.spawn(move || AppEvent::ModelList { result: mtt_translate::list_models(&config), source });
    }

    pub fn on_model_list(&mut self, source: String, result: Result<Vec<mtt_translate::ModelOption>, String>) {
        if let Some(list) = self.models.as_mut().filter(|list| list.source == source) {
            list.result = Some(result);
        }
    }

    pub fn on_codex_event(&mut self, event: CodexLoginEvent) {
        match event {
            CodexLoginEvent::Code { user_code, url } => {
                if let Some(login) = &mut self.codex_login {
                    login.code = Some((user_code, url));
                }
            }
            CodexLoginEvent::Finished(result) => {
                self.codex_login = None;
                self.codex_message = Some(match result {
                    Ok(account) => {
                        let message =
                            format!("{}(으)로 로그인했습니다.", account.email.as_deref().unwrap_or(&account.id));
                        self.codex_account = Some(account);
                        Ok(message)
                    }
                    Err(error) => Err(error),
                });
            }
        }
    }

    /// Starts the device sign-in on a worker thread: asks for a code, opens the browser
    /// and waits for the approval.
    fn start_codex_login(&mut self, auth_path: &std::path::Path, worker: &Worker<AppEvent>) {
        let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.codex_login = Some(CodexLogin { code: None, cancel: std::sync::Arc::clone(&cancel) });
        self.codex_message = None;
        let auth = mtt_translate::codex::CodexAuth::new(auth_path);
        let emitter = worker.emitter();
        std::thread::spawn(move || {
            let result = auth.start_device_login().and_then(|device| {
                emitter.emit(AppEvent::CodexLogin(CodexLoginEvent::Code {
                    user_code: device.user_code.clone(),
                    url: device.verification_url.clone(),
                }));
                if let Err(error) =
                    crate::editor::report::open_with_system(std::path::Path::new(&device.verification_url))
                {
                    log::warn!("could not open the sign-in page: {error}");
                }
                auth.finish_device_login(&device, &|| cancel.load(std::sync::atomic::Ordering::SeqCst))
            });
            emitter.emit(AppEvent::CodexLogin(CodexLoginEvent::Finished(result)));
        });
    }

    /// The ChatGPT account row: signed in, signing in (the code), or signed out.
    fn codex_account_ui(&mut self, ui: &mut egui::Ui, auth_path: &std::path::Path, worker: &Worker<AppEvent>) {
        if let Some(login) = &self.codex_login {
            match &login.code {
                Some((code, url)) => {
                    theme::hint(ui, "브라우저에서 ChatGPT에 로그인한 뒤 이 코드를 입력하세요.");
                    ui.label(RichText::new(code).monospace().size(22.0).strong());
                    ui.horizontal(|ui| {
                        if ui.button("코드 복사").clicked() {
                            ui.ctx().copy_text(code.clone());
                        }
                        if ui.button("브라우저 열기").clicked()
                            && let Err(error) = crate::editor::report::open_with_system(std::path::Path::new(url))
                        {
                            self.codex_message = Some(Err(format!("브라우저를 열지 못했습니다: {error} ({url})")));
                        }
                        if ui.button("취소").clicked() {
                            login.cancel.store(true, std::sync::atomic::Ordering::SeqCst);
                        }
                        ui.add(egui::Spinner::new().size(12.0));
                    });
                }
                None => {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().size(12.0));
                        ui.label(RichText::new("로그인 코드를 받는 중…").color(theme::TEXT_WEAK));
                    });
                }
            }
        } else if let Some(account) = &self.codex_account {
            let mut logout = false;
            ui.horizontal(|ui| {
                let name = account.email.as_deref().unwrap_or(&account.id);
                let plan = account.plan.as_deref().map(|plan| format!(" · {plan}")).unwrap_or_default();
                ui.label(RichText::new(format!("{name}{plan}")).color(theme::SUCCESS));
                logout = ui.button("로그아웃").clicked();
            });
            if logout {
                let result = mtt_translate::codex::CodexAuth::new(auth_path).logout();
                self.codex_message = Some(result.map(|()| "로그아웃했습니다.".to_owned()));
                self.codex_account = None;
            }
        } else {
            if ui.button(theme::icon_text(theme::icon::LINK_SIMPLE, "ChatGPT 로그인")).clicked() {
                self.start_codex_login(auth_path, worker);
            }
            theme::hint(
                ui,
                "브라우저에서 ChatGPT 계정으로 로그인합니다. 토큰은 데이터 폴더의 codex-auth.json에 저장됩니다.",
            );
        }
        match &self.codex_message {
            Some(Ok(message)) => {
                ui.label(RichText::new(message).small().color(theme::SUCCESS));
            }
            Some(Err(error)) => {
                ui.label(RichText::new(error).small().color(theme::DANGER));
            }
            None => {}
        }
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
        // Esc is 취소, unless it is closing an open list first.
        if !egui::Popup::is_any_open(ctx)
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape))
        {
            cancel = true;
        }
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
                        (SettingsTab::Inpaint, "인페인트", "LaMa와 글자 분할 모델"),
                        (SettingsTab::Editing, "편집", "화면과 단축키 옵션"),
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
                            &services.paths.codex_auth_path,
                        ),
                        SettingsTab::Inpaint => self.inpaint_tab(ui, services, worker),
                        SettingsTab::Editing => {
                            toggle_row(ui, "한손모드", &mut draft.one_hand_mode);
                            theme::hint(
                                ui,
                                "편집 화면에서 Q 키를 삭제 키로 씁니다. 텍스트 입력 중에는 적용되지 않습니다.",
                            );
                            ui.add_space(14.0);
                            toggle_row(ui, "확대 시 부드럽게 보정", &mut draft.zoom_smoothing);
                            theme::hint(
                                ui,
                                "켜면 확대한 페이지의 픽셀 사이를 보간해 부드럽지만 흐릿하게 보입니다. 끄면 픽셀을 그대로 보여 줍니다. 축소할 때는 항상 부드럽게 표시합니다.",
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
                if self.close_blocked && !can_submit {
                    ui.colored_label(
                        theme::DANGER,
                        "모델 이름과 Base URL을 고쳐야 저장하고 닫을 수 있습니다. 바꾼 내용을 버리려면 취소를 누르세요.",
                    );
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
        // The window's close button acts like the TS header "닫기" (submit); with an
        // invalid model or address it says why it stays open.
        if !open && !can_submit && !cancel {
            self.close_blocked = true;
            self.tab = SettingsTab::Translation;
        }
        if (save || !open) && can_submit && !cancel {
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
        ModelProvider::OpenAiCodex => "ChatGPT 계정으로 로그인해 그 구독으로 Codex 모델에 요청합니다.",
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

/// What a model's part of the 인페인트 tab shows.
struct ModelSection<'a> {
    title: &'a str,
    description: &'a str,
    button: &'a str,
    /// The download sizes, on the button's hover.
    sizes: &'a str,
    model_exists: bool,
    /// `LibTorch`, or `LibTorch (CUDA)` when that's the build to get.
    runtime_label: &'a str,
    runtime_exists: bool,
    /// Another build runs meanwhile (the CPU one while CUDA's isn't there).
    runtime_usable: bool,
    /// Where the model runs, once the status is known.
    device: Option<&'a str>,
    /// Paths and the like, under the status.
    notes: Vec<String>,
}

/// A model's status, download button, progress and last error. Returns whether the
/// download and the refresh were clicked.
fn model_section(ui: &mut egui::Ui, state: &DownloadState, section: &ModelSection) -> (bool, bool) {
    ui.label(theme::semibold(section.title));
    theme::hint(ui, section.description);
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 18.0;
        runtime_state(ui, "모델", section.model_exists, state.running, None);
        let stand_in = section.runtime_usable.then_some("받으면 GPU로 실행");
        runtime_state(ui, section.runtime_label, section.runtime_exists, state.running, stand_in);
        if let Some(device) = section.device {
            ui.label(RichText::new(format!("실행: {device}")).small().color(theme::TEXT_WEAK));
        }
    });
    ui.add_space(8.0);
    let (download, refresh) = ui
        .horizontal(|ui| {
            let label = if state.running { "다운로드 중…" } else { section.button };
            let download =
                ui.add_enabled(!state.running, egui::Button::new(label)).on_hover_text(section.sizes).clicked();
            let refresh = ui.add(egui::Button::new("상태 새로고침").frame_when_inactive(false)).clicked();
            (download, refresh)
        })
        .inner;
    if let Some(progress) = state.progress.as_ref().filter(|_| state.running) {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(12.0));
            ui.label(RichText::new(progress).small());
        });
    }
    if let Some((action, message)) = &state.action {
        let color = match action {
            ActionState::Success => theme::SUCCESS,
            ActionState::Error => theme::DANGER,
        };
        ui.label(RichText::new(message).small().color(color));
    }
    if !section.notes.is_empty() {
        ui.add_space(6.0);
        for note in &section.notes {
            theme::hint(ui, note);
        }
    }
    if let Some(error) = &state.last_error {
        theme::hint(ui, &format!("최근 오류: {error}"));
    }
    (download, refresh)
}

/// Runs `download` on its own thread, reporting progress lines and the outcome as `job`'s
/// events.
fn spawn_download(
    worker: &Worker<AppEvent>,
    job: ModelJob,
    download: impl FnOnce(&mut dyn FnMut(mtt_inpaint::lama::Download, u64, Option<u64>)) -> Result<(), String>
    + Send
    + 'static,
) {
    let emitter = worker.emitter();
    std::thread::spawn(move || {
        let mut last = None;
        let result = download(&mut |part, written, total| {
            let mb = |bytes: u64| bytes as f64 / 1_048_576.0;
            let label = match part {
                mtt_inpaint::lama::Download::Model => "모델",
                mtt_inpaint::lama::Download::Runtime => "LibTorch",
            };
            let line = match total {
                Some(total) if total > 0 => {
                    let percent = written * 100 / total;
                    if last == Some((part, percent)) {
                        return;
                    }
                    last = Some((part, percent));
                    format!("{label} 다운로드 {percent}% ({:.0}/{:.0}MB)", mb(written), mb(total))
                }
                _ => format!("{label} 다운로드 {:.0}MB", mb(written)),
            };
            emitter.emit(AppEvent::Lama(LamaEvent::Progress(job, line)));
        });
        emitter.emit(AppEvent::Lama(LamaEvent::DownloadFinished(job, result)));
    });
}

/// `bytes` as `2.2GB`.
fn gigabytes(bytes: u64) -> String {
    format!("{:.1}GB", bytes as f64 / f64::from(1u32 << 30))
}

/// `optional`: what to say when it's missing but not needed.
fn runtime_state(ui: &mut egui::Ui, label: &str, ready: bool, busy: bool, optional: Option<&str>) {
    let (text, color) = if ready {
        ("준비됨", theme::SUCCESS)
    } else if busy {
        ("진행 중", theme::WARNING)
    } else if let Some(text) = optional {
        (text, theme::WARNING)
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

/// The model name field with the engine's models to pick from beside it, and what
/// loading them said. Sets `reload` when asked to load them again.
fn model_field(
    ui: &mut egui::Ui,
    id: &str,
    model: &mut String,
    hint: &str,
    list: Option<&ModelList>,
    reload: &mut bool,
) {
    let options = list.and_then(|list| list.result.as_ref()?.as_ref().ok());
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(model).hint_text(hint).desired_width(220.0));
        ui.add_enabled_ui(options.is_some(), |ui| {
            ui.push_id(id, |ui| {
                let button =
                    egui::Button::new((RichText::new("목록"), theme::icon(theme::icon::CARET_DOWN).size(12.0)));
                egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                        for option in options.into_iter().flatten() {
                            let label = if option.name == option.id {
                                option.id.clone()
                            } else {
                                format!("{} · {}", option.name, option.id)
                            };
                            let mut item = ui.selectable_label(model.trim() == option.id, label);
                            if !option.description.is_empty() {
                                item = item.on_hover_text(&option.description);
                            }
                            if item.clicked() {
                                model.clone_from(&option.id);
                                ui.close();
                            }
                        }
                    });
                })
            })
        });
        let idle = list.is_some_and(|list| list.result.is_some());
        if ui.add_enabled(idle, egui::Button::new("새로고침")).on_hover_text("모델 목록 다시 불러오기").clicked()
        {
            *reload = true;
        }
    });
    match list.map(|list| list.result.as_ref()) {
        Some(None) => {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(12.0));
                theme::hint(ui, "모델 목록을 불러오는 중…");
            });
        }
        Some(Some(Ok(options))) if !options.iter().any(|option| option.id == model.trim()) => {
            ui.label(
                RichText::new("목록에 없는 모델입니다. 옆의 목록에서 고르거나 이름을 확인하세요.")
                    .small()
                    .color(theme::WARNING),
            );
        }
        Some(Some(Ok(options))) => theme::hint(ui, &format!("모델 {}개 중에서 고를 수 있습니다.", options.len())),
        Some(Some(Err(error))) => {
            ui.label(RichText::new(format!("모델 목록을 불러오지 못했습니다: {error}")).small().color(theme::DANGER));
        }
        None => {}
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        ctx: egui::Context,
        services: Services,
        worker: Worker<AppEvent>,
        _dir: tempfile::TempDir,
    }

    impl Fixture {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let ctx = egui::Context::default();
            theme::apply(&ctx);
            let services = Services::new(&ctx, mtt_core::paths::AppPaths::from_data_root(dir.path()));
            let worker = Worker::new(&ctx, 1);
            Self { ctx, services, worker, _dir: dir }
        }

        /// The dialog on its 편집 tab (no model list to load) with `model` for the ChatGPT
        /// engine, which has no account here.
        fn view(&self, model: &str) -> SettingsView {
            let mut draft = self.services.settings.clone();
            draft.model_provider = ModelProvider::OpenAiCodex;
            draft.codex.model = model.into();
            SettingsView { open: true, tab: SettingsTab::Editing, draft: Some(draft), ..SettingsView::default() }
        }

        fn frame(&mut self, view: &mut SettingsView, events: Vec<egui::Event>) {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440.0, 900.0))),
                events,
                ..Default::default()
            };
            let Self { ctx, services, worker, .. } = self;
            let mut output = ctx.run_ui(input, |ui| view.ui(ui.ctx(), services, worker, false));
            output.textures_delta.clear();
        }

        fn click(&mut self, view: &mut SettingsView, pos: egui::Pos2) {
            let button = |pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame(view, vec![egui::Event::PointerMoved(pos)]);
            self.frame(view, vec![button(true)]);
            self.frame(view, vec![button(false)]);
        }

        /// The ✕ in the window's title bar (16pt margin, 17pt heading).
        fn close_button(&self) -> egui::Pos2 {
            let id = egui::Id::unique(Some(std::borrow::Cow::Borrowed("설정")));
            let rect = self.ctx.memory(|memory| memory.area_rect(id)).expect("settings window");
            egui::pos2(rect.right() - 26.0, rect.top() + 26.0)
        }
    }

    fn escape() -> egui::Event {
        let key = egui::Key::Escape;
        egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn escape_cancels_and_the_close_button_says_why_it_stays_open() {
        let mut fixture = Fixture::new();
        let mut view = fixture.view("gpt-test");
        fixture.frame(&mut view, Vec::new());
        fixture.frame(&mut view, vec![escape()]);
        assert!(!view.is_open(), "Esc closes");
        assert_ne!(fixture.services.settings.codex.model, "gpt-test", "without saving");

        // The ✕ saves a valid draft and closes…
        let mut view = fixture.view("gpt-test");
        fixture.frame(&mut view, Vec::new());
        fixture.frame(&mut view, Vec::new());
        let close = fixture.close_button();
        fixture.click(&mut view, close);
        assert!(!view.is_open());
        assert_eq!(fixture.services.settings.codex.model, "gpt-test");

        // …and with no model stays open, saying why, on the tab to fix it.
        let mut view = fixture.view("");
        fixture.frame(&mut view, Vec::new());
        fixture.frame(&mut view, Vec::new());
        fixture.click(&mut view, close);
        assert!(view.is_open());
        assert!(view.close_blocked);
        assert!(view.tab == SettingsTab::Translation);
        assert_eq!(fixture.services.settings.codex.model, "gpt-test", "nothing saved");
    }
}
