//! Library screen: works and chapters with progress, open/rename/delete.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use std::path::PathBuf;

use eframe::egui::{self, Color32, Rect, RichText, Sense, Stroke};
use mtt_core::import::{
    ImportPreview, ImportResult, ImportTarget, create_import, preview_archive, preview_folder, preview_images,
    preview_zip_folder,
};
use mtt_core::library::{ChapterSummary, WorkSummary};
use mtt_core::model::ChapterStatus;

use crate::app::AppEvent;
use crate::services::Services;
use crate::theme;
use crate::worker::Worker;

struct ImportDialog {
    preview: ImportPreview,
    new_work: bool,
    new_title: String,
    existing_work: Option<String>,
    running: Option<(usize, usize)>,
    cancel: Arc<AtomicBool>,
    error: Option<String>,
}

/// The four import buttons (`ImportMode` in the TS app).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportMode {
    Images,
    Folder,
    Zip,
    ZipFolder,
}

pub enum LibraryAction {
    Open(String),
    /// "작품 일괄 번역": open each chapter in turn and translate its pending pages.
    OpenBatch(Vec<String>),
    OpenSettings,
    /// A message for the screen in front (an import that could not start).
    Notice(String),
}

pub enum ImportEvent {
    /// The files picked or dropped, listed on a worker (folders and ZIPs can be slow to
    /// walk, e.g. on a network drive).
    Preview(Result<ImportPreview, String>),
    Progress {
        done: usize,
        total: usize,
    },
    Finished(Result<ImportResult, String>),
}

/// `RenameModal` target (renaming also offers deletion, like the TS app).
struct Pending {
    work: bool,
    id: String,
    original: String,
    title: String,
}

#[derive(Default)]
pub struct LibraryView {
    works: Vec<WorkSummary>,
    /// Bumped whenever `works` is replaced.
    works_generation: u64,
    /// The search result for `(query, works_generation)`; searching normalizes every title.
    filtered: Option<(String, u64, std::sync::Arc<Vec<WorkSummary>>)>,
    filter: String,
    pending: Option<Pending>,
    import: Option<ImportDialog>,
    pub error: Option<String>,
    /// Last action message (`pushStatus`).
    pub status: Option<String>,
    pending_error: Option<String>,
    /// Work whose chapters are listed.
    selected_work: Option<String>,
    /// A picked or dropped selection waiting to be listed on a worker.
    queued_preview: Option<PreviewJob>,
    previewing: bool,
}

type PreviewJob = Box<dyn FnOnce() -> Result<ImportPreview, String> + Send>;

/// `resolveChapterStatusLabel` (the chapter status follows its pages' AI translation).
pub fn status_label(status: ChapterStatus) -> &'static str {
    match status {
        ChapterStatus::Idle => "번역 전",
        ChapterStatus::Running => "번역 중",
        ChapterStatus::Completed => "번역 완료",
        ChapterStatus::Partial => "일부 번역",
        ChapterStatus::Failed => "번역 실패",
    }
}

impl LibraryView {
    /// `filter_library` for the current query, recomputed only when the query or the
    /// library changes.
    fn filtered_works(&mut self) -> std::sync::Arc<Vec<WorkSummary>> {
        if let Some((query, generation, works)) = &self.filtered
            && *query == self.filter
            && *generation == self.works_generation
        {
            return std::sync::Arc::clone(works);
        }
        let works = std::sync::Arc::new(mtt_core::library_filter::filter_library(&self.works, &self.filter));
        self.filtered = Some((self.filter.clone(), self.works_generation, std::sync::Arc::clone(&works)));
        works
    }

    pub fn refresh(&mut self, services: &mut Services) {
        match services.library.list() {
            Ok(works) => {
                self.works = works;
                self.works_generation += 1;
                self.error = None;
            }
            Err(error) => self.error = Some(format!("보관함을 읽지 못했습니다: {error}")),
        }
    }

    pub fn import_open(&self) -> bool {
        self.import.is_some()
    }

    pub fn first_chapter_id(&self) -> Option<String> {
        self.works.iter().flat_map(|work| work.chapters.iter()).next().map(|chapter| chapter.id.clone())
    }

    pub fn on_import_event(&mut self, event: ImportEvent, services: &mut Services) -> Option<LibraryAction> {
        if let ImportEvent::Preview(preview) = event {
            self.previewing = false;
            self.status = None;
            let before = self.error.clone();
            self.begin_import(preview);
            return self.error.clone().filter(|error| Some(error) != before.as_ref()).map(LibraryAction::Notice);
        }
        let dialog = self.import.as_mut()?;
        match event {
            ImportEvent::Preview(_) => unreachable!("handled above"),
            ImportEvent::Progress { done, total } => {
                dialog.running = Some((done, total));
                None
            }
            ImportEvent::Finished(Ok(result)) => {
                let batch = dialog.preview.batch;
                self.import = None;
                self.selected_work = Some(result.work_id.clone());
                self.refresh(services);
                if batch {
                    Some(LibraryAction::OpenBatch(result.chapter_ids))
                } else {
                    result.chapter_ids.first().cloned().map(LibraryAction::Open)
                }
            }
            ImportEvent::Finished(Err(error)) => {
                dialog.running = None;
                dialog.error = Some(error);
                None
            }
        }
    }

    fn begin_import(&mut self, preview: Result<ImportPreview, String>) {
        match preview {
            Ok(preview) if preview.chapters.iter().any(|chapter| !chapter.pages.is_empty()) => {
                // `useState(preview.suggestedWorkTitle)`.
                let new_title = preview.suggested_work_title.clone();
                self.import = Some(ImportDialog {
                    new_work: true,
                    new_title,
                    existing_work: self.works.first().map(|work| work.work.id.clone()),
                    preview,
                    running: None,
                    cancel: Arc::new(AtomicBool::new(false)),
                    error: None,
                });
            }
            Ok(_) => self.error = Some("가져올 이미지가 없습니다. (png, jpg, jpeg, webp)".into()),
            Err(error) => self.error = Some(error),
        }
    }

    /// Opens the system picker for `mode`; the selection is listed in the background and
    /// then shown in the import dialog.
    pub fn pick_import(&mut self, mode: ImportMode) {
        let images = ["png", "jpg", "jpeg", "webp", "PNG", "JPG", "JPEG", "WEBP"];
        let job: Option<PreviewJob> = match mode {
            ImportMode::Images => rfd::FileDialog::new()
                .add_filter("이미지", &images)
                .pick_files()
                .map(|paths| Box::new(move || Ok(preview_images(&paths))) as PreviewJob),
            ImportMode::Folder => rfd::FileDialog::new()
                .pick_folder()
                .map(|folder| Box::new(move || preview_folder(&folder).map_err(|e| e.to_string())) as PreviewJob),
            ImportMode::Zip => {
                let mut extensions: Vec<String> = mtt_core::import::ARCHIVE_EXTENSIONS.map(str::to_owned).to_vec();
                extensions.extend(mtt_core::import::ARCHIVE_EXTENSIONS.map(str::to_ascii_uppercase));
                rfd::FileDialog::new()
                    .add_filter("압축파일 · PDF", &extensions)
                    .pick_file()
                    .map(|path| Box::new(move || preview_archive(&path).map_err(|e| e.to_string())) as PreviewJob)
            }
            ImportMode::ZipFolder => rfd::FileDialog::new()
                .pick_folder()
                .map(|folder| Box::new(move || preview_zip_folder(&folder).map_err(|e| e.to_string())) as PreviewJob),
        };
        if let Some(job) = job {
            self.queue_preview(job);
        }
    }

    fn queue_preview(&mut self, job: PreviewJob) {
        self.queued_preview = Some(job);
        self.status = Some("가져올 파일을 확인하는 중…".into());
    }

    /// The import dialog; drawn by the app on both screens (the editor imports too).
    pub fn import_dialog(&mut self, ctx: &egui::Context, services: &mut Services, worker: &Worker<AppEvent>) {
        if let Some(job) = self.queued_preview.take() {
            self.previewing = true;
            worker.spawn(move || AppEvent::Import(ImportEvent::Preview(job())));
        }
        let Some(dialog) = &mut self.import else { return };
        let works: Vec<(String, String)> =
            self.works.iter().map(|w| (w.work.id.clone(), w.work.title.clone())).collect();
        // Esc is 취소 (a running import has its own stop button), unless it is closing an
        // open list first.
        let mut close = dialog.running.is_none()
            && !egui::Popup::is_any_open(ctx)
            && ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let mut start = false;
        let title = if dialog.preview.batch { "작품 일괄 번역 준비" } else { "보관함에 추가" };
        theme::modal(
            ctx,
            egui::Window::new(title)
                .collapsible(false)
                .default_width(520.0)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]),
            |ui| {
                let running = dialog.running.is_some();
                let batch = dialog.preview.batch;
                ui.add_enabled_ui(!running, |ui| {
                    ui.horizontal(|ui| {
                        ui.radio_value(&mut dialog.new_work, true, "새 작품 만들기");
                        ui.add_enabled(!works.is_empty(), egui::RadioButton::new(!dialog.new_work, "기존 작품에 추가"))
                            .clicked()
                            .then(|| dialog.new_work = false);
                    });
                    if dialog.new_work {
                        ui.add(egui::TextEdit::singleline(&mut dialog.new_title).desired_width(320.0));
                    } else {
                        let selected = works
                            .iter()
                            .find(|(id, _)| Some(id) == dialog.existing_work.as_ref())
                            .map(|(_, t)| t.clone())
                            .unwrap_or_default();
                        egui::ComboBox::from_id_salt("import-work").selected_text(selected).show_ui(ui, |ui| {
                            for (id, title) in &works {
                                ui.selectable_value(&mut dialog.existing_work, Some(id.clone()), title);
                            }
                        });
                    }
                    ui.add_space(6.0);
                    ui.label(theme::semibold(if batch { "생성할 화" } else { "화 제목" }));
                    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                        for chapter in &mut dialog.preview.chapters {
                            ui.horizontal(|ui| {
                                if batch {
                                    ui.checkbox(&mut chapter.enabled, format!("{}페이지", chapter.pages.len()));
                                } else {
                                    ui.label(RichText::new(format!("{}페이지", chapter.pages.len())).weak());
                                }
                                ui.add_enabled(
                                    chapter.enabled,
                                    egui::TextEdit::singleline(&mut chapter.title).desired_width(320.0),
                                );
                            });
                        }
                    });
                });
                if let Some(error) = &dialog.error {
                    ui.colored_label(theme::DANGER, error);
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if let Some((done, total)) = dialog.running {
                        theme::progress(
                            ui,
                            if total == 0 { 0.0 } else { done as f32 / total as f32 },
                            260.0,
                            theme::ACCENT,
                        );
                        ui.label(RichText::new(format!("{done}/{total}")).small().color(theme::TEXT_WEAK));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(theme::danger("가져오기 중단")).clicked() {
                                dialog.cancel.store(true, Ordering::SeqCst);
                            }
                        });
                    } else {
                        // `isSubmittable`: a title for a new work and at least one titled chapter.
                        let submittable = (!dialog.new_work || !dialog.new_title.trim().is_empty())
                            && (dialog.new_work || dialog.existing_work.is_some())
                            && dialog.preview.chapters.iter().any(|c| c.enabled && !c.title.trim().is_empty());
                        let label = if batch { "생성 후 번역 시작" } else { "보관함에 추가" };
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add_enabled(submittable, theme::primary(label)).clicked() {
                                start = true;
                            }
                            if ui.button("취소").clicked() {
                                close = true;
                            }
                        });
                    }
                });
            },
        );
        if start && !close {
            self.start_import(services, worker);
        }
        if close {
            self.import = None;
        }
    }

    /// Starts the import of the open dialog on a background thread.
    fn start_import(&mut self, services: &Services, worker: &Worker<AppEvent>) {
        let Some(dialog) = &mut self.import else { return };
        let target = if dialog.new_work {
            ImportTarget::NewWork(dialog.new_title.clone())
        } else {
            match &dialog.existing_work {
                Some(id) => ImportTarget::ExistingWork(id.clone()),
                None => return,
            }
        };
        dialog.error = None;
        dialog.running = Some((0, 0));
        dialog.cancel.store(false, Ordering::SeqCst);
        let library = services.library.clone();
        let preview = dialog.preview.clone();
        let cancel = Arc::clone(&dialog.cancel);
        let emitter = worker.emitter();
        std::thread::Builder::new()
            .name("mtt-import".into())
            .spawn(move || {
                let mut last_report = std::time::Instant::now();
                let result = create_import(
                    &library,
                    &preview,
                    &target,
                    |done, total| {
                        if done == total || last_report.elapsed().as_millis() > 100 {
                            last_report = std::time::Instant::now();
                            emitter.emit(AppEvent::Import(ImportEvent::Progress { done, total }));
                        }
                    },
                    || cancel.load(Ordering::SeqCst),
                )
                .map_err(|error| error.to_string());
                emitter.emit(AppEvent::Import(ImportEvent::Finished(result)));
            })
            .expect("spawn import thread");
    }

    /// Imports a folder of chapter folders/ZIPs without dialogs (automation).
    pub fn auto_import(&mut self, folder: &std::path::Path, services: &Services, worker: &Worker<AppEvent>) {
        self.begin_import(preview_zip_folder(folder).map_err(|e| e.to_string()));
        self.start_import(services, worker);
    }

    /// Imports files dropped on the window (both screens); shows a drop hint while files
    /// hover. `allowed` is false while a translation runs or a dialog is open.
    /// Returns an error to show when the drop could not be imported.
    pub fn handle_file_drop(&mut self, ctx: &egui::Context, blocked: Option<&str>) -> Option<String> {
        let allowed = blocked.is_none();
        let (hovering, dropped) = ctx.input(|i| (!i.raw.hovered_files.is_empty(), i.raw.dropped_files.clone()));
        if hovering {
            let rect = ctx.content_rect();
            egui::Area::new(egui::Id::unique("file-drop-hint"))
                .fixed_pos(rect.min)
                .order(egui::Order::Foreground)
                .interactable(false)
                .show(ctx, |ui| {
                    ui.painter().rect_filled(rect, 0.0, egui::Color32::from_black_alpha(170));
                    ui.painter().rect_stroke(
                        rect.shrink(16.0),
                        12.0,
                        egui::Stroke::new(2.0, if allowed { theme::ACCENT } else { theme::DANGER }),
                        egui::StrokeKind::Inside,
                    );
                    let text = blocked.unwrap_or("놓으면 보관함으로 가져옵니다\n이미지 여러 장 · 폴더 · ZIP");
                    ui.painter().text(
                        rect.center(),
                        egui::Align2::CENTER_CENTER,
                        text,
                        egui::FontId::proportional(22.0),
                        egui::Color32::WHITE,
                    );
                });
        }
        if dropped.is_empty() {
            return None;
        }
        if let Some(reason) = blocked {
            return Some(reason.to_owned());
        }
        let paths: Vec<PathBuf> =
            dropped.iter().map(|file| file.path().to_path_buf()).filter(|path| !path.as_os_str().is_empty()).collect();
        if paths.is_empty() {
            return None;
        }
        if self.previewing || self.queued_preview.is_some() {
            return Some("가져올 파일을 확인하는 중입니다. 잠시 뒤에 다시 놓아 주세요.".into());
        }
        self.queue_preview(Box::new(move || preview_dropped(&paths)));
        None
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        services: &mut Services,
        lama_card: &mut dyn FnMut(&mut egui::Ui, &Services) -> bool,
    ) -> Option<LibraryAction> {
        let mut action = None;
        let filtered = self.filtered_works();
        let search_active = !mtt_core::library_filter::normalize_search_text(&self.filter).is_empty();
        if !filtered.iter().any(|work| Some(&work.work.id) == self.selected_work.as_ref()) {
            self.selected_work = filtered.first().map(|work| work.work.id.clone());
        }

        egui::Panel::top("library-header").frame(theme::bar_frame(theme::APP_BG, 8)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(theme::semibold("보관함").size(15.0))
                    .on_hover_text(services.paths.library_dir.display().to_string());
                let chapters: usize = self.works.iter().map(|work| work.chapters.len()).sum();
                ui.label(
                    RichText::new(format!("작품 {} · 화 {chapters}", self.works.len())).small().color(theme::TEXT_WEAK),
                );
                ui.add_space(16.0);
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter)
                        .prefix(theme::icon(theme::icon::MAGNIFYING_GLASS).size(14.0).color(theme::TEXT_FAINT))
                        .hint_text("작품·화 검색")
                        .desired_width(260.0),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let gear = egui::Button::new(theme::icon(theme::icon::GEAR_SIX).size(17.0).color(theme::TEXT_WEAK))
                        .frame_when_inactive(false)
                        .min_size(egui::vec2(28.0, 28.0));
                    if ui.add(gear).on_hover_text("설정").clicked() {
                        action = Some(LibraryAction::OpenSettings);
                    }
                    let more =
                        egui::Button::new(theme::icon(theme::icon::DOTS_THREE).size(17.0).color(theme::TEXT_WEAK))
                            .frame_when_inactive(false)
                            .min_size(egui::vec2(28.0, 28.0));
                    egui::containers::menu::MenuButton::from_button(more).ui(ui, |ui| {
                        if ui.button("새로고침").clicked() {
                            self.refresh(services);
                            ui.close();
                        }
                        if ui.button("보관함 폴더 열기").clicked() {
                            if let Err(error) = crate::editor::report::open_with_system(&services.paths.library_dir) {
                                self.error = Some(format!("폴더를 열 수 없습니다: {error}"));
                            }
                            ui.close();
                        }
                    });
                    ui.add_space(4.0);
                    let import = egui::Button::new((
                        theme::icon(theme::icon::DOWNLOAD_SIMPLE).size(15.0).color(egui::Color32::WHITE),
                        RichText::new("가져오기").color(egui::Color32::WHITE),
                        theme::icon(theme::icon::CARET_DOWN).size(11.0).color(egui::Color32::WHITE),
                    ))
                    .fill(theme::PRIMARY);
                    if let Some(mode) = import_menu(ui, import) {
                        self.pick_import(mode);
                    }
                });
            });
        });

        egui::Panel::left("library-works")
            .resizable(true)
            .default_size(300.0)
            .size_range(240.0..=440.0)
            .frame(theme::side_frame())
            .show(ui, |ui| {
                ui.label(RichText::new("작품").small().color(theme::TEXT_WEAK));
                ui.add_space(2.0);
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    if filtered.is_empty() {
                        theme::hint(
                            ui,
                            if search_active {
                                "검색 결과가 없습니다."
                            } else {
                                "아직 작품이 없습니다."
                            },
                        );
                    }
                    for work in filtered.iter() {
                        let selected = Some(&work.work.id) == self.selected_work.as_ref();
                        let response = work_row(ui, work, selected);
                        if response.clicked() {
                            self.selected_work = Some(work.work.id.clone());
                        }
                        response.context_menu(|ui| {
                            if ui.button("이름 변경 · 삭제…").clicked() {
                                self.pending = Some(Pending {
                                    work: true,
                                    id: work.work.id.clone(),
                                    original: work.work.title.clone(),
                                    title: work.work.title.clone(),
                                });
                                ui.close();
                            }
                        });
                    }
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(theme::CANVAS_BG).inner_margin(egui::Margin::symmetric(32, 24)))
            .show(ui, |ui| {
                if let Some(error) = &self.error {
                    ui.colored_label(theme::DANGER, error);
                }
                if let Some(status) = &self.status {
                    theme::hint(ui, status);
                }
                if services.fonts_failed() {
                    ui.colored_label(theme::DANGER, "시스템 폰트를 불러오지 못했습니다.");
                }
                egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
                    ui.set_max_width(ui.available_width().min(960.0));
                    // `WorkspaceEmptyState`: the LaMa setup card until it is ready.
                    if lama_card(ui, services) {
                        ui.add_space(12.0);
                    }
                    let selected = filtered.iter().find(|work| Some(&work.work.id) == self.selected_work.as_ref());
                    match selected {
                        Some(work) => {
                            if let Some(next) = self.chapter_list(ui, work, search_active) {
                                action = Some(next);
                            }
                        }
                        None if self.works.is_empty() => welcome(ui, self),
                        None => theme::hint(ui, "검색 결과가 없습니다."),
                    }
                });
                self.apply_reorder(ui, services);
            });
        self.pending_dialog(ui, services);
        action
    }

    /// The selected work: header with rename, then its chapters in reading order.
    fn chapter_list(&mut self, ui: &mut egui::Ui, work: &WorkSummary, search_active: bool) -> Option<LibraryAction> {
        let mut action = None;
        ui.horizontal(|ui| {
            ui.label(theme::semibold(&work.work.title).size(20.0));
            let more = egui::Button::new(theme::icon(theme::icon::DOTS_THREE).size(17.0).color(theme::TEXT_WEAK))
                .frame_when_inactive(false);
            egui::containers::menu::MenuButton::from_button(more).ui(ui, |ui| {
                if ui.button("이름 변경 · 삭제…").clicked() {
                    self.pending = Some(Pending {
                        work: true,
                        id: work.work.id.clone(),
                        original: work.work.title.clone(),
                        title: work.work.title.clone(),
                    });
                    ui.close();
                }
            });
        });
        let pages: usize = work.chapters.iter().map(|chapter| chapter.page_count).sum();
        let done: usize = work.chapters.iter().map(|chapter| chapter.completed_page_count).sum();
        theme::hint(ui, &format!("{}화 · {pages}페이지 · 검수 완료 {done}페이지", work.chapters.len()));
        ui.add_space(16.0);
        if work.chapters.is_empty() {
            theme::hint(ui, "이 작품에는 아직 화가 없습니다.");
        } else {
            chapter_header(ui);
        }
        for chapter in &work.chapters {
            let response = chapter_row(ui, chapter);
            let response = if search_active {
                response
            } else {
                crate::dnd::drag_source(ui, response, (work.work.id.clone(), chapter.id.clone()), &chapter.title)
            };
            if response.clicked() {
                action = Some(LibraryAction::Open(chapter.id.clone()));
            }
            response.context_menu(|ui| {
                if ui.button("열기").clicked() {
                    action = Some(LibraryAction::Open(chapter.id.clone()));
                    ui.close();
                }
                if ui.button("이름 변경 · 삭제…").clicked() {
                    self.pending = Some(Pending {
                        work: false,
                        id: chapter.id.clone(),
                        original: chapter.title.clone(),
                        title: chapter.title.clone(),
                    });
                    ui.close();
                }
            });
            if !search_active && response.dnd_hover_payload::<(String, String)>().is_some() {
                ui.painter().hline(response.rect.x_range(), response.rect.top(), (2.0, theme::ACCENT));
            }
            if !search_active
                && let Some(source) = response.dnd_release_payload::<(String, String)>()
                && source.0 == work.work.id
                && source.1 != chapter.id
            {
                ui.data_mut(|data| {
                    data.insert_temp(
                        egui::Id::unique("library-reorder"),
                        (work.work.id.clone(), source.1.clone(), chapter.id.clone()),
                    )
                });
            }
        }
        action
    }

    /// Applies a chapter drop recorded by `chapter_list` (`reorderByTarget` + `chapterOrder`).
    fn apply_reorder(&mut self, ui: &mut egui::Ui, services: &mut Services) {
        let id = egui::Id::unique("library-reorder");
        let Some((work_id, source, target)) = ui.data_mut(|data| data.remove_temp::<(String, String, String)>(id))
        else {
            return;
        };
        let Some(work) = self.works.iter().find(|work| work.work.id == work_id) else { return };
        let order: Vec<String> = work.chapters.iter().map(|chapter| chapter.id.clone()).collect();
        // `reorderByTarget(work.chapterOrder, source, target)`: before the target row.
        let next =
            mtt_core::library::reorder_by_target(&order, &source, &target, mtt_core::library::DropPosition::Before);
        let result = services.library.reorder_chapters(&work_id, &next);
        self.refresh(services);
        if let Err(error) = result {
            self.error = Some(error.to_string());
        }
    }

    /// `RenameModal`: rename with 저장 (disabled when empty) or delete after confirming.
    fn pending_dialog(&mut self, ui: &mut egui::Ui, services: &mut Services) {
        let Some(pending) = &mut self.pending else { return };
        // Esc is 취소.
        let mut close = ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
        let mut save = false;
        let mut delete = false;
        let heading = if pending.work { "작품 이름 변경" } else { "화 이름 변경" };
        let ctx = ui.ctx().clone();
        theme::modal(
            &ctx,
            egui::Window::new(heading)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]),
            |ui| {
                theme::caption(ui, "이름");
                let response = ui.add(egui::TextEdit::singleline(&mut pending.title).desired_width(320.0));
                let trimmed = pending.title.trim().to_owned();
                if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !trimmed.is_empty() {
                    save = true;
                }
                ui.label(
                    RichText::new(if pending.work {
                        "작품을 삭제하면 포함된 모든 화, 페이지, 번역 결과가 함께 삭제됩니다."
                    } else {
                        "화를 삭제하면 포함된 페이지와 번역 결과가 함께 삭제됩니다."
                    })
                    .weak()
                    .small(),
                );
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let delete_label = if pending.work { "작품 삭제…" } else { "화 삭제…" };
                    if ui.add(theme::danger(delete_label)).clicked() {
                        delete = true;
                    }
                    ui.add_space(40.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_enabled(!trimmed.is_empty(), theme::primary("저장")).clicked() {
                            save = true;
                        }
                        if ui.button("취소").clicked() {
                            close = true;
                        }
                    });
                });
            },
        );
        if save {
            let title = pending.title.trim().to_owned();
            let result = if pending.work {
                services.library.rename_work(&pending.id, &title)
            } else {
                services.library.rename_chapter(&pending.id, &title)
            };
            if let Err(error) = result {
                self.pending_error = Some(error.to_string());
            }
            close = true;
        }
        if delete {
            let (kind, detail) = if pending.work {
                (
                    "작품 삭제",
                    format!(
                        "\"{}\" 작품과 포함된 모든 화, 페이지, 번역 결과가 보관함에서 삭제됩니다.",
                        pending.original
                    ),
                )
            } else {
                (
                    "화 삭제",
                    format!("\"{}\" 화와 포함된 모든 페이지, 번역 결과가 보관함에서 삭제됩니다.", pending.original),
                )
            };
            if crate::theme::confirm(kind, "정말 삭제하시겠습니까?", &detail) {
                let result = if pending.work {
                    services.library.delete_work(&pending.id)
                } else {
                    services.library.delete_chapter(&pending.id)
                };
                self.status = Some(match (result, pending.work) {
                    (Ok(()), true) => format!("{} 작품을 삭제했습니다.", pending.original),
                    (Ok(()), false) => format!("{} 화를 삭제했습니다.", pending.original),
                    (Err(error), true) => {
                        log::error!("delete work failed: {error}");
                        "작품을 삭제하지 못했습니다.".into()
                    }
                    (Err(error), false) => {
                        log::error!("delete chapter failed: {error}");
                        "화를 삭제하지 못했습니다.".into()
                    }
                });
                close = true;
            }
        }
        if close {
            self.pending = None;
            self.refresh(services);
            // `refresh` clears the error; show the rename failure after it.
            if let Some(error) = self.pending_error.take() {
                self.error = Some(error);
            }
        }
    }
}

/// `ImportMode` entries of the "가져오기" menu.
pub const IMPORT_MODES: [(ImportMode, &str, &str); 4] = [
    (ImportMode::Images, "이미지 파일…", "고른 이미지들을 한 화로 가져옵니다"),
    (ImportMode::Folder, "폴더…", "이미지가 든 폴더 하나를 한 화로 가져옵니다"),
    (ImportMode::Zip, "압축파일 · PDF…", "ZIP·CBZ·RAR·CBR·PDF 하나를 한 화로 가져옵니다"),
    (
        ImportMode::ZipFolder,
        "여러 화 일괄 번역…",
        "폴더 안의 압축파일·PDF와 하위 폴더를 각각 한 화로 가져와 차례로 번역합니다",
    ),
];

/// A menu button listing the import modes; returns the picked one.
pub fn import_menu(ui: &mut egui::Ui, button: egui::Button<'static>) -> Option<ImportMode> {
    let mut picked = None;
    egui::containers::menu::MenuButton::from_button(button).ui(ui, |ui| {
        for (mode, label, hint) in IMPORT_MODES {
            if mode == ImportMode::ZipFolder {
                ui.separator();
            }
            if ui.button(label).on_hover_text(hint).clicked() {
                picked = Some(mode);
                ui.close();
            }
        }
    });
    picked
}

/// What a set of dropped paths imports as: images → one chapter; one folder of images →
/// one chapter; one folder of chapter folders/ZIPs → a batch; ZIPs and folders → one
/// chapter each.
pub fn preview_dropped(paths: &[PathBuf]) -> Result<ImportPreview, String> {
    let images: Vec<PathBuf> =
        paths.iter().filter(|path| path.is_file() && mtt_core::paths::is_supported_image_path(path)).cloned().collect();
    let containers: Vec<&PathBuf> =
        paths.iter().filter(|path| path.is_dir() || mtt_core::import::ArchiveKind::of(path).is_some()).collect();
    match (images.is_empty(), containers.as_slice()) {
        (false, []) => Ok(preview_images(&images)),
        (true, [folder]) if folder.is_dir() => {
            let single = preview_folder(folder).map_err(|e| e.to_string())?;
            if single.chapters.iter().any(|chapter| !chapter.pages.is_empty()) {
                Ok(single)
            } else {
                preview_zip_folder(folder).map_err(|e| e.to_string())
            }
        }
        (true, [archive]) => preview_archive(archive).map_err(|e| e.to_string()),
        (true, many) if !many.is_empty() => {
            let mut chapters = Vec::new();
            for path in many {
                let preview = if path.is_dir() { preview_folder(path) } else { preview_archive(path) };
                chapters.extend(preview.map_err(|e| e.to_string())?.chapters);
            }
            chapters.retain(|chapter| !chapter.pages.is_empty());
            chapters.sort_by(|a, b| mtt_core::import::natural_cmp(&a.title, &b.title));
            let suggested_work_title = many[0]
                .parent()
                .and_then(|parent| parent.file_name())
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            Ok(ImportPreview {
                batch: false,
                source_kind: mtt_core::model::ImportSourceKind::Folder,
                suggested_work_title,
                chapters,
            })
        }
        (true, _) => {
            Err("가져올 수 있는 파일이 없습니다. png · jpg · jpeg · webp 이미지, 폴더, ZIP을 놓아 주세요.".into())
        }
        _ => Err("이미지 파일과 폴더/ZIP을 한 번에 섞어서 가져올 수 없습니다. 따로 놓아 주세요.".into()),
    }
}

fn welcome(ui: &mut egui::Ui, view: &mut LibraryView) {
    ui.add_space(60.0);
    ui.vertical_centered(|ui| {
        ui.label(theme::icon(theme::icon::FOLDER_OPEN).size(40.0).color(theme::TEXT_FAINT));
        ui.add_space(10.0);
        ui.label(theme::semibold("보관함이 비어 있습니다").size(18.0));
        ui.add_space(4.0);
        theme::hint(ui, "만화 원고를 가져오면 작품과 화 단위로 보관하고 이어서 번역합니다.");
        ui.add_space(18.0);
        let primary = theme::primary_icon(theme::icon::FOLDER_OPEN, "폴더 가져오기").min_size(egui::vec2(180.0, 34.0));
        if ui.add(primary).on_hover_text("이미지가 든 폴더 하나를 한 화로 가져옵니다").clicked() {
            view.pick_import(ImportMode::Folder);
        }
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let width: f32 = 3.0 * 120.0 + 2.0 * 8.0;
            ui.add_space(((ui.available_width() - width) / 2.0).max(0.0));
            for (mode, glyph, label, hint) in [
                (ImportMode::Images, theme::icon::IMAGE, "이미지 파일", "고른 이미지들을 한 화로 가져옵니다"),
                (
                    ImportMode::Zip,
                    theme::icon::FILE_ZIP,
                    "압축파일 · PDF",
                    "ZIP·CBZ·RAR·CBR·PDF 하나를 한 화로 가져옵니다",
                ),
                (
                    ImportMode::ZipFolder,
                    theme::icon::STACK,
                    "여러 화",
                    "폴더 안의 ZIP과 하위 폴더를 각각 한 화로 가져와 차례로 번역합니다",
                ),
            ] {
                if ui
                    .add(theme::icon_button(glyph, label).min_size(egui::vec2(120.0, 30.0)))
                    .on_hover_text(hint)
                    .clicked()
                {
                    view.pick_import(mode);
                }
            }
        });
        ui.add_space(14.0);
        theme::hint(ui, "이미지 · 폴더 · ZIP을 창에 끌어다 놓아도 됩니다.");
    });
}

fn row_fill(response: &egui::Response, selected: bool) -> Color32 {
    if selected {
        theme::SELECTED
    } else if response.hovered() {
        theme::ROW_HOVER
    } else {
        Color32::TRANSPARENT
    }
}

fn truncated(
    ui: &egui::Ui,
    text: &str,
    font: egui::FontId,
    color: Color32,
    width: f32,
) -> std::sync::Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(width.max(10.0));
    ui.painter().layout_job(job)
}

fn work_row(ui: &mut egui::Ui, work: &WorkSummary, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 46.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        painter.rect_filled(rect, 6.0, row_fill(&response, selected));
        let inner = rect.shrink2(egui::vec2(10.0, 6.0));
        let color = if selected { Color32::WHITE } else { theme::TEXT };
        let title = truncated(ui, &work.work.title, egui::FontId::proportional(13.5), color, inner.width());
        painter.galley(inner.min, title, color);
        let pages: usize = work.chapters.iter().map(|chapter| chapter.page_count).sum();
        let done: usize = work.chapters.iter().map(|chapter| chapter.completed_page_count).sum();
        let meta = format!("{}화 · 검수 {done}/{pages}", work.chapters.len());
        let meta = truncated(ui, &meta, egui::FontId::proportional(11.5), theme::TEXT_WEAK, inner.width());
        painter.galley(inner.left_bottom() - egui::vec2(0.0, meta.size().y), meta, theme::TEXT_WEAK);
    }
    response.on_hover_text(&work.work.title)
}

/// Column rects of a chapter row: title, pages, status, review progress, last change.
fn chapter_columns(rect: Rect) -> [Rect; 5] {
    let inner = rect.shrink2(egui::vec2(12.0, 0.0));
    let widths = [96.0, 150.0, 104.0, 64.0];
    let mut right = inner.right();
    let mut columns = [Rect::NOTHING; 5];
    for (slot, width) in [4, 3, 2, 1].into_iter().zip(widths) {
        columns[slot] = Rect::from_x_y_ranges((right - width)..=right, inner.y_range());
        right -= width + 12.0;
    }
    columns[0] = Rect::from_x_y_ranges(inner.left()..=right.max(inner.left()), inner.y_range());
    columns
}

fn chapter_header(ui: &mut egui::Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 24.0), Sense::hover());
    let painter = ui.painter();
    let columns = chapter_columns(rect);
    for (column, label) in columns.iter().zip(["화", "페이지", "상태", "검수", "수정"]) {
        painter.text(
            column.left_center(),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(11.5),
            theme::TEXT_FAINT,
        );
    }
    painter.hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, theme::BORDER));
}

fn chapter_row(ui: &mut egui::Ui, chapter: &ChapterSummary) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 44.0), Sense::click_and_drag());
    if ui.is_rect_visible(rect) {
        let painter = ui.painter();
        if response.hovered() {
            painter.rect_filled(rect.shrink2(egui::vec2(0.0, 1.0)), 5.0, theme::ROW_HOVER);
        }
        painter.hline(rect.x_range(), rect.bottom(), Stroke::new(1.0, theme::BORDER));
        let [title, pages, status, review, updated] = chapter_columns(rect);
        let center = rect.center().y;

        let galley = truncated(ui, &chapter.title, theme::semibold_font(13.5), theme::TEXT, title.width());
        painter.galley(egui::pos2(title.left(), center - galley.size().y / 2.0), galley, theme::TEXT);

        let text = |column: Rect, text: String, color: Color32| {
            let galley = truncated(ui, &text, egui::FontId::proportional(12.0), color, column.width());
            painter.galley(egui::pos2(column.left(), center - galley.size().y / 2.0), galley, color);
        };
        text(pages, format!("{}p", chapter.page_count), theme::TEXT_WEAK);

        let (status_color, label) = status_style(chapter.status);
        painter.circle_filled(egui::pos2(status.left() + 3.5, center), 3.5, status_color);
        text(status.with_min_x(status.left() + 13.0), label.into(), theme::TEXT);

        let ratio =
            if chapter.page_count == 0 { 0.0 } else { chapter.completed_page_count as f32 / chapter.page_count as f32 };
        let bar = Rect::from_min_size(egui::pos2(review.left(), center - 2.0), egui::vec2(90.0, 4.0));
        theme::paint_progress(painter, bar, ratio, if ratio >= 1.0 { theme::SUCCESS } else { theme::ACCENT });
        text(
            review.with_min_x(bar.right() + 10.0),
            format!("{}/{}", chapter.completed_page_count, chapter.page_count),
            theme::TEXT_WEAK,
        );

        let date = chapter.updated_at.get(..10).unwrap_or(&chapter.updated_at).to_owned();
        text(updated, date, theme::TEXT_WEAK);
    }
    response.on_hover_text(format!("{} 열기 · 끌어서 순서 변경 · 우클릭으로 이름 변경과 삭제", chapter.title))
}

fn status_style(status: ChapterStatus) -> (Color32, &'static str) {
    let color = match status {
        ChapterStatus::Completed => theme::SUCCESS,
        ChapterStatus::Running => theme::ACCENT,
        ChapterStatus::Partial => theme::WARNING,
        ChapterStatus::Failed => theme::DANGER,
        ChapterStatus::Idle => theme::IDLE,
    };
    (color, status_label(status))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(path: &std::path::Path) {
        image::RgbaImage::new(2, 2).save(path).unwrap();
    }

    #[test]
    fn dropped_paths_pick_the_import_kind() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Loose images → one chapter of those images.
        png(&root.join("b.png"));
        png(&root.join("a.png"));
        let preview = preview_dropped(&[root.join("b.png"), root.join("a.png")]).unwrap();
        assert!(!preview.batch);
        assert_eq!(preview.chapters.len(), 1);
        assert_eq!(preview.chapters[0].pages.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["a.png", "b.png"]);

        // A folder of images → one chapter named after it.
        let single = root.join("1화");
        std::fs::create_dir(&single).unwrap();
        png(&single.join("001.png"));
        let preview = preview_dropped(std::slice::from_ref(&single)).unwrap();
        assert!(!preview.batch);
        assert_eq!(preview.chapters[0].title, "1화");

        // A folder of chapter folders → batch.
        let series = root.join("작품");
        for name in ["2화", "1화"] {
            std::fs::create_dir_all(series.join(name)).unwrap();
            png(&series.join(name).join("001.png"));
        }
        let preview = preview_dropped(std::slice::from_ref(&series)).unwrap();
        assert!(preview.batch);
        assert_eq!(preview.chapters.iter().map(|c| c.title.as_str()).collect::<Vec<_>>(), ["1화", "2화"]);

        // Several folders → one chapter each, in natural order, not a batch.
        let preview = preview_dropped(&[series.join("2화"), series.join("1화")]).unwrap();
        assert!(!preview.batch);
        assert_eq!(preview.chapters.iter().map(|c| c.title.as_str()).collect::<Vec<_>>(), ["1화", "2화"]);
        assert_eq!(preview.suggested_work_title, "작품");

        // Images mixed with folders are refused.
        assert!(preview_dropped(&[root.join("a.png"), single]).is_err());
    }
}
