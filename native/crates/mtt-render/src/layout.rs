//! Block text layout (`overlayLayout.ts`): padding, wrapping and auto-fit font size.

use std::collections::HashMap;

use mtt_core::font_presets::{
    DEFAULT_FONT_BACKGROUND_OPACITY, DEFAULT_FONT_BACKGROUND_RADIUS_PX, DEFAULT_FONT_BACKGROUND_SCALE,
    DEFAULT_OVERLAY_FONT_FAMILY, DEFAULT_OVERLAY_FONT_WEIGHT, DEFAULT_SCREENTONE_FILL_DENSITY,
    DEFAULT_SCREENTONE_FILL_INTENSITY, MAX_FONT_BACKGROUND_RADIUS_PX, MAX_FONT_BACKGROUND_SCALE,
    MIN_FONT_BACKGROUND_RADIUS_PX, MIN_FONT_BACKGROUND_SCALE,
};
use mtt_core::geometry::{bbox_to_pixels, clamp, clamp_text_padding_px, resolve_block_render_bbox};
use mtt_core::model::{RenderTextDirection, TextFontStyle, TextPosition, TranslationBlock};

use crate::fonts::{BOLD_WEIGHT_THRESHOLD, FontContext, FontSpec};

const MIN_FONT_SIZE_PX: f64 = 2.0;
const MAX_BLOCK_PADDING_PX: f64 = 14.0;
const MIN_INNER_SIZE_PX: f64 = 1.0;
pub const BLOCK_BORDER_PX: f64 = 1.0;
const TEXT_FIT_SAFETY_PX: f64 = 6.0;
const TEXT_MEASURE_GUARD_PX: f64 = TEXT_FIT_SAFETY_PX + 4.0;
const SYNTHETIC_BOLD_FALLBACK_BASE_WEIGHT: u16 = 400;
const SYNTHETIC_BOLD_MIN_STROKE_WIDTH_PX: f64 = 0.35;
const SYNTHETIC_BOLD_MAX_STROKE_WIDTH_PX: f64 = 8.0;
const SYNTHETIC_BOLD_MIN_RATIO: f64 = 0.018;
const SYNTHETIC_BOLD_MAX_RATIO: f64 = 0.05;
const SYNTHETIC_ITALIC_SKEW_DEG: f64 = -12.0;

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
    pub inner_width: f64,
    pub inner_height: f64,
    pub fit_inner_width: f64,
    pub fit_inner_height: f64,
    pub font_size_px: f64,
    pub overflow: bool,
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
        let base = ctx.resolve_spec(family, canvas_font_weight(ctx, family, weight));
        let mut overrides = HashMap::new();
        for entry in block.style.character_font_overrides.iter().flatten() {
            let (Some(ch), override_family) = (entry.character.chars().next(), entry.font_family.trim()) else {
                continue;
            };
            if !override_family.is_empty() {
                overrides
                    .insert(ch, ctx.resolve_spec(override_family, canvas_font_weight(ctx, override_family, weight)));
            }
        }
        Self { base, overrides, synthetic_bold: weight >= BOLD_WEIGHT_THRESHOLD && !has_native_bold(ctx, family) }
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

fn has_native_bold(ctx: &FontContext, family: &str) -> bool {
    ctx.family_weights(family)
        .map(|(weights, variable_bold)| variable_bold || weights.iter().any(|weight| *weight >= BOLD_WEIGHT_THRESHOLD))
        .unwrap_or(false)
}

/// `resolveOverlayCanvasFontWeight`: without a native bold face, draw the regular face and
/// embolden it with a stroke instead.
fn canvas_font_weight(ctx: &FontContext, family: &str, weight: u16) -> u16 {
    if weight < BOLD_WEIGHT_THRESHOLD || has_native_bold(ctx, family) {
        return weight;
    }
    ctx.family_weights(family)
        .and_then(|(weights, _)| weights.into_iter().filter(|w| *w > 0 && *w < BOLD_WEIGHT_THRESHOLD).max())
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

/// `resolveTextLetterSpacingPx`: letter spacing scales with the rendered font size.
pub fn letter_spacing_px(block: &TranslationBlock, font_size: f64) -> f64 {
    let source = block.style.font_size_px.unwrap_or(font_size).max(1.0);
    block.style.letter_spacing_px.unwrap_or(0.0) * (font_size / source)
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

/// `resolveBlockTextLayout`.
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
    let border_inset = BLOCK_BORDER_PX * 2.0;
    let inner_width = (rect.width - padding_px * 2.0 - border_inset).max(MIN_INNER_SIZE_PX);
    let inner_height = (rect.height - padding_px * 2.0 - border_inset).max(MIN_INNER_SIZE_PX);
    let fit_inner_width = (inner_width - TEXT_MEASURE_GUARD_PX * 2.0).max(MIN_INNER_SIZE_PX);
    let fit_inner_height = (inner_height - TEXT_MEASURE_GUARD_PX * 2.0).max(MIN_INNER_SIZE_PX);
    let preferred = (block.font_size_px() * render_scale(page, stage)).floor().max(MIN_FONT_SIZE_PX);
    let auto_fit = block.style.auto_fit_text.unwrap_or(true);
    let max_font_size =
        if auto_fit { MIN_FONT_SIZE_PX.max(preferred).max(fit_inner_width).max(fit_inner_height) } else { preferred };
    let font_size_px =
        resolve_text_font_size(ctx, fonts, block, text, max_font_size, fit_inner_width, fit_inner_height);
    let overflow = !text.trim().is_empty()
        && !does_text_fit(ctx, fonts, block, text, font_size_px, fit_inner_width, fit_inner_height);
    BlockTextLayout {
        rect,
        padding_px,
        inner_width,
        inner_height,
        fit_inner_width,
        fit_inner_height,
        font_size_px,
        overflow,
    }
}

fn resolve_text_font_size(
    ctx: &mut FontContext,
    fonts: &BlockFonts,
    block: &TranslationBlock,
    text: &str,
    max_font_size: f64,
    inner_width: f64,
    inner_height: f64,
) -> f64 {
    let capped = max_font_size.floor().max(MIN_FONT_SIZE_PX);
    if !block.style.auto_fit_text.unwrap_or(true) || text.trim().is_empty() {
        return capped;
    }
    let (mut low, mut high, mut best) = (MIN_FONT_SIZE_PX as i64, capped as i64, MIN_FONT_SIZE_PX as i64);
    while low <= high {
        let mid = (low + high).div_euclid(2);
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
    let line_height = font_size * block.line_height();
    if block.render_direction == RenderTextDirection::Vertical {
        return measure_vertical_text(block, text, font_size, inner_width, inner_height, line_height);
    }
    let lines = wrap_text_to_width(ctx, fonts, block, text, font_size, inner_width);
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
/// dashes), never inside a word.
pub fn wrap_text_to_width(
    ctx: &mut FontContext,
    fonts: &BlockFonts,
    block: &TranslationBlock,
    text: &str,
    font_size: f64,
    max_width: f64,
) -> Vec<WrappedLine> {
    let spacing = letter_spacing_px(block, font_size);
    let mut lines = Vec::new();
    for paragraph in text.replace('\r', "").split('\n') {
        let normalized = collapse_whitespace(paragraph);
        if normalized.is_empty() {
            lines.push(WrappedLine { text: String::new(), width: 0.0 });
            continue;
        }
        let run = ctx.shape(&normalized, &fonts.base, &fonts.spans_for(&normalized));
        let chars: Vec<char> = normalized.chars().collect();
        let width_of = |start: usize, end: usize| -> f64 {
            let advance: f32 = run.char_advances_em[start..end].iter().sum();
            let count = (end - start) as f64;
            f64::from(advance) * font_size + if count > 1.0 { spacing * (count - 1.0) } else { 0.0 }
        };
        let segments = split_text_wrap_segments(&chars);
        let (mut start, mut end) = segments[0];
        for &(segment_start, segment_end) in &segments[1..] {
            if width_of(start, segment_end) <= max_width {
                end = segment_end;
                continue;
            }
            lines.push(WrappedLine { text: chars[start..end].iter().collect(), width: width_of(start, end) });
            (start, end) = (segment_start, segment_end);
        }
        if end > start {
            lines.push(WrappedLine { text: chars[start..end].iter().collect(), width: width_of(start, end) });
        }
    }
    if lines.is_empty() {
        lines.push(WrappedLine { text: text.to_owned(), width: 0.0 });
    }
    lines
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
