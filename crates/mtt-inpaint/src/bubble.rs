//! Speech balloons around blocks, without a model: from the background inside a block's
//! box, fill outward through pixels of that color and the block's glyphs, stopping at the
//! balloon outline. A fill that stays inside a window around the box is the balloon; the
//! largest rectangle inside it (with some air) is where the translation goes. koharu fits
//! text to detected balloons the same way, with a segmentation model.

use image::RgbaImage;
use mtt_core::model::{BBox, TranslationBlock};

use crate::Rect;
use crate::text_mask::{rect_strokes, scan_rect};

/// The fill may reach this many box sizes past the box on each side; touching that
/// window's edge means it leaked out of the balloon.
const WINDOW_REACH: f64 = 1.0;
/// Pixels within this of the background color are inside.
const BACKGROUND_TOLERANCE: u8 = 48;
/// The balloon must be at least this much bigger than the box to be worth using.
const MIN_GROWTH: f64 = 1.15;
/// Air between the text area and the balloon outline, as a share of the area's shorter side.
const AIR_SHARE: f64 = 0.08;

/// A balloon found around a block: its bounds and the text area inside it, in 0..1000
/// page space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Balloon {
    pub bounds: BBox,
    pub text_area: BBox,
}

/// The balloon around `block`, `None` when its box isn't on a plain background or the
/// fill leaks out (text on art, a balloon open to the panel).
pub fn find_balloon(source: &RgbaImage, block: &TranslationBlock) -> Option<Balloon> {
    let (width, height) = source.dimensions();
    let scan = scan_rect(block, width, height)?;
    let found = rect_strokes(source, scan);
    let (background, plain) = found.edge?;
    if !plain {
        return None;
    }
    let reach = (f64::from(scan.width.max(scan.height)) * WINDOW_REACH) as f32;
    let window = Rect::clamped(
        scan.x as f32 - reach,
        scan.y as f32 - reach,
        scan.width as f32 + reach * 2.0,
        scan.height as f32 + reach * 2.0,
        width,
        height,
    )?;
    let (ww, wh) = (window.width as usize, window.height as usize);
    // Inside: the background color, or text-colored pixels in the box (its glyphs, kept
    // as text or not, so a glyph cut by the box edge doesn't wall the fill in).
    let mut passable = vec![false; ww * wh];
    for y in 0..wh {
        for x in 0..ww {
            let p = source.get_pixel(window.x + x as u32, window.y + y as u32).0;
            passable[y * ww + x] = (0..3).all(|c| p[c].abs_diff(background[c]) <= BACKGROUND_TOLERANCE);
        }
    }
    let sw = scan.width as usize;
    for (index, candidate) in found.candidates.iter().enumerate() {
        if *candidate > 0 {
            let (x, y) = ((scan.x - window.x) as usize + index % sw, (scan.y - window.y) as usize + index / sw);
            passable[y * ww + x] = true;
        }
    }
    // Seeds: background pixels in the middle half of the box.
    let (bx, by) = ((scan.x - window.x) as usize, (scan.y - window.y) as usize);
    let (bw, bh) = (scan.width as usize, scan.height as usize);
    let mut inside = vec![false; ww * wh];
    let mut stack: Vec<usize> = (by + bh / 4..by + bh * 3 / 4)
        .flat_map(|y| (bx + bw / 4..bx + bw * 3 / 4).map(move |x| y * ww + x))
        .filter(|index| passable[*index])
        .collect();
    if stack.is_empty() {
        return None;
    }
    for index in &stack {
        inside[*index] = true;
    }
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (usize::MAX, usize::MAX, 0, 0);
    while let Some(index) = stack.pop() {
        let (x, y) = (index % ww, index / ww);
        // Reaching the window's edge (unless it is the page's) means the fill escaped.
        let at_window_edge = (x == 0 && window.x > 0)
            || (y == 0 && window.y > 0)
            || (x == ww - 1 && window.x + window.width < width)
            || (y == wh - 1 && window.y + window.height < height);
        if at_window_edge {
            return None;
        }
        (min_x, max_x, min_y, max_y) = (min_x.min(x), max_x.max(x), min_y.min(y), max_y.max(y));
        let neighbours = [
            (x > 0).then(|| index - 1),
            (x + 1 < ww).then(|| index + 1),
            (y > 0).then(|| index - ww),
            (y + 1 < wh).then(|| index + ww),
        ];
        for next in neighbours.into_iter().flatten() {
            if passable[next] && !inside[next] {
                inside[next] = true;
                stack.push(next);
            }
        }
    }
    let (fw, fh) = (max_x - min_x + 1, max_y - min_y + 1);
    if ((fw * fh) as f64) < (bw * bh) as f64 * MIN_GROWTH {
        return None;
    }
    // Holes (glyph edges, dots inside letters) count as inside: whatever the window's
    // border can't reach through non-inside pixels.
    let inside = fill_holes(&inside, ww, wh);
    let area = largest_inner_rect(&inside, ww, (min_x, min_y, max_x, max_y))?;
    let air = (area.2.min(area.3) as f64 * AIR_SHARE).round() as usize;
    let text = (area.0 + air, area.1 + air, area.2.saturating_sub(air * 2), area.3.saturating_sub(air * 2));
    if text.2 == 0 || text.3 == 0 || ((text.2 * text.3) as f64) < (bw * bh) as f64 * 0.5 {
        return None;
    }
    let to_bbox = |x: usize, y: usize, w: usize, h: usize| {
        let (fx, fy) = (f64::from(width) / 1000.0, f64::from(height) / 1000.0);
        BBox::new(
            (window.x as usize + x) as f64 / fx,
            (window.y as usize + y) as f64 / fy,
            w as f64 / fx,
            h as f64 / fy,
        )
    };
    Some(Balloon { bounds: to_bbox(min_x, min_y, fw, fh), text_area: to_bbox(text.0, text.1, text.2, text.3) })
}

/// `inside` plus every pixel not connected to the border through outside pixels.
fn fill_holes(inside: &[bool], width: usize, height: usize) -> Vec<bool> {
    let mut outside = vec![false; inside.len()];
    let mut stack: Vec<usize> = (0..width)
        .flat_map(|x| [x, (height - 1) * width + x])
        .chain((0..height).flat_map(|y| [y * width, y * width + width - 1]))
        .filter(|index| !inside[*index])
        .collect();
    for index in &stack {
        outside[*index] = true;
    }
    while let Some(index) = stack.pop() {
        let (x, y) = (index % width, index / width);
        let neighbours = [
            (x > 0).then(|| index - 1),
            (x + 1 < width).then(|| index + 1),
            (y > 0).then(|| index - width),
            (y + 1 < height).then(|| index + width),
        ];
        for next in neighbours.into_iter().flatten() {
            if !inside[next] && !outside[next] {
                outside[next] = true;
                stack.push(next);
            }
        }
    }
    outside.into_iter().map(|out| !out).collect()
}

/// Largest axis-aligned rectangle of `inside` pixels centred on the region's middle row
/// band: for each half height around the centre row, the widest span every row shares.
/// Returns `(x, y, width, height)`.
fn largest_inner_rect(
    inside: &[bool],
    width: usize,
    (min_x, min_y, max_x, max_y): (usize, usize, usize, usize),
) -> Option<(usize, usize, usize, usize)> {
    let cx = (min_x + max_x) / 2;
    let cy = (min_y + max_y) / 2;
    // Each row's run of inside pixels through the centre column.
    let span = |y: usize| -> Option<(usize, usize)> {
        if !inside[y * width + cx] {
            return None;
        }
        let row = &inside[y * width..(y + 1) * width];
        let left = (min_x..=cx).rev().take_while(|x| row[*x]).last()?;
        let right = (cx..=max_x).take_while(|x| row[*x]).last()?;
        Some((left, right))
    };
    let (mut left, mut right) = span(cy)?;
    let mut best = (left, cy, right - left + 1, 1);
    for half in 1..=(cy - min_y).min(max_y - cy) {
        let (Some(top), Some(bottom)) = (span(cy - half), span(cy + half)) else { break };
        left = left.max(top.0).max(bottom.0);
        right = right.min(top.1).min(bottom.1);
        if right <= left {
            break;
        }
        let (w, h) = (right - left + 1, half * 2 + 1);
        if w * h > best.2 * best.3 {
            best = (left, cy - half, w, h);
        }
    }
    Some(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1000×1000 page with an elliptical balloon (outline 3px) centred at (500, 400),
    /// radii 220×160, and two vertical text columns in the middle.
    fn page_with_balloon() -> RgbaImage {
        let mut page = RgbaImage::from_fn(1000, 1000, |x, y| {
            let v = if (x / 3 + y / 3) % 2 == 0 { 90 } else { 200 };
            image::Rgba([v, v, v, 255])
        });
        for y in 0..1000u32 {
            for x in 0..1000u32 {
                let (dx, dy) = ((f64::from(x) - 500.0) / 220.0, (f64::from(y) - 400.0) / 160.0);
                let r = (dx * dx + dy * dy).sqrt();
                if r <= 1.0 {
                    page.put_pixel(x, y, image::Rgba([255, 255, 255, 255]));
                }
                if (0.985..=1.0).contains(&r) {
                    page.put_pixel(x, y, image::Rgba([0, 0, 0, 255]));
                }
            }
        }
        for x0 in [475u32, 515] {
            for y in 320..480 {
                for x in x0..x0 + 14 {
                    if y % 30 < 24 {
                        page.put_pixel(x, y, image::Rgba([10, 10, 10, 255]));
                    }
                }
            }
        }
        page
    }

    #[test]
    fn finds_the_balloon_and_a_text_area_inside_it() {
        let page = page_with_balloon();
        // The model's box: tight around the two columns.
        let block = TranslationBlock { bbox: BBox::new(468.0, 312.0, 68.0, 176.0), ..TranslationBlock::default() };
        let balloon = find_balloon(&page, &block).unwrap();
        let b = balloon.bounds;
        assert!((b.x - 280.0).abs() < 6.0 && (b.w - 440.0).abs() < 12.0, "{b:?}");
        assert!((b.y - 240.0).abs() < 6.0 && (b.h - 320.0).abs() < 12.0, "{b:?}");
        let t = balloon.text_area;
        assert!(t.w > 200.0 && t.h > 150.0, "much wider than the box: {t:?}");
        // Inside the ellipse: every corner of the text area.
        for (x, y) in [(t.x, t.y), (t.x + t.w, t.y), (t.x, t.y + t.h), (t.x + t.w, t.y + t.h)] {
            let (dx, dy) = ((x - 500.0) / 220.0, (y - 400.0) / 160.0);
            assert!(dx * dx + dy * dy < 1.0, "corner ({x}, {y}) outside");
        }
    }

    #[test]
    fn text_on_art_or_an_open_balloon_has_none() {
        let mut page = page_with_balloon();
        let block = TranslationBlock { bbox: BBox::new(468.0, 312.0, 68.0, 176.0), ..TranslationBlock::default() };
        // A gap in the outline lets the fill out.
        for y in 380..420 {
            for x in 715..725 {
                page.put_pixel(x, y, image::Rgba([255, 255, 255, 255]));
            }
        }
        for y in 380..420 {
            for x in 725..1000 {
                page.put_pixel(x, y, image::Rgba([255, 255, 255, 255]));
            }
        }
        assert_eq!(find_balloon(&page, &block), None, "leaks through the gap");
        let art = TranslationBlock { bbox: BBox::new(50.0, 800.0, 80.0, 120.0), ..TranslationBlock::default() };
        assert_eq!(find_balloon(&page, &art), None, "no plain background");
    }
}
