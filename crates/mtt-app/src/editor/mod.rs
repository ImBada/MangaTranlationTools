//! Chapter editor: state and edits. Undo lives in `history.rs`, saving in `persistence.rs`,
//! inpaint runs in `inpaint.rs` and the PSD and export jobs in `jobs.rs`; rendering of the
//! stage and panels in `stage.rs` / `panels.rs`.

pub mod assets;
pub mod block_list;
mod chrome;
pub mod clipboard;
pub mod color;
pub mod font_picker;
pub mod history;
mod inpaint;
mod inspector;
mod jobs;
pub mod layer_ops;
pub mod notifications;
pub mod overlay;
mod panels;
mod persistence;
pub mod presets;
pub mod report;
pub mod result_tools;
mod stage;
pub mod thumbnails;
pub mod translation;
pub mod widgets;

#[cfg(test)]
mod ui_tests;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;
use mtt_core::font_presets::{apply_block_type_font_preset_to_block, ensure_block_type_font_presets};
use mtt_core::library::resolve_chapter_status;
use mtt_core::model::{
    BBox, Chapter, ChapterStatus, InpaintEngine, InpaintStatus, PageAnalysisStatus, PageRecord, StyleKey,
    TranslationBlock,
};
use mtt_inpaint::lama::Lama;
use mtt_inpaint::text_segmentation::TextSegmentation;
use mtt_render::page::{LayerOpacity, LayerVisibility};

use crate::app::AppEvent;
use crate::services::Services;
use crate::worker::{Worker, catch_panic};
use assets::{LoadedPage, PageAssets, PageLoad};
use history::{BlocksSnapshot, Entry, History, LayerKind, LayerPatch};
use overlay::OverlayCache;

pub use inpaint::{HealStroke, InpaintOutcome, InpaintQueue, InpaintScope, LastInpaint, engine_label};
pub use jobs::EditorJob;
use persistence::{SaveState, WriteGeneration};

const AUTOSAVE_DELAY: Duration = Duration::from_millis(800);
const LAYER_SAVE_DELAY: Duration = Duration::from_millis(1000);
/// Failed layer writes retry after `LAYER_SAVE_DELAY` doubled per failure, up to 2^5.
const LAYER_RETRY_BACKOFF_MAX: u32 = 5;
const TEXT_COMMIT_DELAY: Duration = Duration::from_millis(900);
const PAGE_CACHE_RADIUS: usize = 2;
pub const MASK_BRUSH_MIN: f32 = 4.0;
pub const MASK_BRUSH_MAX: f32 = 96.0;
/// `OVERLAY_FONT_SIZE_MIN_PX` / `OVERLAY_FONT_SIZE_MAX_PX`.
pub const OVERLAY_FONT_SIZE_MIN: f64 = 8.0;
pub const OVERLAY_FONT_SIZE_MAX: f64 = 240.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Output,
    Overlay,
    InpaintResult,
    InpaintMask,
    Image,
}

/// What the editor is being used for; each mode works on its own layer(s) and shows its
/// own tools and inspector. The original image (`5`) is a view toggle on top of a mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Text blocks (layer 2).
    Translate,
    /// Inpaint result / mask (layers 3, 4).
    Inpaint,
    /// Final output: page check-off, export, report (layer 1).
    Review,
}

/// Tabs of the translate-mode inspector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorTab {
    Blocks,
    Presets,
}

impl Layer {
    /// Whether translation blocks can be selected and edited on this layer (layer 1 is
    /// view-only in the TS app).
    pub fn edits_blocks(self) -> bool {
        self == Layer::Overlay
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Select,
    Range,
    Zoom,
    MaskBrush,
    MaskEraser,
    /// Drag over mask islands, release to erase them whole.
    MaskAutoEraser,
    /// Inpaint result touch-up; which one is in `Editor::result_tool`.
    Result,
}

pub const DEFAULT_LAYER_OPACITY: LayerOpacity =
    LayerOpacity { image: 1.0, inpaint: 1.0, inpaint_result: 1.0, inpaint_mask: 0.0, overlay: 1.0 };

/// `LAYER_FOCUS_OPACITY`: opacities applied when a layer becomes active (missing entries
/// keep their current value).
fn focus_opacity(layer: Layer, current: LayerOpacity) -> LayerOpacity {
    let mut next = current;
    let mut set =
        |image: Option<f32>, inpaint: Option<f32>, result: Option<f32>, mask: Option<f32>, overlay: Option<f32>| {
            if let Some(v) = image {
                next.image = v;
            }
            if let Some(v) = inpaint {
                next.inpaint = v;
            }
            if let Some(v) = result {
                next.inpaint_result = v;
            }
            if let Some(v) = mask {
                next.inpaint_mask = v;
            }
            if let Some(v) = overlay {
                next.overlay = v;
            }
        };
    match layer {
        Layer::Output => return DEFAULT_LAYER_OPACITY,
        Layer::Image => set(Some(1.0), Some(0.0), Some(0.0), Some(0.0), Some(0.0)),
        // Inpaint modes show what they work on over a half-faded original, without the
        // translation on top; translate mode shows the page as the reader will see it.
        Layer::InpaintResult => set(Some(0.5), Some(1.0), Some(1.0), Some(0.0), Some(0.0)),
        Layer::InpaintMask => set(Some(0.5), Some(1.0), Some(0.0), Some(1.0), Some(0.0)),
        Layer::Overlay => set(Some(1.0), Some(1.0), Some(1.0), Some(0.0), Some(1.0)),
    }
    next
}

/// Text being edited directly on the canvas (`OverlayBlock` inline editor).
pub struct InlineEdit {
    pub block_id: String,
    pub draft: String,
    pub focus_requested: bool,
    /// Last non-empty text selection (char range) for ⌘+click splitting.
    pub selection: Option<(usize, usize)>,
}

/// State of the preset backup/restore window.
pub struct PresetBackupDialog {
    pub restore: bool,
    pub name: String,
    pub selected: Option<String>,
    pub backups: Option<Result<Vec<mtt_core::preset_backups::PresetBackup>, String>>,
    pub error: Option<String>,
}

impl PresetBackupDialog {
    pub fn backup() -> Self {
        Self { restore: false, name: default_backup_name(), selected: None, backups: None, error: None }
    }

    pub fn restore() -> Self {
        Self { restore: true, ..Self::backup() }
    }
}

/// `buildDefaultBackupName`: "폰트 프리셋 백업 2026. 10. 27. 오후 3:04:05".
fn default_backup_name() -> String {
    format!("폰트 프리셋 백업 {}", report::ko_kr_timestamp(&chrono::Local::now()))
}

pub enum EditorAction {
    Close,
    OpenSettings,
    /// Open another chapter from the context bar's library popover.
    OpenChapter(String),
    Import(crate::library_view::ImportMode),
}

pub struct Editor {
    pub chapter: Chapter,
    pub current: usize,
    pub selection: Vec<String>,
    pub layer: Layer,
    pub visibility: LayerVisibility,
    pub opacity: LayerOpacity,
    pub tool: Tool,
    /// Tool used on the mask layer (`inpaintTool`), restored when returning to it.
    pub mask_tool: Tool,
    /// Whether the result layer has a paint tool selected (`inpaintResultTool !== "select"`).
    pub result_tool_active: bool,
    /// FOCUS MODE: selecting a layer applies `LAYER_FOCUS_OPACITY`.
    pub focus_mode: bool,
    /// "배경 투명도 편집": the overlay slider edits every block's opacity on the page.
    pub overlay_opacity_edit: bool,
    /// Space held: temporary pan plus the final output preview.
    pub final_preview: bool,
    /// Block selected on the overlay layer before switching away (restored on return).
    overlay_selection: Option<String>,
    pub brush_size: f32,
    pub view: stage::ViewState,
    assets: HashMap<String, PageAssets>,
    overlay: OverlayCache,
    history: History,
    /// Last committed state of the current page's blocks (undo baseline).
    checkpoint: Option<(String, BlocksSnapshot)>,
    /// Chapter-wide baseline while a font preset edit is open.
    preset_checkpoint: Option<presets::PresetSnapshot>,
    last_text_edit: Option<Instant>,
    revision: u64,
    dirty_pages: HashSet<String>,
    chapter_dirty: bool,
    last_change: Option<Instant>,
    save: SaveState,
    layer_dirty: HashMap<(String, LayerKind), Instant>,
    layer_saves_in_flight: usize,
    drag: Option<stage::Drag>,
    clipboard: Vec<TranslationBlock>,
    /// Font settings copied with "글꼴 설정 복사"; `⌘V` applies them when present.
    font_clipboard: Option<clipboard::FontStyleCopy>,
    pub status: String,
    pub text_editing: bool,
    /// Ask the properties panel to focus the translation text field (`E`).
    pub focus_text_requested: bool,
    /// Range selection in page pixels (`T` tool).
    pub range: Option<egui::Rect>,
    pub result_tool: result_tools::ResultTool,
    pub result_brush: result_tools::BrushSettings,
    pub inpaint_engine: InpaintEngine,
    pub lama: Option<Lama>,
    /// The text segmentation model for block text masks, when downloaded.
    pub text_segmentation: Option<TextSegmentation>,
    inpaint_running: Option<String>,
    pub translation: Option<translation::TranslationJob>,
    pub find: panels::FindState,
    pub preset_backup_dialog: Option<PresetBackupDialog>,
    /// Preset edited through the style panel when no block is selected (`editingFontPresetId`).
    pub editing_preset_id: Option<String>,
    /// PSD files of the current page (last export / last import).
    pub psd_files: layer_ops::PsdFiles,
    /// Library listing for the context bar popover (read when it opens).
    pub library_cache: Option<Vec<mtt_core::library::WorkSummary>>,
    /// "전체 페이지 출력" progress `(done, total)`.
    pub export_progress: Option<(usize, usize)>,
    /// "이 페이지 출력" or a PSD export rendering on the worker.
    pub page_export_running: bool,
    /// Page whose layers a PSD import is replacing on the worker (layer edits wait).
    psd_import_running: Option<String>,
    /// Pages still waiting in an all-pages inpaint run.
    pub inpaint_queue: Option<InpaintQueue>,
    pub notifications: notifications::Notifications,
    pub block_list_collapsed: bool,
    /// Which widgets float as their own window (`widgets`).
    pub widgets: widgets::Layout,
    pub character_dialog: Option<panels::CharacterOverrideDialog>,
    /// Settings or import dialog open (set by the app each frame).
    pub app_modal_open: bool,
    /// Eyedropper / colour picker sampling state.
    pub eyedropper: Option<color::Eyedropper>,
    /// Last colour the result picker sampled and where (for the loupe).
    pub picked_color: Option<(egui::Pos2, [u8; 3])>,
    /// Page and page-px point where the last mask or result stroke ended (Shift+click
    /// draws a straight line on from it).
    pub last_stroke_end: Option<(String, egui::Pos2)>,
    /// Page and mask of the last smart brush stroke; kept so a failed fill can retry.
    pub heal_stroke: Option<HealStroke>,
    /// A smart brush stroke ended and waits for `tick` to start its fill.
    pub heal_requested: bool,
    pub font_previews: font_picker::FontPreviews,
    /// Anchor of Shift range selection in the block list.
    list_anchor: Option<String>,
    write_generation: WriteGeneration,
    /// Layer writes handed to the background writer and not confirmed yet.
    layers_queued: HashSet<(String, LayerKind)>,
    /// Selection captured before the first change of the pending undo step.
    pending_context: Option<history::HistoryContext>,
    /// Canvas size stepper hold state `(pressed at, last step)`.
    pub size_repeat: Option<(Instant, Instant)>,
    last_inpaint: Option<LastInpaint>,
    last_run_mode: Option<translation::RunMode>,
    /// Consecutive failed writes of layers still in `layer_dirty` (retry backoff; the
    /// failure card's 재시도 resets it).
    layer_failures: HashMap<(String, LayerKind), u32>,
    /// Running result report (`reportProgress`).
    pub report: Option<report::ReportProgress>,
    /// HTML file of the last generated report.
    pub last_report: Option<std::path::PathBuf>,
    pub inline_edit: Option<InlineEdit>,
    pub inspector_tab: InspectorTab,
    /// Inpaint layer the inpaint mode returns to.
    pub inpaint_layer: Layer,
    /// Layer to go back to when the original-image view is turned off.
    source_return: Option<Layer>,
    /// Title of the chapter's work (breadcrumb).
    pub work_title: String,
    pub thumbnails: thumbnails::Thumbnails,
    /// Tab / Shift+Tab presses taken before egui's focus handling (see `MttApp::raw_input_hook`).
    pub pending_block_step: i32,
    /// Tab / Shift+Tab pressed in the 번역 field: applied after the frame's typing, then the
    /// next block's 번역 field takes focus.
    pub pending_text_step: i32,
    /// "다시 번역" on the selected blocks, started next frame (where the services are).
    pub block_translation_requested: bool,
    /// The work's translation notes being edited (`작품 › 화 › 번역 메모`).
    pub work_notes_draft: Option<String>,
    /// The shortcut sheet (`?` or the status bar button).
    pub shortcut_sheet: bool,
}

impl Editor {
    /// Opens a chapter with services-dependent setup (LaMa detection).
    pub fn open(chapter: Chapter, services: &Services) -> Self {
        let mut editor = Self::new(chapter);
        editor.configure_inpaint(services);
        editor.work_title = services
            .library
            .read_work(&editor.chapter.work_id)
            .ok()
            .flatten()
            .map(|work| work.title)
            .unwrap_or_default();
        editor.set_layer(Layer::Overlay);
        editor
    }

    pub fn new(mut chapter: Chapter) -> Self {
        chapter.normalize_page_order();
        let current = chapter
            .last_opened_page_id
            .as_deref()
            .and_then(|id| chapter.pages.iter().position(|page| page.id == id))
            .unwrap_or(0);
        let anti_alias = !chapter.inpaint_aliased;
        let mut editor = Self {
            chapter,
            current,
            selection: Vec::new(),
            layer: Layer::Output,
            visibility: LayerVisibility {
                image: true,
                inpaint: true,
                inpaint_result: true,
                inpaint_mask: true,
                overlay: true,
            },
            opacity: DEFAULT_LAYER_OPACITY,
            tool: Tool::Select,
            mask_tool: Tool::Select,
            result_tool_active: false,
            focus_mode: true,
            overlay_opacity_edit: false,
            final_preview: false,
            overlay_selection: None,
            brush_size: 28.0,
            view: stage::ViewState::default(),
            assets: HashMap::new(),
            overlay: OverlayCache::default(),
            history: History::default(),
            checkpoint: None,
            preset_checkpoint: None,
            last_text_edit: None,
            revision: 0,
            dirty_pages: HashSet::new(),
            chapter_dirty: false,
            last_change: None,
            save: SaveState::default(),
            layer_dirty: HashMap::new(),
            layer_saves_in_flight: 0,
            drag: None,
            clipboard: Vec::new(),
            font_clipboard: None,
            status: String::new(),
            text_editing: false,
            focus_text_requested: false,
            range: None,
            result_tool: result_tools::ResultTool::Brush,
            // Pages inpainted without anti-aliasing get hard-edged touch-ups too.
            result_brush: result_tools::BrushSettings { anti_alias, ..result_tools::BrushSettings::default() },
            inpaint_engine: InpaintEngine::LocalFillFallback,
            lama: None,
            text_segmentation: None,
            inpaint_running: None,
            translation: None,
            find: panels::FindState::default(),
            preset_backup_dialog: None,
            editing_preset_id: None,
            report: None,
            last_report: None,
            inpaint_queue: None,
            notifications: Default::default(),
            size_repeat: None,
            pending_context: None,
            write_generation: WriteGeneration::default(),
            layers_queued: HashSet::new(),
            block_list_collapsed: false,
            widgets: widgets::Layout::default(),
            character_dialog: None,
            app_modal_open: false,
            eyedropper: None,
            picked_color: None,
            last_stroke_end: None,
            heal_stroke: None,
            heal_requested: false,
            font_previews: Default::default(),
            list_anchor: None,
            last_inpaint: None,
            last_run_mode: None,
            layer_failures: HashMap::new(),
            psd_files: Default::default(),
            library_cache: None,
            export_progress: None,
            page_export_running: false,
            psd_import_running: None,
            inline_edit: None,
            inspector_tab: InspectorTab::Blocks,
            inpaint_layer: Layer::InpaintResult,
            source_return: None,
            work_title: String::new(),
            thumbnails: Default::default(),
            pending_block_step: 0,
            pending_text_step: 0,
            block_translation_requested: false,
            work_notes_draft: None,
            shortcut_sheet: false,
        };
        editor.reset_interrupted_jobs();
        editor.reset_checkpoint();
        editor
    }

    /// Quitting during a translation or inpaint leaves its pages saved as running. Nothing
    /// runs yet when a chapter opens, so those pages wait for translation again, and the
    /// inpaint counts as failed (its card offers 다시 실행; the layers are as before the run).
    fn reset_interrupted_jobs(&mut self) {
        let mut changed = false;
        for page in &mut self.chapter.pages {
            if page.analysis_status == PageAnalysisStatus::Running {
                page.analysis_status = PageAnalysisStatus::Idle;
                changed = true;
            }
            if page.inpaint_status == Some(InpaintStatus::Running) {
                page.inpaint_status = Some(InpaintStatus::Failed);
                changed = true;
            }
        }
        let status = resolve_chapter_status(&self.chapter.pages);
        if changed || (self.chapter.status == ChapterStatus::Running && status != ChapterStatus::Running) {
            log::info!("chapter {}: reset pages left running by an interrupted job", self.chapter.id);
            self.chapter.status = status;
            self.mark_chapter_dirty();
        }
    }

    pub fn page(&self) -> Option<&PageRecord> {
        self.chapter.pages.get(self.current)
    }

    fn page_mut(&mut self) -> Option<&mut PageRecord> {
        self.chapter.pages.get_mut(self.current)
    }

    pub fn page_count(&self) -> usize {
        self.chapter.pages.len()
    }

    fn loaded_page(&self) -> Option<&LoadedPage> {
        match self.page().and_then(|page| self.assets.get(&page.id)) {
            Some(PageAssets::Ready(page)) => Some(page),
            _ => None,
        }
    }

    /// Whether the current page finished decoding (or failed to).
    pub fn is_page_decoded(&self) -> bool {
        self.page()
            .is_none_or(|page| matches!(self.assets.get(&page.id), Some(PageAssets::Ready(_) | PageAssets::Failed(_))))
    }

    /// Current page decoded and every block rendered (used by automation).
    pub fn is_ready(&self) -> bool {
        self.loaded_page().is_some() && self.overlay.is_complete(self.revision) && !self.view.fit_requested
    }

    /// Zoom overlay -/+ and ⌘-/⌘+ (`STAGE_ZOOM_STEP`), around the stage centre.
    pub fn zoom_step(&mut self, zoom_in: bool) {
        let factor = if zoom_in { stage::ZOOM_STEP } else { 1.0 / stage::ZOOM_STEP };
        self.zoom_relative(factor);
    }

    /// Zoom relative to the fitted size (automation).
    pub fn zoom_relative(&mut self, factor: f32) {
        let center = self.view.stage_center();
        self.view.zoom_at_public(center, factor);
    }

    // ---- page navigation -------------------------------------------------------------

    pub fn go_to_page(&mut self, index: usize) {
        if index >= self.page_count() || index == self.current {
            return;
        }
        // Leaving the page blurs the inline editor, which commits it (`OverlayBlock` onBlur).
        if self.inline_edit.is_some() {
            self.commit_inline_edit();
        }
        self.commit_checkpoint();
        self.current = index;
        self.selection.clear();
        self.range = None;
        self.drag = None;
        self.view.fit_requested = true;
        let page_id = self.chapter.pages[index].id.clone();
        if self.chapter.last_opened_page_id.as_deref() != Some(page_id.as_str()) {
            self.chapter.last_opened_page_id = Some(page_id);
            self.mark_chapter_dirty();
        }
        self.reset_checkpoint();
    }

    pub fn next_page(&mut self) {
        self.go_to_page((self.current + 1).min(self.page_count().saturating_sub(1)));
    }

    pub fn previous_page(&mut self) {
        self.go_to_page(self.current.saturating_sub(1));
    }

    /// Requests decoding of the current page and its neighbours; drops far-away pages.
    fn request_assets(&mut self, services: &Services, worker: &Worker<AppEvent>) {
        if self.page_count() == 0 {
            self.assets.clear();
            return;
        }
        let wanted: Vec<usize> = (self.current.saturating_sub(PAGE_CACHE_RADIUS)
            ..(self.current + PAGE_CACHE_RADIUS + 1).min(self.page_count()))
            .collect();
        let wanted_ids: HashSet<String> = wanted.iter().map(|index| self.chapter.pages[*index].id.clone()).collect();
        let layer_dirty = &self.layer_dirty;
        let layers_queued = &self.layers_queued;
        let running = self.inpaint_running.as_deref();
        let queued = self.inpaint_queue.as_ref().and_then(|queue| queue.pages.front()).cloned();
        self.assets.retain(|id, _| {
            wanted_ids.contains(id)
                || running == Some(id.as_str())
                || queued.as_deref() == Some(id.as_str())
                || layer_dirty.keys().any(|(page_id, _)| page_id == id)
                || layers_queued.iter().any(|(page_id, _)| page_id == id)
        });
        // Current page first so it decodes before the neighbours.
        let mut order = vec![self.current];
        order.extend(wanted.into_iter().filter(|index| *index != self.current));
        if let Some(index) = queued.and_then(|id| self.chapter.pages.iter().position(|page| page.id == id)) {
            order.push(index);
        }
        let mut header: Option<Arc<Chapter>> = None;
        for index in order {
            if self.assets.contains_key(&self.chapter.pages[index].id) {
                continue;
            }
            let chapter = Arc::clone(header.get_or_insert_with(|| Arc::new(self.chapter_header())));
            let page = &self.chapter.pages[index];
            self.assets.insert(page.id.clone(), PageAssets::Loading);
            let library = services.library.clone();
            let page = page.clone();
            let ctx = worker.ctx().clone();
            worker.spawn(move || {
                let load = catch_panic(|| assets::load_page(&ctx, &library, &chapter, &page)).unwrap_or_else(|error| {
                    PageLoad { chapter_id: chapter.id.clone(), page_id: page.id.clone(), page: Err(error) }
                });
                AppEvent::PageLoaded(Box::new(load))
            });
        }
    }

    /// The chapter without its pages (what path lookups need), without cloning the pages.
    fn chapter_header(&mut self) -> Chapter {
        let pages = std::mem::take(&mut self.chapter.pages);
        let header = self.chapter.clone();
        self.chapter.pages = pages;
        header
    }

    pub fn on_page_loaded(&mut self, load: PageLoad) {
        if load.chapter_id != self.chapter.id || !matches!(self.assets.get(&load.page_id), Some(PageAssets::Loading)) {
            return;
        }
        let state = match load.page {
            Ok(loaded) => {
                let size = loaded.source.dimensions();
                if let Some(page) = self.chapter.pages.iter_mut().find(|page| page.id == load.page_id)
                    && (page.width, page.height) != size
                {
                    log::warn!(
                        "page {} size {}x{} differs from image {size:?}; using image size",
                        page.id,
                        page.width,
                        page.height,
                    );
                    (page.width, page.height) = size;
                    self.revision += 1;
                }
                PageAssets::Ready(loaded)
            }
            Err(error) => {
                log::error!("page load failed: {error}");
                PageAssets::Failed(error)
            }
        };
        self.assets.insert(load.page_id, state);
    }

    // ---- layers ----------------------------------------------------------------------

    /// Layer shortcuts `1`..`5` (1 출력, 2 번역, 3 인페인트 결과, 4 마스크, 5 원본 보기).
    pub fn select_layer_shortcut(&mut self, key: u8) {
        let layer = match key {
            1 => Layer::Output,
            2 => Layer::Overlay,
            3 => Layer::InpaintResult,
            4 => Layer::InpaintMask,
            5 => return self.toggle_source_view(),
            _ => return,
        };
        self.set_layer(layer);
    }

    pub fn mode(&self) -> Mode {
        let layer = match self.layer {
            Layer::Image => self.source_return.unwrap_or(Layer::Overlay),
            layer => layer,
        };
        match layer {
            Layer::Output => Mode::Review,
            Layer::InpaintResult | Layer::InpaintMask => Mode::Inpaint,
            Layer::Overlay | Layer::Image => Mode::Translate,
        }
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.set_layer(match mode {
            Mode::Translate => Layer::Overlay,
            Mode::Inpaint => self.inpaint_layer,
            Mode::Review => Layer::Output,
        });
    }

    /// Original image only (`5`); pressing again returns to the mode's layer.
    pub fn source_view(&self) -> bool {
        self.layer == Layer::Image
    }

    pub fn toggle_source_view(&mut self) {
        if self.layer == Layer::Image {
            let back = self.source_return.take().unwrap_or(Layer::Overlay);
            self.set_layer(back);
        } else {
            self.source_return = Some(self.layer);
            self.set_layer(Layer::Image);
        }
    }

    /// `selectLayerWithBlockSelection` + `selectLayer`: focus opacities, per-layer inpaint
    /// tools, and the overlay selection that is put aside and restored.
    pub fn set_layer(&mut self, layer: Layer) {
        match self.layer {
            Layer::InpaintMask if matches!(self.tool, Tool::MaskBrush | Tool::MaskEraser | Tool::MaskAutoEraser) => {
                self.mask_tool = self.tool
            }
            Layer::Overlay if layer != Layer::Overlay => {
                self.overlay_selection = self.selection.first().cloned();
            }
            _ => {}
        }
        let previous = self.layer;
        self.layer = layer;
        if matches!(layer, Layer::InpaintResult | Layer::InpaintMask) {
            self.inpaint_layer = layer;
        }
        if layer != Layer::Image {
            self.source_return = None;
        }
        if self.focus_mode {
            self.opacity = focus_opacity(layer, self.opacity);
        }
        if !matches!(self.tool, Tool::Range | Tool::Zoom) {
            self.tool = self.layer_tool();
        }
        if layer == Layer::Overlay {
            if previous != Layer::Overlay {
                let restored = self
                    .overlay_selection
                    .take()
                    .filter(|id| self.page().is_some_and(|page| page.blocks.iter().any(|block| &block.id == id)));
                // A block picked in the inspector while the original was shown wins.
                if self.selection.is_empty() {
                    self.selection = restored.map(|id| self.block_or_group_ids(&id)).unwrap_or_default();
                }
            }
        } else if layer == Layer::Image && previous == Layer::Overlay {
            // The translate inspector stays usable over the original view.
        } else {
            self.selection.clear();
        }
    }

    /// The pointer-level tool for the active layer when neither range nor zoom is active.
    fn layer_tool(&self) -> Tool {
        match self.layer {
            Layer::InpaintMask => self.mask_tool,
            Layer::InpaintResult if self.result_tool_active => Tool::Result,
            _ => Tool::Select,
        }
    }

    /// `selectPointerTool` (A).
    pub fn select_pointer_tool(&mut self) {
        self.mask_tool = Tool::Select;
        self.result_tool_active = false;
        self.tool = Tool::Select;
    }

    /// `selectRangeTool` (T).
    pub fn select_range_tool(&mut self) {
        self.mask_tool = Tool::Select;
        self.result_tool_active = false;
        self.tool = Tool::Range;
    }

    /// `selectZoomTool` (Z).
    pub fn select_zoom_tool(&mut self) {
        self.tool = Tool::Zoom;
    }

    /// Esc: leaves the zoom tool, then the range tool (`useWorkspaceShortcuts`).
    pub fn escape_tool(&mut self) -> bool {
        match self.tool {
            Tool::Zoom | Tool::Range => {
                self.tool = self.layer_tool();
                true
            }
            _ => false,
        }
    }

    /// `selectSharedInpaintTool`: brush/eraser apply to both inpaint layers; the auto
    /// eraser only exists on the mask layer.
    pub fn select_shared_inpaint_tool(&mut self, mask_tool: Tool) {
        self.mask_tool = mask_tool;
        match mask_tool {
            Tool::MaskBrush => {
                self.result_tool_active = true;
                self.result_tool = result_tools::ResultTool::Brush;
            }
            Tool::MaskEraser => {
                self.result_tool_active = true;
                self.result_tool = result_tools::ResultTool::Eraser;
            }
            _ => {}
        }
        self.tool = self.layer_tool();
    }

    /// `selectInpaintResultEditTool`: a result-only tool (smart brush, blur, picker, …).
    pub fn select_result_edit_tool(&mut self, tool: result_tools::ResultTool) {
        self.result_tool = tool;
        self.result_tool_active = true;
        self.tool = self.layer_tool();
    }

    /// "마스크 유지" (`/`): result tools leave the inpaint mask alone, or cover what they
    /// change with it again.
    pub fn toggle_keep_mask(&mut self) {
        let keep = !self.result_brush.keep_mask;
        self.result_brush.keep_mask = keep;
        self.status = if keep {
            "마스크 유지: 결과 레이어만 고치고 마스크는 그대로 둡니다."
        } else {
            "마스크 유지 끔: 고친 곳을 마스크로도 덮습니다."
        }
        .into();
    }

    /// `setSelectedBlockOrGroupId`: a grouped block selects its whole group unless it is
    /// already the single selection.
    pub fn select_block_or_group(&mut self, id: &str) {
        if self.selection.len() == 1 && self.selection[0] == id {
            return;
        }
        self.selection = self.block_or_group_ids(id);
    }

    /// Selects the next (`step > 0`) or previous block in output order on the overlay
    /// layer; with nothing selected it starts at the first or last block.
    pub fn select_adjacent_block(&mut self, step: i32) {
        let Some(page) = self.page() else { return };
        let ids: Vec<String> = page.blocks.iter().map(|block| block.id.clone()).collect();
        if ids.is_empty() {
            return;
        }
        let current = self.selection.first().and_then(|id| ids.iter().position(|other| other == id));
        let next = match current {
            Some(index) if step > 0 => (index + 1).min(ids.len() - 1),
            Some(index) => index.saturating_sub(1),
            None if step > 0 => 0,
            None => ids.len() - 1,
        };
        if self.inline_edit.is_some() {
            self.commit_inline_edit();
        }
        self.selection = vec![ids[next].clone()];
    }

    fn block_or_group_ids(&self, id: &str) -> Vec<String> {
        let group = self.page().and_then(|page| mtt_core::groups::group_block_ids(page, id));
        match group {
            // Keep the clicked block first: it is the primary selection.
            Some(mut ids) => {
                ids.retain(|other| other != id);
                ids.insert(0, id.to_owned());
                ids
            }
            None => vec![id.to_owned()],
        }
    }

    /// Layer state used for drawing (`resolveStageLayerPreviewState`): Space shows the final
    /// output; the "배경 투명도 편집" mode forces the overlay to full opacity.
    /// Space held, or the result colour picker sampling (`forceFinalOutputPreviewActive`).
    pub fn final_output_preview(&self) -> bool {
        self.final_preview
            || matches!(
                &self.eyedropper,
                Some(color::Eyedropper::Waiting { target: color::SampleTarget::ResultBrush, .. })
                    | Some(color::Eyedropper::Requested { target: color::SampleTarget::ResultBrush, .. })
            )
    }

    pub fn preview_layers(&self) -> (LayerVisibility, LayerOpacity) {
        if self.final_output_preview() {
            let visibility = LayerVisibility {
                image: true,
                inpaint: true,
                inpaint_result: true,
                inpaint_mask: false,
                overlay: true,
            };
            return (visibility, DEFAULT_LAYER_OPACITY);
        }
        if self.layer == Layer::Image {
            // "원본": only the source image, whatever FOCUS MODE did to the opacities.
            let visibility = LayerVisibility {
                image: true,
                inpaint: false,
                inpaint_result: false,
                inpaint_mask: false,
                overlay: false,
            };
            return (visibility, LayerOpacity { image: 1.0, ..self.opacity });
        }
        let mut opacity = self.opacity;
        if self.overlay_opacity_edit {
            opacity.overlay = 1.0;
        }
        (self.visibility, opacity)
    }

    /// `updateSelectedPageBlockOpacity`: sets every block's opacity on the current page.
    pub fn set_page_block_opacity(&mut self, opacity: f64) {
        if let Some(page) = self.page_mut() {
            for block in &mut page.blocks {
                block.opacity = opacity;
            }
        }
        self.last_text_edit = Some(Instant::now());
        self.touch_page();
    }

    /// `overlayBackgroundOpacity`: the first block's opacity.
    pub fn page_block_opacity(&self) -> f64 {
        self.page().and_then(|page| page.blocks.first()).map_or(1.0, |block| block.opacity)
    }

    // ---- block operations ------------------------------------------------------------

    pub fn select_all(&mut self) {
        self.selection =
            self.page().map(|page| page.blocks.iter().map(|block| block.id.clone()).collect()).unwrap_or_default();
    }

    pub fn delete_selected(&mut self) {
        if self.selection.is_empty() {
            return;
        }
        // Undoing the delete selects the blocks again.
        self.commit_checkpoint();
        self.capture_history_context();
        let selection: HashSet<String> = self.selection.drain(..).collect();
        self.edit_page(|page| {
            page.blocks.retain(|block| !selection.contains(&block.id));
            if let Some(groups) = &mut page.block_groups {
                for group in groups.iter_mut() {
                    group.block_ids.retain(|id| !selection.contains(id));
                }
                groups.retain(|group| group.block_ids.len() >= 2);
            }
        });
    }

    fn insert_blocks(&mut self, blocks: Vec<TranslationBlock>) {
        if blocks.is_empty() {
            return;
        }
        let ids: Vec<String> = blocks.iter().map(|block| block.id.clone()).collect();
        self.edit_page(|page| page.blocks.extend(blocks));
        self.selection = ids;
    }

    pub fn selected_blocks(&self) -> Vec<TranslationBlock> {
        let Some(page) = self.page() else {
            return Vec::new();
        };
        page.blocks.iter().filter(|block| self.selection.contains(&block.id)).cloned().collect()
    }

    /// "빈 블록 생성" (`createEmptyBlock`): a fixed box styled with the preset being edited,
    /// else the default preset; switches to the overlay layer.
    pub fn create_empty_block(&mut self) {
        let Some(page_id) = self.page().map(|page| page.id.clone()) else { return };
        let editing = self.editing_preset_id.as_deref().and_then(|id| {
            ensure_block_type_font_presets(self.chapter.font_presets.as_deref().unwrap_or(&[]))
                .into_iter()
                .find(|preset| preset.id == id)
        });
        let style = match &editing {
            Some(preset) => {
                mtt_core::font_presets::resolve_font_preset(
                    preset,
                    self.chapter.font_size_presets.as_deref().unwrap_or(&[]),
                )
                .style
            }
            None => mtt_core::font_presets::default_text_style(),
        };
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or_default();
        let block = TranslationBlock {
            id: format!("{page_id}-block-manual-{millis}"),
            block_type: mtt_core::model::BlockType::Speech,
            bbox: BBox::new(350.0, 420.0, 300.0, 140.0),
            confidence: 1.0,
            source_direction: mtt_core::model::SourceTextDirection::Vertical,
            render_direction: mtt_core::model::RenderTextDirection::Horizontal,
            font_preset_id: editing.map(|preset| preset.id),
            style,
            text_align: mtt_core::model::TextAlign::Center,
            text_position: Some(mtt_core::model::TextPosition::Center),
            background_color: "#fffdf5".into(),
            opacity: 0.88,
            layout_version: Some(mtt_core::model::LAYOUT_VERSION),
            ..TranslationBlock::default()
        };
        self.visibility.overlay = true;
        self.set_layer(Layer::Overlay);
        self.insert_blocks(vec![block]);
    }

    /// Creates a block covering `rect` (page pixels) styled with the speech preset.
    pub fn create_block(&mut self, rect: egui::Rect) {
        let Some(page) = self.page() else {
            return;
        };
        let (width, height) = (f64::from(page.width.max(1)), f64::from(page.height.max(1)));
        let bbox = mtt_core::geometry::pixels_to_bbox(
            BBox::new(f64::from(rect.min.x), f64::from(rect.min.y), f64::from(rect.width()), f64::from(rect.height())),
            width,
            height,
        );
        let presets = ensure_block_type_font_presets(self.chapter.font_presets.as_deref().unwrap_or(&[]));
        let mut block = TranslationBlock {
            id: uuid::Uuid::new_v4().to_string(),
            block_type: mtt_core::model::BlockType::Speech,
            bbox,
            confidence: 1.0,
            background_color: "#fffdf5".into(),
            opacity: 0.88,
            layout_version: Some(mtt_core::model::LAYOUT_VERSION),
            ..TranslationBlock::default()
        };
        apply_block_type_font_preset_to_block(&mut block, &presets);
        self.insert_blocks(vec![block]);
    }

    /// Delete/Backspace (and Q in one-hand mode): with a range on an inpaint layer, clears
    /// that layer inside it (Photoshop's Delete); otherwise deletes the selected blocks.
    pub fn delete_selection_or_range(&mut self, ctx: &egui::Context) {
        if self.inpaint_busy() && self.layer != Layer::Overlay {
            return;
        }
        if self.range.is_some() && matches!(self.layer, Layer::InpaintMask | Layer::InpaintResult) {
            let changed = if self.layer == Layer::InpaintMask {
                self.fill_range_on_mask(ctx, false)
            } else {
                self.fill_range_on_result(ctx, false)
            };
            if !changed {
                self.status = "선택 범위에서 지울 레이어 내용이 없습니다.".into();
            }
        } else if !self.selection.is_empty() && self.layer.edits_blocks() {
            self.delete_selected();
        }
    }

    /// Fills (`true`) or clears the range selection on the mask layer. Returns whether any
    /// pixel changed.
    pub fn fill_range_on_mask(&mut self, ctx: &egui::Context, fill: bool) -> bool {
        let Some(range) = self.range else { return false };
        let Some(page_id) = self.page().map(|page| page.id.clone()) else { return false };
        let Some(PageAssets::Ready(loaded)) = self.assets.get_mut(&page_id) else { return false };
        let (width, height) = (loaded.width(), loaded.height());
        let x = (range.min.x.floor().max(0.0) as u32).min(width);
        let y = (range.min.y.floor().max(0.0) as u32).min(height);
        let right = (range.max.x.ceil().max(0.0) as u32).min(width);
        let bottom = (range.max.y.ceil().max(0.0) as u32).min(height);
        if right <= x || bottom <= y {
            return false;
        }
        let (w, h) = (right - x, bottom - y);
        let mask = loaded.ensure_mask();
        let before = history::read_rect(mask.as_raw(), width, 1, (x, y, w, h));
        let value = if fill { 255 } else { 0 };
        if before.iter().all(|pixel| *pixel == value) {
            return false;
        }
        let raw: &mut [u8] = mask;
        for py in y..bottom {
            let start = (py * width + x) as usize;
            raw[start..start + w as usize].fill(value);
        }
        let after = history::read_rect(mask.as_raw(), width, 1, (x, y, w, h));
        loaded.refresh_layer_region(ctx, &page_id, x, y, w, h);
        self.history.push(Entry::Layer {
            page_id: page_id.clone(),
            layer: LayerKind::Mask,
            patch: LayerPatch::new((x, y, w, h), &before, &after),
        });
        self.layer_dirty.insert((page_id, LayerKind::Mask), Instant::now());
        self.status = if fill {
            "선택 범위를 인페인트 마스크로 채웠습니다.".into()
        } else {
            "선택 범위의 인페인트 마스크를 지웠습니다.".into()
        };
        true
    }

    /// Paints one mask stroke in page pixels as a single undo step (automation/tests).
    pub fn paint_stroke(&mut self, ctx: &egui::Context, from: (f32, f32), to: (f32, f32), erase: bool) {
        let Some(page_id) = self.page().map(|page| page.id.clone()) else { return };
        let diameter = self.brush_size;
        let Some(PageAssets::Ready(loaded)) = self.assets.get_mut(&page_id) else { return };
        let mask = loaded.ensure_mask();
        let Some(bounds) = assets::stroke_bounds(mask.width(), mask.height(), from, to, diameter) else { return };
        let before = history::read_rect(mask.as_raw(), mask.width(), 1, bounds);
        let Some((x, y, w, h)) = assets::stroke_mask(mask, from, to, diameter, if erase { 0 } else { 255 }) else {
            return;
        };
        debug_assert_eq!((x, y, w, h), bounds);
        let after = history::read_rect(mask.as_raw(), mask.width(), 1, bounds);
        let patch = LayerPatch::new(bounds, &before, &after);
        loaded.refresh_layer_region(ctx, &page_id, x, y, w, h);
        self.history.push(Entry::Layer { page_id: page_id.clone(), layer: LayerKind::Mask, patch });
        self.layer_dirty.insert((page_id, LayerKind::Mask), Instant::now());
    }

    pub fn group_selected(&mut self) {
        let Some(page) = self.page() else { return };
        let id = format!("text-block-group-{}", uuid::Uuid::new_v4());
        let Some(groups) =
            mtt_core::groups::groups_after_grouping(page, &self.selection, &id, &mtt_core::time::now_iso())
        else {
            self.status = "블록을 두 개 이상 선택하세요.".into();
            return;
        };
        self.edit_page(|page| page.block_groups = Some(groups));
    }

    pub fn ungroup_selected(&mut self) {
        let Some(page) = self.page() else { return };
        let Some(groups) = mtt_core::groups::groups_after_ungrouping(page, &self.selection) else { return };
        self.edit_page(|page| page.block_groups = groups);
    }

    /// Updates the drop shadow of the group matching the selection.
    pub fn set_group_drop_shadow(&mut self, enabled: bool, settings: &mtt_core::groups::DropShadowSettings) {
        let Some(group_id) =
            self.page().and_then(|page| mtt_core::groups::selected_group(page, &self.selection)).map(|g| g.id)
        else {
            return;
        };
        if let Some(page) = self.page_mut()
            && let Some(group) = page.block_groups.iter_mut().flatten().find(|g| g.id == group_id)
        {
            mtt_core::groups::set_drop_shadow(group, enabled, settings);
            group.updated_at = mtt_core::time::now_iso();
        }
        self.last_text_edit = Some(Instant::now());
        self.touch_page();
    }

    /// Opens the canvas text editor on a block (double-click or `E`).
    pub fn start_inline_edit(&mut self, block_id: &str) {
        if self.page_edit_locked() {
            return;
        }
        let Some(draft) = self
            .page()
            .and_then(|page| page.blocks.iter().find(|b| b.id == block_id))
            .map(|b| b.translated_text.clone())
        else {
            return;
        };
        if !self.layer.edits_blocks() {
            self.set_layer(Layer::Overlay);
        }
        self.inline_edit =
            Some(InlineEdit { block_id: block_id.to_owned(), draft, focus_requested: true, selection: None });
        self.selection = vec![block_id.to_owned()];
    }

    /// Writes the inline draft into the block as one undo step.
    pub fn commit_inline_edit(&mut self) {
        let Some(edit) = self.inline_edit.take() else { return };
        // The block may be gone (its page deleted while the editor was open).
        let Some(current) = self
            .page()
            .and_then(|page| page.blocks.iter().find(|b| b.id == edit.block_id))
            .map(|b| b.translated_text.clone())
        else {
            return;
        };
        if current == edit.draft {
            return;
        }
        let draft = edit.draft;
        let id = edit.block_id;
        self.edit_page(|page| {
            if let Some(block) = page.blocks.iter_mut().find(|b| b.id == id) {
                block.translated_text = draft.clone();
            }
        });
    }

    pub fn cancel_inline_edit(&mut self) {
        self.inline_edit = None;
    }

    /// Page delete (`removePage`): applies the change, saves the chapter, then removes the
    /// page's files. The next page is selected, else the previous one.
    pub fn delete_page(&mut self, page_id: &str, services: &mut Services) {
        let Some(index) = self.chapter.pages.iter().position(|page| page.id == page_id) else { return };
        self.commit_checkpoint();
        let Some(removed) = mtt_core::library::remove_chapter_page(&mut self.chapter, page_id) else { return };
        self.assets.remove(page_id);
        if let Some(queue) = &mut self.inpaint_queue {
            queue.pages.retain(|id| id != page_id);
        }
        self.layer_dirty.retain(|(id, _), _| id != page_id);
        self.layer_failures.retain(|(id, _), _| id != page_id);
        self.dirty_pages.remove(page_id);
        self.history.forget_page(page_id);
        self.checkpoint = None;
        self.selection.clear();
        self.range = None;
        self.current = index.min(self.page_count().saturating_sub(1));
        if let Some(page) = self.chapter.pages.get(self.current) {
            self.chapter.last_opened_page_id = Some(page.id.clone());
        }
        self.view.fit_requested = true;
        self.revision += 1;
        self.chapter_dirty = true;
        if let Err(error) = self.flush(services) {
            self.status = format!("저장 실패: {error}");
            return;
        }
        let work_id = self.chapter.work_id.clone();
        if let Err(error) = services.library.remove_page_files(&work_id, &self.chapter.id, &removed) {
            log::warn!("page file cleanup failed: {error}");
        }
        self.status = format!("{} 페이지를 삭제했습니다.", removed.name);
    }

    /// Page drag-reorder: moves `source` before/after `target` (`reorderByTarget`).
    pub fn reorder_page(&mut self, source: &str, target: &str, position: mtt_core::library::DropPosition) {
        let current_id = self.page().map(|page| page.id.clone());
        let order: Vec<String> = self.chapter.pages.iter().map(|page| page.id.clone()).collect();
        let next = mtt_core::library::reorder_by_target(&order, source, target, position);
        if next == order {
            return;
        }
        mtt_core::library::reorder_chapter_pages(&mut self.chapter, &next);
        if let Some(index) = current_id.and_then(|id| self.chapter.pages.iter().position(|page| page.id == id)) {
            self.current = index;
        }
        self.revision += 1;
        self.mark_chapter_dirty();
    }

    /// The page list checkbox: toggles without leaving the current page.
    pub fn toggle_page_completed_at(&mut self, index: usize) {
        if index == self.current {
            self.toggle_page_completed();
            return;
        }
        let Some(page) = self.chapter.pages.get_mut(index) else { return };
        let done = !page.is_completed();
        page.progress_completed = Some(done);
        page.updated_at = mtt_core::time::now_iso();
        let page_id = page.id.clone();
        self.dirty_pages.insert(page_id);
        self.chapter.status = resolve_chapter_status(&self.chapter.pages);
        self.mark_chapter_dirty();
    }

    pub fn toggle_page_completed(&mut self) {
        self.edit_page(|page| page.progress_completed = Some(!page.is_completed()));
        // 검수: checking a page off moves on to the next page still to review.
        if self.mode() == Mode::Review
            && self.page().is_some_and(PageRecord::is_completed)
            && let Some(next) = (self.current + 1..self.page_count()).find(|&i| !self.chapter.pages[i].is_completed())
        {
            self.go_to_page(next);
        }
    }

    /// `selectedPageEditLocked`: during a translation run, pages not translated yet are
    /// read-only.
    pub fn page_edit_locked(&self) -> bool {
        let Some(job) = &self.translation else { return false };
        self.page().is_some_and(|page| {
            page.analysis_status != mtt_core::model::PageAnalysisStatus::Completed || job.covers_blocks_of(&page.id)
        })
    }

    /// Canvas "+/-" (`updateSelectedBlockIndividualFontSize`): sets the size on the block
    /// and unlinks it from the preset.
    pub fn set_individual_font_size(&mut self, size: f64) {
        let size = size.clamp(OVERLAY_FONT_SIZE_MIN, OVERLAY_FONT_SIZE_MAX);
        self.edit_selected(|block| {
            block.style.font_size_px = Some(size);
            block.links.set(StyleKey::FontSizePx, Some(false));
        });
    }

    /// Canvas "자동 맞춤 OFF" (`disableSelectedBlockAutoFit`).
    pub fn disable_auto_fit(&mut self) {
        let selection = self.selection.clone();
        self.edit_page(|page| {
            for block in page.blocks.iter_mut().filter(|block| selection.contains(&block.id)) {
                block.style.auto_fit_text = Some(false);
                block.links.set(StyleKey::AutoFitText, Some(false));
            }
        });
    }

    /// A dialog is up (`modalOpen`): the editor behind it takes no keys and the canvas
    /// quick controls stay hidden.
    fn dialog_open(&self) -> bool {
        self.app_modal_open
            || self.shortcut_sheet
            || self.work_notes_draft.is_some()
            || self.find.open
            || self.preset_backup_dialog.is_some()
            || self.character_dialog.is_some()
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        services: &mut Services,
        worker: &Worker<AppEvent>,
    ) -> Option<EditorAction> {
        let pointer_down = ui.input(|i| i.pointer.any_down());
        for (page_id, assets) in &mut self.assets {
            if let PageAssets::Ready(page) = assets {
                page.refresh_texture_filters(ui.ctx(), page_id);
            }
        }
        // Number and colour fields don't set `text_editing`, but typing in one is typing too:
        // its keystrokes make one undo step.
        if ui.ctx().text_edit_focused() {
            self.text_editing = true;
        }
        self.tick(services, worker, pointer_down);
        if std::mem::take(&mut self.block_translation_requested) {
            self.start_block_translation(services, worker);
        }
        if let Some(wait) = self.next_tick_due() {
            ui.ctx().request_repaint_after(wait);
        }
        let ui_state = &services.ui_state;
        self.widgets.load(&ui_state.widgets, &ui_state.inspector_order, ui_state.widget_canvas);
        self.text_editing = false;
        self.font_previews.begin_frame();
        self.eyedropper_frame(&ui.ctx().clone());
        // Tab / Shift+Tab taken by `MttApp::raw_input_hook`, applied before drawing so the
        // new selection shows this frame.
        let step = std::mem::take(&mut self.pending_block_step);
        let dialog_open = self.dialog_open();
        if step != 0 && !dialog_open && self.mode() == Mode::Translate {
            self.select_adjacent_block(step);
        }
        // Space held: temporary pan + final output preview (`temporaryPanActive`), decided
        // before drawing so the canvas switches on the same frame. A focused text field
        // gets the space instead.
        self.final_preview = !dialog_open
            && !ui.ctx().text_edit_focused()
            && ui.input(|input| input.key_down(egui::Key::Space) && !input.modifiers.any())
            // Space while drawing a range moves it instead.
            && !matches!(self.drag, Some(stage::Drag::Range { .. }));
        let action = chrome::top_bar(self, ui, services, worker);
        chrome::status_bar(self, ui);
        chrome::page_strip(self, ui, services, worker);
        inspector::inspector(self, ui, services, worker);
        chrome::canvas_toolbar(self, ui);
        stage::stage(self, ui, services);
        let stage_rect = self.view.stage_rect();
        self.notifications_ui(&ui.ctx().clone(), stage_rect, services, worker);
        stage::inline_editor(self, ui);
        widgets::floating_windows(self, &ui.ctx().clone(), services, worker);
        if self.shortcut_sheet {
            let mut open = true;
            let ctx = ui.ctx().clone();
            let screen = ctx.content_rect();
            // As many columns as fit, so the whole sheet shows without scrolling on a
            // normal window; the footer stays outside the scroll area.
            let (columns, width) = crate::shortcuts::sheet_layout(screen.width());
            let max_height = (screen.height() - 200.0).max(200.0);
            crate::theme::modal(
                &ctx,
                egui::Window::new("단축키")
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .min_width(width)
                    .max_width(width),
                |ui| {
                    egui::ScrollArea::vertical()
                        .max_height(max_height)
                        .show(ui, |ui| crate::shortcuts::sheet(ui, columns));
                    ui.separator();
                    crate::theme::hint(ui, "? 또는 Esc로 닫습니다. 버튼 옆의 작은 글자도 단축키입니다.");
                },
            );
            self.shortcut_sheet = open;
        }
        panels::find_replace_window(self, ui);
        panels::preset_backup_window(self, ui, services);
        panels::character_override_window(self, ui, services);
        // Tab in the 번역 field, after this frame's typing (an IME commit that came with the
        // key) has gone into the block it was typed for.
        let text_step = std::mem::take(&mut self.pending_text_step);
        if text_step != 0 && !dialog_open && self.mode() == Mode::Translate {
            self.select_adjacent_block(text_step);
            self.focus_text_requested = true;
            ui.ctx().request_repaint();
        }
        // `modalOpen`: no editor shortcuts while a dialog is up (or was when the frame
        // began: the Esc that closed it is not the editor's); the shortcut sheet only takes
        // the keys that close it. A focused text field keeps its keys, number and colour
        // fields too.
        let ctx = ui.ctx().clone();
        let text_focused = self.text_editing || ctx.text_edit_focused();
        if self.shortcut_sheet && !self.app_modal_open {
            stage::shortcut_sheet_keys(self, &ctx);
        } else if !text_focused && !dialog_open && !self.dialog_open() {
            stage::shortcuts(self, &ctx, services);
        }
        let ui_state = &mut services.ui_state;
        if self.widgets.store(&mut ui_state.widgets, &mut ui_state.inspector_order, &mut ui_state.widget_canvas) {
            services.save_ui_state();
        }
        action
    }
}

#[cfg(test)]
mod tests;
