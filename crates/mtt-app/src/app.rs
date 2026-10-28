//! Top-level app: routes background events and switches between library and editor.

use eframe::egui;
use mtt_core::paths::AppPaths;

use crate::automation::{Automation, Request};
use crate::editor::assets::PageLoad;
use crate::editor::{Editor, EditorAction};
use crate::library_view::{LibraryAction, LibraryView};
use crate::services::Services;
use crate::settings_view::SettingsView;
use crate::worker::Worker;

pub enum AppEvent {
    PageLoaded(Box<PageLoad>),
    Thumbnail(Box<crate::editor::thumbnails::ThumbLoad>),
    ChapterSaved { chapter_id: String, seq: u64, error: Option<String> },
    LayerSaved { page_id: String, error: Option<String> },
    ExportProgress { current: usize, total: usize },
    ExportFinished { written: usize, errors: Vec<String> },
    InpaintFinished { chapter_id: String, page_id: String, outcome: Result<crate::editor::InpaintOutcome, String> },
    Translation { chapter_id: String, event: crate::editor::translation::TranslationEvent },
    ModelTest(Result<String, String>),
    Lama(crate::settings_view::LamaEvent),
    UpdateChecked(crate::update_check::UpdateStatus),
    Report { chapter_id: String, event: crate::editor::report::ReportEvent },
    ReportFocus { page_id: String, block_id: String },
    Import(crate::library_view::ImportEvent),
}

enum Screen {
    Library,
    Editor(Box<Editor>),
}

pub struct MttApp {
    services: Services,
    worker: Worker<AppEvent>,
    screen: Screen,
    library: LibraryView,
    settings: SettingsView,
    automation: Option<Automation>,
    /// Chapters still to translate in a "작품 일괄 번역" run.
    batch_queue: std::collections::VecDeque<String>,
}

impl MttApp {
    pub fn new(ctx: &egui::Context, paths: AppPaths) -> Self {
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(2, 6);
        let mut services = Services::new(ctx, paths);
        let mut library = LibraryView::default();
        library.refresh(&mut services);
        let worker = Worker::new(ctx, threads);
        services.report_focus_url = crate::editor::report::start_focus_server(worker.emitter());
        Self {
            services,
            worker,
            screen: Screen::Library,
            library,
            settings: SettingsView::default(),
            automation: Automation::from_env(),
            batch_queue: Default::default(),
        }
    }

    fn handle_events(&mut self, ctx: &egui::Context) {
        let mut batch_finished: Option<bool> = None;
        let mut pending_open: Option<String> = None;
        let mut pending_batch: Option<Vec<String>> = None;
        for event in self.worker.drain() {
            match (event, &mut self.screen) {
                (AppEvent::PageLoaded(load), Screen::Editor(editor)) => editor.on_page_loaded(*load),
                (AppEvent::Thumbnail(load), Screen::Editor(editor)) => {
                    let chapter_id = editor.chapter.id.clone();
                    editor.thumbnails.on_loaded(ctx, &chapter_id, *load);
                }
                (AppEvent::ChapterSaved { chapter_id, seq, error }, Screen::Editor(editor)) => {
                    editor.on_chapter_saved(&chapter_id, seq, error)
                }
                (AppEvent::LayerSaved { page_id, error }, Screen::Editor(editor)) => {
                    editor.on_layer_saved(&page_id, error)
                }
                (AppEvent::InpaintFinished { chapter_id, page_id, outcome }, Screen::Editor(editor)) => {
                    editor.on_inpaint_finished(&chapter_id, &page_id, outcome)
                }
                (AppEvent::Translation { chapter_id, event }, Screen::Editor(editor)) => {
                    if let Some(success) = editor.on_translation_event(&chapter_id, event) {
                        batch_finished = Some(success);
                    }
                }
                (AppEvent::ModelTest(result), _) => self.settings.on_test_result(result),
                (AppEvent::UpdateChecked(status), _) => self.settings.on_update_checked(status),
                (AppEvent::Report { chapter_id, event }, Screen::Editor(editor)) => {
                    editor.on_report_event(&chapter_id, event)
                }
                (AppEvent::ReportFocus { page_id, block_id }, Screen::Editor(editor)) => {
                    editor.focus_report_block(&page_id, &block_id);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                (AppEvent::Lama(event), screen) => {
                    if self.settings.on_lama_event(event, &self.services, &self.worker)
                        && let Screen::Editor(editor) = screen
                    {
                        editor.configure_inpaint(&self.services);
                    }
                }
                (AppEvent::Import(event), _) => match self.library.on_import_event(event, &mut self.services) {
                    Some(LibraryAction::Open(chapter_id)) => pending_open = Some(chapter_id),
                    Some(LibraryAction::OpenBatch(ids)) => pending_batch = Some(ids),
                    _ => {}
                },
                (AppEvent::ExportProgress { current, total }, Screen::Editor(editor)) => {
                    editor.export_progress = Some((current, total));
                }
                (AppEvent::ExportFinished { written, errors }, Screen::Editor(editor)) => {
                    editor.export_progress = None;
                    editor.status = if errors.is_empty() {
                        format!("전체 페이지 출력 완료: {written}p")
                    } else {
                        format!("출력 {written}페이지, 실패 {}건: {}", errors.len(), errors.join(", "))
                    };
                }
                (AppEvent::ChapterSaved { error: Some(error), .. }, _)
                | (AppEvent::LayerSaved { error: Some(error), .. }, _) => {
                    log::error!("save failed after leaving the editor: {error}");
                    self.library.error = Some(format!("저장 실패: {error}"));
                }
                _ => {}
            }
        }
        match batch_finished {
            Some(true) if !self.batch_queue.is_empty() => self.advance_batch(),
            Some(false) => self.batch_queue.clear(),
            _ => {}
        }
        if let Some(chapter_id) = pending_open {
            self.switch_chapter(&chapter_id);
        }
        if let Some(ids) = pending_batch {
            self.batch_queue = ids.into();
            self.advance_batch();
        }
    }

    /// Opens another chapter from the editor (the TS library popover) after saving.
    fn switch_chapter(&mut self, chapter_id: &str) {
        if let Screen::Editor(editor) = &mut self.screen {
            if editor.chapter.id == chapter_id {
                return;
            }
            if editor.translation_running() || editor.inpaint_busy() {
                editor.status = "작업이 끝난 뒤에 다른 화를 여세요.".into();
                return;
            }
            if let Err(error) = editor.flush(&mut self.services) {
                editor.status = format!("저장 실패: {error}");
                return;
            }
        }
        self.open_chapter(chapter_id);
    }

    /// Opens the next queued chapter and starts translating its pending pages; stops the
    /// batch when a chapter did not finish cleanly (like the TS loop's `break`).
    fn advance_batch(&mut self) {
        if let Screen::Editor(editor) = &mut self.screen
            && editor.inpaint_busy()
        {
            editor.status = "인페인트가 실행 중이라 작품 일괄 번역을 멈춥니다.".into();
            self.batch_queue.clear();
            return;
        }
        if let Screen::Editor(editor) = &mut self.screen
            && let Err(error) = editor.flush(&mut self.services)
        {
            editor.status = format!("저장 실패로 일괄 번역을 멈춥니다: {error}");
            self.batch_queue.clear();
            return;
        }
        let Some(chapter_id) = self.batch_queue.pop_front() else { return };
        self.open_chapter(&chapter_id);
        if let Screen::Editor(editor) = &mut self.screen {
            editor.start_translation(crate::editor::translation::RunMode::Pending, &self.services, &self.worker);
            let remaining = self.batch_queue.len();
            if remaining > 0 {
                editor.status = format!("작품 일괄 번역: 이 화 다음 {remaining}개 화가 남았습니다.");
            }
        }
    }

    fn open_chapter(&mut self, chapter_id: &str) {
        match self.services.library.open_chapter(chapter_id) {
            Ok(chapter) => self.screen = Screen::Editor(Box::new(Editor::open(chapter, &self.services))),
            Err(error) => self.library.error = Some(error.to_string()),
        }
    }

    fn run_automation(&mut self, ctx: &egui::Context) {
        let Some(automation) = &mut self.automation else { return };
        let editor = match &mut self.screen {
            Screen::Editor(editor) => Some(editor.as_mut()),
            Screen::Library => None,
        };
        match automation.tick(ctx, editor, &mut self.services, Some(&self.worker)) {
            Some(Request::OpenFirstChapter) => match self.library.first_chapter_id() {
                Some(id) => self.open_chapter(&id),
                None => log::error!("automation: library is empty"),
            },
            Some(Request::ImportFolder(path)) => self.library.auto_import(&path, &self.services, &self.worker),
            Some(Request::OpenSettings) => self.settings.open(&self.services, &self.worker),
            None => {}
        }
    }

    fn close_editor(&mut self) {
        if let Screen::Editor(editor) = &mut self.screen {
            if editor.translation_running() {
                editor.status = "번역이 실행 중입니다. 취소하거나 끝난 뒤에 나가세요.".into();
                return;
            }
            if editor.inpaint_busy() {
                editor.status = "인페인트가 끝난 뒤에 나가세요.".into();
                return;
            }
            if let Err(error) = editor.flush(&mut self.services) {
                editor.status = format!("저장 실패: {error}");
                return;
            }
            self.screen = Screen::Library;
            self.library.refresh(&mut self.services);
        }
    }
}

impl eframe::App for MttApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_events(&ctx);
        match &mut self.screen {
            Screen::Library => match self.library.ui(ui, &mut self.services, &mut |ui, services| {
                self.settings.lama_empty_card(ui, services, &self.worker)
            }) {
                Some(LibraryAction::Open(chapter_id)) => self.open_chapter(&chapter_id),
                Some(LibraryAction::OpenBatch(ids)) => {
                    self.batch_queue = ids.into();
                    self.advance_batch();
                }
                Some(LibraryAction::OpenSettings) => self.settings.open(&self.services, &self.worker),
                None => {}
            },
            Screen::Editor(editor) => {
                editor.app_modal_open = self.settings.is_open() || self.library.import_open();
                let action = editor.ui(ui, &mut self.services, &self.worker);
                self.on_editor_action(action);
            }
        }
        let job_active = matches!(&self.screen, Screen::Editor(editor) if editor.translation_running());
        let blocked = if job_active {
            Some("번역 중에는 가져올 수 없습니다")
        } else if self.settings.is_open() {
            Some("설정 창을 닫은 뒤 놓아 주세요")
        } else if self.library.import_open() {
            Some("열려 있는 가져오기 창을 먼저 끝내 주세요")
        } else {
            None
        };
        if let Some(error) = self.library.handle_file_drop(&ctx, blocked)
            && let Screen::Editor(editor) = &mut self.screen
        {
            editor.status = error;
        }
        self.library.import_dialog(&ctx, &mut self.services, &self.worker);
        self.settings.ui(&ctx, &mut self.services, &self.worker, job_active);
        self.run_automation(&ctx);
        self.services.tick_ui_state(&ctx);
    }

    /// Tab / Shift+Tab step through text blocks. egui would otherwise move keyboard focus
    /// to the first button (which Space, the pan key, then presses), so the key is taken
    /// out before egui sees it unless a text field or dialog has focus.
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if let Some(automation) = &mut self.automation {
            raw_input.events.extend(automation.take_injected_events());
        }
        let Screen::Editor(editor) = &mut self.screen else { return };
        if self.settings.is_open() || self.library.import_open() || ctx.memory(|memory| memory.focused().is_some()) {
            return;
        }
        raw_input.events.retain(|event| match event {
            egui::Event::Key { key: egui::Key::Tab, pressed, modifiers, .. }
                if !modifiers.command && !modifiers.alt && !modifiers.ctrl =>
            {
                if *pressed {
                    editor.pending_block_step += if modifiers.shift { -1 } else { 1 };
                    ctx.request_repaint();
                }
                false
            }
            _ => true,
        });
    }

    fn on_exit(&mut self) {
        if let Screen::Editor(editor) = &mut self.screen
            && let Err(error) = editor.flush(&mut self.services)
        {
            log::error!("final save failed: {error}");
        }
        self.services.flush_ui_state();
    }
}

impl MttApp {
    fn on_editor_action(&mut self, action: Option<EditorAction>) {
        match action {
            Some(EditorAction::Close) => self.close_editor(),
            Some(EditorAction::OpenSettings) => self.settings.open(&self.services, &self.worker),
            Some(EditorAction::OpenChapter(chapter_id)) => self.switch_chapter(&chapter_id),
            Some(EditorAction::Import(mode)) => {
                self.library.refresh(&mut self.services);
                self.library.pick_import(mode);
            }
            None => {}
        }
    }
}
