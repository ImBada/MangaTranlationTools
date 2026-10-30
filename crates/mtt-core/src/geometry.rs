//! Geometry helpers (`src/shared/geometry.ts`). Block boxes are stored in a 0..1000
//! normalized page space.

use crate::model::{BBox, TranslationBlock};

/// `clamp` from the TS code: non-finite values collapse to `min`.
pub fn clamp(value: f64, min: f64, max: f64) -> f64 {
    if !value.is_finite() {
        return min;
    }
    value.max(min).min(max)
}

pub fn clamp_bbox(bbox: BBox) -> BBox {
    let x = clamp(bbox.x, 0.0, 1000.0);
    let y = clamp(bbox.y, 0.0, 1000.0);
    BBox { x, y, w: clamp(bbox.w, 1.0, 1000.0 - x), h: clamp(bbox.h, 1.0, 1000.0 - y) }
}

pub fn clamp_editable_bbox(bbox: BBox) -> BBox {
    BBox {
        x: clamp(bbox.x, -10000.0, 10000.0),
        y: clamp(bbox.y, -10000.0, 10000.0),
        w: clamp(bbox.w, 1.0, 10000.0),
        h: clamp(bbox.h, 1.0, 10000.0),
    }
}

pub fn bbox_to_pixels(bbox: BBox, width: f64, height: f64) -> BBox {
    BBox {
        x: bbox.x / 1000.0 * width,
        y: bbox.y / 1000.0 * height,
        w: bbox.w / 1000.0 * width,
        h: bbox.h / 1000.0 * height,
    }
}

/// Converts a pixel rect to normalized space without clamping to the page, so blocks can be
/// dragged partly outside it (matches `clampEditableBbox` limits).
pub fn pixels_to_editable_bbox(bbox: BBox, width: f64, height: f64) -> BBox {
    clamp_editable_bbox(BBox {
        x: bbox.x / width.max(1.0) * 1000.0,
        y: bbox.y / height.max(1.0) * 1000.0,
        w: bbox.w / width.max(1.0) * 1000.0,
        h: bbox.h / height.max(1.0) * 1000.0,
    })
}

pub fn pixels_to_bbox(bbox: BBox, width: f64, height: f64) -> BBox {
    clamp_bbox(BBox {
        x: bbox.x / width.max(1.0) * 1000.0,
        y: bbox.y / height.max(1.0) * 1000.0,
        w: bbox.w / width.max(1.0) * 1000.0,
        h: bbox.h / height.max(1.0) * 1000.0,
    })
}

pub fn resolve_block_render_bbox(block: &TranslationBlock) -> BBox {
    clamp_editable_bbox(block.render_bbox.unwrap_or(block.bbox))
}

/// Which box editing operations should change: the render box when present.
pub fn editable_block_bbox(block: &TranslationBlock) -> BBox {
    resolve_block_render_bbox(block)
}

/// Sets the box editing changes (see [`editable_block_bbox`]). A changed box drops the
/// text shape: it outlines the balloon where it was found, not where the box goes.
pub fn apply_editable_block_bbox(block: &mut TranslationBlock, next: BBox) {
    let clamped = clamp_editable_bbox(next);
    if clamped != editable_block_bbox(block) {
        block.text_shape = None;
    }
    if block.render_bbox.is_some() {
        block.render_bbox = Some(clamped);
    } else {
        block.bbox = clamped;
    }
}

/// Moves both boxes by `(dx, dy)`; moving drops the text shape, as resizing does.
pub fn offset_block_bboxes(block: &mut TranslationBlock, dx: f64, dy: f64) {
    if dx != 0.0 || dy != 0.0 {
        block.text_shape = None;
    }
    block.bbox = clamp_editable_bbox(BBox { x: block.bbox.x + dx, y: block.bbox.y + dy, ..block.bbox });
    if let Some(render) = block.render_bbox {
        block.render_bbox = Some(clamp_editable_bbox(BBox { x: render.x + dx, y: render.y + dy, ..render }));
    }
}

pub fn clamp_rotation_deg(value: Option<f64>) -> f64 {
    match value {
        Some(value) if value.is_finite() => clamp(value, -180.0, 180.0),
        _ => 0.0,
    }
}

pub fn resolve_block_rotation_deg(block: &TranslationBlock) -> f64 {
    clamp_rotation_deg(block.rotation_deg)
}

pub fn clamp_text_padding_px(value: f64) -> f64 {
    if !value.is_finite() {
        return 0.0;
    }
    clamp(value, 0.0, 80.0).round()
}

pub fn is_hex_color(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

pub fn normalize_color(value: &str, fallback: &str) -> String {
    let trimmed = value.trim();
    if is_hex_color(trimmed) { trimmed.to_owned() } else { fallback.to_owned() }
}

/// `#rrggbb` → `(r, g, b)`; `None` for anything else.
pub fn parse_hex_color(value: &str) -> Option<(u8, u8, u8)> {
    if !is_hex_color(value) {
        return None;
    }
    let channel = |range: std::ops::Range<usize>| u8::from_str_radix(&value[range], 16).ok();
    Some((channel(1..3)?, channel(3..5)?, channel(5..7)?))
}

/// The usable span of a [`TranslationBlock::text_shape`] across `top..bottom` (fractions
/// of the box height): what every row there shares. `None` outside the shape or where
/// nothing fits.
pub fn text_shape_span(shape: &[[f64; 2]], top: f64, bottom: f64) -> Option<(f64, f64)> {
    const EDGE: f64 = 1e-9;
    let rows = shape.len() as f64;
    if shape.is_empty() || top < -EDGE || bottom > 1.0 + EDGE || bottom < top {
        return None;
    }
    let first = ((top.max(0.0) * rows).floor() as usize).min(shape.len() - 1);
    let last = ((bottom.min(1.0) * rows).ceil() as usize).clamp(first + 1, shape.len());
    let (left, right) =
        shape[first..last].iter().fold((0.0f64, 1.0f64), |(left, right), [l, r]| (left.max(*l), right.min(*r)));
    (right > left).then_some((left, right))
}

/// The largest rectangle inside a [`TranslationBlock::text_shape`], as `[left, top, right,
/// bottom]` fractions of the box: for text that can't follow the shape line by line.
pub fn text_shape_inner_rect(shape: &[[f64; 2]]) -> Option<[f64; 4]> {
    let rows = shape.len();
    let mut best: Option<([f64; 4], f64)> = None;
    for first in 0..rows {
        let (mut left, mut right) = (0.0f64, 1.0f64);
        for (last, [l, r]) in shape.iter().enumerate().skip(first) {
            (left, right) = (left.max(*l), right.min(*r));
            if right <= left {
                break;
            }
            let (top, bottom) = (first as f64 / rows as f64, (last + 1) as f64 / rows as f64);
            let area = (right - left) * (bottom - top);
            if best.is_none_or(|(_, best)| area > best) {
                best = Some(([left, top, right, bottom], area));
            }
        }
    }
    best.map(|(rect, _)| rect)
}

/// Drops the block's text shape, shrinking its box to the shape's largest rectangle so the
/// text stays inside the balloon.
pub fn clear_text_shape(block: &mut TranslationBlock) {
    let Some(shape) = block.text_shape.take() else { return };
    if let Some([left, top, right, bottom]) = text_shape_inner_rect(&shape) {
        let b = editable_block_bbox(block);
        apply_editable_block_bbox(
            block,
            BBox { x: b.x + left * b.w, y: b.y + top * b.h, w: (right - left) * b.w, h: (bottom - top) * b.h },
        );
    }
}

pub fn estimate_font_size_px(text: &str, bbox: BBox, page_width: f64, page_height: f64) -> f64 {
    let px = bbox_to_pixels(bbox, page_width, page_height);
    let compact_length = text.chars().filter(|ch| !ch.is_whitespace()).count().max(1) as f64;
    let approx_chars_per_line = (px.w / 20.0).floor().max(4.0);
    let line_count = (compact_length / approx_chars_per_line).ceil().max(1.0);
    let height_limited = (px.h / (line_count * 1.2)).floor();
    let width_limited = (px.w / compact_length.clamp(4.0, 12.0)).floor();
    clamp(height_limited.min(width_limited).min(40.0), 12.0, 72.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_bbox_keeps_box_inside_page() {
        let bbox = clamp_bbox(BBox::new(990.0, -5.0, 50.0, 0.0));
        assert_eq!(bbox, BBox::new(990.0, 0.0, 10.0, 1.0));
    }

    #[test]
    fn editable_bbox_prefers_render_box() {
        let mut block = TranslationBlock { bbox: BBox::new(0.0, 0.0, 10.0, 10.0), ..TranslationBlock::default() };
        apply_editable_block_bbox(&mut block, BBox::new(5.0, 5.0, 20.0, 20.0));
        assert_eq!(block.bbox, BBox::new(5.0, 5.0, 20.0, 20.0));

        block.render_bbox = Some(BBox::new(1.0, 1.0, 2.0, 2.0));
        apply_editable_block_bbox(&mut block, BBox::new(7.0, 7.0, 3.0, 3.0));
        assert_eq!(block.render_bbox, Some(BBox::new(7.0, 7.0, 3.0, 3.0)));
        assert_eq!(block.bbox, BBox::new(5.0, 5.0, 20.0, 20.0));
    }

    #[test]
    fn moving_or_resizing_drops_the_text_shape() {
        let shaped = TranslationBlock {
            bbox: BBox::new(0.0, 0.0, 10.0, 10.0),
            render_bbox: Some(BBox::new(1.0, 1.0, 20.0, 20.0)),
            text_shape: Some(vec![[0.0, 1.0]]),
            ..TranslationBlock::default()
        };
        let mut block = shaped.clone();
        offset_block_bboxes(&mut block, 0.0, 0.0);
        apply_editable_block_bbox(&mut block, BBox::new(1.0, 1.0, 20.0, 20.0));
        assert!(block.text_shape.is_some(), "unchanged boxes keep it");
        offset_block_bboxes(&mut block, 5.0, 0.0);
        assert_eq!(block.text_shape, None, "moved");
        let mut block = shaped;
        apply_editable_block_bbox(&mut block, BBox::new(1.0, 1.0, 30.0, 20.0));
        assert_eq!(block.text_shape, None, "resized");
    }

    #[test]
    fn hex_color_parsing() {
        assert_eq!(parse_hex_color("#ff8000"), Some((255, 128, 0)));
        assert_eq!(parse_hex_color("ff8000"), None);
        assert_eq!(parse_hex_color("#ff80"), None);
        assert_eq!(normalize_color(" #ABCDEF ", "#000000"), "#ABCDEF");
    }

    #[test]
    fn text_shape_spans_and_inner_rect() {
        // A diamond-ish shape: wide in the middle rows.
        let shape = [[0.4, 0.6], [0.2, 0.8], [0.0, 1.0], [0.2, 0.8], [0.45, 0.45]];
        assert_eq!(text_shape_span(&shape, 0.4, 0.6), Some((0.0, 1.0)));
        assert_eq!(text_shape_span(&shape, 0.3, 0.7), Some((0.2, 0.8)), "rows 1..4 share 0.2..0.8");
        assert_eq!(text_shape_span(&shape, 0.85, 0.95), None, "empty last row");
        assert_eq!(text_shape_span(&shape, -0.1, 0.1), None, "above the shape");
        assert_eq!(text_shape_span(&shape, 0.0, 1.0), None);
        // 0.6 wide over rows 1..4 (0.36) beats the full middle row (0.2).
        let rect = text_shape_inner_rect(&shape).unwrap();
        assert!((rect[0] - 0.2).abs() < 1e-9 && (rect[1] - 0.2).abs() < 1e-9, "{rect:?}");
        assert!((rect[2] - 0.8).abs() < 1e-9 && (rect[3] - 0.8).abs() < 1e-9, "{rect:?}");
        assert_eq!(text_shape_inner_rect(&[[0.5, 0.5]]), None);

        let mut block = TranslationBlock {
            render_bbox: Some(BBox::new(100.0, 100.0, 200.0, 100.0)),
            text_shape: Some(shape.to_vec()),
            ..TranslationBlock::default()
        };
        clear_text_shape(&mut block);
        assert_eq!(block.text_shape, None);
        let b = block.render_bbox.unwrap();
        assert!((b.x - 140.0).abs() < 1e-9 && (b.y - 120.0).abs() < 1e-9, "{b:?}");
        assert!((b.w - 120.0).abs() < 1e-9 && (b.h - 60.0).abs() < 1e-9, "{b:?}");
    }

    #[test]
    fn estimate_font_size_matches_ts_formula() {
        // 1000x1000 page, box 200x100 px, 10 chars: chars/line = 10, lines = 1,
        // height limit = floor(100 / 1.2) = 83, width limit = floor(200 / 10) = 20.
        assert_eq!(estimate_font_size_px("0123456789", BBox::new(0.0, 0.0, 200.0, 100.0), 1000.0, 1000.0), 20.0);
    }
}
