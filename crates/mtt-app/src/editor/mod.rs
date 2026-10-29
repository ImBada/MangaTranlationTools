//! Chapter editor: state, edits, history and persistence. Rendering of the stage and
//! panels lives in `stage.rs` / `panels.rs`.

pub mod assets;
pub mod block_list;
mod chrome;
pub mod clipboard;
pub mod color;
pub mod font_picker;
pub mod history;
mod inspector;
pub mod layer_ops;
pub mod notifications;
pub mod overlay;
mod panels;
pub mod presets;
pub mod report;
pub mod result_tools;
mod stage;
pub mod thumbnails;
pub mod translation;
pub mod widgets;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui;
use image::{GrayImage, RgbaImage};
use mtt_core::font_presets::{apply_block_type_font_preset_to_block, ensure_block_type_font_presets};
use mtt_core::library::resolve_chapter_status;
use mtt_core::model::{
    BBox, Chapter, InpaintEngine, InpaintSettings, InpaintStatus, PageRecord, StyleKey, TranslationBlock,
};
use mtt_inpaint::lama::Lama;
use mtt_render::page::{LayerOpacity, LayerVisibility, PageImages, PageRenderOptions};

use crate::app::AppEvent;
use crate::services::Services;
use crate::worker::Worker;
use assets::{LoadedPage, PageAssets, PageLoad};
use history::{BlocksSnapshot, Entry, History, LayerKind, LayerPatch};
use overlay::OverlayCache;

const AUTOSAVE_DELAY: Duration = Duration::from_millis(800);
const LAYER_SAVE_DELAY: Duration = Duration::from_millis(1000);
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

/// Orders the background writer against synchronous flushes: queued writes remember the
/// generation they were queued in and are skipped once a flush has advanced it.
#[derive(Clone, Default)]
pub struct WriteGeneration(std::sync::Arc<std::sync::Mutex<u64>>);

impl WriteGeneration {
    fn current(&self) -> u64 {
        self.0.lock().map(|generation| *generation).unwrap_or_default()
    }

    /// Runs `write` unless a flush happened after `queued_at` (the lock covers the write).
    fn write_if_current(&self, queued_at: u64, write: impl FnOnce() -> Result<(), String>) -> Result<(), String> {
        let guard = self.0.lock().map_err(|_| "저장 잠금 실패".to_owned())?;
        if *guard != queued_at {
            return Ok(());
        }
        let result = write();
        drop(guard);
        result
    }

    /// Bumps the generation and runs `flush` while holding the lock.
    fn advance_while<T>(&self, flush: impl FnOnce() -> T) -> T {
        let mut guard = match self.0.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        *guard += 1;
        flush()
    }
}

pub enum EditorAction {
    Close,
    OpenSettings,
    /// Open another chapter from the context bar's library popover.
    OpenChapter(String),
    Import(crate::library_view::ImportMode),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InpaintScope {
    /// The whole current mask.
    WholeMask,
    /// The mask inside the range selection, merged into the existing result.
    Range,
    /// Text pixels detected inside the selected blocks.
    SelectedBlocks,
    /// "전체 블록 인페인트": a fresh mask from every block's text pixels, replacing the
    /// existing mask and result.
    AllBlocks,
}

/// What the "인페인트 실패" card's 재시도 runs again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LastInpaint {
    Page(InpaintScope),
    AllPages,
}

/// "전체 인페인트" over the chapter (`applyInpaintAllPages`).
pub struct InpaintQueue {
    pub pages: std::collections::VecDeque<String>,
    pub total: usize,
}

/// Builds the imported layers on the worker (see `Editor::psd_import_job`).
trait PsdImportJob: FnOnce(&egui::Context) -> Result<InpaintOutcome, String> + Send + 'static {}
impl<F: FnOnce(&egui::Context) -> Result<InpaintOutcome, String> + Send + 'static> PsdImportJob for F {}

/// One-off jobs the editor runs on the worker.
pub enum EditorJob {
    PageExported(Result<PathBuf, String>),
    PsdExported(Result<PathBuf, String>),
    PsdImported { page_id: String, quick: bool, result: Result<InpaintOutcome, String> },
}

/// A finished inpaint, ready to swap in: the new layers with their textures and the undo
/// patches, all made on the worker.
pub struct InpaintOutcome {
    pub layers: assets::LayerSet,
    pub patches: Vec<(LayerKind, LayerPatch)>,
    pub engine: InpaintEngine,
    pub message: String,
}

struct ComputedInpaint {
    before_mask: Option<Arc<GrayImage>>,
    before_result: Option<Arc<RgbaImage>>,
    mask: GrayImage,
    result: RgbaImage,
    engine: InpaintEngine,
    message: String,
}

impl ComputedInpaint {
    fn finish(self, ctx: &egui::Context, page_id: &str) -> InpaintOutcome {
        let (width, height) = self.mask.dimensions();
        let mut patches = Vec::new();
        let mask_before = self.before_mask.as_deref().map_or_else(|| vec![0; self.mask.len()], |m| m.as_raw().clone());
        if let Some(patch) = history::diff_patch(&mask_before, self.mask.as_raw(), width, height, 1) {
            patches.push((LayerKind::Mask, patch));
        }
        drop(mask_before);
        let result_before =
            self.before_result.as_deref().map_or_else(|| vec![0; self.result.len()], |r| r.as_raw().clone());
        if let Some(patch) = history::diff_patch(&result_before, self.result.as_raw(), width, height, 4) {
            patches.push((LayerKind::Result, patch));
        }
        drop(result_before);
        let layers = assets::LayerSet::build(ctx, page_id, self.mask, self.result);
        InpaintOutcome { layers, patches, engine: self.engine, message: self.message }
    }
}

#[derive(Default)]
struct SaveState {
    seq: u64,
    saved_seq: u64,
    in_flight: bool,
    error: Option<String>,
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
    /// Pages whose layer PNG failed to save (retried from the failure card).
    failed_layer_pages: HashSet<String>,
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
            result_brush: result_tools::BrushSettings::default(),
            inpaint_engine: InpaintEngine::LocalFillFallback,
            lama: None,
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
            font_previews: Default::default(),
            list_anchor: None,
            last_inpaint: None,
            last_run_mode: None,
            failed_layer_pages: HashSet::new(),
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
            shortcut_sheet: false,
        };
        editor.reset_checkpoint();
        editor
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
            worker.spawn(move || AppEvent::PageLoaded(Box::new(assets::load_page(&ctx, &library, &chapter, &page))));
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

    // ---- edits & history -------------------------------------------------------------

    fn reset_checkpoint(&mut self) {
        self.checkpoint = self.page().map(|page| (page.id.clone(), BlocksSnapshot::of(page)));
        self.last_text_edit = None;
    }

    /// Records the change since the last checkpoint as one undo step.
    pub fn commit_checkpoint(&mut self) {
        if self.commit_preset_checkpoint() {
            self.reset_checkpoint();
            self.pending_context = None;
            return;
        }
        let Some((page_id, before)) = self.checkpoint.take() else {
            self.reset_checkpoint();
            return;
        };
        if let Some(page) = self.chapter.pages.iter().find(|page| page.id == page_id) {
            let after = BlocksSnapshot::of(page);
            if after != before {
                self.history.push(Entry::Blocks { page_id: page_id.clone(), before, after });
                // The recorded selection belongs to this step only.
                self.pending_context = None;
            }
        }
        self.reset_checkpoint();
    }

    /// Called after every mutation of the current page.
    fn touch_page(&mut self) {
        self.capture_history_context();
        self.revision += 1;
        if let Some(page) = self.chapter.pages.get_mut(self.current) {
            page.updated_at = mtt_core::time::now_iso();
            self.dirty_pages.insert(page.id.clone());
        }
        self.mark_chapter_dirty();
    }

    fn mark_chapter_dirty(&mut self) {
        self.chapter_dirty = true;
        self.last_change = Some(Instant::now());
    }

    /// Applies `edit` to the current page and commits it as its own undo step.
    pub fn edit_page(&mut self, edit: impl FnOnce(&mut PageRecord)) {
        self.commit_checkpoint();
        if let Some(page) = self.page_mut() {
            edit(page);
        }
        self.touch_page();
        self.commit_checkpoint();
    }

    /// Applies `edit` to every selected block. Continuous edits (sliders, typing) are
    /// merged into one undo step by the checkpoint logic.
    pub fn edit_selected(&mut self, mut edit: impl FnMut(&mut TranslationBlock)) {
        let selection: HashSet<String> = self.selection.iter().cloned().collect();
        if let Some(page) = self.page_mut() {
            for block in page.blocks.iter_mut().filter(|block| selection.contains(&block.id)) {
                edit(block);
            }
        }
        self.last_text_edit = Some(Instant::now());
        self.touch_page();
    }

    /// Edits a style field of the selected blocks; linked fields of assigned blocks edit
    /// the font preset (see `presets.rs`).
    pub fn edit_selected_style(&mut self, _key: StyleKey, edit: impl FnMut(&mut TranslationBlock)) {
        self.edit_style(edit);
    }

    pub fn undo(&mut self, ctx: &egui::Context) {
        self.commit_checkpoint();
        let Some((entry, context)) = self.history.pop_undo() else {
            return;
        };
        if !self.prepare_history_entry(&entry) {
            self.history.push_undone((entry, context));
            return;
        }
        self.apply_history_entry(ctx, &entry, true);
        self.restore_history_context(&context);
        self.reset_checkpoint();
        self.pending_context = None;
        self.history.push_redo((entry, context));
    }

    pub fn redo(&mut self, ctx: &egui::Context) {
        self.commit_checkpoint();
        let Some((entry, context)) = self.history.pop_redo() else {
            return;
        };
        if !self.prepare_history_entry(&entry) {
            self.history.push_redo((entry, context));
            return;
        }
        self.apply_history_entry(ctx, &entry, false);
        self.restore_history_context(&context);
        self.reset_checkpoint();
        self.pending_context = None;
        self.history.push_undone((entry, context));
    }

    /// Selection and editing preset as they were when the step was recorded.
    fn restore_history_context(&mut self, context: &history::HistoryContext) {
        let live: HashSet<String> =
            self.page().map(|page| page.blocks.iter().map(|block| block.id.clone()).collect()).unwrap_or_default();
        if self.layer.edits_blocks() {
            self.selection = context.selection.iter().filter(|id| live.contains(*id)).cloned().collect();
        }
        self.editing_preset_id = context.editing_preset_id.clone();
    }

    fn current_history_context(&self) -> history::HistoryContext {
        history::HistoryContext { selection: self.selection.clone(), editing_preset_id: self.editing_preset_id.clone() }
    }

    #[cfg(test)]
    fn history_can_redo_for_test(&self) -> bool {
        self.history.can_redo()
    }

    #[cfg(test)]
    fn tick_for_test(&mut self) {
        if self.pending_context.is_none() {
            self.history.context = self.current_history_context();
        }
    }

    /// Remembers the selection before the first change of the next undo step
    /// (`recordTranslationUndoSnapshot` is called before the change in the TS app).
    fn capture_history_context(&mut self) {
        if self.pending_context.is_none() {
            let context = self.current_history_context();
            self.history.context = context.clone();
            self.pending_context = Some(context);
        }
    }

    /// Navigates to the entry's page. Returns `false` (entry kept) while layer pixels the
    /// entry needs are still loading.
    fn prepare_history_entry(&mut self, entry: &Entry) -> bool {
        let Some(index) = self.chapter.pages.iter().position(|page| page.id == entry.page_id()) else {
            // Chapter-wide preset steps still apply after the page they were made on is gone.
            return matches!(entry, Entry::Presets { .. });
        };
        if index != self.current {
            self.go_to_page(index);
        }
        if entry_touches_layers(entry) && !matches!(self.assets.get(entry.page_id()), Some(PageAssets::Ready(_))) {
            self.status = "페이지 이미지를 불러오는 중입니다. 잠시 후 다시 시도하세요.".into();
            return false;
        }
        true
    }

    fn apply_history_entry(&mut self, ctx: &egui::Context, entry: &Entry, undo: bool) {
        match entry {
            Entry::Blocks { before, after, .. } => {
                let snapshot = if undo { before } else { after };
                if let Some(page) = self.page_mut() {
                    snapshot.apply(page);
                }
                let live: HashSet<String> = snapshot.blocks.iter().map(|block| block.id.clone()).collect();
                self.selection.retain(|id| live.contains(id));
                self.touch_page();
            }
            Entry::Layer { page_id, layer, patch } => {
                let bytes = if undo { patch.before.unpack() } else { patch.after.unpack() };
                self.write_layer_patch(ctx, page_id, *layer, patch, &bytes);
            }
            Entry::Presets { before, after, .. } => {
                self.apply_preset_snapshot(if undo { before } else { after });
            }
            Entry::Batch { entries, .. } => {
                if undo {
                    for entry in entries.iter().rev() {
                        self.apply_history_entry(ctx, entry, true);
                    }
                } else {
                    for entry in entries {
                        self.apply_history_entry(ctx, entry, false);
                    }
                }
            }
        }
    }

    fn write_layer_patch(
        &mut self,
        ctx: &egui::Context,
        page_id: &str,
        layer: LayerKind,
        patch: &LayerPatch,
        bytes: &[u8],
    ) {
        let Some(PageAssets::Ready(loaded)) = self.assets.get_mut(page_id) else {
            self.status = "레이어가 아직 로드되지 않아 되돌릴 수 없습니다.".into();
            return;
        };
        match layer {
            LayerKind::Mask => {
                let mask = loaded.ensure_mask();
                write_patch_gray(mask, patch, bytes);
            }
            LayerKind::Result => {
                let result = loaded.ensure_result();
                write_patch_rgba(result, patch, bytes);
            }
        }
        loaded.refresh_layer_region(ctx, page_id, patch.x, patch.y, patch.width, patch.height);
        self.layer_dirty.insert((page_id.to_owned(), layer), Instant::now());
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
            ..TranslationBlock::default()
        };
        apply_block_type_font_preset_to_block(&mut block, &presets);
        self.insert_blocks(vec![block]);
    }

    /// Delete/Backspace (and Q in one-hand mode): with the range tool on an inpaint layer,
    /// clears that layer inside the range; otherwise deletes the selected blocks.
    pub fn delete_selection_or_range(&mut self, ctx: &egui::Context) {
        if self.inpaint_busy() && self.layer != Layer::Overlay {
            return;
        }
        if self.range.is_some() && self.tool == Tool::Range && self.layer == Layer::InpaintMask {
            if !self.fill_range_on_mask(ctx, false) {
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

    /// Detects LaMa (the downloaded model or a configured command) and prefers it when available.
    pub fn configure_inpaint(&mut self, services: &Services) {
        self.lama = Lama::detect(&services.paths);
        self.inpaint_engine = if self.lama.is_some() { InpaintEngine::Lama } else { InpaintEngine::LocalFillFallback };
    }

    /// Starts an inpaint run for the current page on a worker thread.
    pub fn start_inpaint(&mut self, scope: InpaintScope, worker: &Worker<AppEvent>) {
        let Some(page_id) = self.page().map(|page| page.id.clone()) else { return };
        self.start_inpaint_on(&page_id, scope, worker);
    }

    fn start_inpaint_on(&mut self, page_id: &str, scope: InpaintScope, worker: &Worker<AppEvent>) {
        if self.inpaint_running.is_some() {
            self.status = "인페인트가 이미 실행 중입니다.".into();
            return;
        }
        let Some(page) = self.chapter.pages.iter().find(|page| page.id == page_id).cloned() else { return };
        let Some(PageAssets::Ready(loaded)) = self.assets.get(&page.id) else {
            self.status = "페이지 이미지를 불러오는 중입니다.".into();
            return;
        };
        // Shared, not copied: the job copies a layer only if it changes it.
        let source = Arc::clone(&loaded.source);
        let mask = loaded.mask.clone();
        let result = loaded.result.clone();
        let has_mask = loaded.mask_has_pixels();
        let range = self.range.and_then(|r| {
            mtt_inpaint::Rect::clamped(r.min.x, r.min.y, r.width(), r.height(), source.width(), source.height())
        });
        let blocks: Vec<TranslationBlock> =
            if scope == InpaintScope::AllBlocks { page.blocks.clone() } else { self.selected_blocks() };
        if matches!(scope, InpaintScope::WholeMask | InpaintScope::Range) && !has_mask {
            self.status = "다시 인페인트할 마스크가 없습니다.".into();
            return;
        }
        if scope == InpaintScope::Range && range.is_none() {
            self.status = "부분 인페인트할 범위를 먼저 선택하세요.".into();
            return;
        }
        if matches!(scope, InpaintScope::SelectedBlocks | InpaintScope::AllBlocks) && blocks.is_empty() {
            return;
        }
        let engine = if self.inpaint_engine == InpaintEngine::Lama && self.lama.is_none() {
            InpaintEngine::LocalFillFallback
        } else {
            self.inpaint_engine
        };
        let lama = self.lama.clone();
        let jpeg = matches!(
            std::path::Path::new(if page.image_path.is_empty() { &page.name } else { &page.image_path })
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.to_ascii_lowercase())
                .as_deref(),
            Some("jpg" | "jpeg")
        );
        let settings = InpaintSettings {
            engine,
            // Artifact cleanup only helps JPEG compression noise (`resolvePageInpaintSettings`).
            artifact_cleanup_px: Some(if jpeg { 8.0 } else { 0.0 }),
            ..page.inpaint_settings.clone().unwrap_or_default()
        };
        let chapter_id = self.chapter.id.clone();
        let page_id = page.id.clone();
        if self.inpaint_queue.is_none() {
            self.last_inpaint = Some(LastInpaint::Page(scope));
        }
        self.inpaint_running = Some(page_id.clone());
        if let Some(record) = self.chapter.pages.iter_mut().find(|record| record.id == page_id) {
            record.inpaint_status = Some(InpaintStatus::Running);
        }
        self.status = format!("인페인트 실행 중… ({})", engine_label(engine));
        let ctx = worker.ctx().clone();
        worker.spawn(move || {
            let outcome =
                compute_inpaint(scope, &source, mask, result, range, &blocks, engine, &settings, lama.as_ref())
                    .map(|computed| computed.finish(&ctx, &page_id));
            AppEvent::InpaintFinished { chapter_id, page_id, outcome }
        });
    }

    pub fn on_inpaint_finished(&mut self, chapter_id: &str, page_id: &str, outcome: Result<InpaintOutcome, String>) {
        if chapter_id != self.chapter.id {
            return;
        }
        self.inpaint_running = None;
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => {
                log::error!("inpaint failed: {error}");
                let name = self.chapter.pages.iter().find(|page| page.id == page_id).map(|page| page.name.clone());
                self.status = match (&self.inpaint_queue, name) {
                    // The all-pages run stops at the first failure.
                    (Some(_), Some(name)) => format!("{name} 인페인트 실패: {error}"),
                    _ => format!("인페인트 실패: {error}"),
                };
                let (title, message) = match self.last_inpaint {
                    Some(LastInpaint::AllPages) => {
                        ("모든 페이지 인페인트 실패", "완료된 페이지는 유지됩니다. 실패한 페이지부터 다시 실행하세요.")
                    }
                    Some(LastInpaint::Page(InpaintScope::Range)) => {
                        ("부분 인페인트 실패", "기존 결과 레이어와 선택 범위는 유지됩니다. 다시 실행하세요.")
                    }
                    Some(LastInpaint::Page(InpaintScope::SelectedBlocks)) => {
                        ("선택 블록 인페인트 실패", "기존 결과 레이어와 마스크는 유지됩니다. 다시 실행하세요.")
                    }
                    _ => ("인페인트 실패", "마스크와 편집 상태는 유지됩니다. 설정을 확인한 뒤 다시 실행하세요."),
                };
                self.notifications.report(notifications::FailureId::InpaintRun, title, message);
                self.inpaint_queue = None;
                if let Some(page) = self.chapter.pages.iter_mut().find(|page| page.id == page_id) {
                    page.inpaint_status = Some(InpaintStatus::Failed);
                }
                return;
            }
        };
        let Some(PageAssets::Ready(loaded)) = self.assets.get_mut(page_id) else {
            self.status = "인페인트 결과를 적용할 페이지가 없습니다.".into();
            if let Some(queue) = &mut self.inpaint_queue {
                queue.pages.retain(|id| id != page_id);
                if queue.pages.is_empty() {
                    self.inpaint_queue = None;
                }
            }
            return;
        };
        // Layer edits are locked while an inpaint runs, so the job's diff against the layers
        // it started from is the change to undo.
        let entries: Vec<Entry> = outcome
            .patches
            .into_iter()
            .map(|(layer, patch)| Entry::Layer { page_id: page_id.to_owned(), layer, patch })
            .collect();
        loaded.install_layers(outcome.layers);
        if !entries.is_empty() {
            self.history.push(Entry::Batch { page_id: page_id.to_owned(), entries });
        }
        let now = Instant::now();
        self.layer_dirty.insert((page_id.to_owned(), LayerKind::Mask), now);
        self.layer_dirty.insert((page_id.to_owned(), LayerKind::Result), now);
        if let Some(page) = self.chapter.pages.iter_mut().find(|page| page.id == page_id) {
            page.inpaint_status = Some(InpaintStatus::Completed);
            let mut settings = page.inpaint_settings.clone().unwrap_or_default();
            settings.engine = outcome.engine;
            page.inpaint_settings = Some(settings);
            page.updated_at = mtt_core::time::now_iso();
        }
        self.mark_chapter_dirty();
        self.status = outcome.message;
        self.notifications.clear(notifications::FailureId::InpaintRun);
        if let Some(queue) = &mut self.inpaint_queue
            && queue.pages.front().map(String::as_str) == Some(page_id)
        {
            queue.pages.pop_front();
            let done = queue.total - queue.pages.len();
            let name = self.chapter.pages.iter().find(|page| page.id == page_id).map(|p| p.name.clone());
            self.status = format!("{done}/{} {} 인페인트 완료", queue.total, name.unwrap_or_default());
            if queue.pages.is_empty() {
                self.status = format!("모든 페이지 인페인트 완료: {}p", queue.total);
                self.inpaint_queue = None;
            }
        }
    }

    /// `applyInpaintAllPages`: queues every unfinished page with blocks; `tick` runs them
    /// one by one once each page's images are loaded.
    pub fn start_inpaint_all_pages(&mut self) {
        if self.inpaint_running.is_some() || self.inpaint_queue.is_some() {
            return;
        }
        let pages: std::collections::VecDeque<String> = self
            .chapter
            .pages
            .iter()
            .filter(|page| !page.blocks.is_empty() && !page.is_completed())
            .map(|page| page.id.clone())
            .collect();
        if pages.is_empty() {
            self.status = "인페인트할 미완료 페이지가 없습니다.".into();
            return;
        }
        self.status = format!("모든 페이지 인페인트 시작: {}p", pages.len());
        self.last_inpaint = Some(LastInpaint::AllPages);
        self.inpaint_queue = Some(InpaintQueue { total: pages.len(), pages });
    }

    /// Starts the next queued page once it is decoded.
    fn advance_inpaint_queue(&mut self, worker: &Worker<AppEvent>) {
        if self.inpaint_running.is_some() {
            return;
        }
        let Some(page_id) = self.inpaint_queue.as_ref().and_then(|queue| queue.pages.front()).cloned() else {
            return;
        };
        if !self.chapter.pages.iter().any(|page| page.id == page_id) {
            // The page was deleted while queued.
            if let Some(queue) = &mut self.inpaint_queue {
                queue.pages.pop_front();
                if queue.pages.is_empty() {
                    self.status = format!("모든 페이지 인페인트 완료: {}p", queue.total);
                    self.inpaint_queue = None;
                }
            }
            return;
        }
        match self.assets.get(&page_id) {
            Some(PageAssets::Ready(_)) => self.start_inpaint_on(&page_id, InpaintScope::AllBlocks, worker),
            Some(PageAssets::Failed(error)) => {
                let name = self.chapter.pages.iter().find(|page| page.id == page_id).map(|p| p.name.clone());
                self.status = format!("{} 인페인트 실패: {error}", name.unwrap_or_default());
                self.inpaint_queue = None;
            }
            _ => {}
        }
    }

    pub fn inpaint_busy(&self) -> bool {
        self.inpaint_running.is_some() || self.inpaint_queue.is_some() || self.psd_import_running.is_some()
    }

    /// Exports source, masked result and the rendered translation as a layered PSD.
    /// PSD bytes of the current page (`exportInpaintPsd`).
    #[cfg(test)]
    pub fn render_psd(&mut self, services: &mut Services) -> Result<Vec<u8>, String> {
        self.psd_export_job(services)?()
    }

    /// Builds the PSD on whatever thread runs the returned job, from shared copies of the
    /// page's layers.
    fn psd_export_job(
        &mut self,
        services: &mut Services,
    ) -> Result<impl FnOnce() -> Result<Vec<u8>, String> + Send + 'static, String> {
        use mtt_inpaint::psd::{
            BACKGROUND_LAYER_NAME, ExportLayer, RESULT_LAYER_NAME, TRANSLATION_LAYER_NAME, write_psd,
        };
        let page = self.page().cloned().ok_or("페이지가 없습니다.")?;
        let fonts = services.fonts().ok_or("폰트를 불러오는 중입니다.")?;
        let loaded = self.loaded_page().ok_or("페이지 이미지가 아직 로드되지 않았습니다.")?;
        let (width, height) = (loaded.width(), loaded.height());
        let (source, mask, result) = (Arc::clone(&loaded.source), loaded.mask.clone(), loaded.result.clone());
        Ok(move || {
            let mut source = (*source).clone();
            source.pixels_mut().for_each(|p| p[3] = 255);
            let mut result = result.as_deref().cloned().unwrap_or_else(|| RgbaImage::new(width, height));
            if let Some(mask) = &mask {
                for (pixel, covered) in result.pixels_mut().zip(mask.iter()) {
                    if *covered == 0 {
                        pixel[3] = 0;
                    }
                }
            }
            let mut composite = source.clone();
            image::imageops::overlay(&mut composite, &result, 0, 0);
            let translation = {
                let mut fonts = fonts.lock().map_err(|_| "폰트 잠금 실패")?;
                let options = PageRenderOptions {
                    visibility: mtt_render::page::LayerVisibility {
                        image: false,
                        inpaint: false,
                        inpaint_result: false,
                        inpaint_mask: false,
                        overlay: true,
                    },
                    ..PageRenderOptions::OUTPUT
                };
                mtt_render::page::render_page(
                    &mut fonts,
                    &page,
                    &PageImages { source: None, inpaint_mask: None, inpaint_result: None },
                    &options,
                )
            };
            let mut layers = vec![
                ExportLayer::new(BACKGROUND_LAYER_NAME, &source, false),
                ExportLayer::new(RESULT_LAYER_NAME, &result, false),
            ];
            if let Some(translation) = &translation {
                layers.push(ExportLayer::new(TRANSLATION_LAYER_NAME, translation, true));
            }
            Ok(write_psd(width, height, &composite, &layers))
        })
    }

    /// Replaces the result and mask with the visible non-background layers of a PSD.
    #[cfg(test)]
    pub fn import_psd(&mut self, ctx: &egui::Context, path: &std::path::Path) -> Result<(), String> {
        let (page_id, job) = self.psd_import_job(path)?;
        let outcome = job(ctx)?;
        self.apply_psd_layers(&page_id, outcome)
    }

    /// Reads the PSD and builds the new layers (and their undo patches and textures) on the
    /// thread that runs the returned job.
    fn psd_import_job(&self, path: &std::path::Path) -> Result<(String, impl PsdImportJob), String> {
        let page_id = self.page().map(|page| page.id.clone()).ok_or("페이지가 없습니다.")?;
        let Some(PageAssets::Ready(loaded)) = self.assets.get(&page_id) else {
            return Err("페이지 이미지가 아직 로드되지 않았습니다.".into());
        };
        let (width, height) = (loaded.width(), loaded.height());
        let (before_mask, before_result) = (loaded.mask.clone(), loaded.result.clone());
        let path = path.to_owned();
        let job_page = page_id.clone();
        Ok((page_id, move |ctx: &egui::Context| {
            let bytes = std::fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
            let document = mtt_inpaint::psd::read_psd(&bytes)?;
            let (result, mask) = mtt_inpaint::psd::import_inpaint_layers(&document, width, height)?;
            let computed = ComputedInpaint {
                before_mask,
                before_result,
                mask,
                result,
                engine: InpaintEngine::LocalFillFallback,
                message: String::new(),
            };
            Ok(computed.finish(ctx, &job_page))
        }))
    }

    fn apply_psd_layers(&mut self, page_id: &str, outcome: InpaintOutcome) -> Result<(), String> {
        let Some(PageAssets::Ready(loaded)) = self.assets.get_mut(page_id) else {
            return Err("PSD를 적용할 페이지가 더 이상 열려 있지 않습니다.".into());
        };
        let entries: Vec<Entry> = outcome
            .patches
            .into_iter()
            .map(|(layer, patch)| Entry::Layer { page_id: page_id.to_owned(), layer, patch })
            .collect();
        loaded.install_layers(outcome.layers);
        if !entries.is_empty() {
            self.history.push(Entry::Batch { page_id: page_id.to_owned(), entries });
        }
        let now = Instant::now();
        self.layer_dirty.insert((page_id.to_owned(), LayerKind::Mask), now);
        self.layer_dirty.insert((page_id.to_owned(), LayerKind::Result), now);
        if let Some(page) = self.chapter.pages.iter_mut().find(|page| page.id == page_id) {
            page.inpaint_status = Some(InpaintStatus::Completed);
        }
        self.mark_chapter_dirty();
        Ok(())
    }

    /// Starts a PSD import of `path` for the current page; `keep_as` is where to keep a copy
    /// (`-last-import.psd`), `quick` picks the status message.
    pub(super) fn start_psd_import(
        &mut self,
        worker: &Worker<AppEvent>,
        path: &std::path::Path,
        keep_as: PathBuf,
        quick: bool,
    ) {
        let (page_id, job) = match self.psd_import_job(path) {
            Ok(job) => job,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        self.psd_import_running = Some(page_id.clone());
        self.status = "PSD를 불러오는 중…".into();
        let chapter_id = self.chapter.id.clone();
        let source = path.to_owned();
        let ctx = worker.ctx().clone();
        worker.spawn(move || {
            let result = job(&ctx).and_then(|outcome| {
                if let Some(parent) = keep_as.parent() {
                    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                }
                if source != keep_as {
                    std::fs::copy(&source, &keep_as).map_err(|error| error.to_string())?;
                }
                Ok(outcome)
            });
            AppEvent::EditorJob { chapter_id, job: EditorJob::PsdImported { page_id, quick, result } }
        });
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
        let current = self
            .page()
            .and_then(|page| page.blocks.iter().find(|b| b.id == edit.block_id))
            .map(|b| b.translated_text.clone());
        if current.as_deref() == Some(edit.draft.as_str()) {
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

    // ---- persistence -----------------------------------------------------------------

    /// Starts due background saves and closes undo steps. Call once per frame.
    /// `pointer_down` keeps a slider or canvas drag in one undo step.
    pub fn tick(&mut self, services: &mut Services, worker: &Worker<AppEvent>, pointer_down: bool) {
        if self.pending_context.is_none() {
            self.history.context = self.current_history_context();
        }
        self.request_assets(services, worker);
        self.advance_inpaint_queue(worker);
        let pointer_busy = self.drag.is_some() || pointer_down;
        let text_idle = self.last_text_edit.is_some_and(|at| at.elapsed() > TEXT_COMMIT_DELAY);
        if (!self.text_editing && !pointer_busy) || (self.text_editing && text_idle) {
            self.commit_checkpoint_if_changed();
        }

        let due_layers: Vec<(String, LayerKind)> = self
            .layer_dirty
            .iter()
            .filter(|(_, at)| at.elapsed() >= LAYER_SAVE_DELAY && !pointer_busy)
            .map(|(key, _)| key.clone())
            .collect();
        for (page_id, layer) in due_layers {
            self.layer_dirty.remove(&(page_id.clone(), layer));
            self.save_layer(services, worker, &page_id, layer);
        }

        if self.chapter_dirty
            && !pointer_busy
            && !self.save.in_flight
            && self.last_change.is_some_and(|at| at.elapsed() >= AUTOSAVE_DELAY)
        {
            self.save_chapter(services, worker);
        }
    }

    /// How long until `tick` has a save or an undo step to close. egui only runs a frame on
    /// input, so without a repaint for this an edit would sit unsaved until the mouse moves.
    /// (A drag's release and a finished save run frames of their own.)
    fn next_tick_due(&self) -> Option<Duration> {
        let left = |at: &Instant, delay: Duration| delay.saturating_sub(at.elapsed());
        let layers = self.layer_dirty.values().map(|at| left(at, LAYER_SAVE_DELAY));
        let chapter = self
            .last_change
            .as_ref()
            .filter(|_| self.chapter_dirty && !self.save.in_flight)
            .map(|at| left(at, AUTOSAVE_DELAY));
        let text = self.last_text_edit.as_ref().filter(|_| self.text_editing).map(|at| left(at, TEXT_COMMIT_DELAY));
        // Just past the deadline: `tick` compares with `>`/`>=`.
        layers.chain(chapter).chain(text).min().map(|wait| wait + Duration::from_millis(10))
    }

    fn commit_checkpoint_if_changed(&mut self) {
        if self.preset_checkpoint.is_some() {
            self.commit_checkpoint();
            return;
        }
        let changed = match (&self.checkpoint, self.page()) {
            (Some((page_id, snapshot)), Some(page)) => page_id != &page.id || !snapshot.matches(page),
            _ => true,
        };
        if changed {
            self.commit_checkpoint();
        }
    }

    fn save_chapter(&mut self, services: &Services, worker: &Worker<AppEvent>) {
        self.chapter.updated_at = mtt_core::time::now_iso();
        self.chapter.status = resolve_chapter_status(&self.chapter.pages);
        self.chapter.page_order = self.chapter.pages.iter().map(|page| page.id.clone()).collect();
        self.save.seq += 1;
        self.save.in_flight = true;
        self.chapter_dirty = false;
        self.dirty_pages.clear();
        let seq = self.save.seq;
        // Only the saved copy gets "..."→"…" and "--"→"—" (`normalizeChapterTranslatedText`).
        let mut chapter = self.chapter.clone();
        mtt_core::text::normalize_chapter_translated_text(&mut chapter);
        let library = services.library.clone();
        let generation = self.write_generation.clone();
        let queued_at = generation.current();
        worker.spawn_serial(move || {
            // A synchronous `flush` since queuing wrote newer data; don't overwrite it.
            let error =
                generation.write_if_current(queued_at, || library.write_chapter(&chapter).map_err(|e| e.to_string()));
            AppEvent::ChapterSaved { chapter_id: chapter.id.clone(), seq, error: error.err() }
        });
    }

    pub fn on_chapter_saved(&mut self, chapter_id: &str, seq: u64, error: Option<String>) {
        if chapter_id != self.chapter.id || seq < self.save.saved_seq {
            return;
        }
        self.save.saved_seq = seq;
        self.save.in_flight = seq < self.save.seq;
        match error {
            Some(error) => {
                log::error!("chapter save failed: {error}");
                self.save.error = Some(error);
                self.notifications.report(
                    notifications::FailureId::ChapterSave,
                    "자동 저장 실패",
                    "변경 내용은 화면에 남아 있습니다. 네트워크 또는 저장 위치를 확인한 뒤 다시 저장하세요.",
                );
                // Retry on the next tick.
                self.mark_chapter_dirty();
            }
            None => {
                self.save.error = None;
                self.notifications.clear(notifications::FailureId::ChapterSave);
                if !self.save.in_flight {
                    self.notifications.flash_saved();
                }
            }
        }
    }

    /// `selectedPageEditLocked`: during a translation run, pages not translated yet are
    /// read-only.
    pub fn page_edit_locked(&self) -> bool {
        self.translation.is_some()
            && self.page().is_some_and(|page| page.analysis_status != mtt_core::model::PageAnalysisStatus::Completed)
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

    /// Unsaved edits (`dirty`): navigation-only changes don't count.
    pub fn has_unsaved_changes(&self) -> bool {
        !self.dirty_pages.is_empty() || !self.layer_dirty.is_empty() || self.save.in_flight
    }

    /// Re-queues layer saves that failed.
    fn retry_layer_saves(&mut self) {
        let now = Instant::now() - LAYER_SAVE_DELAY;
        for page_id in std::mem::take(&mut self.failed_layer_pages) {
            self.layer_dirty.insert((page_id.clone(), LayerKind::Mask), now);
            self.layer_dirty.insert((page_id, LayerKind::Result), now);
        }
    }

    fn save_layer(&mut self, services: &Services, worker: &Worker<AppEvent>, page_id: &str, layer: LayerKind) {
        let Some(PageAssets::Ready(loaded)) = self.assets.get(page_id) else {
            return;
        };
        let Some(page_index) = self.chapter.pages.iter().position(|page| page.id == page_id) else {
            return;
        };
        let Some(payload) = LayerImage::of(loaded, layer) else {
            return;
        };
        let path = services.library.inpaint_layer_path(&self.chapter, page_id, layer.page_layer());
        let path_text = path.to_string_lossy().into_owned();
        let page = &mut self.chapter.pages[page_index];
        let field = match layer {
            LayerKind::Mask => &mut page.inpaint_mask_path,
            LayerKind::Result => &mut page.inpaint_result_path,
        };
        if field.as_deref() != Some(path_text.as_str()) {
            *field = Some(path_text);
            self.mark_chapter_dirty();
        }
        self.layer_saves_in_flight += 1;
        self.layers_queued.insert((page_id.to_owned(), layer));
        let page_id = page_id.to_owned();
        let generation = self.write_generation.clone();
        let queued_at = generation.current();
        worker.spawn_serial(move || {
            let result = payload.encode().and_then(|bytes| {
                generation.write_if_current(queued_at, || {
                    mtt_core::json_io::write_bytes_atomic(&path, &bytes).map_err(|error| error.to_string())
                })
            });
            AppEvent::LayerSaved { page_id, error: result.err() }
        });
    }

    pub fn on_layer_saved(&mut self, page_id: &str, error: Option<String>) {
        self.layer_saves_in_flight = self.layer_saves_in_flight.saturating_sub(1);
        // Kept until every queued write is confirmed; rewriting a saved layer is harmless.
        if self.layer_saves_in_flight == 0 {
            self.layers_queued.clear();
        }
        if let Some(error) = error {
            log::error!("layer save failed for {page_id}: {error}");
            self.save.error = Some(error);
            self.failed_layer_pages.insert(page_id.to_owned());
            self.notifications.report(
                notifications::FailureId::LayerSave,
                "인페인트 레이어 저장 실패",
                "마스크와 결과 레이어는 현재 화면에 남아 있습니다. 다시 저장을 시도하세요.",
            );
        } else if self.layer_saves_in_flight == 0 {
            self.notifications.flash_saved();
        }
    }

    /// Synchronously writes pending changes (used when leaving the editor or quitting).
    /// Background saves still queued are made stale so they can't land on top afterwards.
    pub fn flush(&mut self, services: &mut Services) -> Result<(), String> {
        let generation = self.write_generation.clone();
        generation.advance_while(|| self.flush_now(services))
    }

    fn flush_now(&mut self, services: &mut Services) -> Result<(), String> {
        self.commit_checkpoint();
        // Queued background writes are skipped after this flush, so cover them here too.
        let mut pending: Vec<(String, LayerKind)> = self.layer_dirty.drain().map(|(key, _)| key).collect();
        for key in self.layers_queued.drain() {
            if !pending.contains(&key) {
                pending.push(key);
            }
        }
        for (page_id, layer) in pending {
            let Some(PageAssets::Ready(loaded)) = self.assets.get(&page_id) else {
                continue;
            };
            let Some(payload) = LayerImage::of(loaded, layer) else { continue };
            let path = services.library.inpaint_layer_path(&self.chapter, &page_id, layer.page_layer());
            let bytes = payload.encode()?;
            mtt_core::json_io::write_bytes_atomic(&path, &bytes).map_err(|error| error.to_string())?;
            if let Some(page) = self.chapter.pages.iter_mut().find(|page| page.id == page_id) {
                let text = path.to_string_lossy().into_owned();
                match layer {
                    LayerKind::Mask => page.inpaint_mask_path = Some(text),
                    LayerKind::Result => page.inpaint_result_path = Some(text),
                }
            }
            self.chapter_dirty = true;
        }
        if self.chapter_dirty || self.save.in_flight {
            self.chapter.updated_at = mtt_core::time::now_iso();
            self.chapter.status = resolve_chapter_status(&self.chapter.pages);
            let mut chapter = self.chapter.clone();
            mtt_core::text::normalize_chapter_translated_text(&mut chapter);
            services.library.write_chapter(&chapter).map_err(|error| error.to_string())?;
            self.chapter_dirty = false;
        }
        Ok(())
    }

    // ---- export ----------------------------------------------------------------------

    /// Renders the current page like "출력" and writes it to `renders/` (automation).
    pub fn export_current(&mut self, services: &mut Services) -> Result<std::path::PathBuf, String> {
        self.page_export_job(services)?()
    }

    /// "이 페이지 출력" on the worker.
    pub fn export_current_in_background(&mut self, services: &mut Services, worker: &Worker<AppEvent>) {
        match self.page_export_job(services) {
            Ok(job) => {
                self.page_export_running = true;
                self.status = "이 페이지를 출력하는 중…".into();
                let chapter_id = self.chapter.id.clone();
                worker.spawn(move || AppEvent::EditorJob { chapter_id, job: EditorJob::PageExported(job()) });
            }
            Err(error) => self.status = format!("출력 실패: {error}"),
        }
    }

    /// Renders and writes the page from shared copies of its layers, on any thread. The
    /// editor's fonts are held only while rendering; the canvas skips its update meanwhile.
    fn page_export_job(
        &mut self,
        services: &mut Services,
    ) -> Result<impl FnOnce() -> Result<std::path::PathBuf, String> + Send + 'static, String> {
        let page = self.page().cloned().ok_or("페이지가 없습니다.")?;
        let fonts = services.fonts().ok_or("폰트를 불러오는 중입니다.")?;
        let loaded = self.loaded_page().ok_or("페이지 이미지가 아직 로드되지 않았습니다.")?;
        let (source, mask, result) = (Arc::clone(&loaded.source), loaded.mask.clone(), loaded.result.clone());
        let path = services.library.render_output_path(&self.chapter, &page);
        Ok(move || {
            let images =
                PageImages { source: Some(&source), inpaint_mask: mask.as_deref(), inpaint_result: result.as_deref() };
            let rendered = {
                let mut fonts = fonts.lock().map_err(|_| "폰트 잠금 실패")?;
                mtt_render::page::render_page(&mut fonts, &page, &images, &PageRenderOptions::OUTPUT)
                    .ok_or("렌더링 실패")?
            };
            let bytes = encode_png(&rendered)?;
            mtt_core::json_io::write_bytes_atomic(&path, &bytes).map_err(|error| error.to_string())?;
            Ok(path)
        })
    }

    /// A finished editor job (see `EditorJob`).
    pub fn on_editor_job(&mut self, chapter_id: &str, job: EditorJob, services: &Services) {
        if chapter_id != self.chapter.id {
            return;
        }
        let file_name = |path: &std::path::Path| {
            path.file_name().map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned())
        };
        match job {
            EditorJob::PageExported(result) => {
                self.page_export_running = false;
                self.status = match result {
                    Ok(path) => {
                        format!("이 페이지를 출력했습니다: {} (출력 폴더 열기는 검수 · 출력 모드)", file_name(&path))
                    }
                    Err(error) => format!("출력 실패: {error}"),
                };
            }
            EditorJob::PsdExported(result) => {
                self.page_export_running = false;
                self.status = match result {
                    Ok(path) => format!("인페인트 PSD를 내보냈습니다: {}", file_name(&path)),
                    Err(error) => error,
                };
                self.refresh_psd_files(services, true);
            }
            EditorJob::PsdImported { page_id, quick, result } => {
                self.psd_import_running = None;
                let applied = result.and_then(|outcome| self.apply_psd_layers(&page_id, outcome));
                self.status = match applied {
                    Ok(()) if quick => "마지막으로 내보낸 PSD를 퀵로드했습니다.".into(),
                    Ok(()) => "PSD에서 인페인트 결과와 마스크를 가져왔습니다.".into(),
                    Err(error) => error,
                };
                self.refresh_psd_files(services, true);
                if self.page().is_some_and(|page| page.id == page_id) && self.layer != Layer::InpaintResult {
                    self.set_layer(Layer::InpaintResult);
                }
            }
        }
    }

    /// Renders every page in the background (decoding images there too), reporting
    /// `전체 출력 {i}/{n}` progress.
    pub fn export_all(&mut self, services: &mut Services, worker: &Worker<AppEvent>) -> Result<(), String> {
        let fonts = services.fonts().ok_or("폰트를 불러오는 중입니다.")?;
        // Its own font context: rendering every page must not hold the editor's.
        let mut fonts = fonts.lock().map_err(|_| "폰트 잠금 실패")?.fork();
        self.flush(services)?;
        let library = services.library.clone();
        let chapter = self.chapter.clone();
        let emitter = worker.emitter();
        let total = chapter.pages.len();
        self.export_progress = Some((0, total));
        std::thread::spawn(move || {
            let mut written = 0usize;
            let mut errors = Vec::new();
            for (index, page) in chapter.pages.iter().enumerate() {
                emitter.emit(AppEvent::ExportProgress { current: index, total });
                let load = assets::decode_page(&library, &chapter, page);
                let source = match load.source {
                    Ok(source) => source,
                    Err(error) => {
                        errors.push(error);
                        continue;
                    }
                };
                let images = PageImages {
                    source: Some(&source),
                    inpaint_mask: load.mask.as_ref(),
                    inpaint_result: load.result.as_ref(),
                };
                let rendered = mtt_render::page::render_page(&mut fonts, page, &images, &PageRenderOptions::OUTPUT);
                let Some(rendered) = rendered else {
                    errors.push(format!("{}: 렌더링 실패", page.name));
                    continue;
                };
                let path = library.render_output_path(&chapter, page);
                match encode_png(&rendered)
                    .and_then(|bytes| mtt_core::json_io::write_bytes_atomic(&path, &bytes).map_err(|e| e.to_string()))
                {
                    Ok(()) => written += 1,
                    Err(error) => errors.push(format!("{}: {error}", page.name)),
                }
            }
            emitter.emit(AppEvent::ExportFinished { written, errors });
        });
        Ok(())
    }

    pub fn ui(
        &mut self,
        ui: &mut egui::Ui,
        services: &mut Services,
        worker: &Worker<AppEvent>,
    ) -> Option<EditorAction> {
        let pointer_down = ui.input(|i| i.pointer.any_down());
        self.tick(services, worker, pointer_down);
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
        let dialog_open = self.app_modal_open
            || self.find.open
            || self.preset_backup_dialog.is_some()
            || self.character_dialog.is_some();
        if step != 0 && !dialog_open && self.mode() == Mode::Translate {
            self.select_adjacent_block(step);
        }
        // Space held: temporary pan + final output preview (`temporaryPanActive`), decided
        // before drawing so the canvas switches on the same frame. A focused text field
        // gets the space instead.
        self.final_preview = !dialog_open
            && !ui.ctx().text_edit_focused()
            && ui.input(|input| input.key_down(egui::Key::Space) && !input.modifiers.any());
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
        // `modalOpen`: no editor shortcuts while a dialog is up.
        let modal_open = self.app_modal_open
            || self.find.open
            || self.preset_backup_dialog.is_some()
            || self.character_dialog.is_some();
        if !self.text_editing && !modal_open {
            stage::shortcuts(self, &ui.ctx().clone(), services);
        }
        let ui_state = &mut services.ui_state;
        if self.widgets.store(&mut ui_state.widgets, &mut ui_state.inspector_order, &mut ui_state.widget_canvas) {
            services.save_ui_state();
        }
        action
    }
}

pub fn engine_label(engine: InpaintEngine) -> &'static str {
    match engine {
        InpaintEngine::Lama => "LaMa",
        InpaintEngine::MaskFillFallback => "마스크 채우기",
        _ => "로컬 채우기",
    }
}

/// Worker-side inpaint for one scope; returns the new full mask and result layers.
#[allow(clippy::too_many_arguments)]
fn compute_inpaint(
    scope: InpaintScope,
    source: &RgbaImage,
    before_mask: Option<Arc<GrayImage>>,
    before_result: Option<Arc<RgbaImage>>,
    range: Option<mtt_inpaint::Rect>,
    blocks: &[TranslationBlock],
    engine: InpaintEngine,
    settings: &InpaintSettings,
    lama: Option<&Lama>,
) -> Result<ComputedInpaint, String> {
    let started = Instant::now();
    let (width, height) = source.dimensions();
    let (mask, result) = (before_mask.as_deref(), before_result.as_deref());
    let has_layers = mask.is_some_and(|m| m.iter().any(|value| *value > 0)) || result.is_some();
    let patch_mask = match scope {
        InpaintScope::WholeMask => {
            let mask = mask.ok_or("다시 인페인트할 마스크가 없습니다.")?;
            if !mask.iter().any(|value| *value > 0) {
                return Err("다시 인페인트할 마스크가 없습니다.".into());
            }
            mask.clone()
        }
        InpaintScope::Range => {
            let base = mask.ok_or("다시 인페인트할 마스크가 없습니다.")?;
            mtt_inpaint::mask_for_selection(base, range.ok_or("부분 인페인트할 범위를 먼저 선택하세요.")?)
                .ok_or("선택 범위 안에 마스크 픽셀이 없습니다.")?
        }
        InpaintScope::SelectedBlocks | InpaintScope::AllBlocks => {
            let refs: Vec<&TranslationBlock> = blocks.iter().collect();
            let patch = mtt_inpaint::text_mask::mask_for_blocks(source, &refs, None);
            if !patch.pixels().any(|p| p[0] > 0) {
                return Err("블록에서 지울 글자를 찾지 못했습니다.".into());
            }
            patch
        }
    };
    let mut patch_mask = patch_mask;
    let (mut patch_result, region) = mtt_inpaint::run_inpaint_in(source, &patch_mask, engine, settings, lama)?;
    // Outside the engine's region the patch is empty and the masks are binary, so the
    // follow-up steps only look inside it.
    let region = region.unwrap_or(mtt_inpaint::Rect { x: 0, y: 0, width: 0, height: 0 });
    mtt_inpaint::normalize_layers_in(&mut patch_mask, &mut patch_result, region);
    let whole = matches!(scope, InpaintScope::WholeMask | InpaintScope::AllBlocks) || !has_layers;
    let (mask, result) = if whole {
        (patch_mask, patch_result)
    } else {
        let merged_result = mtt_inpaint::merge_partial_result_in(result, &patch_result, &patch_mask, region);
        let mut merged_mask = mask.cloned().unwrap_or_else(|| GrayImage::new(width, height));
        mtt_inpaint::merge_masks_in(&mut merged_mask, &patch_mask, region);
        (merged_mask, merged_result)
    };
    let local = engine == InpaintEngine::LocalFillFallback;
    let multi = blocks.len() > 1;
    // `useInpaintRunActions` status messages.
    let message = match (scope, local) {
        (InpaintScope::WholeMask, false) => "현재 마스크 기준으로 인페인트 결과를 다시 저장했습니다.",
        (InpaintScope::AllBlocks, false) => "전체 블록 인페인트 결과를 저장했습니다.",
        (InpaintScope::WholeMask | InpaintScope::AllBlocks, true) => "로컬 인페인트 결과를 저장했습니다.",
        (InpaintScope::Range, false) => "선택 범위만 다시 인페인트했습니다.",
        (InpaintScope::Range, true) => "선택 범위 로컬 인페인트 결과를 저장했습니다.",
        (InpaintScope::SelectedBlocks, false) if multi => "선택된 블록만 인페인트했습니다.",
        (InpaintScope::SelectedBlocks, false) => "선택 블록만 인페인트했습니다.",
        (InpaintScope::SelectedBlocks, true) if multi => "선택된 블록 로컬 인페인트 결과를 저장했습니다.",
        (InpaintScope::SelectedBlocks, true) => "선택 블록 로컬 인페인트 결과를 저장했습니다.",
    };
    log::info!("inpaint {scope:?} done with {} in {:.1}s", engine_label(engine), started.elapsed().as_secs_f32());
    Ok(ComputedInpaint { before_mask, before_result, mask, result, engine, message: message.into() })
}

fn entry_touches_layers(entry: &Entry) -> bool {
    match entry {
        Entry::Blocks { .. } | Entry::Presets { .. } => false,
        Entry::Layer { .. } => true,
        Entry::Batch { entries, .. } => entries.iter().any(entry_touches_layers),
    }
}

/// Copy of `block` with a new id, shifted by `(dx, dy)` page pixels in normalized space.
fn encode_png(image: &RgbaImage) -> Result<Vec<u8>, String> {
    encode_png_raw(image.as_raw(), image.dimensions(), image::ExtendedColorType::Rgba8, false)
}

/// PNG from raw pixels (no copy into a `DynamicImage`). `fast` trades file size for speed:
/// layer autosaves run after every stroke.
fn encode_png_raw(
    data: &[u8],
    (width, height): (u32, u32),
    color: image::ExtendedColorType,
    fast: bool,
) -> Result<Vec<u8>, String> {
    use image::ImageEncoder;
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    let mut bytes = Vec::new();
    let encoder = if fast {
        PngEncoder::new_with_quality(&mut bytes, CompressionType::Fast, FilterType::Adaptive)
    } else {
        PngEncoder::new(&mut bytes)
    };
    encoder.write_image(data, width, height, color).map_err(|error| error.to_string())?;
    Ok(bytes)
}

/// A layer handed to the writer thread: shared with the editor, not copied.
enum LayerImage {
    Mask(Arc<GrayImage>),
    Result(Arc<RgbaImage>),
}

impl LayerImage {
    fn of(loaded: &LoadedPage, layer: LayerKind) -> Option<Self> {
        match layer {
            LayerKind::Mask => loaded.mask.clone().map(Self::Mask),
            LayerKind::Result => loaded.result.clone().map(Self::Result),
        }
    }

    /// Masks are stored as grayscale (both readers also take the older black/white RGBA).
    fn encode(&self) -> Result<Vec<u8>, String> {
        match self {
            Self::Mask(mask) => encode_png_raw(mask.as_raw(), mask.dimensions(), image::ExtendedColorType::L8, true),
            Self::Result(result) => {
                encode_png_raw(result.as_raw(), result.dimensions(), image::ExtendedColorType::Rgba8, true)
            }
        }
    }
}

fn write_patch_gray(image: &mut GrayImage, patch: &LayerPatch, bytes: &[u8]) {
    for row in 0..patch.height {
        for col in 0..patch.width {
            let (x, y) = (patch.x + col, patch.y + row);
            if x < image.width() && y < image.height() {
                image.put_pixel(x, y, image::Luma([bytes[(row * patch.width + col) as usize]]));
            }
        }
    }
}

fn write_patch_rgba(image: &mut RgbaImage, patch: &LayerPatch, bytes: &[u8]) {
    for row in 0..patch.height {
        for col in 0..patch.width {
            let (x, y) = (patch.x + col, patch.y + row);
            if x < image.width() && y < image.height() {
                let offset = ((row * patch.width + col) * 4) as usize;
                image.put_pixel(
                    x,
                    y,
                    image::Rgba([bytes[offset], bytes[offset + 1], bytes[offset + 2], bytes[offset + 3]]),
                );
            }
        }
    }
}

/// Copies a rect of a gray image into raw bytes.
#[cfg(test)]
mod tests {
    use super::*;

    fn chapter_with_pages(count: usize) -> Chapter {
        let pages = (0..count)
            .map(|index| PageRecord {
                id: format!("p{index}"),
                width: 1000,
                height: 1000,
                blocks: vec![TranslationBlock {
                    id: format!("b{index}"),
                    bbox: BBox::new(100.0, 100.0, 200.0, 100.0),
                    ..TranslationBlock::default()
                }],
                ..PageRecord::default()
            })
            .collect::<Vec<_>>();
        Chapter {
            id: "c".into(),
            work_id: "w".into(),
            page_order: pages.iter().map(|p| p.id.clone()).collect(),
            pages,
            ..Chapter::default()
        }
    }

    #[test]
    fn edits_are_undoable_per_commit() {
        let ctx = egui::Context::default();
        let mut editor = Editor::new(chapter_with_pages(1));
        editor.selection = vec!["b0".into()];
        editor.edit_selected(|block| block.translated_text = "안".into());
        editor.edit_selected(|block| block.translated_text = "안녕".into());
        editor.commit_checkpoint();
        editor.delete_selected();
        assert!(editor.page().unwrap().blocks.is_empty());

        editor.undo(&ctx);
        assert_eq!(editor.page().unwrap().blocks[0].translated_text, "안녕");
        editor.undo(&ctx);
        assert_eq!(editor.page().unwrap().blocks[0].translated_text, "", "typing merged into one step");
        editor.redo(&ctx);
        assert_eq!(editor.page().unwrap().blocks[0].translated_text, "안녕");
        assert!(editor.chapter_dirty);
    }

    #[test]
    fn continuous_edits_while_pointer_down_are_one_step() {
        let ctx = egui::Context::default();
        let mut editor = Editor::new(chapter_with_pages(1));
        editor.selection = vec!["b0".into()];
        let paths = mtt_core::paths::AppPaths::from_data_root(std::env::temp_dir().join("mtt-tick-test"));
        let mut services = Services::new(&ctx, paths);
        let worker = Worker::new(&ctx, 1);
        for size in [10.0, 12.0, 14.0] {
            editor.edit_selected_style(StyleKey::FontSizePx, |block| block.style.font_size_px = Some(size));
            editor.tick(&mut services, &worker, true);
        }
        editor.tick(&mut services, &worker, false);
        editor.undo(&ctx);
        assert_eq!(editor.page().unwrap().blocks[0].style.font_size_px, None, "one undo reverts the whole drag");
    }

    #[test]
    fn undo_switches_to_the_edited_page() {
        let ctx = egui::Context::default();
        let mut editor = Editor::new(chapter_with_pages(3));
        editor.go_to_page(2);
        editor.toggle_page_completed();
        editor.go_to_page(0);
        editor.undo(&ctx);
        assert_eq!(editor.current, 2);
        assert!(!editor.page().unwrap().is_completed());
    }

    #[test]
    fn style_edits_without_preset_change_only_the_block() {
        let mut editor = Editor::new(chapter_with_pages(1));
        editor.selection = vec!["b0".into()];
        editor.edit_selected_style(StyleKey::TextColor, |block| block.style.text_color = Some("#ff0000".into()));
        let block = &editor.page().unwrap().blocks[0];
        assert_eq!(block.style.text_color.as_deref(), Some("#ff0000"));
        assert_eq!(block.links.text_color_linked_to_preset, None);
        assert!(editor.chapter.font_presets.is_none(), "no preset created");
    }

    #[test]
    fn duplicate_and_paste_create_new_ids() {
        let mut editor = Editor::new(chapter_with_pages(1));
        editor.selection = vec!["b0".into()];
        editor.duplicate_selected();
        assert_eq!(editor.page().unwrap().blocks.len(), 2);
        assert_ne!(editor.selection[0], "b0");
        editor.copy_selected(&egui::Context::default());
        editor.paste(None);
        let ids: HashSet<&str> = editor.page().unwrap().blocks.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn range_inpaint_merges_into_existing_layers() {
        let source = RgbaImage::from_fn(60, 60, |x, _| {
            if x < 30 { image::Rgba([200, 0, 0, 255]) } else { image::Rgba([0, 0, 200, 255]) }
        });
        let mut mask = GrayImage::new(60, 60);
        for (x, y) in [(10, 10), (50, 50)] {
            mask.put_pixel(x, y, image::Luma([255]));
        }
        let mut existing = RgbaImage::new(60, 60);
        existing.put_pixel(50, 50, image::Rgba([1, 2, 3, 255]));
        let settings = InpaintSettings { artifact_cleanup_px: Some(0.0), ..InpaintSettings::default() };
        let outcome = compute_inpaint(
            InpaintScope::Range,
            &source,
            Some(Arc::new(mask)),
            Some(Arc::new(existing)),
            Some(mtt_inpaint::Rect { x: 0, y: 0, width: 30, height: 30 }),
            &[],
            InpaintEngine::LocalFillFallback,
            &settings,
            None,
        )
        .unwrap();
        assert_eq!(outcome.result.get_pixel(10, 10)[3], 255, "range pixel inpainted");
        assert_eq!(outcome.result.get_pixel(50, 50).0, [1, 2, 3, 255], "outside range kept");
        assert_eq!(outcome.mask.get_pixel(50, 50)[0], 255, "existing mask kept");

        let empty_range = compute_inpaint(
            InpaintScope::Range,
            &source,
            Some(Arc::new(GrayImage::new(60, 60))),
            None,
            Some(mtt_inpaint::Rect { x: 0, y: 0, width: 10, height: 10 }),
            &[],
            InpaintEngine::LocalFillFallback,
            &settings,
            None,
        );
        assert!(empty_range.is_err());
    }

    #[test]
    fn inpaint_result_is_one_undo_step() {
        let ctx = egui::Context::default();
        let mut editor = Editor::new(chapter_with_pages(1));
        let page_id = editor.page().unwrap().id.clone();
        let source = RgbaImage::from_pixel(10, 10, image::Rgba([255, 255, 255, 255]));
        editor
            .assets
            .insert(page_id.clone(), PageAssets::Ready(Box::new(LoadedPage::new(&ctx, &page_id, source, None, None))));
        editor.inpaint_running = Some(page_id.clone());
        let mut mask = GrayImage::new(10, 10);
        mask.put_pixel(3, 3, image::Luma([255]));
        let mut result = RgbaImage::new(10, 10);
        result.put_pixel(3, 3, image::Rgba([9, 9, 9, 255]));
        let computed = ComputedInpaint {
            before_mask: None,
            before_result: None,
            mask,
            result,
            engine: InpaintEngine::LocalFillFallback,
            message: "ok".into(),
        };
        editor.on_inpaint_finished("c", &page_id, Ok(computed.finish(&ctx, &page_id)));
        assert!(!editor.inpaint_busy());
        assert_eq!(editor.page().unwrap().inpaint_status, Some(InpaintStatus::Completed));
        editor.undo(&ctx);
        let Some(PageAssets::Ready(loaded)) = editor.assets.get(&page_id) else { panic!() };
        assert_eq!(loaded.mask.as_ref().unwrap().get_pixel(3, 3)[0], 0);
        assert_eq!(loaded.result.as_ref().unwrap().get_pixel(3, 3)[3], 0);
    }

    #[test]
    fn psd_export_import_round_trip() {
        let ctx = egui::Context::default();
        let dir = tempfile::tempdir().unwrap();
        let mut services = Services::new(&ctx, mtt_core::paths::AppPaths::from_data_root(dir.path()));
        let started = Instant::now();
        while services.fonts().is_none() && started.elapsed() < Duration::from_secs(20) {
            std::thread::sleep(Duration::from_millis(20));
        }
        let mut editor = Editor::new(chapter_with_pages(1));
        let page_id = editor.page().unwrap().id.clone();
        editor.page_mut().unwrap().width = 8;
        editor.page_mut().unwrap().height = 6;
        let source = RgbaImage::from_pixel(8, 6, image::Rgba([10, 20, 30, 255]));
        let mut mask = GrayImage::new(8, 6);
        mask.put_pixel(2, 2, image::Luma([255]));
        let mut result = RgbaImage::new(8, 6);
        result.put_pixel(2, 2, image::Rgba([200, 150, 100, 255]));
        result.put_pixel(5, 5, image::Rgba([1, 1, 1, 255])); // hidden by the mask
        editor.assets.insert(
            page_id.clone(),
            PageAssets::Ready(Box::new(LoadedPage::new(&ctx, &page_id, source, Some(mask), Some(result)))),
        );

        let path = dir.path().join("page.psd");
        let bytes = editor.render_psd(&mut services).unwrap();
        std::fs::write(&path, bytes).unwrap();
        if let Some(PageAssets::Ready(loaded)) = editor.assets.get_mut(&page_id) {
            loaded.mask = None;
            loaded.result = None;
        }
        editor.import_psd(&ctx, &path).unwrap();
        let Some(PageAssets::Ready(loaded)) = editor.assets.get(&page_id) else { panic!() };
        assert_eq!(loaded.result.as_ref().unwrap().get_pixel(2, 2).0, [200, 150, 100, 255]);
        assert_eq!(loaded.result.as_ref().unwrap().get_pixel(5, 5)[3], 0, "masked-out pixel not exported");
        assert_eq!(loaded.mask.as_ref().unwrap().get_pixel(2, 2)[0], 255);
        editor.undo(&ctx);
        let Some(PageAssets::Ready(loaded)) = editor.assets.get(&page_id) else { panic!() };
        assert_eq!(loaded.result.as_ref().unwrap().get_pixel(2, 2)[3], 0, "undo restores the cleared state");
    }

    #[test]
    fn group_and_ungroup_are_undoable() {
        let ctx = egui::Context::default();
        let mut editor = Editor::new(chapter_with_pages(1));
        editor.selection = vec!["b0".into()];
        editor.duplicate_selected();
        editor.select_all();
        editor.group_selected();
        assert_eq!(editor.page().unwrap().block_groups.as_ref().unwrap().len(), 1);
        editor.set_group_drop_shadow(true, &mtt_core::groups::DropShadowSettings::default());
        editor.commit_checkpoint();
        let group = editor.page().unwrap().block_groups.as_ref().unwrap()[0].clone();
        assert!(mtt_core::groups::drop_shadow_effect(&group).unwrap().enabled);
        editor.ungroup_selected();
        assert!(editor.page().unwrap().block_groups.is_none());
        editor.undo(&ctx);
        assert_eq!(editor.page().unwrap().block_groups.as_ref().unwrap().len(), 1);
    }

    #[test]
    fn paste_applies_copied_font_settings_or_pastes_blocks() {
        let mut editor = Editor::new(chapter_with_pages(1));
        editor.selection = vec!["b0".into()];
        editor.edit_style(|block| block.style.text_color = Some("#00ff00".into()));
        editor.copy_font_settings(&egui::Context::default());
        editor.duplicate_selected();
        let copy_id = editor.selection[0].clone();
        editor.edit_style(|block| block.style.text_color = Some("#0000ff".into()));
        editor.paste(None);
        let copy = editor.page().unwrap().blocks.iter().find(|b| b.id == copy_id).unwrap();
        assert_eq!(copy.style.text_color.as_deref(), Some("#00ff00"), "font settings applied");
        assert_eq!(editor.page().unwrap().blocks.len(), 2, "no block pasted");

        editor.copy_selected(&egui::Context::default());
        editor.paste(None);
        assert_eq!(editor.page().unwrap().blocks.len(), 3, "block copy clears the font clipboard");
    }

    #[test]
    fn inline_edit_commit_and_cancel() {
        let ctx = egui::Context::default();
        let mut editor = Editor::new(chapter_with_pages(1));
        editor.set_layer(Layer::InpaintMask);
        editor.start_inline_edit("b0");
        assert!(editor.layer.edits_blocks(), "switches to a block layer");
        editor.inline_edit.as_mut().unwrap().draft = "새 문장".into();
        editor.cancel_inline_edit();
        assert_eq!(editor.page().unwrap().blocks[0].translated_text, "");
        editor.start_inline_edit("b0");
        editor.inline_edit.as_mut().unwrap().draft = "새 문장".into();
        editor.commit_inline_edit();
        assert_eq!(editor.page().unwrap().blocks[0].translated_text, "새 문장");
        editor.undo(&ctx);
        assert_eq!(editor.page().unwrap().blocks[0].translated_text, "");
    }

    fn services_in(dir: &std::path::Path) -> (egui::Context, Services, Worker<AppEvent>) {
        let ctx = egui::Context::default();
        let services = Services::new(&ctx, mtt_core::paths::AppPaths::from_data_root(dir));
        let worker = Worker::new(&ctx, 1);
        (ctx, services, worker)
    }

    #[test]
    fn pending_saves_schedule_a_frame() {
        let mut editor = Editor::new(chapter_with_pages(1));
        editor.selection = vec!["b0".into()];
        assert_eq!(editor.next_tick_due(), None, "nothing to save");
        editor.edit_selected(|block| block.translated_text = "x".into());
        let wait = editor.next_tick_due().expect("the autosave needs a frame");
        assert!(wait <= AUTOSAVE_DELAY + Duration::from_millis(10));
        editor.layer_dirty.insert(("p0".into(), LayerKind::Mask), Instant::now() - LAYER_SAVE_DELAY);
        assert!(editor.next_tick_due().unwrap() <= Duration::from_millis(10), "an overdue layer save comes first");
    }

    #[test]
    fn deleting_the_last_page_leaves_a_usable_editor() {
        let dir = tempfile::tempdir().unwrap();
        let (_ctx, mut services, worker) = services_in(dir.path());
        let mut editor = Editor::new(chapter_with_pages(1));
        editor.delete_page("p0", &mut services);
        assert_eq!(editor.page_count(), 0);
        editor.tick(&mut services, &worker, false);
        assert!(editor.page().is_none());
    }

    #[test]
    fn preset_undo_survives_deleting_its_page() {
        let dir = tempfile::tempdir().unwrap();
        let (ctx, mut services, _worker) = services_in(dir.path());
        let mut editor = Editor::new(chapter_with_pages(2));
        for page in &mut editor.chapter.pages {
            page.blocks[0].translated_text = "a x".into();
        }
        editor.reset_checkpoint();
        // A chapter-wide step recorded while page p0 is current.
        editor.replace_in_all_pages("x", "y");
        editor.delete_page("p0", &mut services);
        assert_eq!(editor.chapter.pages[0].blocks[0].translated_text, "a y");
        editor.undo(&ctx);
        assert_eq!(editor.chapter.pages[0].blocks[0].translated_text, "a x", "undo still applies");
        assert!(editor.history_can_redo_for_test());
    }

    #[test]
    fn inpaint_queue_skips_deleted_pages() {
        let dir = tempfile::tempdir().unwrap();
        let (_ctx, _services, worker) = services_in(dir.path());
        let mut editor = Editor::new(chapter_with_pages(1));
        editor.inpaint_queue = Some(InpaintQueue { pages: ["gone".to_owned()].into_iter().collect(), total: 1 });
        editor.advance_inpaint_queue(&worker);
        assert!(!editor.inpaint_busy());
    }

    #[test]
    fn undo_restores_the_selection_of_the_step() {
        let ctx = egui::Context::default();
        let mut chapter = chapter_with_pages(1);
        let mut second = chapter.pages[0].blocks[0].clone();
        second.id = "b1".into();
        chapter.pages[0].blocks.push(second);
        let mut editor = Editor::new(chapter);
        editor.set_layer(Layer::Overlay);
        editor.selection = vec!["b0".into()];
        editor.tick_for_test();
        editor.edit_selected(|block| block.translated_text = "x".into());
        editor.commit_checkpoint();
        editor.selection = vec!["b1".into()];
        editor.undo(&ctx);
        assert_eq!(editor.selection, ["b0"], "undo brings back the block it changed");
        assert_eq!(editor.page().unwrap().blocks[0].translated_text, "");
    }

    #[test]
    fn block_stepping_follows_output_order() {
        let mut chapter = chapter_with_pages(1);
        let page = &mut chapter.pages[0];
        page.blocks.push(TranslationBlock { id: "x".into(), ..TranslationBlock::default() });
        page.blocks.push(TranslationBlock { id: "y".into(), ..TranslationBlock::default() });
        let mut editor = Editor::new(chapter);
        editor.select_adjacent_block(1);
        assert_eq!(editor.selection, ["b0"], "nothing selected starts at the first block");
        editor.select_adjacent_block(1);
        editor.select_adjacent_block(1);
        editor.select_adjacent_block(1);
        assert_eq!(editor.selection, ["y"], "stops at the last block");
        editor.select_adjacent_block(-1);
        assert_eq!(editor.selection, ["x"]);
        editor.selection.clear();
        editor.select_adjacent_block(-1);
        assert_eq!(editor.selection, ["y"], "Shift+Tab with nothing selected starts at the end");
    }

    #[test]
    fn review_check_off_moves_to_the_next_page_to_review() {
        let mut editor = Editor::new(chapter_with_pages(4));
        editor.chapter.pages[1].progress_completed = Some(true);
        editor.select_layer_shortcut(1);
        editor.toggle_page_completed();
        assert!(editor.chapter.pages[0].is_completed());
        assert_eq!(editor.current, 2, "page 2 is already reviewed, so it goes on to 3");
        editor.toggle_page_completed();
        editor.toggle_page_completed();
        assert_eq!(editor.current, 3, "the last page stays put");
        assert!(editor.chapter.pages[3].is_completed());
        editor.toggle_page_completed();
        assert!(!editor.chapter.pages[3].is_completed(), "unchecking stays on the page");
        assert_eq!(editor.current, 3);

        editor.select_layer_shortcut(2);
        editor.go_to_page(0);
        editor.toggle_page_completed();
        assert_eq!(editor.current, 0, "other modes stay on the page");
    }

    #[test]
    fn layer_shortcuts_and_modes() {
        let mut editor = Editor::new(chapter_with_pages(1));
        editor.select_layer_shortcut(4);
        assert_eq!(editor.layer, Layer::InpaintMask);
        assert_eq!(editor.mode(), Mode::Inpaint);
        assert_eq!(editor.tool, Tool::Select, "entering the mask layer keeps the pointer");
        assert_eq!(editor.opacity.inpaint_mask, 1.0);
        editor.select_shared_inpaint_tool(Tool::MaskBrush);
        editor.set_mode(Mode::Translate);
        assert_eq!(editor.layer, Layer::Overlay);
        editor.set_mode(Mode::Inpaint);
        assert_eq!(editor.layer, Layer::InpaintMask, "inpaint mode returns to the last inpaint layer");
        assert_eq!(editor.tool, Tool::MaskBrush, "the mask tool is remembered");
        editor.select_layer_shortcut(5);
        assert_eq!(editor.layer, Layer::Image);
        assert_eq!(editor.mode(), Mode::Inpaint, "the original view stays in its mode");
        editor.select_layer_shortcut(5);
        assert_eq!(editor.layer, Layer::InpaintMask, "5 again goes back");
        editor.select_layer_shortcut(3);
        assert_eq!(editor.tool, Tool::Result, "brush is shared with the result layer");
        editor.select_layer_shortcut(1);
        assert_eq!(editor.opacity, DEFAULT_LAYER_OPACITY);
        editor.focus_mode = false;
        editor.select_layer_shortcut(5);
        assert_eq!(editor.opacity, DEFAULT_LAYER_OPACITY, "focus mode off keeps opacities");
    }

    #[test]
    fn overlay_selection_is_restored_and_groups_select_together() {
        let mut chapter = chapter_with_pages(1);
        let mut second = chapter.pages[0].blocks[0].clone();
        second.id = "b1".into();
        chapter.pages[0].blocks.push(second);
        chapter.pages[0].block_groups = Some(vec![mtt_core::model::TranslationBlockGroup {
            id: "g".into(),
            block_ids: vec!["b0".into(), "b1".into()],
            ..Default::default()
        }]);
        let mut editor = Editor::new(chapter);
        editor.set_layer(Layer::Overlay);
        editor.select_block_or_group("b1");
        assert_eq!(editor.selection, vec!["b1".to_owned(), "b0".to_owned()]);
        editor.select_layer_shortcut(4);
        assert!(editor.selection.is_empty());
        editor.select_layer_shortcut(2);
        assert_eq!(editor.selection.len(), 2, "the overlay selection comes back");
    }
}
