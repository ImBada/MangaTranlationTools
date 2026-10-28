//! Renders one translation block into its own pixmap (`drawRenderedBlock` in `pageRender.ts`).
//!
//! Drawing happens in page pixel space through [`Canvas`]; the pixmap covers the block's
//! dirty rect so blocks can be cached and moved independently in the editor.

use mtt_core::geometry::{parse_hex_color, resolve_block_rotation_deg};
use mtt_core::model::{FontRenderingMode, RenderTextDirection, TextAlign, TextDecoration, TranslationBlock};
use tiny_skia::{FilterQuality, LineCap, Path, PathBuilder, Pattern, Pixmap, Shader, SpreadMode, Transform};

use crate::canvas::{Canvas, circles_path, hex_color, round_rect_path, solid};
use crate::effects::{
    apply_font_filter_effects, filter_effect_margin_px, gaussian_blur, harden_text_pixels, has_active_filter_effects,
};
use crate::fonts::FontContext;
use crate::layout::{
    BLOCK_BORDER_PX, BlockFonts, BlockTextLayout, PixelRect, Size, font_background_enabled, font_background_opacity,
    font_background_radius_px, font_background_scale, is_js_whitespace, letter_spacing_px, measure_text_width,
    resolve_block_text_layout, scaled_rect, screentone_dot_radius_px, screentone_tile_size_px,
    secondary_outline_enabled, synthetic_bold_stroke_width_px, synthetic_italic_skew_x, text_position_factors,
    wrap_text_to_width,
};

const RENDER_FONT_SIZE_RATIO: f64 = 0.985;
const RENDER_Y_OFFSET_RATIO: f64 = 0.04;
const CENTERED_ELLIPSIS: &str = "…";
const CENTERED_PERIOD_ELLIPSIS: &str = "...";
const DEFAULT_OUTLINE_COLOR: &str = "#000000";
const ELLIPSIS_DOT_RADIUS_RATIO: f64 = 0.065;
const ELLIPSIS_DOT_SPACING_RATIO: f64 = 0.26;

/// A rendered block positioned in page pixels.
pub struct BlockRaster {
    pub pixmap: Pixmap,
    /// Page-pixel position of the pixmap's top-left corner.
    pub x: i32,
    pub y: i32,
    /// Device pixels per page pixel; the pixmap covers `pixmap.width() / scale` page pixels.
    pub scale: f32,
    /// The text does not fit its render box (`layout.overflow`).
    pub overflow: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct BlockRenderOptions {
    /// Draw the translucent block background used while editing the overlay layer.
    pub editing_enabled: bool,
    /// Rasterization scale. Layout is always computed at page resolution so a zoomed
    /// preview wraps and fits text exactly like the exported page.
    pub scale: f32,
}

impl Default for BlockRenderOptions {
    fn default() -> Self {
        Self { editing_enabled: false, scale: 1.0 }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeviceRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Page-pixel area a block can paint into, clamped to the page (`resolveBlockCanvasDirtyRect`).
pub fn block_dirty_rect(block: &TranslationBlock, page: Size, layout: &BlockTextLayout) -> Option<DeviceRect> {
    let outline = block.style.outline_width_px.unwrap_or(0.0).max(0.0);
    let secondary = if secondary_outline_enabled(block) {
        block.style.secondary_outline_width_px.unwrap_or(0.0).max(0.0)
    } else {
        0.0
    };
    let shadow_distance = block.style.shadow_distance_px.unwrap_or(0.0).max(0.0);
    let shadow_blur = shadow_blur_px(block);
    let filter_margin = filter_effect_margin_px(block, layout.font_size_px);
    let background_margin = if font_background_enabled(block) && font_background_opacity(block) > 0.0 {
        (layout.fit_inner_width.max(layout.fit_inner_height) * (font_background_scale(block) - 1.0) / 2.0).max(0.0)
    } else {
        0.0
    };
    let margin = 24f64
        .max(background_margin)
        .max(
            layout.font_size_px * 0.45
                + outline * 3.0
                + secondary * 4.0
                + shadow_distance
                + shadow_blur * 3.0
                + filter_margin
                + 8.0,
        )
        .ceil();
    let rect = layout.rect;
    let expanded = PixelRect {
        left: rect.left - margin,
        top: rect.top - margin,
        width: rect.width + margin * 2.0,
        height: rect.height + margin * 2.0,
    };
    let rotation = resolve_block_rotation_deg(block);
    let bounds = if rotation == 0.0 {
        expanded
    } else {
        rotated_bounds(expanded, rect.left + rect.width / 2.0, rect.top + rect.height / 2.0, rotation)
    };
    let x = bounds.left.floor().max(0.0);
    let y = bounds.top.floor().max(0.0);
    let right = (bounds.left + bounds.width).ceil().min(page.width);
    let bottom = (bounds.top + bounds.height).ceil().min(page.height);
    (right > x && bottom > y).then_some(DeviceRect {
        x: x as i32,
        y: y as i32,
        width: (right - x) as u32,
        height: (bottom - y) as u32,
    })
}

fn rotated_bounds(rect: PixelRect, cx: f64, cy: f64, degrees: f64) -> PixelRect {
    let (sin, cos) = degrees.to_radians().sin_cos();
    let corners = [
        (rect.left, rect.top),
        (rect.left + rect.width, rect.top),
        (rect.left + rect.width, rect.top + rect.height),
        (rect.left, rect.top + rect.height),
    ]
    .map(|(x, y)| (cx + (x - cx) * cos - (y - cy) * sin, cy + (x - cx) * sin + (y - cy) * cos));
    let min_x = corners.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
    let max_x = corners.iter().map(|c| c.0).fold(f64::NEG_INFINITY, f64::max);
    let min_y = corners.iter().map(|c| c.1).fold(f64::INFINITY, f64::min);
    let max_y = corners.iter().map(|c| c.1).fold(f64::NEG_INFINITY, f64::max);
    PixelRect { left: min_x, top: min_y, width: max_x - min_x, height: max_y - min_y }
}

fn shadow_blur_px(block: &TranslationBlock) -> f64 {
    let blur = block.style.shadow_blur_px.unwrap_or(0.0);
    if blur.is_finite() { blur.clamp(0.0, 80.0) } else { 0.0 }
}

fn shadow_opacity(block: &TranslationBlock) -> f64 {
    let opacity = block.style.shadow_opacity.unwrap_or(1.0);
    if opacity.is_finite() { opacity.clamp(0.0, 1.0) } else { 1.0 }
}

fn has_visible_shadow(block: &TranslationBlock) -> bool {
    let enabled = block.style.shadow_enabled.unwrap_or(
        block.style.shadow_distance_px.unwrap_or(0.0) > 0.0 || block.style.shadow_blur_px.unwrap_or(0.0) > 0.0,
    );
    enabled && shadow_opacity(block) > 0.0
}

fn outline_width_px(block: &TranslationBlock, font_size: f64) -> f64 {
    block.style.outline_width_px.unwrap_or(0.0).max(0.0).min(font_size * 0.35)
}

fn secondary_outline_width_px(block: &TranslationBlock, font_size: f64) -> f64 {
    if !secondary_outline_enabled(block) {
        return 0.0;
    }
    block.style.secondary_outline_width_px.unwrap_or(0.0).max(0.0).min(font_size * 0.35)
}

/// Renders `block` at page resolution. Returns `None` for hidden blocks or blocks fully
/// outside the page.
pub fn render_block(
    ctx: &mut FontContext,
    page: Size,
    block: &TranslationBlock,
    options: BlockRenderOptions,
) -> Option<BlockRaster> {
    if block.render_direction == RenderTextDirection::Hidden {
        return None;
    }
    let text = block.display_text().to_owned();
    let fonts = BlockFonts::resolve(ctx, block);
    let layout = resolve_block_text_layout(ctx, &fonts, block, &text, page, page);
    let area = block_dirty_rect(block, page, &layout)?;
    let scale = f64::from(options.scale.clamp(0.1, 8.0));
    let (width, height) =
        ((f64::from(area.width) * scale).ceil() as u32, (f64::from(area.height) * scale).ceil() as u32);
    let mut canvas = Canvas::new(width, height)?;
    canvas.scale(scale);
    canvas.translate(-f64::from(area.x), -f64::from(area.y));

    let mut painter = BlockPainter::new(ctx, block, &fonts, &layout, &text);
    painter.device_scale = scale;
    painter.apply_block_transform(&mut canvas);
    if options.editing_enabled {
        canvas.fill_rect(
            layout.rect.left,
            layout.rect.top,
            layout.rect.width,
            layout.rect.height,
            hex_color(&block.background_color, block.opacity),
        );
    }

    let hard = block.style.font_rendering_mode.unwrap_or_default() == FontRenderingMode::Hard;
    if has_active_filter_effects(block) || hard {
        painter.draw(&mut canvas, true, false);
        let mut glyphs = Canvas::new(width, height)?;
        glyphs.transform = canvas.transform;
        painter.draw(&mut glyphs, false, true);
        if has_active_filter_effects(block) {
            apply_font_filter_effects(&mut glyphs.pixmap, block, painter.font_size * scale);
        }
        if hard {
            harden_text_pixels(&mut glyphs.pixmap, block);
        }
        canvas.draw_pixmap_device(&glyphs.pixmap, 0, 0, 1.0, None);
    } else {
        painter.draw(&mut canvas, true, true);
    }
    Some(BlockRaster { pixmap: canvas.pixmap, x: area.x, y: area.y, scale: scale as f32, overflow: layout.overflow })
}

/// One line (horizontal) or character (vertical) to draw.
struct TextItem {
    text: String,
    x: f64,
    y: f64,
    align: TextAlign,
}

/// A run of the item placed at `x` (left edge).
struct PlacedSegment {
    text: String,
    x: f64,
    advance: f64,
    centered_ellipsis: bool,
}

struct EllipsisDots {
    centers: [f64; 3],
    center_y: f64,
    radius: f64,
}

struct ItemGeometry {
    glyphs: Option<Path>,
    dots: Vec<EllipsisDots>,
    /// Pivot for the synthetic italic skew (`y + fontSize / 2`).
    pivot_y: f64,
    underline: Option<(f64, f64, f64)>,
}

struct BlockPainter<'a> {
    ctx: &'a mut FontContext,
    block: &'a TranslationBlock,
    fonts: &'a BlockFonts,
    layout: &'a BlockTextLayout,
    text: &'a str,
    font_size: f64,
    bold_px: f64,
    skew: f64,
    /// Device pixels per page pixel (blur radii are in device pixels).
    device_scale: f64,
}

impl<'a> BlockPainter<'a> {
    fn new(
        ctx: &'a mut FontContext,
        block: &'a TranslationBlock,
        fonts: &'a BlockFonts,
        layout: &'a BlockTextLayout,
        text: &'a str,
    ) -> Self {
        let font_size = layout.font_size_px * RENDER_FONT_SIZE_RATIO;
        Self {
            bold_px: synthetic_bold_stroke_width_px(block, fonts, font_size),
            skew: synthetic_italic_skew_x(block),
            device_scale: 1.0,
            ctx,
            block,
            fonts,
            layout,
            text,
            font_size,
        }
    }

    fn apply_block_transform(&self, canvas: &mut Canvas) {
        let rotation = resolve_block_rotation_deg(self.block);
        if rotation != 0.0 {
            let rect = self.layout.rect;
            let (cx, cy) = (rect.left + rect.width / 2.0, rect.top + rect.height / 2.0);
            canvas.translate(cx, cy);
            canvas.rotate_deg(rotation);
            canvas.translate(-cx, -cy);
        }
    }

    fn draw(&mut self, canvas: &mut Canvas, include_background: bool, include_glyphs: bool) {
        let layout = *self.layout;
        let y_offset = self.font_size * RENDER_Y_OFFSET_RATIO;
        let left = layout.rect.left + BLOCK_BORDER_PX + layout.padding_px;
        let top = layout.rect.top + BLOCK_BORDER_PX + layout.padding_px + y_offset;
        let (items, background) = if self.block.render_direction == RenderTextDirection::Vertical {
            self.vertical_items(left, top, layout.fit_inner_width, layout.fit_inner_height)
        } else {
            self.horizontal_items(left, top, layout.inner_width, layout.inner_height, layout.fit_inner_width)
        };
        if include_background {
            self.draw_font_background(canvas, background);
        }
        if !include_glyphs {
            return;
        }
        let geometries: Vec<ItemGeometry> = items.iter().map(|item| self.item_geometry(item)).collect();
        self.draw_shadow_phase(canvas, &geometries);
        for geometry in &geometries {
            self.draw_outline_phase(canvas, geometry);
        }
        for geometry in &geometries {
            self.draw_fill_phase(canvas, geometry);
        }
        if self.block.style.text_decoration.unwrap_or_default() == TextDecoration::Underline {
            for geometry in &geometries {
                if let Some((x, y, width)) = geometry.underline {
                    let mut pb = PathBuilder::new();
                    pb.move_to(x as f32, y as f32);
                    pb.line_to((x + width) as f32, y as f32);
                    if let Some(path) = pb.finish() {
                        canvas.stroke_path(
                            &path,
                            (self.font_size * 0.06).max(1.0),
                            solid(hex_color(self.block.text_color(), 1.0)),
                            LineCap::Butt,
                        );
                    }
                }
            }
        }
    }

    fn horizontal_items(
        &mut self,
        left: f64,
        top: f64,
        inner_width: f64,
        inner_height: f64,
        fit_width: f64,
    ) -> (Vec<TextItem>, PixelRect) {
        let lines = wrap_text_to_width(self.ctx, self.fonts, self.block, self.text, self.font_size, fit_width);
        let line_height = self.font_size * self.block.line_height();
        let total_height = lines.len() as f64 * line_height;
        let (fx, fy) = text_position_factors(self.block.text_position);
        let start_y = top + (inner_height - total_height) * fy + ((line_height - self.font_size) / 2.0).max(0.0);
        let widest = lines.iter().map(|line| line.width).fold(0.0, f64::max);
        let text_left = left + (inner_width - widest) * fx;
        let x = match self.block.text_align {
            TextAlign::Left => text_left,
            TextAlign::Right => text_left + widest,
            TextAlign::Center => text_left + widest / 2.0,
        };
        let items = lines
            .into_iter()
            .enumerate()
            .map(|(index, line)| TextItem {
                text: line.text,
                x,
                y: start_y + index as f64 * line_height,
                align: self.block.text_align,
            })
            .collect();
        (items, PixelRect { left: text_left, top: start_y, width: widest, height: total_height })
    }

    fn vertical_items(
        &mut self,
        left: f64,
        top: f64,
        inner_width: f64,
        inner_height: f64,
    ) -> (Vec<TextItem>, PixelRect) {
        let chars: Vec<char> = self.text.chars().filter(|ch| !is_js_whitespace(*ch)).collect();
        let line_height = self.font_size * self.block.line_height();
        let spacing = letter_spacing_px(self.block, self.font_size);
        let advance = (line_height + spacing).max(1.0);
        let count = chars.len() as f64;
        let total_height = count * line_height + (count - 1.0).max(0.0) * (advance - line_height);
        let (fx, fy) = text_position_factors(self.block.text_position);
        let start_y = top + (inner_height - total_height) * fy;
        let x = left + (inner_width - self.font_size) * fx + self.font_size / 2.0;
        let items = chars
            .iter()
            .enumerate()
            .map(|(index, ch)| TextItem {
                text: ch.to_string(),
                x,
                y: start_y + index as f64 * advance,
                align: TextAlign::Center,
            })
            .collect();
        (items, PixelRect { left: x - self.font_size / 2.0, top: start_y, width: self.font_size, height: total_height })
    }

    fn draw_font_background(&self, canvas: &mut Canvas, text_rect: PixelRect) {
        let opacity = font_background_opacity(self.block);
        if !font_background_enabled(self.block) || opacity <= 0.0 || text_rect.width <= 0.0 || text_rect.height <= 0.0 {
            return;
        }
        let rect = scaled_rect(text_rect, font_background_scale(self.block));
        let color = self
            .block
            .style
            .font_background_color
            .as_deref()
            .unwrap_or(mtt_core::font_presets::DEFAULT_FONT_BACKGROUND_COLOR);
        if let Some(path) =
            round_rect_path(rect.left, rect.top, rect.width, rect.height, font_background_radius_px(self.block))
        {
            canvas.fill_path(&path, solid(hex_color(color, opacity)));
        }
    }

    /// Splits an item into positioned runs (`drawTextRunGlyphs` / `resolveTextRunSegmentPositions`).
    fn place_segments(&mut self, item: &TextItem) -> Vec<PlacedSegment> {
        let spacing = letter_spacing_px(self.block, self.font_size);
        let has_ellipsis = item.text.contains(CENTERED_ELLIPSIS) || item.text.contains(CENTERED_PERIOD_ELLIPSIS);
        if spacing == 0.0 && !has_ellipsis && self.fonts.overrides.is_empty() {
            let width = measure_text_width(self.ctx, self.fonts, self.block, &item.text, self.font_size);
            return vec![PlacedSegment {
                x: aligned_start(item.x, width, item.align),
                advance: width,
                text: item.text.clone(),
                centered_ellipsis: false,
            }];
        }
        let mut pieces: Vec<(String, bool)> = Vec::new();
        for run in self.font_runs(&item.text) {
            for piece in split_by_centered_ellipsis(&run) {
                let ellipsis = piece == CENTERED_ELLIPSIS || piece == CENTERED_PERIOD_ELLIPSIS;
                if spacing != 0.0 && !ellipsis {
                    pieces.extend(piece.chars().map(|ch| (ch.to_string(), false)));
                } else {
                    pieces.push((piece, ellipsis));
                }
            }
        }
        let advances: Vec<f64> = pieces
            .iter()
            .map(|(text, _)| {
                let count = text.chars().count() as f64;
                let run = self.ctx.shape(text, &self.fonts.base, &self.fonts.spans_for(text));
                f64::from(run.advance_em) * self.font_size + if count > 1.0 { spacing * (count - 1.0) } else { 0.0 }
            })
            .collect();
        let piece_count = pieces.len();
        let gap = |index: usize| {
            if spacing != 0.0 && index + 1 < piece_count { spacing } else { 0.0 }
        };
        let run_width: f64 = advances.iter().enumerate().map(|(i, a)| a + gap(i)).sum();
        let mut cursor = aligned_start(item.x, run_width, item.align);
        pieces
            .into_iter()
            .zip(advances)
            .enumerate()
            .map(|(index, ((text, centered_ellipsis), advance))| {
                let segment = PlacedSegment { text, x: cursor, advance, centered_ellipsis };
                cursor += advance + gap(index);
                segment
            })
            .collect()
    }

    /// Consecutive characters sharing the same override font (`resolveCharacterFontRuns`).
    fn font_runs(&self, text: &str) -> Vec<String> {
        let mut runs: Vec<(String, Option<&crate::fonts::FontSpec>)> = Vec::new();
        for ch in text.chars() {
            let spec = self.fonts.overrides.get(&ch);
            match runs.last_mut() {
                Some((run, last)) if *last == spec => run.push(ch),
                _ => runs.push((ch.to_string(), spec)),
            }
        }
        runs.into_iter().map(|(run, _)| run).collect()
    }

    fn item_geometry(&mut self, item: &TextItem) -> ItemGeometry {
        let segments = self.place_segments(item);
        let mut builder = PathBuilder::new();
        let mut dots = Vec::new();
        for segment in &segments {
            if segment.centered_ellipsis {
                dots.push(self.ellipsis_dots(segment, item.y));
            } else {
                self.append_glyphs(&mut builder, &segment.text, segment.x, item.y);
            }
        }
        let underline =
            (self.block.style.text_decoration.unwrap_or_default() == TextDecoration::Underline).then(|| {
                let width = measure_text_width(self.ctx, self.fonts, self.block, &item.text, self.font_size);
                (aligned_start(item.x, width, item.align), item.y + self.font_size * 1.05, width)
            });
        ItemGeometry { glyphs: builder.finish(), dots, pivot_y: item.y + self.font_size / 2.0, underline }
    }

    fn append_glyphs(&mut self, builder: &mut PathBuilder, text: &str, x: f64, top: f64) {
        let spans = self.fonts.spans_for(text);
        let run = self.ctx.shape(text, &self.fonts.base, &spans);
        let baseline = top + f64::from(self.ctx.vertical_metrics(&self.fonts.base).top_to_baseline_em) * self.font_size;
        let size = self.font_size as f32;
        for glyph in &run.glyphs {
            let Some(outline) = self.ctx.glyph_outline(glyph.font_id, glyph.weight, glyph.glyph_id) else {
                continue;
            };
            let gx = x as f32 + (glyph.x_em + glyph.x_offset_em) * size;
            let gy = baseline as f32 - glyph.y_offset_em * size;
            if let Some(placed) = (*outline).clone().transform(Transform::from_row(size, 0.0, 0.0, size, gx, gy)) {
                builder.push_path(&placed);
            }
        }
    }

    fn ellipsis_dots(&mut self, segment: &PlacedSegment, y: f64) -> EllipsisDots {
        let fs = self.font_size;
        let advance = (fs * 0.45).max(segment.advance);
        let spec = segment
            .text
            .chars()
            .next()
            .map(|c| self.fonts.spec_for_char(c).clone())
            .unwrap_or_else(|| self.fonts.base.clone());
        let measured = self
            .ctx
            .ink_bounds(".", &spec)
            .map(|bounds| f64::from(bounds.ascent_em + bounds.descent_em) * fs)
            .filter(|height| height.is_finite() && *height > 0.0 && *height < fs * 0.4)
            .map(|height| height / 2.0)
            .unwrap_or(fs * ELLIPSIS_DOT_RADIUS_RATIO);
        let radius = (fs * 0.045).max((fs * 0.09).min(measured));
        let spacing = (radius * 2.8).max((fs * ELLIPSIS_DOT_SPACING_RATIO).min(advance / 3.0));
        let center_x = segment.x + advance / 2.0;
        EllipsisDots { centers: [center_x - spacing, center_x, center_x + spacing], center_y: y + fs / 2.0, radius }
    }

    fn with_skew(&self, canvas: &mut Canvas, pivot_y: f64, draw: impl FnOnce(&mut Canvas)) {
        if self.skew == 0.0 {
            draw(canvas);
            return;
        }
        canvas.save();
        canvas.translate(0.0, pivot_y);
        canvas.skew_x(self.skew);
        canvas.translate(0.0, -pivot_y);
        draw(canvas);
        canvas.restore();
    }

    fn draw_dots(canvas: &mut Canvas, dots: &EllipsisDots, radius: f64, shader: Shader<'_>) {
        let centers: Vec<(f64, f64)> = dots.centers.iter().map(|x| (*x, dots.center_y)).collect();
        if let Some(path) = circles_path(&centers, radius) {
            canvas.fill_path(&path, shader);
        }
    }

    fn draw_shadow_phase(&self, canvas: &mut Canvas, geometries: &[ItemGeometry]) {
        let block = self.block;
        if !has_visible_shadow(block) {
            return;
        }
        let distance = block.style.shadow_distance_px.unwrap_or(0.0).max(0.0);
        let blur = shadow_blur_px(block);
        if distance == 0.0 && blur == 0.0 {
            return;
        }
        // A wide blur leaves no detail, so at preview scales the shadow is drawn and blurred
        // at a lower resolution (keeping at least 2 px of blur there) and scaled up. At
        // scale 1 (export) this is 1 and the output is unchanged.
        let sigma = blur * self.device_scale;
        let reduce = (sigma / 2.0).clamp(1.0, self.device_scale.max(1.0));
        let layer_size = |size: u32| (f64::from(size) / reduce).ceil() as u32;
        let Some(mut layer) = Canvas::new(layer_size(canvas.width()), layer_size(canvas.height())) else {
            return;
        };
        let angle = block.style.shadow_angle_deg.unwrap_or(45.0).to_radians();
        let shrink = (1.0 / reduce) as f32;
        layer.transform = canvas.transform.post_scale(shrink, shrink);
        layer.translate(angle.cos() * distance, angle.sin() * distance);
        let opaque = hex_color(block.style.shadow_color.as_deref().unwrap_or("#000000"), 1.0);
        let outline = outline_width_px(block, self.font_size);
        let secondary = secondary_outline_width_px(block, self.font_size);
        for geometry in geometries {
            self.with_skew(&mut layer, geometry.pivot_y, |layer| {
                if let Some(path) = &geometry.glyphs {
                    if secondary > 0.0 {
                        layer.stroke_path(path, outline + secondary * 2.0 + self.bold_px, solid(opaque), LineCap::Butt);
                    }
                    if outline > 0.0 {
                        layer.stroke_path(path, outline + self.bold_px, solid(opaque), LineCap::Butt);
                    }
                    layer.stroke_path(path, self.bold_px, solid(opaque), LineCap::Butt);
                    layer.fill_path(path, solid(opaque));
                }
                let outline_radius = if secondary > 0.0 { outline / 2.0 + secondary } else { outline / 2.0 };
                for dots in &geometry.dots {
                    Self::draw_dots(layer, dots, dots.radius + self.bold_px / 2.0 + outline_radius, solid(opaque));
                }
            });
        }
        gaussian_blur(&mut layer.pixmap, sigma / reduce);
        let opacity = shadow_opacity(block) as f32;
        if reduce == 1.0 {
            canvas.draw_pixmap_device(&layer.pixmap, 0, 0, opacity, None);
        } else {
            let paint = tiny_skia::PixmapPaint { opacity, quality: FilterQuality::Bilinear, ..Default::default() };
            let grow = reduce as f32;
            canvas.pixmap.draw_pixmap(0, 0, layer.pixmap.as_ref(), &paint, Transform::from_scale(grow, grow), None);
        }
    }

    fn draw_outline_phase(&self, canvas: &mut Canvas, geometry: &ItemGeometry) {
        let block = self.block;
        let outline = outline_width_px(block, self.font_size);
        let secondary = secondary_outline_width_px(block, self.font_size);
        let outline_color = hex_color(block.style.outline_color.as_deref().unwrap_or(DEFAULT_OUTLINE_COLOR), 1.0);
        let secondary_color = hex_color(block.style.secondary_outline_color.as_deref().unwrap_or("#ffffff"), 1.0);
        let collapse_knockout = outline > 0.0
            && secondary > 0.0
            && match (
                block.style.outline_color.as_deref().and_then(parse_hex_color),
                parse_hex_color(&block.background_color),
            ) {
                (Some(a), Some(b)) => a == b,
                _ => false,
            };
        self.with_skew(canvas, geometry.pivot_y, |canvas| {
            if let Some(path) = &geometry.glyphs {
                if secondary > 0.0 {
                    canvas.stroke_path(
                        path,
                        outline + secondary * 2.0 + self.bold_px,
                        solid(secondary_color),
                        LineCap::Butt,
                    );
                }
                if outline > 0.0 {
                    canvas.stroke_path(path, outline + self.bold_px, solid(outline_color), LineCap::Butt);
                    canvas.fill_path(path, solid(outline_color));
                }
            }
            let bold_radius = self.bold_px / 2.0;
            for dots in &geometry.dots {
                if secondary > 0.0 {
                    Self::draw_dots(
                        canvas,
                        dots,
                        dots.radius + outline / 2.0 + secondary + bold_radius,
                        solid(secondary_color),
                    );
                }
                if outline > 0.0 && !collapse_knockout {
                    Self::draw_dots(canvas, dots, dots.radius + outline / 2.0 + bold_radius, solid(outline_color));
                    Self::draw_dots(canvas, dots, dots.radius + bold_radius, solid(outline_color));
                }
            }
        });
    }

    fn draw_fill_phase(&self, canvas: &mut Canvas, geometry: &ItemGeometry) {
        let block = self.block;
        let text_color = hex_color(block.text_color(), 1.0);
        let screentone = block.style.screentone_fill_enabled.unwrap_or(false);
        let tile = screentone.then(|| screentone_tile(block, self.font_size)).flatten();
        fn fill_shader(tile: &Option<Pixmap>, text_color: tiny_skia::Color) -> Shader<'_> {
            match tile {
                Some(tile) => {
                    Pattern::new(tile.as_ref(), SpreadMode::Repeat, FilterQuality::Nearest, 1.0, Transform::identity())
                }
                None => solid(text_color),
            }
        }
        self.with_skew(canvas, geometry.pivot_y, |canvas| {
            let white = hex_color("#ffffff", 1.0);
            if let Some(path) = &geometry.glyphs {
                if screentone {
                    canvas.fill_path(path, solid(white));
                }
                canvas.stroke_path(path, self.bold_px, fill_shader(&tile, text_color), LineCap::Butt);
                canvas.fill_path(path, fill_shader(&tile, text_color));
            }
            for dots in &geometry.dots {
                let radius = dots.radius + self.bold_px / 2.0;
                if screentone {
                    Self::draw_dots(canvas, dots, radius, solid(white));
                }
                Self::draw_dots(canvas, dots, radius, fill_shader(&tile, text_color));
            }
        });
    }
}

fn aligned_start(x: f64, width: f64, align: TextAlign) -> f64 {
    match align {
        TextAlign::Left => x,
        TextAlign::Center => x - width / 2.0,
        TextAlign::Right => x - width,
    }
}

/// `text.split(/(…|\.{3})/u).filter(Boolean)`.
fn split_by_centered_ellipsis(text: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let next = [
            rest.find(CENTERED_ELLIPSIS).map(|i| (i, CENTERED_ELLIPSIS.len())),
            rest.find(CENTERED_PERIOD_ELLIPSIS).map(|i| (i, 3)),
        ]
        .into_iter()
        .flatten()
        .min_by_key(|(index, _)| *index);
        match next {
            Some((index, len)) => {
                if index > 0 {
                    parts.push(rest[..index].to_owned());
                }
                parts.push(rest[index..index + len].to_owned());
                rest = &rest[index + len..];
            }
            None => {
                parts.push(rest.to_owned());
                break;
            }
        }
    }
    parts
}

/// Screentone tile (`createScreentonePattern`): white tile with one dot.
fn screentone_tile(block: &TranslationBlock, font_size: f64) -> Option<Pixmap> {
    let tile_size = screentone_tile_size_px(font_size, block.style.screentone_fill_density);
    let radius = screentone_dot_radius_px(tile_size, block.style.screentone_fill_intensity);
    let canvas_size = tile_size.ceil().max(3.0) as u32;
    let mut tile = Canvas::new(canvas_size, canvas_size)?;
    let (r, g, b) = parse_hex_color(block.text_color()).unwrap_or((0x11, 0x11, 0x11));
    if block.style.screentone_fill_antialias == Some(false) {
        let center = tile_size / 2.0;
        for (index, pixel) in tile.pixmap.pixels_mut().iter_mut().enumerate() {
            let x = (index as u32 % canvas_size) as f64 + 0.5;
            let y = (index as u32 / canvas_size) as f64 + 0.5;
            let inside =
                x <= tile_size && y <= tile_size && (x - center).powi(2) + (y - center).powi(2) <= radius * radius;
            *pixel = if inside {
                tiny_skia::ColorU8::from_rgba(r, g, b, 255)
            } else {
                tiny_skia::ColorU8::from_rgba(255, 255, 255, 255)
            }
            .premultiply();
        }
    } else {
        tile.fill_rect(0.0, 0.0, f64::from(canvas_size), f64::from(canvas_size), hex_color("#ffffff", 1.0));
        if let Some(path) = circles_path(&[(tile_size / 2.0, tile_size / 2.0)], radius) {
            tile.fill_path(&path, solid(tiny_skia::Color::from_rgba8(r, g, b, 255)));
        }
    }
    Some(tile.pixmap)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ellipsis_split_matches_js() {
        assert_eq!(split_by_centered_ellipsis("아…니...야"), ["아", "…", "니", "...", "야"]);
        assert_eq!(split_by_centered_ellipsis("....."), ["...", ".."]);
        assert_eq!(split_by_centered_ellipsis("…"), ["…"]);
    }

    #[test]
    fn rotated_bounds_of_square_at_45_degrees() {
        let bounds = rotated_bounds(PixelRect { left: 0.0, top: 0.0, width: 10.0, height: 10.0 }, 5.0, 5.0, 45.0);
        let diagonal = 10.0 * std::f64::consts::SQRT_2;
        assert!((bounds.width - diagonal).abs() < 1e-9 && (bounds.left - (5.0 - diagonal / 2.0)).abs() < 1e-9);
    }
}
