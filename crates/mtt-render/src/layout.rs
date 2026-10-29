//! Block text layout (`overlayLayout.ts`): padding, wrapping and auto-fit font size.

use std::collections::HashMap;

use mtt_core::font_presets::{
    DEFAULT_FONT_BACKGROUND_OPACITY, DEFAULT_FONT_BACKGROUND_RADIUS_PX, DEFAULT_FONT_BACKGROUND_SCALE,
    DEFAULT_FONT_SIZE_PX, DEFAULT_LINE_HEIGHT, DEFAULT_OVERLAY_FONT_FAMILY, DEFAULT_OVERLAY_FONT_WEIGHT,
    DEFAULT_SCREENTONE_FILL_DENSITY, DEFAULT_SCREENTONE_FILL_INTENSITY, MAX_FONT_BACKGROUND_RADIUS_PX,
    MAX_FONT_BACKGROUND_SCALE, MIN_FONT_BACKGROUND_RADIUS_PX, MIN_FONT_BACKGROUND_SCALE,
};
use mtt_core::geometry::{
    bbox_to_pixels, clamp, clamp_text_padding_px, resolve_block_render_bbox, text_shape_inner_rect, text_shape_span,
};
use mtt_core::model::{RenderTextDirection, TextAlign, TextFontStyle, TextPosition, TranslationBlock};

use crate::fonts::{BOLD_WEIGHT_THRESHOLD, FontContext, FontSpec};

const MIN_FONT_SIZE_PX: f64 = 2.0;
/// Style numbers past these (only reachable by editing the data by hand) are clamped, so
/// size arithmetic stays finite. The editor allows 8..=999px and 0.8..=2.0.
const MAX_FONT_SIZE_PX: f64 = 10_000.0;
const MIN_LINE_HEIGHT: f64 = 0.1;
const MAX_LINE_HEIGHT: f64 = 10.0;
const MAX_BLOCK_PADDING_PX: f64 = 14.0;
const MIN_INNER_SIZE_PX: f64 = 1.0;
const BLOCK_BORDER_PX: f64 = 1.0;
const TEXT_FIT_SAFETY_PX: f64 = 6.0;
const TEXT_MEASURE_GUARD_PX: f64 = TEXT_FIT_SAFETY_PX + 4.0;
const SYNTHETIC_BOLD_FALLBACK_BASE_WEIGHT: u16 = 400;
const SYNTHETIC_BOLD_MIN_STROKE_WIDTH_PX: f64 = 0.35;
const SYNTHETIC_BOLD_MAX_STROKE_WIDTH_PX: f64 = 8.0;
const SYNTHETIC_BOLD_MIN_RATIO: f64 = 0.018;
const SYNTHETIC_BOLD_MAX_RATIO: f64 = 0.05;
const SYNTHETIC_ITALIC_SKEW_DEG: f64 = -12.0;
/// Typesetting v2 (`LAYOUT_VERSION`): auto-fit grows text to at most this many times the
/// estimated source glyph size,
const SOURCE_GLYPH_GROWTH: f64 = 1.2;
/// estimating each source glyph's cell as this many times its size (line and letter gaps),
const SOURCE_GLYPH_SPACING: f64 = 1.2;
/// and shrinks text no further than this many page pixels (it overflows instead).
const READABLE_FONT_SIZE_PX: f64 = 9.0;
/// Line breaking cost: (unused width in em)² per line, plus a penalty for where a line
/// ends: nothing after sentence punctuation, then after a comma, anywhere else, or before
/// a mark (`…`, `~`) that would start the next line. A mid-phrase break costs about as much
/// as moving 1.5em of text between two lines, so "뭐야? / 너 누구야!" beats "뭐야? 너 / 누구야!".
const BREAK_AFTER_COMMA_COST: f64 = 2.0;
const BREAK_MID_PHRASE_COST: f64 = 6.0;
const BREAK_BEFORE_MARK_COST: f64 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

impl Size {
    pub const fn new(width: f64, height: f64) -> Self {
        Self { width, height }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PixelRect {
    pub left: f64,
    pub top: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockTextLayout {
    pub rect: PixelRect,
    pub padding_px: f64,
    /// Where the text goes: inside the padding, and inside the block's text shape when the
    /// lines don't follow it.
    pub inner: PixelRect,
    pub fit_inner_width: f64,
    pub fit_inner_height: f64,
    pub font_size_px: f64,
    pub overflow: bool,
    /// The lines follow the block's text shape ([`shaped_lines`]).
    pub shaped: bool,
}

/// Fonts used to draw one block: the block family (weight adjusted for synthetic bold)
/// and per-character overrides.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockFonts {
    pub base: FontSpec,
    pub overrides: HashMap<char, FontSpec>,
    pub synthetic_bold: bool,
}

impl BlockFonts {
    pub fn resolve(ctx: &FontContext, block: &TranslationBlock) -> Self {
        let family = block.style.font_family.as_deref().unwrap_or(DEFAULT_OVERLAY_FONT_FAMILY);
        let weight = requested_weight(block);
        // The original rules judge bold by the first listed family even when another one
        // draws; v2 by the family that draws.
        let weights_of = |family: &str| {
            if block.layout_version() >= 2 { ctx.resolved_family_weights(family) } else { ctx.family_weights(family) }
        };
        let base_weights = weights_of(family);
        let base = ctx.resolve_spec(family, canvas_font_weight(base_weights.as_ref(), weight));
        let mut overrides = HashMap::new();
        for entry in block.style.character_font_overrides.iter().flatten() {
            let (Some(ch), override_family) = (entry.character.chars().next(), entry.font_family.trim()) else {
                continue;
            };
            if !override_family.is_empty() {
                let override_weight = canvas_font_weight(weights_of(override_family).as_ref(), weight);
                overrides.insert(ch, ctx.resolve_spec(override_family, override_weight));
            }
        }
        let synthetic_bold = weight >= BOLD_WEIGHT_THRESHOLD && !has_native_bold(base_weights.as_ref());
        Self { base, overrides, synthetic_bold }
    }

    /// Byte-range font spans for `text` (`resolveCharacterFontRuns`).
    pub fn spans_for(&self, text: &str) -> Vec<(usize, usize, FontSpec)> {
        if self.overrides.is_empty() {
            return Vec::new();
        }
        let mut spans: Vec<(usize, usize, FontSpec)> = Vec::new();
        for (index, ch) in text.char_indices() {
            let Some(spec) = self.overrides.get(&ch) else {
                continue;
            };
            match spans.last_mut() {
                Some(last) if last.1 == index && last.2 == *spec => last.1 = index + ch.len_utf8(),
                _ => spans.push((index, index + ch.len_utf8(), spec.clone())),
            }
        }
        spans
    }

    pub fn spec_for_char(&self, ch: char) -> &FontSpec {
        self.overrides.get(&ch).unwrap_or(&self.base)
    }
}

fn requested_weight(block: &TranslationBlock) -> u16 {
    let weight = block.style.font_weight.unwrap_or(DEFAULT_OVERLAY_FONT_WEIGHT);
    if weight.is_finite() { weight.round().clamp(1.0, 1000.0) as u16 } else { DEFAULT_OVERLAY_FONT_WEIGHT as u16 }
}

/// Whether a family with these installed weights (and variable bold) has a bold face.
fn has_native_bold(weights: Option<&(Vec<u16>, bool)>) -> bool {
    weights.is_some_and(|(weights, variable_bold)| {
        *variable_bold || weights.iter().any(|weight| *weight >= BOLD_WEIGHT_THRESHOLD)
    })
}

/// `resolveOverlayCanvasFontWeight`: without a native bold face, draw the regular face and
/// embolden it with a stroke instead.
fn canvas_font_weight(weights: Option<&(Vec<u16>, bool)>, weight: u16) -> u16 {
    if weight < BOLD_WEIGHT_THRESHOLD || has_native_bold(weights) {
        return weight;
    }
    weights
        .and_then(|(weights, _)| weights.iter().copied().filter(|w| *w > 0 && *w < BOLD_WEIGHT_THRESHOLD).max())
        .unwrap_or(SYNTHETIC_BOLD_FALLBACK_BASE_WEIGHT)
}

/// `resolveSyntheticBoldStrokeWidthPx`.
pub fn synthetic_bold_stroke_width_px(block: &TranslationBlock, fonts: &BlockFonts, font_size: f64) -> f64 {
    if !fonts.synthetic_bold {
        return 0.0;
    }
    let weight = f64::from(requested_weight(block)).clamp(f64::from(BOLD_WEIGHT_THRESHOLD), 900.0);
    let weight_ratio = (weight - f64::from(BOLD_WEIGHT_THRESHOLD)) / (900.0 - f64::from(BOLD_WEIGHT_THRESHOLD));
    let ratio = SYNTHETIC_BOLD_MIN_RATIO + (SYNTHETIC_BOLD_MAX_RATIO - SYNTHETIC_BOLD_MIN_RATIO) * weight_ratio;
    clamp(font_size * ratio, SYNTHETIC_BOLD_MIN_STROKE_WIDTH_PX, SYNTHETIC_BOLD_MAX_STROKE_WIDTH_PX)
}

pub fn synthetic_italic_skew_x(block: &TranslationBlock) -> f64 {
    if block.style.font_style.unwrap_or_default() == TextFontStyle::Italic {
        SYNTHETIC_ITALIC_SKEW_DEG.to_radians().tan()
    } else {
        0.0
    }
}

pub fn text_position_factors(position: Option<TextPosition>) -> (f64, f64) {
    match position.unwrap_or_default() {
        TextPosition::TopLeft => (0.0, 0.0),
        TextPosition::Top => (0.5, 0.0),
        TextPosition::TopRight => (1.0, 0.0),
        TextPosition::Left => (0.0, 0.5),
        TextPosition::Center => (0.5, 0.5),
        TextPosition::Right => (1.0, 0.5),
        TextPosition::BottomLeft => (0.0, 1.0),
        TextPosition::Bottom => (0.5, 1.0),
        TextPosition::BottomRight => (1.0, 1.0),
    }
}

/// The block's font size, kept within what the renderer can lay out.
pub fn style_font_size_px(block: &TranslationBlock) -> f64 {
    let size = block.font_size_px();
    if size.is_finite() { size.clamp(1.0, MAX_FONT_SIZE_PX) } else { DEFAULT_FONT_SIZE_PX }
}

/// The block's line height (a multiple of the font size), kept within what the renderer
/// can lay out.
pub fn style_line_height(block: &TranslationBlock) -> f64 {
    let height = block.line_height();
    if height.is_finite() { height.clamp(MIN_LINE_HEIGHT, MAX_LINE_HEIGHT) } else { DEFAULT_LINE_HEIGHT }
}

/// `resolveTextLetterSpacingPx`: letter spacing scales with the rendered font size.
pub fn letter_spacing_px(block: &TranslationBlock, font_size: f64) -> f64 {
    let source = block.style.font_size_px.map_or(font_size, |_| style_font_size_px(block)).max(1.0);
    let spacing = block.style.letter_spacing_px.filter(|value| value.is_finite()).unwrap_or(0.0);
    spacing.clamp(-MAX_FONT_SIZE_PX, MAX_FONT_SIZE_PX) * (font_size / source)
}

pub fn secondary_outline_enabled(block: &TranslationBlock) -> bool {
    block.style.secondary_outline_enabled.unwrap_or(block.style.secondary_outline_width_px.unwrap_or(0.0) > 0.0)
}

pub fn font_background_scale(block: &TranslationBlock) -> f64 {
    let value = block.style.font_background_scale.unwrap_or(DEFAULT_FONT_BACKGROUND_SCALE);
    if value.is_finite() {
        clamp(value, MIN_FONT_BACKGROUND_SCALE, MAX_FONT_BACKGROUND_SCALE)
    } else {
        DEFAULT_FONT_BACKGROUND_SCALE
    }
}

pub fn font_background_radius_px(block: &TranslationBlock) -> f64 {
    let value = block.style.font_background_radius_px.unwrap_or(DEFAULT_FONT_BACKGROUND_RADIUS_PX);
    if value.is_finite() {
        clamp(value, MIN_FONT_BACKGROUND_RADIUS_PX, MAX_FONT_BACKGROUND_RADIUS_PX)
    } else {
        DEFAULT_FONT_BACKGROUND_RADIUS_PX
    }
}

pub fn font_background_opacity(block: &TranslationBlock) -> f64 {
    let value = block.style.font_background_opacity.unwrap_or(DEFAULT_FONT_BACKGROUND_OPACITY);
    if value.is_finite() { clamp(value, 0.0, 1.0) } else { DEFAULT_FONT_BACKGROUND_OPACITY }
}

pub fn font_background_enabled(block: &TranslationBlock) -> bool {
    block.style.font_background_enabled.unwrap_or(font_background_opacity(block) > 0.0)
}

pub fn scaled_rect(rect: PixelRect, scale: f64) -> PixelRect {
    let width = rect.width * scale;
    let height = rect.height * scale;
    PixelRect {
        left: rect.left + (rect.width - width) / 2.0,
        top: rect.top + (rect.height - height) / 2.0,
        width,
        height,
    }
}

pub fn screentone_tile_size_px(font_size: f64, density: Option<f64>) -> f64 {
    let density = normalize_unit(density, DEFAULT_SCREENTONE_FILL_DENSITY);
    let (min_ratio, max_ratio) = (0.022, 0.12);
    (font_size * (max_ratio - density * (max_ratio - min_ratio))).max(3.0)
}

pub fn screentone_dot_radius_px(tile_size: f64, intensity: Option<f64>) -> f64 {
    let intensity = normalize_unit(intensity, DEFAULT_SCREENTONE_FILL_INTENSITY);
    clamp(tile_size * (0.08 + intensity * 0.4), 0.2, tile_size * 0.48)
}

fn normalize_unit(value: Option<f64>, fallback: f64) -> f64 {
    match value {
        Some(value) if value.is_finite() => clamp(value, 0.05, 1.0),
        _ => fallback,
    }
}

pub fn resolve_block_rect_px(block: &TranslationBlock, page: Size, stage: Size) -> PixelRect {
    let pixel = bbox_to_pixels(resolve_block_render_bbox(block), page.width, page.height);
    let scale_x = stage.width / page.width.max(1.0);
    let scale_y = stage.height / page.height.max(1.0);
    PixelRect { left: pixel.x * scale_x, top: pixel.y * scale_y, width: pixel.w * scale_x, height: pixel.h * scale_y }
}

/// The block's detection box (where the source text is) in stage pixels.
fn detection_rect_px(block: &TranslationBlock, page: Size, stage: Size) -> PixelRect {
    let pixel = bbox_to_pixels(block.bbox, page.width, page.height);
    let scale_x = stage.width / page.width.max(1.0);
    let scale_y = stage.height / page.height.max(1.0);
    PixelRect { left: pixel.x * scale_x, top: pixel.y * scale_y, width: pixel.w * scale_x, height: pixel.h * scale_y }
}

fn auto_block_padding_px(rect: PixelRect) -> f64 {
    let shortest = rect.width.min(rect.height);
    if shortest <= 48.0 {
        0.0
    } else if shortest <= 72.0 {
        1.0
    } else if shortest <= 96.0 {
        2.0
    } else {
        clamp(shortest * 0.06, 3.0, MAX_BLOCK_PADDING_PX).round()
    }
}

pub fn resolve_block_padding_px(block: &TranslationBlock, rect: PixelRect, page: Size, stage: Size) -> f64 {
    match block.text_padding_px {
        Some(padding) if padding.is_finite() => clamp_text_padding_px(padding) * render_scale(page, stage),
        _ => auto_block_padding_px(rect),
    }
}

pub fn render_scale(page: Size, stage: Size) -> f64 {
    (stage.width / page.width.max(1.0)).min(stage.height / page.height.max(1.0))
}

/// `resolveBlockTextLayout`. With a text shape (typesetting v2), centred horizontal lines
/// follow it; other text, or text that can't follow it at a readable size, keeps to the
/// shape's largest rectangle.
pub fn resolve_block_text_layout(
    ctx: &mut FontContext,
    fonts: &BlockFonts,
    block: &TranslationBlock,
    text: &str,
    page: Size,
    stage: Size,
) -> BlockTextLayout {
    let rect = resolve_block_rect_px(block, page, stage);
    let padding_px = resolve_block_padding_px(block, rect, page, stage);
    let inset = BLOCK_BORDER_PX + padding_px;
    let inner = PixelRect {
        left: rect.left + inset,
        top: rect.top + inset,
        width: (rect.width - inset * 2.0).max(MIN_INNER_SIZE_PX),
        height: (rect.height - inset * 2.0).max(MIN_INNER_SIZE_PX),
    };
    let follow = line_shape(block).filter(|_| !text.trim().is_empty());
    if follow.is_some() {
        let layout = layout_in(ctx, fonts, block, text, (page, stage), (rect, padding_px, inner), follow);
        if layout.shaped {
            return layout;
        }
    }
    let inner = match block.text_shape.as_deref().and_then(text_shape_inner_rect) {
        Some([left, top, right, bottom]) => {
            let (x0, y0) = (rect.left + left * rect.width, rect.top + top * rect.height);
            let (x1, y1) = (rect.left + right * rect.width, rect.top + bottom * rect.height);
            let (x0, y0) = (x0.max(inner.left), y0.max(inner.top));
            let (x1, y1) = (x1.min(inner.left + inner.width), y1.min(inner.top + inner.height));
            PixelRect {
                left: x0,
                top: y0,
                width: (x1 - x0).max(MIN_INNER_SIZE_PX),
                height: (y1 - y0).max(MIN_INNER_SIZE_PX),
            }
        }
        None => inner,
    };
    layout_in(ctx, fonts, block, text, (page, stage), (rect, padding_px, inner), None)
}

/// The block's text shape when its lines follow it: typesetting v2, horizontal and centred.
fn line_shape(block: &TranslationBlock) -> Option<&[[f64; 2]]> {
    block.text_shape.as_deref().filter(|shape| {
        !shape.is_empty()
            && block.layout_version() >= 2
            && block.render_direction == RenderTextDirection::Horizontal
            && block.text_align == TextAlign::Center
    })
}

/// Lays `text` out in `inner`, following `follow` (the block's text shape) when given and
/// some size from the box's fit down to the readable size fits it.
fn layout_in(
    ctx: &mut FontContext,
    fonts: &BlockFonts,
    block: &TranslationBlock,
    text: &str,
    (page, stage): (Size, Size),
    (rect, padding_px, inner): (PixelRect, f64, PixelRect),
    follow: Option<&[[f64; 2]]>,
) -> BlockTextLayout {
    let fit_inner_width = (inner.width - TEXT_MEASURE_GUARD_PX * 2.0).max(MIN_INNER_SIZE_PX);
    let fit_inner_height = (inner.height - TEXT_MEASURE_GUARD_PX * 2.0).max(MIN_INNER_SIZE_PX);
    let scale = render_scale(page, stage);
    let preferred = (style_font_size_px(block) * scale).floor().max(MIN_FONT_SIZE_PX);
    let auto_fit = block.style.auto_fit_text.unwrap_or(true);
    let v2 = block.layout_version() >= 2;
    let max_font_size = if auto_fit {
        let box_cap = fit_inner_width.max(fit_inner_height);
        // The detection box holds the source text; the render box may be the whole balloon.
        let cap = match source_glyph_size_px(block, detection_rect_px(block, page, stage)).filter(|_| v2) {
            Some(glyph) => box_cap.min(glyph * SOURCE_GLYPH_GROWTH),
            None => box_cap,
        };
        MIN_FONT_SIZE_PX.max(preferred).max(cap).min(MAX_FONT_SIZE_PX)
    } else {
        preferred
    };
    let min_font_size = if v2 { (READABLE_FONT_SIZE_PX * scale).max(MIN_FONT_SIZE_PX) } else { MIN_FONT_SIZE_PX };
    let font_size_px = resolve_text_font_size(
        ctx,
        fonts,
        block,
        text,
        (min_font_size, max_font_size),
        fit_inner_width,
        fit_inner_height,
    );
    // Following the shape is harder than fitting its box, so count down from the box's fit.
    let shaped_size = follow.and_then(|shape| {
        let paragraphs = Paragraph::measure_all(ctx, fonts, text);
        let floor = min_font_size.ceil().min(font_size_px);
        let mut size = font_size_px;
        loop {
            if fit_shape(&paragraphs, block, size, shape, rect, inner).is_some() {
                return Some(size);
            }
            if !auto_fit || size - 1.0 < floor {
                return None;
            }
            size -= 1.0;
        }
    });
    let overflow = shaped_size.is_none()
        && !text.trim().is_empty()
        && !does_text_fit(ctx, fonts, block, text, font_size_px, fit_inner_width, fit_inner_height);
    BlockTextLayout {
        rect,
        padding_px,
        inner,
        fit_inner_width,
        fit_inner_height,
        font_size_px: shaped_size.unwrap_or(font_size_px),
        overflow,
        shaped: shaped_size.is_some(),
    }
}

/// Lines laid out along a block's text shape, all centred on `center_x`.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapedLines {
    pub lines: Vec<WrappedLine>,
    pub center_x: f64,
}

/// The lines of a layout that follows its block's text shape (`layout.shaped`).
pub fn shaped_lines(
    ctx: &mut FontContext,
    fonts: &BlockFonts,
    block: &TranslationBlock,
    text: &str,
    layout: &BlockTextLayout,
) -> Option<ShapedLines> {
    let shape = line_shape(block).filter(|_| layout.shaped)?;
    fit_shape(&Paragraph::measure_all(ctx, fonts, text), block, layout.font_size_px, shape, layout.rect, layout.inner)
}

/// Lines of `paragraphs` at `font_size` following `shape` (spans of `rect`) in the fit
/// area of `inner`, placed like the plain layout places lines: the fewest lines that fit,
/// each within the shape's span across its glyphs, all centred on one axis so the text
/// doesn't zigzag (koharu's balloon line profiles). `None` when no line count fits.
fn fit_shape(
    paragraphs: &[Paragraph],
    block: &TranslationBlock,
    font_size: f64,
    shape: &[[f64; 2]],
    rect: PixelRect,
    inner: PixelRect,
) -> Option<ShapedLines> {
    let line_height = font_size * style_line_height(block);
    let spacing = letter_spacing_px(block, font_size);
    let (left, right) = (inner.left + TEXT_MEASURE_GUARD_PX, inner.left + inner.width - TEXT_MEASURE_GUARD_PX);
    let fit_height = (inner.height - TEXT_MEASURE_GUARD_PX * 2.0).max(MIN_INNER_SIZE_PX);
    let (_, fy) = text_position_factors(block.text_position);
    let (rect_width, rect_height) = (rect.width.max(1.0), rect.height.max(1.0));
    let segments: usize = paragraphs.iter().map(|paragraph| paragraph.segments.len().max(1)).sum();
    let most = ((fit_height / line_height.max(1.0)).floor() as usize).min(segments);
    for count in paragraphs.len().max(1)..=most {
        let total = count as f64 * line_height;
        let start_y = inner.top + (inner.height - total) * fy + ((line_height - font_size) / 2.0).max(0.0);
        let spans: Option<Vec<(f64, f64)>> = (0..count)
            .map(|line| {
                let top = start_y + line as f64 * line_height;
                let (l, r) =
                    text_shape_span(shape, (top - rect.top) / rect_height, (top + font_size - rect.top) / rect_height)?;
                let (l, r) = ((rect.left + l * rect_width).max(left), (rect.left + r * rect_width).min(right));
                (r > l).then_some((l, r))
            })
            .collect();
        let Some(spans) = spans else { continue };
        let common_left = spans.iter().map(|span| span.0).fold(f64::NEG_INFINITY, f64::max);
        let common_right = spans.iter().map(|span| span.1).fold(f64::INFINITY, f64::min);
        if common_right <= common_left {
            continue;
        }
        let center =
            (spans.iter().map(|(l, r)| (l + r) / 2.0).sum::<f64>() / count as f64).clamp(common_left, common_right);
        let widths: Vec<f64> = spans.iter().map(|(l, r)| 2.0 * (center - l).min(r - center)).collect();
        if let Some(lines) = break_paragraphs(paragraphs, &widths, font_size, spacing) {
            return Some(ShapedLines { lines, center_x: center });
        }
    }
    None
}

/// A paragraph's line count and the segment index each of its lines ends before.
type ParagraphSplit = (usize, Vec<usize>);

/// Breaks the paragraphs into consecutive lines of `widths` (one per line, in order), every
/// line within its width, at the least [`balanced_breaks`] cost. `None` when none fits.
fn break_paragraphs(
    paragraphs: &[Paragraph],
    widths: &[f64],
    font_size: f64,
    spacing: f64,
) -> Option<Vec<WrappedLine>> {
    let count = widths.len();
    // cost[p][l]: best cost of paragraphs ..p in lines ..l; how[p][l]: the last paragraph's
    // line count and line ends there.
    let mut cost = vec![vec![f64::INFINITY; count + 1]; paragraphs.len() + 1];
    let mut how: Vec<Vec<Option<ParagraphSplit>>> = vec![vec![None; count + 1]; paragraphs.len() + 1];
    cost[0][0] = 0.0;
    for (index, paragraph) in paragraphs.iter().enumerate() {
        for first in 0..count {
            if !cost[index][first].is_finite() {
                continue;
            }
            let most = paragraph.segments.len().max(1).min(count - first);
            for lines in 1..=most {
                let split = if paragraph.segments.is_empty() {
                    Some((Vec::new(), 0.0))
                } else {
                    balanced_breaks(
                        &paragraph.chars,
                        &paragraph.segments,
                        &widths[first..first + lines],
                        true,
                        font_size,
                        |start, end| paragraph.width(start, end, font_size, spacing),
                    )
                };
                if let Some((ends, split_cost)) = split
                    && cost[index][first] + split_cost < cost[index + 1][first + lines]
                {
                    cost[index + 1][first + lines] = cost[index][first] + split_cost;
                    how[index + 1][first + lines] = Some((lines, ends));
                }
            }
        }
    }
    let mut ends = Vec::with_capacity(paragraphs.len());
    let mut line = count;
    for index in (1..=paragraphs.len()).rev() {
        let (lines, paragraph_ends) = how[index][line].clone()?;
        ends.push(paragraph_ends);
        line -= lines;
    }
    let mut out = Vec::with_capacity(count);
    for (paragraph, ends) in paragraphs.iter().zip(ends.into_iter().rev()) {
        if paragraph.segments.is_empty() {
            out.push(WrappedLine { text: String::new(), width: 0.0 });
        }
        let mut first = 0;
        for end in ends {
            out.push(paragraph.line(first, end, font_size, spacing));
            first = end;
        }
    }
    Some(out)
}

/// Rows of the block's text shape inside `rect`, for drawing its background.
pub fn text_shape_strips(block: &TranslationBlock, rect: PixelRect) -> Option<Vec<PixelRect>> {
    let shape = block.text_shape.as_deref().filter(|shape| !shape.is_empty())?;
    let rows = shape.len() as f64;
    Some(
        shape
            .iter()
            .enumerate()
            .filter(|(_, [left, right])| right > left)
            .map(|(row, [left, right])| PixelRect {
                left: rect.left + left * rect.width,
                top: rect.top + rect.height * row as f64 / rows,
                width: (right - left) * rect.width,
                height: rect.height / rows,
            })
            .collect(),
    )
}

/// Rough size of the source glyphs in `rect`: the box area shared by the source
/// characters, at most the box's shorter side. `None` without source text.
fn source_glyph_size_px(block: &TranslationBlock, rect: PixelRect) -> Option<f64> {
    let count = block.source_text.chars().filter(|ch| !is_js_whitespace(*ch)).count();
    (count > 0).then(|| {
        let cell = (rect.width * rect.height / (count as f64 * SOURCE_GLYPH_SPACING)).sqrt();
        cell.min(rect.width.min(rect.height))
    })
}

/// The largest whole size in `min..=max` that fits (`min` when none does).
fn resolve_text_font_size(
    ctx: &mut FontContext,
    fonts: &BlockFonts,
    block: &TranslationBlock,
    text: &str,
    (min_font_size, max_font_size): (f64, f64),
    inner_width: f64,
    inner_height: f64,
) -> f64 {
    let capped = max_font_size.floor().clamp(MIN_FONT_SIZE_PX, MAX_FONT_SIZE_PX);
    if !block.style.auto_fit_text.unwrap_or(true) || text.trim().is_empty() {
        return capped;
    }
    let floor = min_font_size.ceil().min(capped) as i64;
    let (mut low, mut high, mut best) = (floor, capped as i64, floor);
    while low <= high {
        let mid = low + (high - low) / 2;
        if does_text_fit(ctx, fonts, block, text, mid as f64, inner_width, inner_height) {
            best = mid;
            low = mid + 1;
        } else {
            high = mid - 1;
        }
    }
    (best as f64).min(capped)
}

fn does_text_fit(
    ctx: &mut FontContext,
    fonts: &BlockFonts,
    block: &TranslationBlock,
    text: &str,
    font_size: f64,
    inner_width: f64,
    inner_height: f64,
) -> bool {
    let line_height = font_size * style_line_height(block);
    if block.render_direction == RenderTextDirection::Vertical {
        return measure_vertical_text(block, text, font_size, inner_width, inner_height, line_height);
    }
    // Balancing keeps the line count and widths within the limit, so the plain wrap decides.
    let lines = wrap_lines(ctx, fonts, block, text, font_size, inner_width, false);
    let total_height = lines.len() as f64 * line_height;
    let widest = lines.iter().map(|line| line.width).fold(0.0, f64::max);
    total_height <= inner_height && widest <= inner_width
}

fn measure_vertical_text(
    block: &TranslationBlock,
    text: &str,
    font_size: f64,
    max_width: f64,
    max_height: f64,
    line_height: f64,
) -> bool {
    let count = text.chars().filter(|ch| !is_js_whitespace(*ch)).count();
    if count == 0 {
        return true;
    }
    let spacing = letter_spacing_px(block, font_size);
    let advance = (font_size.max(line_height) + spacing).max(1.0);
    let chars_per_column = ((max_height + spacing) / advance.max(1.0)).floor().max(1.0);
    let columns = (count as f64 / chars_per_column).ceil().max(1.0);
    columns * font_size * 1.15 <= max_width
}

#[derive(Debug, Clone, PartialEq)]
pub struct WrappedLine {
    pub text: String,
    /// Width including letter spacing at the requested font size.
    pub width: f64,
}

/// `wrapTextToWidth`: greedy wrap at spaces and soft-wrap marks (ellipses, tildes, long
/// dashes), never inside a word. With typesetting v2 the same number of lines is then
/// balanced (see [`balanced_breaks`]).
pub fn wrap_text_to_width(
    ctx: &mut FontContext,
    fonts: &BlockFonts,
    block: &TranslationBlock,
    text: &str,
    font_size: f64,
    max_width: f64,
) -> Vec<WrappedLine> {
    wrap_lines(ctx, fonts, block, text, font_size, max_width, block.layout_version() >= 2)
}

fn wrap_lines(
    ctx: &mut FontContext,
    fonts: &BlockFonts,
    block: &TranslationBlock,
    text: &str,
    font_size: f64,
    max_width: f64,
    balance: bool,
) -> Vec<WrappedLine> {
    let spacing = letter_spacing_px(block, font_size);
    let mut lines = Vec::new();
    for paragraph in Paragraph::measure_all(ctx, fonts, text) {
        let segments = &paragraph.segments;
        if segments.is_empty() {
            lines.push(WrappedLine { text: String::new(), width: 0.0 });
            continue;
        }
        let width_of = |start: usize, end: usize| paragraph.width(start, end, font_size, spacing);
        // Segment index each line ends before.
        let mut ends = Vec::new();
        let mut start = segments[0].0;
        for (index, &(segment_start, segment_end)) in segments.iter().enumerate().skip(1) {
            if width_of(start, segment_end) > max_width {
                ends.push(index);
                start = segment_start;
            }
        }
        ends.push(segments.len());
        if balance
            && ends.len() > 1
            && let Some((balanced, _)) =
                balanced_breaks(&paragraph.chars, segments, &vec![max_width; ends.len()], false, font_size, width_of)
        {
            ends = balanced;
        }
        let mut first = 0;
        for end in ends {
            lines.push(paragraph.line(first, end, font_size, spacing));
            first = end;
        }
    }
    if lines.is_empty() {
        lines.push(WrappedLine { text: text.to_owned(), width: 0.0 });
    }
    lines
}

/// A paragraph measured for wrapping: its chars, advance sums up to each char (em), so a
/// line's width is one subtraction, and its wrap segments (none when it is empty).
struct Paragraph {
    chars: Vec<char>,
    prefix_em: Vec<f64>,
    segments: Vec<(usize, usize)>,
}

impl Paragraph {
    fn measure_all(ctx: &mut FontContext, fonts: &BlockFonts, text: &str) -> Vec<Self> {
        text.replace('\r', "")
            .split('\n')
            .map(|paragraph| {
                let normalized = collapse_whitespace(paragraph);
                if normalized.is_empty() {
                    return Self { chars: Vec::new(), prefix_em: vec![0.0], segments: Vec::new() };
                }
                let run = ctx.shape(&normalized, &fonts.base, &fonts.spans_for(&normalized));
                let chars: Vec<char> = normalized.chars().collect();
                let mut prefix_em = Vec::with_capacity(chars.len() + 1);
                prefix_em.push(0.0f64);
                for advance in &run.char_advances_em[..chars.len()] {
                    prefix_em.push(prefix_em.last().copied().unwrap_or_default() + f64::from(*advance));
                }
                let segments = split_text_wrap_segments(&chars);
                Self { chars, prefix_em, segments }
            })
            .collect()
    }

    /// Width of chars `start..end` at `font_size`, `spacing` between letters.
    fn width(&self, start: usize, end: usize, font_size: f64, spacing: f64) -> f64 {
        let count = (end - start) as f64;
        (self.prefix_em[end] - self.prefix_em[start]) * font_size
            + if count > 1.0 { spacing * (count - 1.0) } else { 0.0 }
    }

    /// The line of segments `first..end`.
    fn line(&self, first: usize, end: usize, font_size: f64, spacing: f64) -> WrappedLine {
        let (start, end) = (self.segments[first].0, self.segments[end - 1].1);
        WrappedLine { text: self.chars[start..end].iter().collect(), width: self.width(start, end, font_size, spacing) }
    }
}

/// Splits `segments` into one line per entry of `widths`, each no wider than its width (a
/// lone segment may be wider unless `strict`), with the least cost: lines filling their
/// widths, breaks after punctuation. Returns the segment index each line ends before and
/// the cost; `None` when no split fits.
fn balanced_breaks(
    chars: &[char],
    segments: &[(usize, usize)],
    widths: &[f64],
    strict: bool,
    font_size: f64,
    width_of: impl Fn(usize, usize) -> f64,
) -> Option<(Vec<usize>, f64)> {
    let (count, line_count) = (segments.len(), widths.len());
    if line_count == 0 || line_count > count {
        return None;
    }
    let em = font_size.max(1.0);
    let break_cost = |end: usize| -> f64 {
        let next = chars[segments[end].0];
        if matches!(next, '…' | '⋯' | '~' | '～' | '〜' | 'ー' | '—' | '―' | '.') {
            return BREAK_BEFORE_MARK_COST;
        }
        match chars[segments[end - 1].1 - 1] {
            '.' | '!' | '?' | '…' | '⋯' | '！' | '？' | '。' | '‼' | '~' | '～' | '〜' => 0.0,
            ',' | '、' | '，' | ';' | ':' | '—' | '―' => BREAK_AFTER_COMMA_COST,
            _ => BREAK_MID_PHRASE_COST,
        }
    };
    // cost[k][j]: best cost of segments ..j in k lines; from[k][j]: where that last line starts.
    let mut cost = vec![vec![f64::INFINITY; count + 1]; line_count + 1];
    let mut from = vec![vec![0usize; count + 1]; line_count + 1];
    cost[0][0] = 0.0;
    for lines in 1..=line_count {
        for end in lines..=count {
            for start in (lines - 1..end).rev() {
                let (width, max_width) = (width_of(segments[start].0, segments[end - 1].1), widths[lines - 1]);
                if width > max_width && (strict || end - start > 1) {
                    break;
                }
                if !cost[lines - 1][start].is_finite() {
                    continue;
                }
                let slack = (max_width - width).max(0.0) / em;
                let mut total = cost[lines - 1][start] + slack * slack;
                if end < count {
                    total += break_cost(end);
                }
                if total < cost[lines][end] {
                    cost[lines][end] = total;
                    from[lines][end] = start;
                }
            }
        }
    }
    if !cost[line_count][count].is_finite() {
        return None;
    }
    let mut ends = vec![count];
    let mut end = count;
    for lines in (2..=line_count).rev() {
        end = from[lines][end];
        ends.push(end);
    }
    ends.reverse();
    Some((ends, cost[line_count][count]))
}

/// Width of `text` with letter spacing (`measureTextWidthWithLetterSpacing`).
pub fn measure_text_width(
    ctx: &mut FontContext,
    fonts: &BlockFonts,
    block: &TranslationBlock,
    text: &str,
    font_size: f64,
) -> f64 {
    let count = text.chars().count();
    let run = ctx.shape(text, &fonts.base, &fonts.spans_for(text));
    let spacing = if count > 1 { letter_spacing_px(block, font_size) * (count as f64 - 1.0) } else { 0.0 };
    f64::from(run.advance_em) * font_size + spacing
}

/// JavaScript `\s` (Unicode White_Space plus U+FEFF).
pub fn is_js_whitespace(ch: char) -> bool {
    ch.is_whitespace() || ch == '\u{FEFF}'
}

/// `paragraph.replace(/\s+/g, " ").trim()`.
pub fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    for ch in text.chars() {
        if is_js_whitespace(ch) {
            pending_space = !out.is_empty();
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            out.push(ch);
        }
    }
    out
}

/// Char ranges of wrap segments; a segment that starts a new word is preceded by a space.
fn split_text_wrap_segments(chars: &[char]) -> Vec<(usize, usize)> {
    let mut segments = Vec::new();
    let mut word_start = 0;
    for index in 0..=chars.len() {
        if index == chars.len() || chars[index] == ' ' {
            for (start, end) in split_word_at_soft_wrap_marks(&chars[word_start..index]) {
                segments.push((word_start + start, word_start + end));
            }
            word_start = index + 1;
        }
    }
    if segments.is_empty() {
        segments.push((0, chars.len()));
    }
    segments
}

/// Splits a word around matches of `[—―]*(?:[…⋯]+|\.{3,}|[~～〜ー]+)[.!?！？。．…⋯]*`, keeping
/// the matches as their own parts (`splitWordAtSoftWrapMarks`).
fn split_word_at_soft_wrap_marks(word: &[char]) -> Vec<(usize, usize)> {
    let mut parts = Vec::new();
    let mut last = 0;
    let mut index = 0;
    while index < word.len() {
        match match_soft_wrap_mark(word, index) {
            Some(end) => {
                if index > last {
                    parts.push((last, index));
                }
                parts.push((index, end));
                last = end;
                index = end;
            }
            None => index += 1,
        }
    }
    if last < word.len() {
        parts.push((last, word.len()));
    }
    if parts.is_empty() {
        parts.push((0, word.len()));
    }
    parts
}

fn match_soft_wrap_mark(word: &[char], start: usize) -> Option<usize> {
    let mut index = start;
    while index < word.len() && matches!(word[index], '—' | '―') {
        index += 1;
    }
    let core_start = index;
    let take_while = |mut i: usize, predicate: fn(char) -> bool| {
        while i < word.len() && predicate(word[i]) {
            i += 1;
        }
        i
    };
    let first = *word.get(core_start)?;
    let core_end = if matches!(first, '…' | '⋯') {
        take_while(core_start, |c| matches!(c, '…' | '⋯'))
    } else if first == '.' {
        let end = take_while(core_start, |c| c == '.');
        if end - core_start < 3 {
            return None;
        }
        end
    } else if matches!(first, '~' | '～' | '〜' | 'ー') {
        take_while(core_start, |c| matches!(c, '~' | '～' | '〜' | 'ー'))
    } else {
        return None;
    };
    Some(take_while(core_end, |c| matches!(c, '.' | '!' | '?' | '！' | '？' | '。' | '．' | '…' | '⋯')))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(word: &str) -> Vec<String> {
        let chars: Vec<char> = word.chars().collect();
        split_word_at_soft_wrap_marks(&chars).into_iter().map(|(s, e)| chars[s..e].iter().collect()).collect()
    }

    #[test]
    fn soft_wrap_marks_match_js_regex_split() {
        assert_eq!(split("그래서…정말"), ["그래서", "…", "정말"]);
        assert_eq!(split("아..."), ["아", "..."]);
        assert_eq!(split("아.."), ["아.."]);
        assert_eq!(split("——…!?끝"), ["——…!?", "끝"]);
        assert_eq!(split("에~~에"), ["에", "~~", "에"]);
        assert_eq!(split("....!.."), ["....!.."]);
        assert_eq!(split("——끝"), ["——끝"]);
    }

    #[test]
    fn whitespace_collapse_matches_js() {
        assert_eq!(collapse_whitespace("  a \t b\u{3000}c  "), "a b c");
        assert_eq!(collapse_whitespace("\u{FEFF}"), "");
    }

    #[test]
    fn wrap_segments_track_word_boundaries() {
        let chars: Vec<char> = "ab cd…ef".chars().collect();
        assert_eq!(split_text_wrap_segments(&chars), vec![(0, 2), (3, 5), (5, 6), (6, 8)]);
    }

    /// Lines `balanced_breaks` picks for `text` (1em per char, segments at spaces).
    fn balance(text: &str, line_count: usize, max_width: f64) -> Option<Vec<String>> {
        let chars: Vec<char> = text.chars().collect();
        let segments = split_text_wrap_segments(&chars);
        let (ends, _) =
            balanced_breaks(&chars, &segments, &vec![max_width; line_count], false, 1.0, |a, b| (b - a) as f64)?;
        let mut first = 0;
        Some(
            ends.into_iter()
                .map(|end| {
                    let line = chars[segments[first].0..segments[end - 1].1].iter().collect();
                    first = end;
                    line
                })
                .collect(),
        )
    }

    #[test]
    fn balanced_breaks_even_out_lines() {
        // Greedy: "가 나 다 라" / "마".
        assert_eq!(balance("가 나 다 라 마", 2, 7.0).unwrap(), ["가 나 다", "라 마"]);
        assert_eq!(
            balance("가나다 라마바 사아 자", 2, 7.0).unwrap(),
            ["가나다 라마바", "사아 자"],
            "only split that fits"
        );
        assert_eq!(balance("가나다라마바사", 2, 7.0), None, "one segment can't make two lines");
    }

    #[test]
    fn balanced_breaks_prefer_punctuation() {
        // Greedy: "뭐야? 너" / "누구야!".
        assert_eq!(balance("뭐야? 너 누구야!", 2, 7.0).unwrap(), ["뭐야?", "너 누구야!"]);
        // A line doesn't start with the ellipsis when it can be avoided.
        assert_eq!(balance("그래서…정말 그랬어", 2, 6.0).unwrap(), ["그래서…", "정말 그랬어"]);
    }

    #[test]
    fn source_glyph_size_follows_the_source_text() {
        let rect = PixelRect { left: 0.0, top: 0.0, width: 60.0, height: 240.0 };
        let block = |source: &str| TranslationBlock { source_text: source.into(), ..TranslationBlock::default() };
        assert_eq!(source_glyph_size_px(&block(""), rect), None);
        // 10 glyphs in 60×240: cells of 1440/1.2 → 34.6px.
        let size = source_glyph_size_px(&block("あいうえおかきくけこ"), rect).unwrap();
        assert!((size - 34.64).abs() < 0.01, "{size}");
        assert_eq!(source_glyph_size_px(&block("あ"), rect), Some(60.0), "at most the shorter side");
    }

    #[test]
    fn v2_caps_growth_by_the_source_glyphs_and_stops_at_a_readable_size() {
        use mtt_core::model::{BBox, LAYOUT_VERSION};
        let mut ctx = FontContext::new();
        let page = Size::new(1000.0, 1000.0);
        let size_of = |ctx: &mut FontContext, block: &TranslationBlock, text: &str| {
            let fonts = BlockFonts::resolve(ctx, block);
            let layout = resolve_block_text_layout(ctx, &fonts, block, text, page, page);
            (layout.font_size_px, layout.overflow)
        };
        // 20 source glyphs in 300×300: cells of about 61px, so at most about 73px.
        let mut block = TranslationBlock {
            bbox: BBox::new(100.0, 100.0, 300.0, 300.0),
            source_text: "あいうえおかきくけこさしすせそたちつてと".into(),
            ..TranslationBlock::default()
        };
        let (v1, _) = size_of(&mut ctx, &block, "응?");
        block.layout_version = Some(LAYOUT_VERSION);
        let (v2, _) = size_of(&mut ctx, &block, "응?");
        assert!(v2 <= 74.0 && v2 < v1, "v1 {v1}, v2 {v2}");

        let long = "아주 긴 문장이 이 작은 상자에 들어갑니다 정말 길어요";
        let mut tiny = TranslationBlock { bbox: BBox::new(0.0, 0.0, 30.0, 30.0), ..TranslationBlock::default() };
        let (v1, _) = size_of(&mut ctx, &tiny, long);
        tiny.layout_version = Some(LAYOUT_VERSION);
        let (v2, overflow) = size_of(&mut ctx, &tiny, long);
        // Only meaningful where some font measures the text.
        if v1 < READABLE_FONT_SIZE_PX {
            assert_eq!(v2, READABLE_FONT_SIZE_PX);
            assert!(overflow);
        }
    }

    /// A 400×300 render box at (300, 250) whose text shape is the ellipse inside it.
    fn balloon_block(text_align: TextAlign, render_direction: RenderTextDirection) -> TranslationBlock {
        use mtt_core::model::{BBox, LAYOUT_VERSION};
        let rows = 40;
        let shape = (0..rows)
            .map(|row| {
                // The narrower edge of the row.
                let y = [row, row + 1].map(|edge| (edge as f64 / rows as f64 * 2.0 - 1.0).abs());
                let half = 0.5 * (1.0 - y[0].max(y[1]).powi(2)).max(0.0).sqrt();
                [0.5 - half, 0.5 + half]
            })
            .collect();
        TranslationBlock {
            bbox: BBox::new(450.0, 300.0, 100.0, 200.0),
            render_bbox: Some(BBox::new(300.0, 250.0, 400.0, 300.0)),
            text_shape: Some(shape),
            layout_version: Some(LAYOUT_VERSION),
            text_align,
            render_direction,
            ..TranslationBlock::default()
        }
    }

    #[test]
    fn lines_follow_the_text_shape() {
        let mut ctx = FontContext::new();
        let page = Size::new(1000.0, 1000.0);
        let text = "Wait for me, I am coming with you to the old harbour tonight no matter what they say";
        let block = balloon_block(TextAlign::Center, RenderTextDirection::Horizontal);
        let fonts = BlockFonts::resolve(&ctx, &block);
        // Only meaningful where some font measures the text.
        if measure_text_width(&mut ctx, &fonts, &block, text, 20.0) <= 0.0 {
            return;
        }
        let layout = resolve_block_text_layout(&mut ctx, &fonts, &block, text, page, page);
        assert!(layout.shaped && !layout.overflow, "{layout:?}");
        let shaped = shaped_lines(&mut ctx, &fonts, &block, text, &layout).unwrap();
        let widths: Vec<f64> = shaped.lines.iter().map(|line| line.width).collect();
        assert!(widths.len() >= 3, "{:?}", shaped.lines);
        let middle = widths[widths.len() / 2];
        assert!(middle > widths[0] && middle > widths[widths.len() - 1], "wider in the middle: {widths:?}");
        assert!((shaped.center_x - 500.0).abs() < 1.0, "on the balloon's axis: {}", shaped.center_x);

        // Left-aligned text keeps to the ellipse's largest rectangle, so it must be smaller.
        let left = balloon_block(TextAlign::Left, RenderTextDirection::Horizontal);
        let boxed = resolve_block_text_layout(&mut ctx, &fonts, &left, text, page, page);
        assert!(!boxed.shaped && boxed.inner.width < 300.0, "{boxed:?}");
        assert!(layout.font_size_px > boxed.font_size_px, "{} vs {}", layout.font_size_px, boxed.font_size_px);
    }

    #[test]
    fn vertical_text_keeps_to_the_shape_rectangle() {
        let mut ctx = FontContext::new();
        let page = Size::new(1000.0, 1000.0);
        let block = balloon_block(TextAlign::Center, RenderTextDirection::Vertical);
        let fonts = BlockFonts::resolve(&ctx, &block);
        let layout = resolve_block_text_layout(&mut ctx, &fonts, &block, "가나다라", page, page);
        let inner = layout.inner;
        assert!(!layout.shaped);
        assert!(inner.left > 340.0 && inner.left + inner.width < 660.0, "{inner:?}");
        assert!(inner.top > 280.0 && inner.top + inner.height < 520.0, "{inner:?}");
        // Without a shape: the whole box inside the padding.
        let plain = TranslationBlock { text_shape: None, ..block };
        let layout = resolve_block_text_layout(&mut ctx, &fonts, &plain, "가나다라", page, page);
        assert_eq!(layout.inner.width, 400.0 - (BLOCK_BORDER_PX + layout.padding_px) * 2.0);
    }

    #[test]
    fn paragraphs_break_into_their_own_lines_of_the_shape() {
        let paragraphs = [
            Paragraph {
                chars: "ab cd".chars().collect(),
                prefix_em: (0..=5).map(f64::from).collect(),
                segments: vec![(0, 2), (3, 5)],
            },
            Paragraph { chars: Vec::new(), prefix_em: vec![0.0], segments: Vec::new() },
            Paragraph { chars: "ef".chars().collect(), prefix_em: vec![0.0, 1.0, 2.0], segments: vec![(0, 2)] },
        ];
        let text = |lines: Vec<WrappedLine>| lines.into_iter().map(|line| line.text).collect::<Vec<_>>();
        // "ab cd" is 5em: it needs two lines of 4em, then the blank line and "ef".
        let lines = break_paragraphs(&paragraphs, &[4.0, 4.0, 4.0, 4.0], 1.0, 0.0).unwrap();
        assert_eq!(text(lines), ["ab", "cd", "", "ef"]);
        assert_eq!(break_paragraphs(&paragraphs, &[5.0, 5.0, 5.0], 1.0, 0.0).map(text).unwrap(), ["ab cd", "", "ef"]);
        assert_eq!(break_paragraphs(&paragraphs, &[4.0, 4.0, 1.0], 1.0, 0.0), None, "\"ef\" is too wide");
    }

    #[test]
    fn v2_judges_bold_by_the_family_that_draws() {
        let ctx = FontContext::new();
        // Any installed family with a bold face (fonts differ between machines).
        let Some(family) = ctx
            .system_families()
            .iter()
            .find(|family| family.weights.contains(&400) && family.weights.iter().any(|w| *w >= 700))
        else {
            return;
        };
        let mut block = TranslationBlock::default();
        block.style.font_family = Some(format!("\"No Such Font 75\", {}", family.css_family));
        block.style.font_weight = Some(700.0);
        let original = BlockFonts::resolve(&ctx, &block);
        assert!(original.synthetic_bold, "the original rules look at the first family only");
        assert!(original.base.weight < BOLD_WEIGHT_THRESHOLD, "{:?}", original.base);
        block.layout_version = Some(2);
        let v2 = BlockFonts::resolve(&ctx, &block);
        assert!(!v2.synthetic_bold, "{} has a bold face", family.family);
        assert!(v2.base.weight >= BOLD_WEIGHT_THRESHOLD, "{:?}", v2.base);
        assert_eq!(v2.base.family, original.base.family);
    }

    #[test]
    fn huge_style_numbers_lay_out_without_overflowing() {
        use mtt_core::model::BBox;
        let mut ctx = FontContext::new();
        let page = Size::new(1000.0, 1000.0);
        for auto_fit in [true, false] {
            let mut block = TranslationBlock {
                bbox: BBox::new(100.0, 100.0, 300.0, 200.0),
                translated_text: "가나다".into(),
                ..TranslationBlock::default()
            };
            block.style.auto_fit_text = Some(auto_fit);
            block.style.font_size_px = Some(1e300);
            block.style.line_height = Some(1e300);
            block.style.letter_spacing_px = Some(-1e300);
            let fonts = BlockFonts::resolve(&ctx, &block);
            let layout = resolve_block_text_layout(&mut ctx, &fonts, &block, "가나다", page, page);
            assert!(layout.font_size_px.is_finite() && layout.font_size_px <= MAX_FONT_SIZE_PX, "{layout:?}");
            crate::block::render_block(&mut ctx, page, &block, crate::block::BlockRenderOptions::default());
            // A box far larger than any page.
            block.bbox = BBox::new(0.0, 0.0, 1e300, 1e300);
            let layout = resolve_block_text_layout(&mut ctx, &fonts, &block, "가나다", page, page);
            assert!(layout.font_size_px.is_finite() && layout.font_size_px <= MAX_FONT_SIZE_PX, "{layout:?}");
        }
    }

    #[test]
    fn padding_rules() {
        let rect = |w: f64, h: f64| PixelRect { left: 0.0, top: 0.0, width: w, height: h };
        assert_eq!(auto_block_padding_px(rect(40.0, 200.0)), 0.0);
        assert_eq!(auto_block_padding_px(rect(60.0, 200.0)), 1.0);
        assert_eq!(auto_block_padding_px(rect(90.0, 200.0)), 2.0);
        assert_eq!(auto_block_padding_px(rect(100.0, 200.0)), 6.0);
        assert_eq!(auto_block_padding_px(rect(1000.0, 1000.0)), 14.0);
    }

    #[test]
    fn vertical_fit_estimate() {
        let block = TranslationBlock::default();
        // 10 chars, 20px font, 24px advance -> 4 per column in 100px -> 3 columns * 23px = 69px.
        assert!(measure_vertical_text(&block, "가나다라마바사아자차", 20.0, 70.0, 100.0, 24.0));
        assert!(!measure_vertical_text(&block, "가나다라마바사아자차", 20.0, 68.0, 100.0, 24.0));
    }
}
