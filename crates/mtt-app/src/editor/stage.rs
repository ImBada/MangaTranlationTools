//! The page canvas: layer compositing, block overlays, selection handles, pan/zoom, block
//! dragging/resizing, range selection and mask painting.

use std::collections::HashSet;

use eframe::egui::{self, Color32, Key, Modifiers, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use mtt_core::geometry::{apply_editable_block_bbox, bbox_to_pixels, editable_block_bbox, resolve_block_rotation_deg};
use mtt_core::model::{BBox, PageRecord, TranslationBlock};

use super::assets::{PageAssets, stroke_mask};
use super::history::{Entry, LayerKind, LayerPatch, StrokeBackup, read_rect};
use super::{Editor, Layer, MASK_BRUSH_MAX, MASK_BRUSH_MIN, Tool};
use crate::services::Services;
use crate::textures::alpha_tint;
use crate::theme::{self, icon};

/// `MIN_STAGE_VIEW_SCALE` / `MAX_STAGE_VIEW_SCALE` / `MAX_STAGE_WIDTH_PX`.
const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 10.0;
const MAX_FIT_WIDTH: f32 = 1040.0;
/// `STAGE_ZOOM_STEP`.
pub const ZOOM_STEP: f32 = 1.2;
const HANDLE_SIZE: f32 = 8.0;
/// Screen px the result brush trails the pointer by at 100% smoothing.
const RESULT_SMOOTHING_REACH: f32 = 48.0;
const SELECTION_COLOR: Color32 = Color32::from_rgb(56, 140, 255);
const HOVER_COLOR: Color32 = Color32::from_rgba_premultiplied(56, 140, 255, 120);
const RANGE_COLOR: Color32 = Color32::from_rgb(255, 196, 0);

pub struct ViewState {
    pub zoom: f32,
    /// Screen offset of the page origin from the stage rect's top-left corner.
    pub offset: Vec2,
    pub fit_requested: bool,
    /// Still showing the fitted view (`viewScale === null`): refit when the stage resizes.
    pub auto_fit: bool,
    stage_rect: Rect,
}

impl Default for ViewState {
    fn default() -> Self {
        Self { zoom: 1.0, offset: Vec2::ZERO, fit_requested: true, auto_fit: true, stage_rect: Rect::NOTHING }
    }
}

impl ViewState {
    fn to_screen(&self, page: Pos2) -> Pos2 {
        self.stage_rect.min + self.offset + page.to_vec2() * self.zoom
    }

    fn to_page(&self, screen: Pos2) -> Pos2 {
        ((screen - self.stage_rect.min - self.offset) / self.zoom).to_pos2()
    }

    fn rect_to_screen(&self, rect: Rect) -> Rect {
        Rect::from_min_max(self.to_screen(rect.min), self.to_screen(rect.max))
    }

    /// `resolveStageFitSize`: fits the stage but never upscales or exceeds 1040px wide.
    /// `insets` keep the left and right room taken by widget windows free, unless that
    /// would leave the page too little.
    fn fit(&mut self, width: f32, height: f32, insets: (f32, f32)) {
        let stage = self.stage_rect.size();
        let (left, right) = if stage.x - insets.0 - insets.1 < stage.x * 0.45 { (0.0, 0.0) } else { insets };
        let room = egui::vec2(stage.x - left - right, stage.y);
        let available = room - Vec2::splat(32.0);
        let width = width.max(1.0);
        self.zoom = (available.x / width)
            .min(available.y / height.max(1.0))
            .min(MAX_FIT_WIDTH / width)
            .min(1.0)
            .clamp(MIN_ZOOM, MAX_ZOOM);
        self.offset = egui::vec2(left, 0.0) + (room - Vec2::new(width, height) * self.zoom) / 2.0;
        self.fit_requested = false;
        self.auto_fit = true;
    }

    /// "원본": 100% centred on the stage.
    pub fn original_size(&mut self, width: f32, height: f32) {
        self.auto_fit = false;
        self.zoom = 1.0;
        self.offset = (self.stage_rect.size() - Vec2::new(width, height)) / 2.0;
    }

    pub fn stage_rect(&self) -> Rect {
        self.stage_rect
    }

    pub fn stage_center(&self) -> Pos2 {
        self.stage_rect.center()
    }

    pub fn zoom_at_public(&mut self, anchor: Pos2, factor: f32) {
        self.zoom_at(anchor, factor);
    }

    /// Zooms keeping the page point under `anchor` (screen) fixed.
    fn zoom_at(&mut self, anchor: Pos2, factor: f32) {
        self.auto_fit = false;
        let page_point = self.to_page(anchor);
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        self.offset = anchor - self.stage_rect.min - page_point.to_vec2() * self.zoom;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handle {
    N,
    S,
    E,
    W,
    NE,
    NW,
    SE,
    SW,
}

impl Handle {
    const ALL: [Handle; 8] =
        [Handle::NW, Handle::N, Handle::NE, Handle::E, Handle::SE, Handle::S, Handle::SW, Handle::W];

    /// Position in unit box coordinates (0..1).
    fn anchor(self) -> (f32, f32) {
        match self {
            Handle::NW => (0.0, 0.0),
            Handle::N => (0.5, 0.0),
            Handle::NE => (1.0, 0.0),
            Handle::E => (1.0, 0.5),
            Handle::SE => (1.0, 1.0),
            Handle::S => (0.5, 1.0),
            Handle::SW => (0.0, 1.0),
            Handle::W => (0.0, 0.5),
        }
    }

    fn cursor(self) -> egui::CursorIcon {
        match self {
            Handle::N | Handle::S => egui::CursorIcon::ResizeVertical,
            Handle::E | Handle::W => egui::CursorIcon::ResizeHorizontal,
            Handle::NE | Handle::SW => egui::CursorIcon::ResizeNeSw,
            Handle::NW | Handle::SE => egui::CursorIcon::ResizeNwSe,
        }
    }
}

pub enum Drag {
    Move {
        start: Pos2,
        originals: Vec<(String, BBox, Option<BBox>)>,
    },
    Resize {
        id: String,
        handle: Handle,
        start: Pos2,
        original: BBox,
        rotation: f32,
    },
    Pan {
        last: Pos2,
    },
    Range {
        start: Pos2,
        current: Pos2,
        additive: bool,
        /// On the inpaint layers, like Photoshop's marquee: ⇧ draws a square, ⌥ draws from
        /// the centre, Space moves the rectangle being drawn.
        square: bool,
        centered: bool,
    },
    /// Dragging inside the inpaint range moves its outline (Photoshop's marquee).
    RangeMove {
        grab: Pos2,
        original: Rect,
    },
    /// Alt+drag with a selected block: the rectangle becomes that block's box
    /// (`onSelectedBlockRangeChange`).
    BlockRange {
        id: String,
        start: Pos2,
        current: Pos2,
    },
    /// Rotate handle (`mode === "rotate"`): angles around the block centre in page space.
    Rotate {
        id: String,
        center: Pos2,
        start_angle: f32,
        start_rotation: f64,
    },
    /// A press that already did its work (Shift-toggle); nothing follows the pointer.
    Idle,
    Zoom {
        start: Pos2,
        start_zoom: f32,
        anchor: Pos2,
    },
    Paint {
        last: Pos2,
        erase: bool,
        before: StrokeBackup,
        dirty: Option<(u32, u32, u32, u32)>,
    },
    /// Picks islands into `LoadedPage::mask_selection`; the mask changes on release.
    AutoErase {
        last: Pos2,
        dirty: Option<(u32, u32, u32, u32)>,
    },
    /// Smart brush: collects the area to heal; the fill runs on release.
    Heal {
        last: Pos2,
        mask: image::GrayImage,
        /// Pixels painted so far, for the on-screen preview.
        bounds: Option<(u32, u32, u32, u32)>,
    },
    ResultPaint {
        /// The latest pointer sample; the brush may trail it (smoothing).
        last: Pos2,
        stroke: Box<super::result_tools::Stroke>,
        before_result: StrokeBackup,
        before_mask: StrokeBackup,
        dirty: Option<(u32, u32, u32, u32)>,
    },
}

fn page_size(page: &PageRecord) -> Vec2 {
    egui::vec2(page.width as f32, page.height as f32)
}

/// The inpaint range a marquee drag selects: corners on the pixel grid, clamped to the
/// page; `None` when nothing is left.
fn marquee_rect(start: Pos2, current: Pos2, square: bool, centered: bool, page: Vec2) -> Option<Rect> {
    let start = start.round();
    let mut delta = current.round() - start;
    if square {
        let side = delta.x.abs().max(delta.y.abs());
        delta = egui::vec2(side * delta.x.signum(), side * delta.y.signum());
    }
    let rect = if centered {
        Rect::from_center_size(start, delta.abs() * 2.0)
    } else {
        Rect::from_two_pos(start, start + delta)
    };
    let rect = rect.intersect(Rect::from_min_size(Pos2::ZERO, page));
    (rect.width() >= 1.0 && rect.height() >= 1.0).then_some(rect)
}

/// `range` moved by whole pixels, kept on the page.
pub(super) fn moved_range(range: Rect, offset: Vec2, page: Vec2) -> Rect {
    let max = (page - range.size()).max(Vec2::ZERO);
    let min = (range.min + offset).clamp(Pos2::ZERO, max.to_pos2());
    Rect::from_min_size(min, range.size())
}

/// Photoshop's selection outline: black and white dashes crawling along the edge.
fn marching_ants(ui: &egui::Ui, painter: &egui::Painter, rect: Rect) {
    const DASH: f32 = 4.0;
    let snap = |pos: Pos2| Pos2::new(painter.round_to_pixel_center(pos.x), painter.round_to_pixel_center(pos.y));
    let rect = Rect::from_min_max(snap(rect.min), snap(rect.max));
    let path = [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom(), rect.left_top()];
    let width = 1.0 / ui.ctx().pixels_per_point();
    painter.add(egui::Shape::line(path.to_vec(), Stroke::new(width, Color32::BLACK)));
    let time = ui.input(|i| i.time);
    let offset = (time * 12.0) as f32 % (DASH * 2.0);
    painter.extend(egui::Shape::dashed_line_with_offset(
        &path,
        Stroke::new(width, Color32::WHITE),
        &[DASH],
        &[DASH],
        DASH * 2.0 - offset,
    ));
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(80));
}

/// Photoshop's W/H (or ΔX/ΔY) box beside the pointer.
fn range_readout(painter: &egui::Painter, pointer: Pos2, text: &str) {
    let galley = painter.layout_no_wrap(text.to_owned(), egui::FontId::monospace(11.0), Color32::WHITE);
    let rect = Rect::from_min_size(pointer + egui::vec2(16.0, 16.0), galley.size() + egui::vec2(12.0, 8.0));
    painter.rect_filled(rect, 3.0, Color32::from_black_alpha(200));
    painter.galley(rect.min + egui::vec2(6.0, 4.0), galley, Color32::WHITE);
}

/// Page-pixel rect of a block's editable box.
fn block_rect(block: &TranslationBlock, page: &PageRecord) -> Rect {
    let px = bbox_to_pixels(editable_block_bbox(block), f64::from(page.width), f64::from(page.height));
    Rect::from_min_size(egui::pos2(px.x as f32, px.y as f32), egui::vec2(px.w as f32, px.h as f32))
}

fn rotate_around(point: Pos2, center: Pos2, degrees: f32) -> Pos2 {
    let (sin, cos) = degrees.to_radians().sin_cos();
    let d = point - center;
    center + egui::vec2(d.x * cos - d.y * sin, d.x * sin + d.y * cos)
}

/// Corners of the (rotated) block box in page pixels.
fn block_corners(block: &TranslationBlock, page: &PageRecord) -> [Pos2; 4] {
    let rect = block_rect(block, page);
    let rotation = resolve_block_rotation_deg(block) as f32;
    [rect.left_top(), rect.right_top(), rect.right_bottom(), rect.left_bottom()]
        .map(|p| rotate_around(p, rect.center(), rotation))
}

/// Rows of the block's text shape in page pixels, rotated like the box; `None` without one.
fn text_shape_quads(block: &TranslationBlock, page: &PageRecord) -> Option<Vec<[Pos2; 4]>> {
    let rect = block_rect(block, page);
    let rotation = resolve_block_rotation_deg(block) as f32;
    let pixels = mtt_render::layout::PixelRect {
        left: f64::from(rect.left()),
        top: f64::from(rect.top()),
        width: f64::from(rect.width()),
        height: f64::from(rect.height()),
    };
    let strips = mtt_render::layout::text_shape_strips(block, pixels)?;
    Some(
        strips
            .into_iter()
            .map(|strip| {
                let min = egui::pos2(strip.left as f32, strip.top as f32);
                let max = egui::pos2((strip.left + strip.width) as f32, (strip.top + strip.height) as f32);
                [min, egui::pos2(max.x, min.y), max, egui::pos2(min.x, max.y)]
                    .map(|p| rotate_around(p, rect.center(), rotation))
            })
            .collect(),
    )
}

fn block_contains(block: &TranslationBlock, page: &PageRecord, point: Pos2) -> bool {
    let rect = block_rect(block, page);
    let local = rotate_around(point, rect.center(), -(resolve_block_rotation_deg(block) as f32));
    rect.contains(local)
}

fn hit_block(page: &PageRecord, point: Pos2) -> Option<&TranslationBlock> {
    page.blocks.iter().rev().find(|block| block_contains(block, page, point))
}

/// Distance of the rotate handle above the block's top edge, in screen points.
const ROTATE_HANDLE_OFFSET: f32 = 22.0;

/// Page-space position of the rotate handle (above the top edge, turning with the block).
fn rotate_handle_position(block: &TranslationBlock, page: &PageRecord, zoom: f32) -> Pos2 {
    let rect = block_rect(block, page);
    let point = Pos2::new(rect.center().x, rect.top() - ROTATE_HANDLE_OFFSET / zoom.max(0.01));
    rotate_around(point, rect.center(), resolve_block_rotation_deg(block) as f32)
}

fn angle_deg(center: Pos2, point: Pos2) -> f32 {
    (point.y - center.y).atan2(point.x - center.x).to_degrees()
}

fn handle_positions(block: &TranslationBlock, page: &PageRecord) -> Vec<(Handle, Pos2)> {
    let rect = block_rect(block, page);
    let rotation = resolve_block_rotation_deg(block) as f32;
    Handle::ALL
        .iter()
        .map(|handle| {
            let (u, v) = handle.anchor();
            let point = rect.min + egui::vec2(rect.width() * u, rect.height() * v);
            (*handle, rotate_around(point, rect.center(), rotation))
        })
        .collect()
}

/// Raster scale for block textures: sharp at the current zoom without re-rendering on
/// every zoom step.
fn raster_scale(zoom: f32, pixels_per_point: f32) -> f32 {
    let wanted = zoom * pixels_per_point;
    if wanted <= 1.0 {
        1.0
    } else if wanted <= 2.0 {
        2.0
    } else {
        3.0
    }
}

/// Multiline editor placed over the block being edited inline.
pub fn inline_editor(editor: &mut Editor, ui: &mut egui::Ui) {
    let Some(edit) = editor.inline_edit.as_mut() else { return };
    let Some(page) = editor.chapter.pages.get(editor.current) else { return };
    let Some(block) = page.blocks.iter().find(|b| b.id == edit.block_id) else {
        editor.inline_edit = None;
        return;
    };
    let rect = editor.view.rect_to_screen(block_rect(block, page));
    let width = rect.width().max(160.0);
    let mut commit = false;
    let mut cancel = false;
    let mut split = None;
    egui::Area::new(egui::Id::unique("inline-block-editor")).fixed_pos(rect.min).order(egui::Order::Foreground).show(
        ui.ctx(),
        |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                let output = egui::TextEdit::multiline(&mut edit.draft)
                    .desired_width(width)
                    .desired_rows(((rect.height() / 20.0) as usize).clamp(2, 12))
                    .hint_text("번역문 (⌘Enter 확정, Esc 취소)")
                    .show(ui);
                let response = output.response;
                // ⌘+click splits off the selection; remember it before the click moves the cursor.
                let split_click =
                    ui.input(|i| i.pointer.primary_pressed() && i.modifiers.command) && response.contains_pointer();
                if split_click {
                    split = edit.selection;
                } else if let Some(range) = output.cursor_range {
                    let (a, b): (usize, usize) = (range.primary.index.into(), range.secondary.index.into());
                    edit.selection = (a != b).then_some((a.min(b), a.max(b)));
                }
                if edit.focus_requested {
                    response.request_focus();
                    edit.focus_requested = false;
                }
                let (escape, command_enter) =
                    ui.input(|i| (i.key_pressed(Key::Escape), i.key_pressed(Key::Enter) && i.modifiers.command));
                if escape {
                    cancel = true;
                } else if command_enter || (response.lost_focus() && !edit.focus_requested && split.is_none()) {
                    commit = true;
                }
                // The closed editor would hold the focus (and the shortcuts) a frame longer.
                if cancel || commit {
                    response.surrender_focus();
                }
            });
        },
    );
    if let Some((start, end)) = split {
        let (block_id, draft) = (edit.block_id.clone(), edit.draft.clone());
        editor.inline_edit = None;
        editor.split_block_text(&block_id, &draft, start, end);
        return;
    }
    editor.text_editing = !commit && !cancel;
    if cancel {
        editor.cancel_inline_edit();
    } else if commit {
        editor.commit_inline_edit();
    }
}

pub fn stage(editor: &mut Editor, ui: &mut egui::Ui, services: &mut Services) {
    let frame = egui::Frame::NONE.fill(crate::theme::CANVAS_BG);
    egui::CentralPanel::default().frame(frame).show(ui, |ui| {
        let (rect, response) = ui.allocate_exact_size(ui.available_size(), Sense::click_and_drag());
        if editor.view.auto_fit && editor.view.stage_rect.size() != rect.size() {
            editor.view.fit_requested = true;
        }
        editor.view.stage_rect = rect;
        let painter = ui.painter_at(rect);
        let Some(page) = editor.page().cloned() else {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "페이지가 없습니다.",
                egui::FontId::proportional(18.0),
                Color32::GRAY,
            );
            return;
        };
        if editor.view.fit_requested {
            editor.view.fit(page.width as f32, page.height as f32, editor.widgets.side_insets());
        }
        let page_rect = editor
            .view
            .rect_to_screen(Rect::from_min_size(Pos2::ZERO, egui::vec2(page.width as f32, page.height as f32)));

        paint_layers(editor, &painter, &page, page_rect);
        paint_overlay(editor, ui, services, &painter, &page);
        paint_decorations(editor, ui, &painter, &page);
        canvas_notice(editor, &painter, rect);
        handle_view_input(editor, ui, &response);
        handle_pointer(editor, ui, &response, &page);
        block_quick_controls(editor, ui, &page);
    });
}

/// One line at the top of the canvas saying what is shown when it isn't the mode's
/// normal view, or what to do first.
fn canvas_notice(editor: &Editor, painter: &egui::Painter, stage: Rect) {
    let text = if editor.final_output_preview() {
        "최종 결과 미리보기 (Space를 떼면 돌아갑니다)"
    } else if editor.source_view() {
        "원본 이미지 보기 중 · 5를 누르면 돌아갑니다"
    } else if editor.mode() == super::Mode::Inpaint
        && editor.is_page_decoded()
        && !editor.inpaint_busy()
        && !editor.current_result_exists()
        && !editor.current_mask_has_pixels()
    {
        "아직 인페인트하지 않은 페이지"
    } else {
        return;
    };
    let galley = painter.layout_no_wrap(text.to_owned(), egui::FontId::proportional(12.0), crate::theme::TEXT);
    let size = galley.size() + egui::vec2(24.0, 12.0);
    let pill = Rect::from_center_size(egui::pos2(stage.center().x, stage.top() + 14.0 + size.y / 2.0), size);
    painter.rect_filled(pill, 6.0, crate::theme::CARD_BG.gamma_multiply(0.94));
    painter.rect_stroke(pill, 6.0, egui::Stroke::new(1.0, crate::theme::BORDER), egui::StrokeKind::Inside);
    painter.galley(pill.min + egui::vec2(12.0, 6.0), galley, crate::theme::TEXT);
}

/// `OverlayBlock` controls for the single selected block on the overlay layer, in one
/// row: alignment, favourite presets, auto size or the individual size stepper (press
/// and hold repeats), and copying the font settings.
fn block_quick_controls(editor: &mut Editor, ui: &mut egui::Ui, page: &PageRecord) {
    // Areas draw above windows, so stay out of the way while a dialog is open.
    let dialog_open = editor.dialog_open();
    if editor.layer != Layer::Overlay
        || editor.final_output_preview()
        || editor.selection.len() != 1
        || editor.drag.is_some()
        || editor.inline_edit.is_some()
        || editor.page_edit_locked()
        || dialog_open
    {
        editor.size_repeat = None;
        return;
    }
    let Some(block) = page.blocks.iter().find(|block| block.id == editor.selection[0]).cloned() else { return };
    let rect = editor.view.rect_to_screen(block_rect(&block, page));
    // Nothing floats over the toolbars when the block is scrolled out of the canvas.
    if !editor.view.stage_rect().shrink(8.0).intersects(rect) {
        editor.size_repeat = None;
        return;
    }
    let ctx = ui.ctx().clone();
    let stage = editor.view.stage_rect();
    let frame = egui::Frame::popup(ui.style()).inner_margin(egui::Margin::symmetric(4, 3));
    let favorites: Vec<mtt_core::model::FontPreset> = editor
        .chapter
        .favorite_font_preset_ids
        .iter()
        .flatten()
        .filter_map(|id| editor.chapter.font_presets.iter().flatten().find(|preset| &preset.id == id).cloned())
        .collect();
    // One row above the block (below it when there is no room), kept inside the canvas.
    let above = rect.top() - 36.0 - ROTATE_HANDLE_OFFSET;
    let y = if above >= stage.top() + 4.0 { above } else { rect.bottom() + 8.0 };
    // The row draws above windows, so it waits while a widget window sits where it goes.
    let row = Rect::from_min_size(egui::pos2(rect.left(), y), egui::vec2(320.0, 36.0));
    if editor.widgets.covers(row, editor.mode()) {
        editor.size_repeat = None;
        return;
    }
    egui::Area::new(egui::Id::unique("block-quick-controls"))
        .fixed_pos(egui::pos2(rect.left(), y))
        .constrain_to(stage)
        .order(egui::Order::Foreground)
        .show(&ctx, |ui| {
            frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    ui.spacing_mut().button_padding.x = 6.0;
                    use mtt_core::model::TextAlign;
                    for (align, glyph, hint) in [
                        (TextAlign::Left, icon::TEXT_ALIGN_LEFT, "왼쪽 정렬"),
                        (TextAlign::Center, icon::TEXT_ALIGN_CENTER, "가운데 정렬"),
                        (TextAlign::Right, icon::TEXT_ALIGN_RIGHT, "오른쪽 정렬"),
                    ] {
                        if theme::icon_tool(ui, true, block.text_align == align, glyph, "", hint).clicked() {
                            editor.edit_style(|b| b.text_align = align);
                        }
                    }
                    if !favorites.is_empty() {
                        ui.separator();
                    }
                    for preset in &favorites {
                        let active = block.font_preset_id.as_deref() == Some(preset.id.as_str());
                        let name = super::presets::display_name(&preset.name);
                        if ui.selectable_label(active, name).on_hover_text(format!("{name} 프리셋 적용")).clicked()
                        {
                            editor.select_preset(preset);
                        }
                    }
                    ui.separator();
                    if block.style.auto_fit_text != Some(false) {
                        if ui
                            .button(theme::icon_text(icon::TEXT_AA, "자동 크기"))
                            .on_hover_text(
                                "블록에 맞춰 글자 크기를 자동으로 정하는 중입니다. 누르면 크기를 직접 정합니다",
                            )
                            .clicked()
                        {
                            editor.disable_auto_fit();
                        }
                    } else {
                        let size = block.font_size_px();
                        let minus = ui
                            .add_enabled(
                                size > super::OVERLAY_FONT_SIZE_MIN,
                                egui::Button::new(theme::icon(icon::MINUS)),
                            )
                            .on_hover_text("글자 작게 (누르고 있으면 계속)");
                        let mut value = size.round();
                        if ui
                            .add(
                                egui::DragValue::new(&mut value)
                                    .range(super::OVERLAY_FONT_SIZE_MIN..=super::OVERLAY_FONT_SIZE_MAX)
                                    .suffix("px"),
                            )
                            .changed()
                        {
                            editor.set_individual_font_size(value);
                        }
                        let plus = ui
                            .add_enabled(
                                size < super::OVERLAY_FONT_SIZE_MAX,
                                egui::Button::new(theme::icon(icon::PLUS)),
                            )
                            .on_hover_text("글자 크게 (누르고 있으면 계속)");
                        let held = if plus.is_pointer_button_down_on() {
                            Some(1.0)
                        } else if minus.is_pointer_button_down_on() {
                            Some(-1.0)
                        } else {
                            None
                        };
                        repeat_size_step(editor, &ctx, held, size);
                    }
                    ui.separator();
                    let copy_hint = format!(
                        "이 블록의 글꼴 설정 복사\n다른 블록을 고르고 {}로 붙여넣습니다",
                        crate::shortcuts::keys("⌘V")
                    );
                    if theme::icon_tool(ui, true, false, icon::PAINT_ROLLER, "", &copy_hint).clicked() {
                        editor.copy_font_settings(&ctx);
                    }
                });
            });
        });
}

/// `useRepeatingStepControl`: one step on press, then every 70ms after a 350ms hold.
fn repeat_size_step(editor: &mut Editor, ctx: &egui::Context, held: Option<f64>, size: f64) {
    let now = std::time::Instant::now();
    match (held, editor.size_repeat) {
        (Some(direction), None) => {
            editor.set_individual_font_size(size + direction);
            editor.size_repeat = Some((now, now));
            ctx.request_repaint_after(std::time::Duration::from_millis(350));
        }
        (Some(direction), Some((start, last))) => {
            if now - start >= std::time::Duration::from_millis(350)
                && now - last >= std::time::Duration::from_millis(70)
            {
                editor.set_individual_font_size(size + direction);
                editor.size_repeat = Some((start, now));
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(70));
        }
        (None, _) => editor.size_repeat = None,
    }
}

fn paint_layers(editor: &Editor, painter: &egui::Painter, page: &PageRecord, page_rect: Rect) {
    let (visibility, opacity) = editor.preview_layers();
    match editor.assets.get(&page.id) {
        Some(PageAssets::Ready(loaded)) => {
            // A faded original sits on black so the mask and erased areas stand out.
            let dimmed = visibility.image && opacity.image < 1.0;
            painter.rect_filled(page_rect, 0.0, if dimmed { Color32::BLACK } else { Color32::WHITE });
            if visibility.image && opacity.image > 0.0 {
                loaded.source_texture.paint(painter, page_rect, alpha_tint(opacity.image));
            }
            if visibility.inpaint {
                if visibility.inpaint_result && opacity.inpaint_result > 0.0 {
                    // `inpaintResultComposite`: the raw result only while editing it.
                    let texture = if editor.layer == Layer::InpaintResult && !editor.final_output_preview() {
                        loaded.result_texture.as_ref()
                    } else {
                        loaded.result_masked_texture.as_ref()
                    };
                    if let Some(texture) = texture {
                        texture.paint(painter, page_rect, alpha_tint(opacity.inpaint * opacity.inpaint_result));
                    }
                }
                if visibility.inpaint_mask
                    && opacity.inpaint_mask > 0.0
                    && let Some(texture) = &loaded.mask_texture
                {
                    texture.paint(painter, page_rect, alpha_tint(opacity.inpaint * opacity.inpaint_mask * 0.6));
                }
            }
        }
        Some(PageAssets::Failed(error)) => {
            painter.rect_filled(page_rect, 0.0, Color32::from_gray(60));
            painter.text(
                page_rect.center(),
                egui::Align2::CENTER_CENTER,
                error,
                egui::FontId::proportional(14.0),
                Color32::LIGHT_RED,
            );
        }
        _ => {
            painter.rect_filled(page_rect, 0.0, Color32::from_gray(70));
            painter.text(
                page_rect.center(),
                egui::Align2::CENTER_CENTER,
                "불러오는 중…",
                egui::FontId::proportional(16.0),
                Color32::GRAY,
            );
        }
    }
}

fn paint_overlay(
    editor: &mut Editor,
    ui: &egui::Ui,
    services: &mut Services,
    painter: &egui::Painter,
    page: &PageRecord,
) {
    let (visibility, opacity) = editor.preview_layers();
    if !visibility.overlay || opacity.overlay <= 0.0 {
        return;
    }
    let Some(fonts) = services.fonts() else {
        let at = editor.view.to_screen(Pos2::new(8.0, 8.0));
        painter.text(at, egui::Align2::LEFT_TOP, "폰트 불러오는 중…", egui::FontId::proportional(13.0), Color32::GRAY);
        ui.ctx().request_repaint_after(std::time::Duration::from_millis(100));
        return;
    };
    let moving: HashSet<String> = match &editor.drag {
        Some(Drag::Move { originals, .. }) => originals.iter().map(|(id, _, _)| id.clone()).collect(),
        _ => HashSet::new(),
    };
    let scale = raster_scale(editor.view.zoom, ui.ctx().pixels_per_point());
    // The fonts are only needed for changed blocks; a background job may hold them, so the
    // frame draws what is cached and tries again shortly instead of waiting.
    if !editor.overlay.up_to_date(page, editor.revision, scale, &moving) {
        match fonts.try_lock() {
            Ok(mut fonts) => {
                if editor.overlay.sync(ui.ctx(), &mut fonts, page, editor.revision, scale, &moving) {
                    ui.ctx().request_repaint();
                }
            }
            Err(std::sync::TryLockError::WouldBlock) => {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(30));
            }
            Err(std::sync::TryLockError::Poisoned(_)) => {}
        }
    }

    let tint = alpha_tint(opacity.overlay);
    // Block backgrounds are an editing aid, not part of the output: they cover the original
    // text until the page is inpainted, then the page shows as it will be exported (the
    // "배경 투명도 편집" mode keeps them to edit their opacity).
    let backgrounds = editor.overlay_opacity_edit || !editor.current_result_exists();
    if editor.layer == Layer::Overlay && backgrounds && !editor.final_output_preview() {
        for block in
            page.blocks.iter().filter(|block| block.render_direction != mtt_core::model::RenderTextDirection::Hidden)
        {
            let color = mtt_core::geometry::parse_hex_color(&block.background_color).unwrap_or((255, 255, 255));
            let alpha = (block.opacity.clamp(0.0, 1.0) * f64::from(opacity.overlay) * 255.0) as u8;
            let fill = Color32::from_rgba_unmultiplied(color.0, color.1, color.2, alpha);
            // A balloon block's background covers its text shape: one mesh, so rows don't seam.
            if let Some(quads) = text_shape_quads(block, page) {
                let mut mesh = egui::Mesh::default();
                for quad in quads {
                    let first = mesh.vertices.len() as u32;
                    for corner in quad {
                        mesh.colored_vertex(editor.view.to_screen(corner), fill);
                    }
                    mesh.add_triangle(first, first + 1, first + 2);
                    mesh.add_triangle(first, first + 2, first + 3);
                }
                painter.add(egui::Shape::mesh(mesh));
                continue;
            }
            let corners = block_corners(block, page).map(|p| editor.view.to_screen(p));
            painter.add(egui::Shape::convex_polygon(corners.to_vec(), fill, Stroke::NONE));
        }
    }
    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    for shadow in editor.overlay.shadows(page) {
        painter.image(shadow.texture.id(), editor.view.rect_to_screen(shadow.rect), uv, tint);
    }
    for block in &page.blocks {
        if let Some((raster, offset)) = editor.overlay.raster(block, page) {
            painter.image(raster.texture.id(), editor.view.rect_to_screen(raster.rect.translate(offset)), uv, tint);
        }
    }
}

fn paint_decorations(editor: &Editor, ui: &egui::Ui, painter: &egui::Painter, page: &PageRecord) {
    let hover = ui.input(|i| i.pointer.hover_pos()).filter(|pos| editor.view.stage_rect.contains(*pos));
    if let Some(Drag::Heal { mask, bounds: Some(bounds), .. }) = &editor.drag {
        painter.add(heal_preview(editor, mask, *bounds));
    }
    // The final preview shows the page as exported: no outlines, handles or overflow marks.
    if editor.layer.edits_blocks() && !editor.final_output_preview() {
        let hovered = hover.and_then(|pos| hit_block(page, editor.view.to_page(pos))).map(|block| block.id.clone());
        for block in &page.blocks {
            let selected = editor.selection.contains(&block.id);
            // `overflowing`: the text is longer than the render box.
            if editor.overlay.overflowing(&block.id) {
                let corners: Vec<Pos2> = block_corners(block, page).iter().map(|p| editor.view.to_screen(*p)).collect();
                painter.add(egui::Shape::dashed_line(
                    &[corners.as_slice(), &corners[..1]].concat(),
                    Stroke::new(1.5, Color32::from_rgb(239, 68, 68)),
                    6.0,
                    4.0,
                ));
                if hovered.as_deref() == Some(block.id.as_str()) {
                    egui::Tooltip::always_open(
                        ui.ctx().clone(),
                        ui.layer_id(),
                        egui::Id::unique("overflow-tip"),
                        egui::PopupAnchor::Pointer,
                    )
                    .show(|ui| ui.label("현재 render box보다 번역문이 길어서 넘칩니다."));
                }
            }
            let outline = if selected {
                Stroke::new(1.5, SELECTION_COLOR)
            } else if hovered.as_deref() == Some(block.id.as_str()) || editor.layer == Layer::Overlay {
                Stroke::new(1.0, HOVER_COLOR)
            } else {
                continue;
            };
            let corners: Vec<Pos2> = block_corners(block, page).iter().map(|p| editor.view.to_screen(*p)).collect();
            painter.add(egui::Shape::closed_line(corners, outline));
        }
        if editor.selection.len() == 1
            && let Some(block) = page.blocks.iter().find(|block| block.id == editor.selection[0])
        {
            for (_, position) in handle_positions(block, page) {
                let center = editor.view.to_screen(position);
                let handle = Rect::from_center_size(center, Vec2::splat(HANDLE_SIZE));
                painter.rect_filled(handle, 1.0, Color32::WHITE);
                painter.rect_stroke(handle, 1.0, Stroke::new(1.0, SELECTION_COLOR), StrokeKind::Inside);
            }
            let rect = block_rect(block, page);
            let top = rotate_around(
                Pos2::new(rect.center().x, rect.top()),
                rect.center(),
                resolve_block_rotation_deg(block) as f32,
            );
            let knob = editor.view.to_screen(rotate_handle_position(block, page, editor.view.zoom));
            painter.line_segment([editor.view.to_screen(top), knob], Stroke::new(1.0, SELECTION_COLOR));
            painter.circle_filled(knob, HANDLE_SIZE / 2.0 + 1.0, Color32::WHITE);
            painter.circle_stroke(knob, HANDLE_SIZE / 2.0 + 1.0, Stroke::new(1.0, SELECTION_COLOR));
        }
    }
    // The inpaint range belongs to the inpaint modes: marching ants on pixel edges, and
    // while drawing or moving it the size or the offset next to the pointer.
    let inpaint = editor.mode() == super::Mode::Inpaint;
    match &editor.drag {
        Some(Drag::Range { start, current, square, centered, .. }) if inpaint => {
            if let Some(rect) = marquee_rect(*start, *current, *square, *centered, page_size(page)) {
                marching_ants(ui, painter, editor.view.rect_to_screen(rect));
                if let Some(pos) = hover {
                    range_readout(painter, pos, &format!("W: {} px\nH: {} px", rect.width(), rect.height()));
                }
            }
        }
        Some(Drag::Range { start, current, .. }) => {
            let rect = editor.view.rect_to_screen(Rect::from_two_pos(*start, *current));
            painter.rect_filled(rect, 0.0, RANGE_COLOR.gamma_multiply(0.12));
            painter.rect_stroke(rect, 0.0, Stroke::new(1.0, RANGE_COLOR), StrokeKind::Middle);
        }
        Some(Drag::RangeMove { original, .. }) if inpaint => {
            if let Some(range) = editor.range {
                marching_ants(ui, painter, editor.view.rect_to_screen(range));
                if let Some(pos) = hover {
                    let offset = range.min - original.min;
                    range_readout(painter, pos, &format!("ΔX: {} px\nΔY: {} px", offset.x, offset.y));
                }
            }
        }
        _ => {
            if let Some(range) = editor.range
                && inpaint
            {
                marching_ants(ui, painter, editor.view.rect_to_screen(range));
            }
        }
    }
    // Colour picker loupe: brush colour (left) and the last sample (right).
    if editor.tool == Tool::Result
        && editor.result_tool == super::result_tools::ResultTool::ColorPicker
        && let Some(pos) = hover
    {
        let center = pos + egui::vec2(28.0, -28.0);
        let current = editor.result_brush.color;
        let sampled = editor.picked_color.map_or(current, |(_, color)| color);
        let left = Color32::from_rgb(current[0], current[1], current[2]);
        let right = Color32::from_rgb(sampled[0], sampled[1], sampled[2]);
        painter.rect_filled(
            Rect::from_min_max(center - egui::vec2(18.0, 12.0), center + egui::vec2(0.0, 12.0)),
            0.0,
            left,
        );
        painter.rect_filled(
            Rect::from_min_max(center - egui::vec2(0.0, 12.0), center + egui::vec2(18.0, 12.0)),
            0.0,
            right,
        );
        painter.rect_stroke(
            Rect::from_center_size(center, egui::vec2(36.0, 24.0)),
            2.0,
            Stroke::new(1.0, Color32::WHITE),
            StrokeKind::Outside,
        );
    }
    if !editor.final_preview
        && (matches!(editor.tool, Tool::MaskBrush | Tool::MaskEraser | Tool::MaskAutoEraser)
            || (editor.tool == Tool::Result && editor.result_tool != super::result_tools::ResultTool::ColorPicker))
    {
        let Some(pos) = hover else { return };
        let size = if editor.tool == Tool::Result { editor.result_brush.size } else { editor.brush_size };
        let radius = size * editor.view.zoom / 2.0;
        painter.circle_stroke(pos, radius, Stroke::new(1.5, Color32::BLACK));
        painter.circle_stroke(pos, radius, Stroke::new(0.75, Color32::WHITE));
        // Shift previews the straight line a click would draw.
        let modifiers = ui.input(|i| i.modifiers);
        if editor.drag.is_none()
            && editor.tool != Tool::MaskAutoEraser
            && let Some(start) = line_start(editor, &page.id, modifiers)
        {
            let start = editor.view.to_screen(start);
            painter.line_segment([start, pos], Stroke::new(1.5, Color32::BLACK));
            painter.line_segment([start, pos], Stroke::new(0.75, Color32::WHITE));
        }
    }
}

fn handle_view_input(editor: &mut Editor, ui: &egui::Ui, response: &egui::Response) {
    if !response.hovered() && !matches!(editor.drag, Some(Drag::Pan { .. }) | Some(Drag::Zoom { .. })) {
        return;
    }
    let (zoom_delta, scroll, hover) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta(), i.pointer.hover_pos()));
    if let Some(anchor) = hover {
        if zoom_delta != 1.0 {
            editor.view.zoom_at(anchor, zoom_delta);
        } else if scroll != Vec2::ZERO {
            editor.view.offset += scroll;
            editor.view.auto_fit = false;
        }
    }
}

fn handle_pointer(editor: &mut Editor, ui: &egui::Ui, response: &egui::Response, page: &PageRecord) {
    let ctx = ui.ctx().clone();
    if editor.eyedropper.as_ref().is_some_and(|state| !matches!(state, super::color::Eyedropper::Done { .. })) {
        return;
    }
    let (modifiers, space_down, pointer_pos, middle_down) =
        ui.input(|i| (i.modifiers, i.key_down(Key::Space), i.pointer.interact_pos(), i.pointer.middle_down()));

    // Painting tools start on press so a single click stamps once; everything else waits
    // for egui's drag threshold.
    let paint_tool =
        matches!(editor.tool, Tool::MaskBrush | Tool::MaskEraser | Tool::Result) && !(space_down || middle_down);
    let pressed = response.is_pointer_button_down_on() && ui.input(|i| i.pointer.any_pressed());
    if editor.drag.is_none() && ((paint_tool && pressed) || response.drag_started()) {
        // Hit-test where the press happened, not where egui's drag threshold was crossed.
        let Some(screen) = ui.input(|i| i.pointer.press_origin()).or(pointer_pos) else { return };
        let point = editor.view.to_page(screen);
        editor.drag = Some(start_drag(editor, &ctx, page, screen, point, modifiers, space_down || middle_down));
        // Text fields keep focus otherwise and swallow shortcuts.
        ctx.memory_mut(|memory| memory.stop_text_input());
    }

    if let Some(screen) = pointer_pos
        && editor.drag.is_some()
        && (response.dragged() || response.is_pointer_button_down_on())
    {
        if matches!(editor.drag, Some(Drag::Paint { .. } | Drag::ResultPaint { .. } | Drag::Heal { .. })) {
            // Every pointer sample of the frame, not just the last, so fast strokes keep their shape.
            let samples: Vec<Pos2> = ui.input(|i| {
                i.events
                    .iter()
                    .filter_map(|event| match event {
                        egui::Event::PointerMoved(position) => Some(*position),
                        _ => None,
                    })
                    .collect()
            });
            for sample in samples {
                update_drag(editor, &ctx, page, sample, modifiers);
            }
        }
        update_drag(editor, &ctx, page, screen, modifiers);
    }

    if editor.drag.is_some() && !ui.input(|i| i.pointer.any_down()) {
        finish_drag(editor, &ctx, page);
    }

    // Space held only pans: clicks don't select or start editing blocks.
    if !space_down
        && response.double_clicked()
        && editor.layer.edits_blocks()
        && let Some(screen) = pointer_pos
        && let Some(block) = hit_block(page, editor.view.to_page(screen))
    {
        let id = block.id.clone();
        editor.start_inline_edit(&id);
    } else if !space_down
        && response.clicked()
        && let Some(screen) = pointer_pos
    {
        click(editor, page, editor.view.to_page(screen), modifiers);
    }

    if (response.hovered() || editor.drag.is_some())
        && let Some(cursor) = stage_cursor(editor, page, response.hover_pos(), modifiers, space_down || middle_down)
    {
        ctx.set_cursor_icon(cursor);
    }
}

/// The pointer over the canvas says what a press would do: the hand pans, the magnifier
/// zooms (out with ⌥), the cross draws a range or paints, the move arrows drag a block.
fn stage_cursor(
    editor: &Editor,
    page: &PageRecord,
    hover: Option<Pos2>,
    modifiers: Modifiers,
    panning: bool,
) -> Option<egui::CursorIcon> {
    use egui::CursorIcon;
    let zoom = if modifiers.alt { CursorIcon::ZoomOut } else { CursorIcon::ZoomIn };
    match &editor.drag {
        Some(Drag::Pan { .. }) => return Some(CursorIcon::Grabbing),
        Some(Drag::Zoom { .. }) => return Some(zoom),
        Some(Drag::Move { .. }) => return Some(CursorIcon::Move),
        Some(Drag::Resize { handle, .. }) => return Some(handle.cursor()),
        Some(Drag::Rotate { .. }) => return Some(CursorIcon::Grabbing),
        Some(Drag::Range { .. } | Drag::BlockRange { .. }) => return Some(CursorIcon::Crosshair),
        Some(Drag::RangeMove { .. }) => return Some(CursorIcon::Move),
        Some(Drag::Paint { .. } | Drag::AutoErase { .. } | Drag::ResultPaint { .. } | Drag::Heal { .. }) => {
            return Some(CursorIcon::Crosshair);
        }
        Some(Drag::Idle) | None => {}
    }
    if panning {
        return Some(CursorIcon::Grab);
    }
    let painting_blocked = editor.inpaint_busy() || editor.page_edit_locked();
    match editor.tool {
        Tool::Zoom => return Some(zoom),
        Tool::Range => {
            // Inside the inpaint range a drag moves it.
            let inside = !editor.layer.edits_blocks()
                && !modifiers.shift
                && !modifiers.alt
                && editor.range.zip(hover).is_some_and(|(range, pos)| range.contains(editor.view.to_page(pos)));
            return Some(if inside { CursorIcon::Move } else { CursorIcon::Crosshair });
        }
        Tool::MaskBrush | Tool::MaskEraser | Tool::MaskAutoEraser | Tool::Result => {
            // A blocked press pans instead of painting.
            return Some(if painting_blocked { CursorIcon::Grab } else { CursorIcon::Crosshair });
        }
        Tool::Select => {}
    }
    if !editor.layer.edits_blocks() || editor.page_edit_locked() {
        return Some(CursorIcon::Grab);
    }
    // Same order as `start_drag`: Alt redraw, rotate knob, resize handles, block, empty.
    if modifiers.alt && !modifiers.command && !editor.selection.is_empty() {
        return Some(CursorIcon::Crosshair);
    }
    let screen = hover?;
    if editor.selection.len() == 1
        && let Some(block) = page.blocks.iter().find(|block| block.id == editor.selection[0])
    {
        if editor.view.to_screen(rotate_handle_position(block, page, editor.view.zoom)).distance(screen) <= HANDLE_SIZE
        {
            return Some(CursorIcon::Grab);
        }
        if let Some((handle, _)) = handle_positions(block, page)
            .into_iter()
            .find(|(_, position)| editor.view.to_screen(*position).distance(screen) <= HANDLE_SIZE)
        {
            return Some(handle.cursor());
        }
    }
    Some(if hit_block(page, editor.view.to_page(screen)).is_some() { CursorIcon::Move } else { CursorIcon::Grab })
}

fn start_drag(
    editor: &mut Editor,
    ctx: &egui::Context,
    page: &PageRecord,
    screen: Pos2,
    point: Pos2,
    modifiers: Modifiers,
    pan: bool,
) -> Drag {
    if pan {
        return Drag::Pan { last: screen };
    }
    // `inpaintDisabled`: no layer painting while an inpaint runs (its result replaces the
    // layers) or while the page waits for translation.
    let layer_tool = matches!(editor.tool, Tool::MaskBrush | Tool::MaskEraser | Tool::MaskAutoEraser | Tool::Result);
    if layer_tool && (editor.inpaint_busy() || editor.page_edit_locked()) {
        return Drag::Pan { last: screen };
    }
    match editor.tool {
        Tool::Zoom => return Drag::Zoom { start: screen, start_zoom: editor.view.zoom, anchor: screen },
        Tool::MaskBrush | Tool::MaskEraser if editor.layer == Layer::InpaintMask => {
            if let Some(PageAssets::Ready(loaded)) = editor.assets.get_mut(&page.id) {
                let before = StrokeBackup::new(loaded.width(), loaded.height(), 1);
                let start = line_start(editor, &page.id, modifiers).unwrap_or(point);
                let mut drag = Drag::Paint { last: start, erase: editor.tool == Tool::MaskEraser, before, dirty: None };
                paint_to(editor, ctx, &page.id, &mut drag, point);
                return drag;
            }
            return Drag::Pan { last: screen };
        }
        Tool::Range => {
            if !editor.layer.edits_blocks()
                && !modifiers.shift
                && !modifiers.alt
                && let Some(range) = editor.range
                && range.contains(point)
            {
                return Drag::RangeMove { grab: point, original: range };
            }
            return Drag::Range {
                start: point,
                current: point,
                additive: modifiers.shift,
                square: false,
                centered: false,
            };
        }
        Tool::MaskAutoEraser if editor.layer == Layer::InpaintMask => {
            if let Some(PageAssets::Ready(loaded)) = editor.assets.get_mut(&page.id) {
                if loaded.mask.is_some() {
                    loaded.mask_selection = Some(vec![0u8; (loaded.width() * loaded.height()) as usize]);
                }
                let mut drag = Drag::AutoErase { last: point, dirty: None };
                auto_erase_to(editor, ctx, &page.id, &mut drag, point);
                return drag;
            }
            return Drag::Pan { last: screen };
        }
        Tool::Result if editor.layer == Layer::InpaintResult => {
            if let Some(PageAssets::Ready(loaded)) = editor.assets.get_mut(&page.id) {
                if editor.result_tool == super::result_tools::ResultTool::ColorPicker {
                    // Samples what the final output shows and stays in picker mode.
                    editor.sample_screen(super::color::SampleTarget::ResultBrush, screen);
                    return Drag::Idle;
                }
                let (width, height) = (loaded.width(), loaded.height());
                if editor.result_tool == super::result_tools::ResultTool::SmartBrush {
                    let start = line_start(editor, &page.id, modifiers);
                    let mask = image::GrayImage::new(width, height);
                    let mut drag = Drag::Heal { last: start.unwrap_or(point), mask, bounds: None };
                    heal_to(editor, &mut drag, start.unwrap_or(point));
                    heal_to(editor, &mut drag, point);
                    return drag;
                }
                let before_result = StrokeBackup::new(width, height, 4);
                let before_mask = StrokeBackup::new(width, height, 1);
                // The smoothing string is measured on screen, like Photoshop's.
                let string = editor.result_brush.smoothing.clamp(0.0, 1.0) * RESULT_SMOOTHING_REACH / editor.view.zoom;
                let aliased = !editor.result_brush.anti_alias;
                let source = Some(std::sync::Arc::clone(&loaded.source));
                let stroke = Box::new(super::result_tools::Stroke::new(string, aliased, source));
                let start = line_start(editor, &page.id, modifiers);
                let mut drag =
                    Drag::ResultPaint { last: start.unwrap_or(point), stroke, before_result, before_mask, dirty: None };
                if let Some(start) = start {
                    paint_result_to(editor, ctx, &page.id, &mut drag, start);
                }
                paint_result_to(editor, ctx, &page.id, &mut drag, point);
                return drag;
            }
            return Drag::Pan { last: screen };
        }
        _ => {}
    }
    if !editor.layer.edits_blocks() || editor.page_edit_locked() {
        return Drag::Pan { last: screen };
    }
    // Alt+drag anywhere redraws the selected block's box.
    if modifiers.alt
        && !modifiers.command
        && let Some(id) = editor.selection.first().cloned()
    {
        return Drag::BlockRange { id, start: point, current: point };
    }
    if editor.page_edit_locked() {
        return Drag::Pan { last: screen };
    }
    // Rotate handle of the single selected block.
    if editor.selection.len() == 1
        && let Some(block) = page.blocks.iter().find(|block| block.id == editor.selection[0])
        && editor.view.to_screen(rotate_handle_position(block, page, editor.view.zoom)).distance(screen) <= HANDLE_SIZE
    {
        let center = block_rect(block, page).center();
        return Drag::Rotate {
            id: block.id.clone(),
            center,
            start_angle: angle_deg(center, point),
            start_rotation: resolve_block_rotation_deg(block),
        };
    }
    // Resize handle of the single selected block.
    if editor.selection.len() == 1
        && let Some(block) = page.blocks.iter().find(|block| block.id == editor.selection[0])
        && let Some((handle, _)) = handle_positions(block, page)
            .into_iter()
            .find(|(_, position)| editor.view.to_screen(*position).distance(screen) <= HANDLE_SIZE)
    {
        let id = block.id.clone();
        return Drag::Resize {
            id,
            handle,
            start: point,
            original: editable_block_bbox(block),
            rotation: resolve_block_rotation_deg(block) as f32,
        };
    }
    match hit_block(page, point) {
        Some(block) => {
            let id = block.id.clone();
            if modifiers.shift && !modifiers.alt && !modifiers.command && toggle_block_selection(editor, &id) {
                return Drag::Idle;
            }
            if !editor.selection.contains(&id) {
                // Dragging an unselected grouped block moves its whole group
                // (`resolveTranslationBlockDragBlockIds`).
                editor.select_block_or_group(&id);
            }
            bring_to_front(editor, &editor.selection.clone());
            let page = editor.page().cloned().unwrap_or_default();
            let originals = page
                .blocks
                .iter()
                .filter(|block| editor.selection.contains(&block.id))
                .map(|block| (block.id.clone(), block.bbox, block.render_bbox))
                .collect();
            Drag::Move { start: point, originals }
        }
        None => {
            // Pointer on empty stage: deselect and pan (`onStagePointerDown`).
            editor.selection.clear();
            Drag::Pan { last: screen }
        }
    }
}

/// `resolveShiftSelectedTranslationBlockIds`: Shift+click toggles a block in the
/// selection. Returns false when there is nothing to toggle against (plain select).
fn toggle_block_selection(editor: &mut Editor, id: &str) -> bool {
    let mut next = editor.selection.clone();
    match next.iter().position(|selected| selected == id) {
        Some(index) => {
            next.remove(index);
        }
        None => next.push(id.to_owned()),
    }
    if editor.selection.is_empty() && next.len() <= 1 {
        return false;
    }
    editor.selection = next;
    true
}

/// `bringTranslationBlocksToFront`: moved blocks go to the end of the block order.
fn bring_to_front(editor: &mut Editor, ids: &[String]) {
    let Some(page) = editor.page() else { return };
    let already_last = page.blocks.iter().rev().take(ids.len()).all(|block| ids.contains(&block.id));
    if already_last {
        return;
    }
    if let Some(page) = editor.page_mut() {
        let (moved, mut rest): (Vec<_>, Vec<_>) = page.blocks.drain(..).partition(|block| ids.contains(&block.id));
        rest.extend(moved);
        page.blocks = rest;
    }
    editor.touch_page();
}

fn update_drag(editor: &mut Editor, ctx: &egui::Context, page: &PageRecord, screen: Pos2, modifiers: Modifiers) {
    let point = editor.view.to_page(screen);
    let Some(mut drag) = editor.drag.take() else { return };
    match &mut drag {
        Drag::Pan { last } => {
            editor.view.offset += screen - *last;
            if screen != *last {
                editor.view.auto_fit = false;
            }
            *last = screen;
        }
        Drag::Zoom { start, start_zoom, anchor } => {
            // `resolveZoomToolDragScale`: doubles every 240px to the right.
            let factor = 2f32.powf((screen.x - start.x) / 240.0);
            let target = (*start_zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
            editor.view.zoom_at(*anchor, target / editor.view.zoom);
        }
        Drag::Range { start, current, square, centered, .. } => {
            if !editor.layer.edits_blocks() {
                if ctx.input(|i| i.key_down(Key::Space)) {
                    *start += point - *current;
                }
                *square = modifiers.shift;
                *centered = modifiers.alt;
            }
            *current = point;
        }
        Drag::BlockRange { current, .. } => *current = point,
        Drag::RangeMove { grab, original } => {
            editor.range = Some(moved_range(*original, (point - *grab).round(), page_size(page)));
        }
        Drag::Idle => {}
        Drag::Rotate { id, center, start_angle, start_rotation } => {
            // `clampRotationDeg(start + angle - startAngle)`.
            let next = mtt_core::geometry::clamp_rotation_deg(Some(
                *start_rotation + f64::from(angle_deg(*center, point) - *start_angle),
            ));
            let id = id.clone();
            if let Some(page) = editor.page_mut()
                && let Some(block) = page.blocks.iter_mut().find(|block| block.id == id)
            {
                block.rotation_deg = Some(next);
            }
            editor.touch_page();
        }
        Drag::Move { start, originals } => {
            let (width, height) = (f64::from(page.width.max(1)), f64::from(page.height.max(1)));
            let dx = f64::from(point.x - start.x) / width * 1000.0;
            // Shift locks the vertical position.
            let dy = if modifiers.shift { 0.0 } else { f64::from(point.y - start.y) / height * 1000.0 };
            let originals = originals.clone();
            if let Some(page) = editor.page_mut() {
                for (id, bbox, render) in &originals {
                    if let Some(block) = page.blocks.iter_mut().find(|block| &block.id == id) {
                        block.bbox = *bbox;
                        block.render_bbox = *render;
                        mtt_core::geometry::offset_block_bboxes(block, dx, dy);
                    }
                }
            }
            editor.touch_page();
        }
        Drag::Resize { id, handle, start, original, rotation } => {
            let (width, height) = (f64::from(page.width.max(1)), f64::from(page.height.max(1)));
            let delta = rotate_around(point, *start, -*rotation) - *start;
            let mut px = bbox_to_pixels(*original, width, height);
            let (u, v) = handle.anchor();
            let (dx, dy) = (f64::from(delta.x), f64::from(delta.y));
            if u == 0.0 {
                px.x += dx;
                px.w -= dx;
            } else if u == 1.0 {
                px.w += dx;
            }
            if v == 0.0 {
                px.y += dy;
                px.h -= dy;
            } else if v == 1.0 {
                px.h += dy;
            }
            if px.w < 4.0 {
                px.w = 4.0;
            }
            if px.h < 4.0 {
                px.h = 4.0;
            }
            let next = mtt_core::geometry::pixels_to_editable_bbox(px, width, height);
            let id = id.clone();
            if let Some(page) = editor.page_mut()
                && let Some(block) = page.blocks.iter_mut().find(|block| block.id == id)
            {
                apply_editable_block_bbox(block, next);
            }
            editor.touch_page();
        }
        Drag::Paint { .. } => {
            let page_id = page.id.clone();
            paint_to(editor, ctx, &page_id, &mut drag, point);
            ctx.request_repaint();
        }
        Drag::ResultPaint { .. } => {
            let page_id = page.id.clone();
            paint_result_to(editor, ctx, &page_id, &mut drag, point);
            ctx.request_repaint();
        }
        Drag::AutoErase { .. } => {
            let page_id = page.id.clone();
            auto_erase_to(editor, ctx, &page_id, &mut drag, point);
            ctx.request_repaint();
        }
        Drag::Heal { .. } => {
            heal_to(editor, &mut drag, point);
            ctx.request_repaint();
        }
    }
    editor.drag = Some(drag);
}

/// The area the smart brush will fill, tinted evenly: one quad per run of pixels in a row,
/// so nothing overlaps and darkens.
fn heal_preview(editor: &Editor, mask: &image::GrayImage, bounds: (u32, u32, u32, u32)) -> egui::Shape {
    let fill = Color32::from_rgba_unmultiplied(236, 72, 153, 110);
    let mut mesh = egui::Mesh::default();
    let (x0, y0, width, height) = bounds;
    for y in y0..y0 + height {
        let mut x = x0;
        while x < x0 + width {
            if mask.get_pixel(x, y)[0] == 0 {
                x += 1;
                continue;
            }
            let start = x;
            while x < x0 + width && mask.get_pixel(x, y)[0] > 0 {
                x += 1;
            }
            let min = editor.view.to_screen(Pos2::new(start as f32, y as f32));
            let max = editor.view.to_screen(Pos2::new(x as f32, (y + 1) as f32));
            mesh.add_colored_rect(Rect::from_min_max(min, max), fill);
        }
    }
    egui::Shape::mesh(mesh)
}

/// Adds the segment to the smart brush's heal area.
fn heal_to(editor: &Editor, drag: &mut Drag, point: Pos2) {
    let Drag::Heal { last, mask, bounds } = drag else { return };
    if bounds.is_some() && last.distance(point) < 0.5 {
        return;
    }
    let diameter =
        editor.result_brush.size.clamp(super::result_tools::RESULT_BRUSH_MIN, super::result_tools::RESULT_BRUSH_MAX);
    let mut touched = stroke_mask(mask, (last.x, last.y), (point.x, point.y), diameter, 255);
    if let Some(clip) = editor.paint_clip(mask.width(), mask.height())
        && let Some(rect) = touched
    {
        clear_outside(mask, rect, clip);
        touched = intersect_pixels(rect, clip);
    }
    *bounds = super::result_tools::union(*bounds, touched);
    *last = point;
}

/// Zeroes the pixels of `rect` outside `keep`.
fn clear_outside(mask: &mut image::GrayImage, rect: (u32, u32, u32, u32), keep: (u32, u32, u32, u32)) {
    for y in rect.1..rect.1 + rect.3 {
        for x in rect.0..rect.0 + rect.2 {
            let inside = (keep.0..keep.0 + keep.2).contains(&x) && (keep.1..keep.1 + keep.3).contains(&y);
            if !inside {
                mask.put_pixel(x, y, image::Luma([0]));
            }
        }
    }
}

fn intersect_pixels(a: (u32, u32, u32, u32), b: (u32, u32, u32, u32)) -> Option<(u32, u32, u32, u32)> {
    let (left, top) = (a.0.max(b.0), a.1.max(b.1));
    let (right, bottom) = ((a.0 + a.2).min(b.0 + b.2), (a.1 + a.3).min(b.1 + b.3));
    (right > left && bottom > top).then(|| (left, top, right - left, bottom - top))
}

/// Where a Shift+click stroke starts: the end of the last stroke on this page.
fn line_start(editor: &Editor, page_id: &str, modifiers: Modifiers) -> Option<Pos2> {
    let (page, end) = editor.last_stroke_end.as_ref()?;
    (modifiers.shift_only() && page == page_id).then_some(*end)
}

fn paint_to(editor: &mut Editor, ctx: &egui::Context, page_id: &str, drag: &mut Drag, point: Pos2) {
    let Drag::Paint { last, erase, before, dirty } = drag else { return };
    let Some((width, height)) = editor.loaded_page().map(|page| (page.width(), page.height())) else { return };
    let clip = editor.paint_clip(width, height);
    let Some(PageAssets::Ready(loaded)) = editor.assets.get_mut(page_id) else { return };
    let diameter = editor.brush_size.clamp(MASK_BRUSH_MIN, MASK_BRUSH_MAX);
    let value = if *erase { 0 } else { 255 };
    let mask = loaded.ensure_mask();
    let segment = ((last.x, last.y), (point.x, point.y));
    if let Some(rect) = super::assets::stroke_bounds(mask.width(), mask.height(), segment.0, segment.1, diameter) {
        before.save(mask.as_raw(), rect);
    }
    let touched = stroke_mask(mask, segment.0, segment.1, diameter, value);
    if let (Some(rect), Some(clip)) = (touched, clip) {
        before.restore_outside(mask, rect, clip);
    }
    *last = point;
    if let Some((x, y, w, h)) = touched {
        *dirty = Some(match *dirty {
            None => (x, y, w, h),
            Some((dx, dy, dw, dh)) => {
                let (left, top) = (dx.min(x), dy.min(y));
                let (right, bottom) = ((dx + dw).max(x + w), (dy + dh).max(y + h));
                (left, top, right - left, bottom - top)
            }
        });
        loaded.refresh_layer_region(ctx, page_id, x, y, w, h);
    }
}

/// Picks islands along the segment (steps of a third of the brush, like the TS app) and
/// previews them through the mask texture.
fn auto_erase_to(editor: &mut Editor, ctx: &egui::Context, page_id: &str, drag: &mut Drag, point: Pos2) {
    let Drag::AutoErase { last, dirty } = drag else { return };
    let Some(PageAssets::Ready(loaded)) = editor.assets.get_mut(page_id) else { return };
    let loaded = &mut **loaded;
    let (Some(mask), Some(selected)) = (loaded.mask.as_deref(), loaded.mask_selection.as_mut()) else { return };
    let diameter = editor.brush_size.clamp(MASK_BRUSH_MIN, MASK_BRUSH_MAX);
    let distance = last.distance(point);
    let steps = (distance / (diameter / 3.0).max(1.0)).ceil().max(1.0) as u32;
    let mut touched = None;
    for step in 0..=steps {
        let t = step as f32 / steps as f32;
        let at = (last.x + (point.x - last.x) * t, last.y + (point.y - last.y) * t);
        touched = super::result_tools::union(touched, super::assets::select_mask_islands(mask, selected, at, diameter));
    }
    *last = point;
    if let Some((x, y, w, h)) = touched {
        *dirty = super::result_tools::union(*dirty, touched);
        loaded.refresh_layer_region(ctx, page_id, x, y, w, h);
    }
}

/// Feeds a pointer sample to the result stroke, or with `finish` lets the brush catch up
/// with the pointer at release.
fn paint_result_to(editor: &mut Editor, ctx: &egui::Context, page_id: &str, drag: &mut Drag, point: Pos2) {
    advance_result_stroke(editor, ctx, page_id, drag, point, false);
}

fn advance_result_stroke(
    editor: &mut Editor,
    ctx: &egui::Context,
    page_id: &str,
    drag: &mut Drag,
    point: Pos2,
    finish: bool,
) {
    let Drag::ResultPaint { last, stroke, dirty, before_result, before_mask } = drag else { return };
    if !finish && last.distance(point) < 0.25 && dirty.is_some() {
        return;
    }
    let Some((width, height)) = editor.loaded_page().map(|page| (page.width(), page.height())) else { return };
    let clip = editor.paint_clip(width, height);
    let Some(PageAssets::Ready(loaded)) = editor.assets.get_mut(page_id) else { return };
    let (result, mask) = loaded.ensure_layers();
    let (tool, settings) = (editor.result_tool, &editor.result_brush);
    let sample = (point.x, point.y);
    if let Some(rect) = stroke.reach(Some(sample), settings, width, height) {
        before_result.save(result.as_raw(), rect);
        before_mask.save(mask.as_raw(), rect);
    }
    let touched = if finish {
        stroke.finish(tool, result, mask, Some(sample), settings)
    } else {
        stroke.add_point(tool, result, mask, sample, settings)
    };
    if let (Some(rect), Some(clip)) = (touched, clip) {
        before_result.restore_outside(result, rect, clip);
        before_mask.restore_outside(mask, rect, clip);
    }
    *last = point;
    if let Some((x, y, w, h)) = touched {
        *dirty = super::result_tools::union(*dirty, touched);
        loaded.refresh_layer_region(ctx, page_id, x, y, w, h);
    }
}

fn finish_drag(editor: &mut Editor, ctx: &egui::Context, page: &PageRecord) {
    let Some(mut drag) = editor.drag.take() else { return };
    if let Drag::ResultPaint { last, .. } = drag {
        advance_result_stroke(editor, ctx, &page.id, &mut drag, last, true);
    }
    if let Drag::Paint { last, .. } | Drag::ResultPaint { last, .. } | Drag::Heal { last, .. } = drag {
        editor.last_stroke_end = Some((page.id.clone(), last));
    }
    if let Drag::Heal { mask, .. } = drag {
        // `tick` has the worker to start the fill with.
        if mask.iter().any(|value| *value > 0) {
            editor.heal_stroke = Some(super::HealStroke {
                page_id: page.id.clone(),
                mask: std::sync::Arc::new(mask),
                keep_mask: editor.result_brush.keep_mask,
            });
            editor.heal_requested = true;
            ctx.request_repaint();
        }
        return;
    }
    match drag {
        Drag::Range { start, current, additive, square, centered } => {
            if !editor.layer.edits_blocks() {
                // Inpaint range selection on layers 3/4 (`inpaintSelectionRect`).
                editor.range = marquee_rect(start, current, square, centered, page_size(page));
                return;
            }
            let rect = Rect::from_two_pos(start, current)
                .intersect(Rect::from_min_size(Pos2::ZERO, egui::vec2(page.width as f32, page.height as f32)));
            let tiny = rect.width() < 2.0 || rect.height() < 2.0;
            if tiny {
                if !additive {
                    editor.selection.clear();
                }
                return;
            }
            // `resolveTranslationBlockIdsInSelection`: visible blocks touching the rect.
            let inside: Vec<String> = page
                .blocks
                .iter()
                .filter(|block| block.render_direction != mtt_core::model::RenderTextDirection::Hidden)
                .filter(|block| rect.intersects(block_rect(block, page)))
                .map(|block| block.id.clone())
                .collect();
            if additive {
                for id in inside {
                    match editor.selection.iter().position(|selected| *selected == id) {
                        Some(index) => {
                            editor.selection.remove(index);
                        }
                        None => editor.selection.push(id),
                    }
                }
            } else {
                editor.selection = inside;
            }
        }
        Drag::BlockRange { id, start, current } => {
            let rect = Rect::from_two_pos(start, current);
            if rect.width() < 2.0 || rect.height() < 2.0 {
                return;
            }
            let (width, height) = (f64::from(page.width.max(1)), f64::from(page.height.max(1)));
            let next = BBox::new(
                f64::from(rect.min.x) / width * 1000.0,
                f64::from(rect.min.y) / height * 1000.0,
                f64::from(rect.width()) / width * 1000.0,
                f64::from(rect.height()) / height * 1000.0,
            );
            editor.commit_checkpoint();
            if let Some(page) = editor.page_mut()
                && let Some(block) = page.blocks.iter_mut().find(|block| block.id == id)
            {
                apply_editable_block_bbox(block, next);
            }
            editor.selection = vec![id];
            editor.touch_page();
            editor.revision += 1;
            editor.commit_checkpoint();
        }
        Drag::Paint { before, dirty: Some(rect), .. } => {
            let Some(PageAssets::Ready(loaded)) = editor.assets.get(&page.id) else { return };
            let Some(mask) = &loaded.mask else { return };
            let after = read_rect(mask.as_raw(), loaded.width(), 1, rect);
            let patch = LayerPatch::new(rect, &before.before(mask.as_raw(), rect), &after);
            editor.history.push(Entry::Layer { page_id: page.id.clone(), layer: LayerKind::Mask, patch });
            editor.layer_dirty.insert((page.id.clone(), LayerKind::Mask), std::time::Instant::now());
        }
        Drag::AutoErase { dirty, .. } => {
            let Some(PageAssets::Ready(loaded)) = editor.assets.get_mut(&page.id) else { return };
            let selected = loaded.mask_selection.take();
            let (Some(rect), Some(selected)) = (dirty, selected) else {
                return;
            };
            let width = loaded.width();
            let mask = loaded.ensure_mask();
            let before = read_rect(mask.as_raw(), width, 1, rect);
            let raw: &mut [u8] = mask;
            // Picks only happen inside the touched rect.
            for row in rect.1..rect.1 + rect.3 {
                let start = (row * width + rect.0) as usize;
                let end = start + rect.2 as usize;
                for (value, picked) in raw[start..end].iter_mut().zip(&selected[start..end]) {
                    if *picked != 0 {
                        *value = 0;
                    }
                }
            }
            let after = read_rect(mask.as_raw(), width, 1, rect);
            loaded.refresh_layer_region(ctx, &page.id, rect.0, rect.1, rect.2, rect.3);
            let patch = LayerPatch::new(rect, &before, &after);
            editor.history.push(Entry::Layer { page_id: page.id.clone(), layer: LayerKind::Mask, patch });
            editor.layer_dirty.insert((page.id.clone(), LayerKind::Mask), std::time::Instant::now());
        }
        Drag::ResultPaint { before_result, before_mask, dirty: Some(rect), .. } => {
            let Some(PageAssets::Ready(loaded)) = editor.assets.get(&page.id) else { return };
            let (Some(result), Some(mask)) = (&loaded.result, &loaded.mask) else { return };
            let read = |data: &[u8], channels: u32| read_rect(data, loaded.width(), channels, rect);
            let mut entries = Vec::new();
            let (before, after) = (before_result.before(result.as_raw(), rect), read(result.as_raw(), 4));
            if before != after {
                let patch = LayerPatch::new(rect, &before, &after);
                entries.push(Entry::Layer { page_id: page.id.clone(), layer: LayerKind::Result, patch });
            }
            let (before, after) = (before_mask.before(mask.as_raw(), rect), read(mask.as_raw(), 1));
            if before != after {
                let patch = LayerPatch::new(rect, &before, &after);
                entries.push(Entry::Layer { page_id: page.id.clone(), layer: LayerKind::Mask, patch });
                editor.layer_dirty.insert((page.id.clone(), LayerKind::Mask), std::time::Instant::now());
            }
            if !entries.is_empty() {
                editor.history.push(Entry::Batch { page_id: page.id.clone(), entries });
                editor.layer_dirty.insert((page.id.clone(), LayerKind::Result), std::time::Instant::now());
            }
        }
        Drag::Move { .. } | Drag::Resize { .. } | Drag::Rotate { .. } => {
            // Re-render moved blocks at their final position.
            editor.revision += 1;
            editor.commit_checkpoint();
        }
        _ => {}
    }
}

fn click(editor: &mut Editor, page: &PageRecord, point: Pos2, modifiers: Modifiers) {
    if editor.tool == Tool::Range {
        if editor.layer.edits_blocks() {
            if !modifiers.shift {
                editor.selection.clear();
            }
        } else {
            editor.range = None;
        }
        return;
    }
    if !editor.layer.edits_blocks() || editor.tool == Tool::Zoom {
        return;
    }
    match hit_block(page, point) {
        Some(block) => {
            let id = block.id.clone();
            if modifiers.command {
                if editor.page_edit_locked() {
                    return;
                }
                // Ctrl/Cmd+click duplicates the block.
                editor.selection = vec![id];
                editor.duplicate_selected();
            } else if modifiers.shift && toggle_block_selection(editor, &id) {
            } else {
                editor.select_block_or_group(&id);
            }
        }
        None => editor.selection.clear(),
    }
}

type ShortcutAction = Box<dyn FnOnce(&mut Editor, &mut Services)>;

/// Keys while the shortcut sheet is open: Esc and `?` close it, nothing reaches the page
/// behind it.
pub fn shortcut_sheet_keys(editor: &mut Editor, ctx: &egui::Context) {
    let close = ctx.input_mut(|input| {
        input.consume_key(Modifiers::NONE, Key::Escape)
            || input.events.iter().any(|event| matches!(event, egui::Event::Text(text) if text == "?"))
    });
    if close {
        editor.shortcut_sheet = false;
    }
}

/// Editor keyboard shortcuts (only while no text field has focus).
pub fn shortcuts(editor: &mut Editor, ctx: &egui::Context, services: &mut Services) {
    let one_hand = services.settings.one_hand_mode;
    let nudge = editor.tool == Tool::Range && editor.range.is_some() && editor.mode() == super::Mode::Inpaint;
    let mut actions: Vec<ShortcutAction> = Vec::new();
    ctx.input_mut(|input| {
        let command = Modifiers::COMMAND;
        let command_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        if input.consume_key(Modifiers::NONE, Key::Escape) {
            // Esc leaves the zoom tool, then the range tool.
            actions.push(Box::new(|editor, _| {
                editor.escape_tool();
            }));
        }
        if input.events.iter().any(|event| matches!(event, egui::Event::Text(text) if text == "?")) {
            actions.push(Box::new(|editor, _| editor.shortcut_sheet = true));
        }
        if input.consume_key(command_shift, Key::Z) || input.consume_key(command, Key::Y) {
            let ctx = ctx.clone();
            actions.push(Box::new(move |editor, _| {
                if !editor.page_edit_locked() {
                    editor.redo(&ctx)
                }
            }));
        }
        if input.consume_key(command, Key::Z) {
            let ctx = ctx.clone();
            actions.push(Box::new(move |editor, _| {
                if !editor.page_edit_locked() {
                    editor.undo(&ctx)
                }
            }));
        }
        if input.consume_key(command, Key::C) || input.events.iter().any(|e| matches!(e, egui::Event::Copy)) {
            let ctx = ctx.clone();
            actions.push(Box::new(move |editor, _| editor.copy_selected(&ctx)));
        }
        // eframe turns ⌘V into a paste event carrying the system clipboard text.
        let pasted = input.events.iter().find_map(|event| match event {
            egui::Event::Paste(text) => Some(text.clone()),
            _ => None,
        });
        if pasted.is_some() || input.consume_key(command, Key::V) {
            actions.push(Box::new(move |editor, _| {
                if !editor.page_edit_locked() {
                    editor.paste(pasted.as_deref())
                }
            }));
        }
        // On the inpaint layers ⌘D drops the range and ⌘A (with T) selects the page, like
        // Photoshop.
        if input.consume_key(command, Key::D) {
            actions.push(Box::new(|editor, _| {
                if editor.mode() == super::Mode::Inpaint && editor.range.is_some() {
                    editor.range = None;
                } else if !editor.page_edit_locked() {
                    editor.duplicate_selected()
                }
            }));
        }
        if input.consume_key(command, Key::A) {
            actions.push(Box::new(|editor, _| {
                if editor.mode() == super::Mode::Inpaint && editor.tool == Tool::Range {
                    editor.range = editor.page().map(|page| Rect::from_min_size(Pos2::ZERO, page_size(page)));
                } else {
                    editor.select_all()
                }
            }));
        }
        if input.consume_key(command, Key::F) {
            actions.push(Box::new(|editor, _| editor.find.open = true));
        }
        if input.consume_key(command, Key::S) {
            actions.push(Box::new(|editor, _| {
                editor.last_change = Some(std::time::Instant::now() - super::AUTOSAVE_DELAY)
            }));
        }
        if input.consume_key(command, Key::Num0) {
            actions.push(Box::new(|editor, _| editor.view.fit_requested = true));
        }
        if input.consume_key(command, Key::Equals) || input.consume_key(command, Key::Plus) {
            actions.push(Box::new(|editor, _| editor.zoom_step(true)));
        }
        if input.consume_key(command, Key::Minus) {
            actions.push(Box::new(|editor, _| editor.zoom_step(false)));
        }
        // `resolveInpaintToolShortcutAction` (Alt+E auto eraser).
        if input.consume_key(Modifiers::ALT, Key::E) {
            actions.push(Box::new(|editor, _| {
                inpaint_shortcut(editor, InpaintKey::AutoEraser);
            }));
        }
        if input.modifiers.any() && !input.modifiers.shift_only() {
            return;
        }
        for (key, number) in [(Key::Num1, 1), (Key::Num2, 2), (Key::Num3, 3), (Key::Num4, 4), (Key::Num5, 5)] {
            if input.consume_key(Modifiers::NONE, key) {
                actions.push(Box::new(move |editor, _| editor.select_layer_shortcut(number)));
            }
        }
        if input.consume_key(Modifiers::NONE, Key::Z) {
            actions.push(Box::new(|editor, _| editor.select_zoom_tool()));
        }
        if input.consume_key(Modifiers::NONE, Key::A) {
            actions.push(Box::new(|editor, _| editor.select_pointer_tool()));
        }
        if input.consume_key(Modifiers::NONE, Key::T) {
            actions.push(Box::new(|editor, _| {
                if !editor.page_edit_locked() {
                    editor.select_range_tool()
                }
            }));
        }
        // `/` like Photoshop's lock transparent pixels (`?` is the shortcut sheet).
        if input.consume_key(Modifiers::NONE, Key::Slash) {
            actions.push(Box::new(|editor, _| {
                if editor.mode() == super::Mode::Inpaint {
                    editor.toggle_keep_mask();
                }
            }));
        }
        // J like Photoshop's spot healing brush.
        if input.consume_key(Modifiers::NONE, Key::J) {
            actions.push(Box::new(|editor, _| {
                inpaint_shortcut(editor, InpaintKey::SmartBrush);
            }));
        }
        if input.consume_key(Modifiers::NONE, Key::B) {
            actions.push(Box::new(|editor, _| {
                inpaint_shortcut(editor, InpaintKey::Brush);
            }));
        }
        if input.consume_key(Modifiers::NONE, Key::I) {
            actions.push(Box::new(|editor, _| {
                inpaint_shortcut(editor, InpaintKey::ColorPicker);
            }));
        }
        if input.consume_key(Modifiers::NONE, Key::E) {
            actions.push(Box::new(|editor, _| {
                if !inpaint_shortcut(editor, InpaintKey::Eraser)
                    && editor.layer == Layer::Overlay
                    && !editor.final_preview
                    && editor.selection.len() == 1
                {
                    // `BLOCK_INLINE_EDIT_SHORTCUT` on the overlay layer.
                    let id = editor.selection[0].clone();
                    editor.start_inline_edit(&id);
                }
            }));
        }
        // One toggle per press: on a Korean layout ₩ arrives both as text and as the
        // physical Backtick/Backslash key.
        let won =
            input.events.iter().any(|event| matches!(event, egui::Event::Text(text) if text == "₩" || text == "¥"));
        let key =
            input.consume_key(Modifiers::NONE, Key::Backtick) | input.consume_key(Modifiers::NONE, Key::Backslash);
        if won || key {
            actions.push(Box::new(|editor, _| editor.toggle_page_completed()));
        }
        let delete = input.consume_key(Modifiers::NONE, Key::Delete)
            || input.consume_key(Modifiers::NONE, Key::Backspace)
            || (one_hand && input.consume_key(Modifiers::NONE, Key::Q));
        if delete {
            let ctx = ctx.clone();
            actions.push(Box::new(move |editor, _| {
                if !editor.page_edit_locked() {
                    editor.delete_selection_or_range(&ctx)
                }
            }));
        }
        // With T and a range, the arrows nudge it 1px (⇧ 10px) like Photoshop instead of
        // turning the page.
        if nudge {
            for (key, direction) in [
                (Key::ArrowLeft, egui::vec2(-1.0, 0.0)),
                (Key::ArrowRight, egui::vec2(1.0, 0.0)),
                (Key::ArrowUp, egui::vec2(0.0, -1.0)),
                (Key::ArrowDown, egui::vec2(0.0, 1.0)),
            ] {
                let step = if input.consume_key(Modifiers::SHIFT, key) {
                    10.0
                } else if input.consume_key(Modifiers::NONE, key) {
                    1.0
                } else {
                    continue;
                };
                actions.push(Box::new(move |editor, _| {
                    if let (Some(range), Some(page)) = (editor.range, editor.page()) {
                        editor.range = Some(moved_range(range, direction * step, page_size(page)));
                    }
                }));
            }
        }
        // `resolveKeyboardPageNavigation`: ←/→ and D/F; ↑/↓ while the stage has focus.
        if input.consume_key(Modifiers::NONE, Key::D)
            || input.consume_key(Modifiers::NONE, Key::ArrowLeft)
            || input.consume_key(Modifiers::NONE, Key::ArrowUp)
        {
            actions.push(Box::new(|editor, _| editor.previous_page()));
        }
        if input.consume_key(Modifiers::NONE, Key::F)
            || input.consume_key(Modifiers::NONE, Key::ArrowRight)
            || input.consume_key(Modifiers::NONE, Key::ArrowDown)
        {
            actions.push(Box::new(|editor, _| editor.next_page()));
        }
        if input.consume_key(Modifiers::NONE, Key::OpenBracket) {
            actions.push(Box::new(|editor, _| {
                if editor.layer == Layer::InpaintResult {
                    editor.result_brush.size =
                        (editor.result_brush.size - 4.0).max(super::result_tools::RESULT_BRUSH_MIN);
                } else {
                    editor.brush_size = (editor.brush_size - 4.0).max(MASK_BRUSH_MIN);
                }
            }));
        }
        if input.consume_key(Modifiers::NONE, Key::CloseBracket) {
            actions.push(Box::new(|editor, _| {
                if editor.layer == Layer::InpaintResult {
                    editor.result_brush.size =
                        (editor.result_brush.size + 4.0).min(super::result_tools::RESULT_BRUSH_MAX);
                } else {
                    editor.brush_size = (editor.brush_size + 4.0).min(MASK_BRUSH_MAX);
                }
            }));
        }
    });
    for action in actions {
        action(editor, services);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum InpaintKey {
    Brush,
    SmartBrush,
    Eraser,
    AutoEraser,
    ColorPicker,
}

/// `resolveInpaintToolShortcutAction`: brush/smart brush/picker jump to the result layer
/// from anywhere but the mask layer; the others need their (visible) layer. Returns
/// whether the key was used.
fn inpaint_shortcut(editor: &mut Editor, key: InpaintKey) -> bool {
    use super::result_tools::ResultTool;
    if editor.page_edit_locked() {
        return false;
    }
    let visible = |editor: &Editor, child: bool| editor.visibility.inpaint && child;
    let to_result = matches!(key, InpaintKey::ColorPicker | InpaintKey::SmartBrush)
        || (key == InpaintKey::Brush && editor.layer != Layer::InpaintMask);
    if to_result {
        if editor.layer != Layer::InpaintResult {
            editor.set_layer(Layer::InpaintResult);
        }
        if !visible(editor, editor.visibility.inpaint_result) {
            editor.visibility.inpaint = true;
            editor.visibility.inpaint_result = true;
        }
        match key {
            InpaintKey::SmartBrush => editor.select_result_edit_tool(ResultTool::SmartBrush),
            InpaintKey::ColorPicker => editor.select_result_edit_tool(ResultTool::ColorPicker),
            _ => editor.select_shared_inpaint_tool(Tool::MaskBrush),
        }
        return true;
    }
    if editor.layer == Layer::InpaintMask
        && matches!(key, InpaintKey::Brush | InpaintKey::Eraser | InpaintKey::AutoEraser)
        && visible(editor, editor.visibility.inpaint_mask)
    {
        editor.select_shared_inpaint_tool(match key {
            InpaintKey::Brush => Tool::MaskBrush,
            InpaintKey::Eraser => Tool::MaskEraser,
            _ => Tool::MaskAutoEraser,
        });
        return true;
    }
    if editor.layer == Layer::InpaintResult
        && matches!(key, InpaintKey::Eraser)
        && visible(editor, editor.visibility.inpaint_result)
    {
        editor.select_shared_inpaint_tool(Tool::MaskEraser);
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::CursorIcon;
    use mtt_core::model::{Chapter, TranslationBlock};

    fn editor_with_block() -> Editor {
        let page = PageRecord {
            id: "p".into(),
            width: 1000,
            height: 1000,
            blocks: vec![TranslationBlock {
                id: "b".into(),
                bbox: BBox::new(100.0, 100.0, 200.0, 100.0),
                ..TranslationBlock::default()
            }],
            ..PageRecord::default()
        };
        Editor::new(Chapter { id: "c".into(), page_order: vec!["p".into()], pages: vec![page], ..Chapter::default() })
    }

    #[test]
    fn marquee_snaps_to_pixels_and_follows_photoshop_modifiers() {
        let page = egui::vec2(100.0, 80.0);
        let rect = |start: (f32, f32), end: (f32, f32), square, centered| {
            marquee_rect(Pos2::new(start.0, start.1), Pos2::new(end.0, end.1), square, centered, page)
        };
        assert_eq!(
            rect((10.4, 10.6), (30.2, 20.7), false, false),
            Some(Rect::from_min_max(Pos2::new(10.0, 11.0), Pos2::new(30.0, 21.0)))
        );
        assert_eq!(
            rect((50.0, 40.0), (40.0, 45.0), true, false),
            Some(Rect::from_min_max(Pos2::new(40.0, 40.0), Pos2::new(50.0, 50.0))),
            "⇧ keeps the drag direction"
        );
        assert_eq!(
            rect((50.0, 40.0), (60.0, 45.0), false, true),
            Some(Rect::from_min_max(Pos2::new(40.0, 35.0), Pos2::new(60.0, 45.0))),
            "⌥ grows from the press"
        );
        assert_eq!(
            rect((90.0, 70.0), (130.0, 99.0), false, false),
            Some(Rect::from_min_max(Pos2::new(90.0, 70.0), Pos2::new(100.0, 80.0))),
            "clamped to the page"
        );
        assert_eq!(rect((10.0, 10.0), (30.2, 10.4), false, false), None, "no height left");
    }

    #[test]
    fn moving_the_range_keeps_it_on_the_page() {
        let range = Rect::from_min_size(Pos2::new(10.0, 10.0), egui::vec2(20.0, 20.0));
        let page = egui::vec2(100.0, 80.0);
        assert_eq!(moved_range(range, egui::vec2(5.0, -3.0), page).min, Pos2::new(15.0, 7.0));
        assert_eq!(moved_range(range, egui::vec2(500.0, -50.0), page).min, Pos2::new(80.0, 0.0));
        assert_eq!(moved_range(range, egui::vec2(500.0, -50.0), page).size(), range.size());
    }

    #[test]
    fn shift_click_lines_on_from_the_last_stroke_on_the_page() {
        let mut editor = editor_with_block();
        assert_eq!(line_start(&editor, "p", Modifiers::SHIFT), None, "nothing to line on from");
        editor.last_stroke_end = Some(("p".into(), Pos2::new(10.0, 20.0)));
        assert_eq!(line_start(&editor, "p", Modifiers::SHIFT), Some(Pos2::new(10.0, 20.0)));
        assert_eq!(line_start(&editor, "p", Modifiers::NONE), None);
        assert_eq!(line_start(&editor, "p", Modifiers::SHIFT | Modifiers::ALT), None);
        assert_eq!(line_start(&editor, "other", Modifiers::SHIFT), None, "only on the same page");
    }

    #[test]
    fn cursor_shows_what_a_press_would_do() {
        let mut editor = editor_with_block();
        editor.select_layer_shortcut(2);
        editor.view.stage_rect = Rect::from_min_size(Pos2::ZERO, Vec2::splat(1200.0));
        let page = editor.page().cloned().unwrap();
        let block = block_rect(&page.blocks[0], &page);
        let on_block = Some(editor.view.to_screen(block.center()));
        let empty = Some(editor.view.to_screen(block.right_bottom() + Vec2::splat(200.0)));
        let none = Modifiers::NONE;
        let cursor =
            |editor: &Editor, hover, modifiers, panning| stage_cursor(editor, &page, hover, modifiers, panning);

        assert_eq!(cursor(&editor, on_block, none, false), Some(CursorIcon::Move));
        assert_eq!(cursor(&editor, empty, none, false), Some(CursorIcon::Grab), "empty canvas pans");
        assert_eq!(cursor(&editor, on_block, none, true), Some(CursorIcon::Grab), "Space pans");
        editor.selection = vec!["b".into()];
        assert_eq!(cursor(&editor, empty, Modifiers::ALT, false), Some(CursorIcon::Crosshair), "⌥ redraws the box");

        editor.select_zoom_tool();
        assert_eq!(cursor(&editor, on_block, none, false), Some(CursorIcon::ZoomIn));
        assert_eq!(cursor(&editor, on_block, Modifiers::ALT, false), Some(CursorIcon::ZoomOut));
        editor.select_range_tool();
        assert_eq!(cursor(&editor, on_block, none, false), Some(CursorIcon::Crosshair));

        editor.select_layer_shortcut(4);
        editor.select_pointer_tool();
        assert_eq!(cursor(&editor, on_block, none, false), Some(CursorIcon::Grab), "inpaint pointer pans");
        editor.select_shared_inpaint_tool(Tool::MaskBrush);
        assert_eq!(cursor(&editor, on_block, none, false), Some(CursorIcon::Crosshair));

        editor.drag = Some(Drag::Pan { last: Pos2::ZERO });
        assert_eq!(cursor(&editor, on_block, none, false), Some(CursorIcon::Grabbing));
    }
}
