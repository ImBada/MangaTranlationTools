//! Library screen: works and chapters with progress, open/rename/delete.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{self, RichText};
use mtt_core::import::{
    ImportPreview, ImportResult, ImportTarget, create_import, preview_folder, preview_images, preview_zip,
    preview_zip_folder,
};
use mtt_core::library::WorkSummary;
use mtt_core::model::ChapterStatus;

use crate::app::AppEvent;
use crate::services::Services;
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
}

pub enum ImportEvent {
    Progress { done: usize, total: usize },
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
    filter: String,
    pending: Option<Pending>,
    import: Option<ImportDialog>,
    pub error: Option<String>,
    /// Last action message (`pushStatus`).
    pub status: Option<String>,
    pending_error: Option<String>,
}

/// `resolveChapterStatusLabel`.
pub fn status_label(status: ChapterStatus) -> &'static str {
    match status {
        ChapterStatus::Idle => "대기",
        ChapterStatus::Running => "진행 중",
        ChapterStatus::Completed => "완료",
        ChapterStatus::Partial => "부분 완료",
        ChapterStatus::Failed => "실패",
    }
}

impl LibraryView {
    pub fn refresh(&mut self, services: &mut Services) {
        match services.library.list() {
            Ok(works) => {
                self.works = works;
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
        let dialog = self.import.as_mut()?;
        match event {
            ImportEvent::Progress { done, total } => {
                dialog.running = Some((done, total));
                None
            }
            ImportEvent::Finished(Ok(result)) => {
                let batch = dialog.preview.batch;
                self.import = None;
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

    fn import_buttons(&mut self, ui: &mut egui::Ui) {
        for (mode, label) in [
            (ImportMode::Images, "이미지 열기"),
            (ImportMode::Folder, "폴더 열기"),
            (ImportMode::Zip, "압축파일 열기"),
            (ImportMode::ZipFolder, "작품 일괄 번역"),
        ] {
            let response = ui.button(label);
            let response = if mode == ImportMode::ZipFolder {
                response.on_hover_text("폴더 안의 ZIP과 하위 폴더를 각각 한 화로 가져와 차례로 번역합니다")
            } else {
                response
            };
            if response.clicked() {
                self.pick_import(mode);
            }
        }
    }

    /// Opens the system picker for `mode` and shows the import dialog for the selection.
    pub fn pick_import(&mut self, mode: ImportMode) {
        let images = ["png", "jpg", "jpeg", "webp", "PNG", "JPG", "JPEG", "WEBP"];
        match mode {
            ImportMode::Images => {
                if let Some(paths) = rfd::FileDialog::new().add_filter("이미지", &images).pick_files() {
                    self.begin_import(Ok(preview_images(&paths)));
                }
            }
            ImportMode::Folder => {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    self.begin_import(preview_folder(&folder).map_err(|e| e.to_string()));
                }
            }
            ImportMode::Zip => {
                if let Some(path) = rfd::FileDialog::new().add_filter("ZIP", &["zip", "ZIP"]).pick_file() {
                    self.begin_import(preview_zip(&path).map_err(|e| e.to_string()));
                }
            }
            ImportMode::ZipFolder => {
                if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                    self.begin_import(preview_zip_folder(&folder).map_err(|e| e.to_string()));
                }
            }
        }
    }

    /// The import dialog; drawn by the app on both screens (the editor imports too).
    pub fn import_dialog(&mut self, ctx: &egui::Context, services: &mut Services, worker: &Worker<AppEvent>) {
        let works: Vec<(String, String)> =
            self.works.iter().map(|w| (w.work.id.clone(), w.work.title.clone())).collect();
        let Some(dialog) = &mut self.import else { return };
        let mut close = false;
        let mut start = false;
        let title = if dialog.preview.batch { "작품 일괄 번역 준비" } else { "보관함에 추가" };
        egui::Window::new(title)
            .collapsible(false)
            .default_width(520.0)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
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
                    ui.separator();
                    ui.label(RichText::new(if batch { "생성할 화" } else { "화 제목" }).strong());
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
                    ui.colored_label(egui::Color32::LIGHT_RED, error);
                }
                ui.horizontal(|ui| {
                    if let Some((done, total)) = dialog.running {
                        ui.add(
                            egui::ProgressBar::new(if total == 0 { 0.0 } else { done as f32 / total as f32 })
                                .desired_width(260.0)
                                .text(format!("{done}/{total}")),
                        );
                        if ui.button(RichText::new("가져오기 중단").color(egui::Color32::LIGHT_RED)).clicked() {
                            dialog.cancel.store(true, Ordering::SeqCst);
                        }
                    } else {
                        if ui.button("취소").clicked() {
                            close = true;
                        }
                        // `isSubmittable`: a title for a new work and at least one titled chapter.
                        let submittable = (!dialog.new_work || !dialog.new_title.trim().is_empty())
                            && (dialog.new_work || dialog.existing_work.is_some())
                            && dialog.preview.chapters.iter().any(|c| c.enabled && !c.title.trim().is_empty());
                        let label = if batch { "생성 후 번역 시작" } else { "보관함에 추가" };
                        if ui.add_enabled(submittable, egui::Button::new(label)).clicked() {
                            start = true;
                        }
                    }
                });
            });
        if start {
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

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        services: &mut Services,
        lama_card: &mut dyn FnMut(&mut egui::Ui, &Services) -> bool,
    ) -> Option<LibraryAction> {
        let mut action = None;
        egui::Panel::top("library-header").show(ui, |ui| {
            ui.horizontal(|ui| {
                let count: usize = self.works.iter().map(|work| work.chapters.len()).sum();
                ui.heading("보관함");
                ui.label(RichText::new(count.to_string()).weak());
                ui.separator();
                ui.label(RichText::new("가져오기").weak().small());
                self.import_buttons(ui);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("설정").clicked() {
                        action = Some(LibraryAction::OpenSettings);
                    }
                    if ui.button("새로고침").clicked() {
                        self.refresh(services);
                    }
                    ui.label(RichText::new(services.paths.library_dir.display().to_string()).weak().small());
                });
            });
        });
        egui::Panel::left("library-tree").resizable(true).default_size(380.0).size_range(260.0..=640.0).show(
            ui,
            |ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.filter).hint_text("작품/화 검색").desired_width(f32::INFINITY),
                );
                ui.add_space(4.0);
                if let Some(next) = self.library_tree(ui) {
                    action = Some(next);
                }
                self.apply_reorder(ui, services);
            },
        );
        egui::CentralPanel::default().show(ui, |ui| {
            if let Some(error) = &self.error {
                ui.colored_label(egui::Color32::LIGHT_RED, error);
            }
            if let Some(status) = &self.status {
                ui.label(status);
            }
            if services.fonts_failed() {
                ui.colored_label(egui::Color32::LIGHT_RED, "시스템 폰트를 불러오지 못했습니다.");
            }
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.set_max_width(560.0);
                // `WorkspaceEmptyState`: the LaMa card until it is ready, then the import prompt.
                if lama_card(ui, services) {
                    return;
                }
                ui.heading("보관함에서 화를 열거나 새로 가져오세요.");
                ui.label(
                    RichText::new("작품과 화 단위로 저장해두고, 이어서 번역하거나 페이지별로 다시 번역할 수 있습니다.")
                        .weak(),
                );
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    for (mode, label) in [
                        (ImportMode::Images, "이미지 열기"),
                        (ImportMode::Folder, "폴더 열기"),
                        (ImportMode::Zip, "압축파일 열기"),
                        (ImportMode::ZipFolder, "작품 일괄 번역"),
                    ] {
                        if ui.button(label).clicked() {
                            self.pick_import(mode);
                        }
                    }
                });
            });
        });
        self.pending_dialog(ui, services);
        action
    }

    /// `LibraryTree`: works with their chapters (`{n}페이지 · {status}`), ✎ rename, and
    /// chapter drag-reorder within a work while no search is active.
    fn library_tree(&mut self, ui: &mut egui::Ui) -> Option<LibraryAction> {
        let mut action = None;
        let filtered = mtt_core::library_filter::filter_library(&self.works, &self.filter);
        let search_active = !mtt_core::library_filter::normalize_search_text(&self.filter).is_empty();
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            if filtered.is_empty() {
                ui.label(
                    RichText::new(if search_active {
                        "검색 결과가 없습니다."
                    } else {
                        "아직 보관함에 저장된 작품이 없습니다."
                    })
                    .weak(),
                );
            }
            for work in &filtered {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(RichText::new(&work.work.title).strong()).truncate());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✏").on_hover_text("이름 변경").clicked() {
                                self.pending = Some(Pending {
                                    work: true,
                                    id: work.work.id.clone(),
                                    original: work.work.title.clone(),
                                    title: work.work.title.clone(),
                                });
                            }
                        });
                    });
                    for chapter in &work.chapters {
                        // Title truncates; "{n}페이지 · {status}" and ✏ stay visible on the right.
                        let row = ui.horizontal(|ui| {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("✏").on_hover_text("이름 변경").clicked() {
                                    self.pending = Some(Pending {
                                        work: false,
                                        id: chapter.id.clone(),
                                        original: chapter.title.clone(),
                                        title: chapter.title.clone(),
                                    });
                                }
                                ui.label(
                                    RichText::new(format!(
                                        "{}페이지 · {}",
                                        chapter.page_count,
                                        status_label(chapter.status)
                                    ))
                                    .small()
                                    .weak(),
                                );
                                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    let title = egui::Button::new(&chapter.title).truncate();
                                    let response = if search_active {
                                        ui.add(title)
                                    } else {
                                        let payload = (work.work.id.clone(), chapter.id.clone());
                                        crate::dnd::drag_button(ui, title, payload, &chapter.title)
                                    };
                                    if response.clicked() {
                                        action = Some(LibraryAction::Open(chapter.id.clone()));
                                    }
                                    response.on_hover_text(&chapter.title);
                                });
                            });
                        });
                        if !search_active
                            && let Some(source) = row.response.dnd_release_payload::<(String, String)>()
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
                });
                ui.add_space(6.0);
            }
        });
        action
    }

    /// Applies a chapter drop recorded by `library_tree` (`reorderByTarget` + `chapterOrder`).
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
        let mut close = false;
        let mut save = false;
        let mut delete = false;
        let heading = if pending.work { "작품 이름 변경" } else { "화 이름 변경" };
        egui::Window::new(heading)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ui.ctx(), |ui| {
                ui.label("새 이름");
                let response = ui.text_edit_singleline(&mut pending.title);
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
                ui.horizontal(|ui| {
                    let delete_label = if pending.work { "작품 삭제" } else { "화 삭제" };
                    if ui.button(RichText::new(delete_label).color(egui::Color32::LIGHT_RED)).clicked() {
                        delete = true;
                    }
                    if ui.button("취소").clicked() {
                        close = true;
                    }
                    if ui.add_enabled(!trimmed.is_empty(), egui::Button::new("저장")).clicked() {
                        save = true;
                    }
                });
            });
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
            if crate::editor::layout::confirm(kind, "정말 삭제하시겠습니까?", &detail) {
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
