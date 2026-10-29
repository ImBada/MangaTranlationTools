//! Automatic text masks for selected blocks (`drawBlocksOnInpaintMask`): dark source pixels
//! inside each block's detection box, minus connected components that look like balloon
//! outlines or panel borders, grown so anti-aliased edges and thin light outlines go too.

use image::{GrayImage, RgbaImage};
use mtt_core::geometry::bbox_to_pixels;
use mtt_core::model::TranslationBlock;

use crate::{Rect, close, dilate};

const TEXT_MASK_MIN_ALPHA: u8 = 48;
const TEXT_MASK_MAX_LUMA: f32 = 205.0;
const TEXT_MASK_MIN_COMPONENT_RATIO: f32 = 0.00002;
const TEXT_MASK_EDGE_COMPONENT_RATIO: f32 = 0.008;
const TEXT_MASK_BROAD_OUTER_RATIO: f32 = 0.01;
const TEXT_MASK_CORNER_STROKE_RATIO: f32 = 0.0015;
const TEXT_MASK_CENTER_KEEP_INSET_RATIO: f32 = 0.14;
/// Growth around the strokes per 1024 pixels of the page's long side (koharu grows its
/// text masks with the page the same way), within these bounds.
const GROWTH_PER_1024_PX: f32 = 3.0;
const MIN_GROWTH_PX: usize = 2;
const MAX_GROWTH_PX: usize = 8;

struct Component {
    pixels: Vec<usize>,
    min_x: usize,
    min_y: usize,
    max_x: usize,
    max_y: usize,
    touches_edge: bool,
}

impl Component {
    fn width(&self) -> f32 {
        (self.max_x - self.min_x + 1) as f32
    }

    fn height(&self) -> f32 {
        (self.max_y - self.min_y + 1) as f32
    }
}

/// How far the strokes are grown on a `width`×`height` page.
pub fn growth_radius(width: u32, height: u32) -> usize {
    ((width.max(height) as f32 / 1024.0 * GROWTH_PER_1024_PX).round() as usize).clamp(MIN_GROWTH_PX, MAX_GROWTH_PX)
}

/// Mask covering the text strokes of `blocks`, added to `base` when given. The strokes are
/// grown by [`growth_radius`] and closed (gaps between them filled), staying within that
/// distance of the boxes and off the dark pixels set aside as balloon outlines.
pub fn mask_for_blocks(source: &RgbaImage, blocks: &[&TranslationBlock], base: Option<&GrayImage>) -> GrayImage {
    let (width, height) = source.dimensions();
    let mut mask = base.cloned().unwrap_or_else(|| GrayImage::new(width, height));
    let rects: Vec<Rect> = blocks.iter().filter_map(|block| scan_rect(block, width, height)).collect();
    let radius = growth_radius(width, height);
    let grow = |rect: Rect| {
        let by = radius as f32;
        Rect::clamped(
            rect.x as f32 - by,
            rect.y as f32 - by,
            rect.width as f32 + by * 2.0,
            rect.height as f32 + by * 2.0,
            width,
            height,
        )
        .unwrap_or(rect)
    };
    // Everything happens inside the blocks' boxes plus the growth.
    let Some(area) = rects.iter().copied().map(grow).reduce(|a, b| {
        let (x, y) = (a.x.min(b.x), a.y.min(b.y));
        let (right, bottom) = ((a.x + a.width).max(b.x + b.width), (a.y + a.height).max(b.y + b.height));
        Rect { x, y, width: right - x, height: bottom - y }
    }) else {
        return mask;
    };
    let area_width = area.width as usize;
    let area_len = area_width * area.height as usize;
    let mut strokes = vec![0u8; area_len];
    let mut outlines = vec![false; area_len];
    let mut allowed = vec![false; area_len];
    for rect in rects {
        let (rw, rh) = (rect.width as usize, rect.height as usize);
        let mut candidates = vec![0u8; rw * rh];
        for y in 0..rh {
            for x in 0..rw {
                let p = source.get_pixel(rect.x + x as u32, rect.y + y as u32).0;
                let luma = f32::from(p[0]) * 0.299 + f32::from(p[1]) * 0.587 + f32::from(p[2]) * 0.114;
                if p[3] >= TEXT_MASK_MIN_ALPHA && luma <= TEXT_MASK_MAX_LUMA {
                    candidates[y * rw + x] = 255;
                }
            }
        }
        let filtered = retain_centered_components(&filter_candidates(&candidates, rw, rh), rw, rh);
        for (index, (value, candidate)) in filtered.iter().zip(&candidates).enumerate() {
            let (x, y) = ((rect.x - area.x) as usize + index % rw, (rect.y - area.y) as usize + index / rw);
            if *value > 0 {
                strokes[y * area_width + x] = 255;
            } else if *candidate > 0 {
                outlines[y * area_width + x] = true;
            }
        }
        let grown = grow(rect);
        for y in grown.y - area.y..grown.y - area.y + grown.height {
            let start = y as usize * area_width + (grown.x - area.x) as usize;
            allowed[start..start + grown.width as usize].fill(true);
        }
    }
    let grown =
        close(&dilate(&strokes, area_width, area.height as usize, radius), area_width, area.height as usize, radius);
    for index in 0..area_len {
        // A stroke in one box wins over the same pixel set aside as outline in another.
        if strokes[index] > 0 || (grown[index] > 0 && allowed[index] && !outlines[index]) {
            mask.put_pixel(
                area.x + (index % area_width) as u32,
                area.y + (index / area_width) as u32,
                image::Luma([255]),
            );
        }
    }
    mask
}

/// Detection box plus a small padding (`textMaskScanRect`).
fn scan_rect(block: &TranslationBlock, width: u32, height: u32) -> Option<Rect> {
    let px = bbox_to_pixels(block.bbox, f64::from(width), f64::from(height));
    let block_short = px.w.min(px.h).max(1.0);
    let page_short = f64::from(width.min(height));
    let padding = (block_short * 0.08).min(page_short * 0.004).round().clamp(2.0, 10.0);
    Rect::clamped(
        (px.x - padding) as f32,
        (px.y - padding) as f32,
        (px.w + padding * 2.0) as f32,
        (px.h + padding * 2.0) as f32,
        width,
        height,
    )
}

fn collect(mask: &[u8], visited: &mut [bool], width: usize, height: usize, start: usize) -> Component {
    let mut queue = vec![start];
    visited[start] = true;
    let (sx, sy) = (start % width, start / width);
    let mut component =
        Component { pixels: Vec::new(), min_x: sx, min_y: sy, max_x: sx, max_y: sy, touches_edge: false };
    let mut cursor = 0;
    while cursor < queue.len() {
        let index = queue[cursor];
        cursor += 1;
        component.pixels.push(index);
        let (x, y) = (index % width, index / width);
        component.min_x = component.min_x.min(x);
        component.min_y = component.min_y.min(y);
        component.max_x = component.max_x.max(x);
        component.max_y = component.max_y.max(y);
        component.touches_edge |= x == 0 || y == 0 || x == width - 1 || y == height - 1;
        for ny in y.saturating_sub(1)..=(y + 1).min(height - 1) {
            for nx in x.saturating_sub(1)..=(x + 1).min(width - 1) {
                let neighbor = ny * width + nx;
                if (nx == x && ny == y) || visited[neighbor] || mask[neighbor] == 0 {
                    continue;
                }
                visited[neighbor] = true;
                queue.push(neighbor);
            }
        }
    }
    component
}

fn filter_candidates(mask: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut output = vec![0u8; mask.len()];
    let mut visited = vec![false; mask.len()];
    let min_pixels = ((mask.len() as f32 * TEXT_MASK_MIN_COMPONENT_RATIO).floor() as usize).max(2);
    for index in 0..mask.len() {
        if visited[index] || mask[index] == 0 {
            continue;
        }
        let component = collect(mask, &mut visited, width, height, index);
        if component.pixels.len() < min_pixels || is_likely_non_text(&component, width as f32, height as f32) {
            continue;
        }
        for pixel in component.pixels {
            output[pixel] = 255;
        }
    }
    output
}

/// Keeps components that reach the central area of the box.
fn retain_centered_components(mask: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut output = vec![0u8; mask.len()];
    let mut visited = vec![false; mask.len()];
    let inset_x = (width as f32 * TEXT_MASK_CENTER_KEEP_INSET_RATIO).floor() as usize;
    let inset_y = (height as f32 * TEXT_MASK_CENTER_KEEP_INSET_RATIO).floor() as usize;
    let (left, top) = (inset_x, inset_y);
    let right = inset_x.max((width - 1).saturating_sub(inset_x));
    let bottom = inset_y.max((height - 1).saturating_sub(inset_y));
    for index in 0..mask.len() {
        if visited[index] || mask[index] == 0 {
            continue;
        }
        let component = collect(mask, &mut visited, width, height, index);
        if component.max_x >= left && component.min_x <= right && component.max_y >= top && component.min_y <= bottom {
            for pixel in component.pixels {
                output[pixel] = 255;
            }
        }
    }
    output
}

fn is_likely_non_text(c: &Component, width: f32, height: f32) -> bool {
    let (cw, ch) = (c.width(), c.height());
    let count = c.pixels.len() as f32;
    let density = count / (cw * ch).max(1.0);
    let edge_guard = (width.min(height) * 0.04).round().clamp(1.0, 8.0);
    let near_edge = c.min_x as f32 <= edge_guard
        || c.min_y as f32 <= edge_guard
        || c.max_x as f32 >= width - 1.0 - edge_guard
        || c.max_y as f32 >= height - 1.0 - edge_guard;
    let band_x = (width * 0.08).round().clamp(2.0, 18.0);
    let band_y = (height * 0.08).round().clamp(2.0, 18.0);
    let in_outer_band = c.min_x as f32 <= band_x
        || c.min_y as f32 <= band_y
        || c.max_x as f32 >= width - 1.0 - band_x
        || c.max_y as f32 >= height - 1.0 - band_y;
    let corner_x = (width * 0.12).round().clamp(3.0, 24.0);
    let corner_y = (height * 0.12).round().clamp(3.0, 24.0);
    let in_corner_band = (c.min_x as f32 <= corner_x || c.max_x as f32 >= width - 1.0 - corner_x)
        && (c.min_y as f32 <= corner_y || c.max_y as f32 >= height - 1.0 - corner_y);
    let elongated = (cw >= (width * 0.35).max(12.0) && ch <= (height * 0.08).max(4.0))
        || (ch >= (height * 0.35).max(12.0) && cw <= (width * 0.08).max(4.0));
    let outer_stroke = (ch >= (height * 0.12).max(8.0) && cw <= (width * 0.1).max(6.0))
        || (cw >= (width * 0.12).max(8.0) && ch <= (height * 0.1).max(6.0))
        || (cw >= (width * 0.08).max(7.0) && ch >= (height * 0.08).max(7.0) && density < 0.58);
    let edge_sized =
        cw >= width * 0.18 || ch >= height * 0.18 || count >= width * height * TEXT_MASK_EDGE_COMPONENT_RATIO;
    let oversized = cw >= width * 0.78 || ch >= height * 0.78;
    let sparse_large = density < 0.5 && (cw >= width * 0.28 || ch >= height * 0.28);
    let broad_outer = in_outer_band
        && (c.touches_edge || elongated || sparse_large)
        && (cw >= width * 0.2 || ch >= height * 0.2 || count >= width * height * TEXT_MASK_BROAD_OUTER_RATIO);
    let corner_stroke = in_corner_band
        && (outer_stroke || sparse_large || cw >= width * 0.12 || ch >= height * 0.12)
        && count >= (width * height * TEXT_MASK_CORNER_STROKE_RATIO).max(6.0);
    corner_stroke
        || broad_outer
        || (near_edge && (outer_stroke || elongated || oversized || sparse_large || edge_sized))
        || (c.touches_edge && edge_sized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtt_core::model::BBox;

    #[test]
    fn keeps_glyph_strokes_and_drops_balloon_outline() {
        // 200x200 page; block covers 50..150. Draw a balloon outline at the box edge and a
        // small "glyph" in the middle.
        let mut source = RgbaImage::from_pixel(200, 200, image::Rgba([255, 255, 255, 255]));
        for i in 48..152 {
            for t in 0..2 {
                source.put_pixel(i, 48 + t, image::Rgba([0, 0, 0, 255]));
                source.put_pixel(48 + t, i, image::Rgba([0, 0, 0, 255]));
            }
        }
        for y in 95..105 {
            for x in 97..100 {
                source.put_pixel(x, y, image::Rgba([10, 10, 10, 255]));
            }
        }
        let block = TranslationBlock { bbox: BBox::new(250.0, 250.0, 500.0, 500.0), ..TranslationBlock::default() };
        let mask = mask_for_blocks(&source, &[&block], None);
        assert_eq!(mask.get_pixel(98, 100)[0], 255, "glyph covered");
        assert_eq!(mask.get_pixel(95, 100)[0], 255, "grown by the minimum radius");
        assert_eq!(mask.get_pixel(94, 100)[0], 0);
        assert_eq!(mask.get_pixel(100, 48)[0], 0, "outline dropped");
        assert_eq!(mask.get_pixel(20, 20)[0], 0);
    }

    #[test]
    fn growth_follows_the_page_size() {
        assert_eq!(growth_radius(200, 200), MIN_GROWTH_PX);
        assert_eq!(growth_radius(1400, 2048), 6);
        assert_eq!(growth_radius(4000, 6000), MAX_GROWTH_PX);
    }

    fn paint(source: &mut RgbaImage, x0: u32, y0: u32, w: u32, h: u32, v: u8) {
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                source.put_pixel(x, y, image::Rgba([v, v, v, 255]));
            }
        }
    }

    /// A block over 900..1140 × 150..250 of a 2048×400 page (growth radius 6).
    fn wide_block() -> TranslationBlock {
        let bbox = BBox::new(900.0 / 2048.0 * 1000.0, 150.0 / 400.0 * 1000.0, 240.0 / 2048.0 * 1000.0, 250.0);
        TranslationBlock { bbox, ..TranslationBlock::default() }
    }

    #[test]
    fn growth_closes_gaps_in_a_balloon_but_keeps_its_edge() {
        let mut source = RgbaImage::from_pixel(2048, 400, image::Rgba([255, 255, 255, 255]));
        paint(&mut source, 1000, 180, 10, 40, 10);
        paint(&mut source, 1022, 180, 10, 40, 10);
        // Faint anti-aliasing next to a glyph.
        paint(&mut source, 1010, 180, 2, 40, 230);
        // Balloon edge: a long dark line across the top of the box.
        paint(&mut source, 900, 150, 240, 3, 0);
        let mask = mask_for_blocks(&source, &[&wide_block()], None);
        assert_eq!(mask.get_pixel(1011, 200)[0], 255, "anti-aliasing covered");
        assert_eq!(mask.get_pixel(1016, 200)[0], 255, "gap between glyphs closed");
        assert_eq!(mask.get_pixel(994, 200)[0], 255, "grown by 6");
        assert_eq!(mask.get_pixel(993, 200)[0], 0);
        assert_eq!(mask.get_pixel(1005, 151)[0], 0, "balloon edge kept");
    }

    #[test]
    fn growth_covers_a_light_outline_but_not_the_art_around_it() {
        // A dark glyph with a 3px white outline on mid-gray art.
        let mut source = RgbaImage::from_pixel(2048, 400, image::Rgba([150, 150, 150, 255]));
        paint(&mut source, 997, 177, 16, 46, 250);
        paint(&mut source, 1000, 180, 10, 40, 10);
        let mask = mask_for_blocks(&source, &[&wide_block()], None);
        assert_eq!(mask.get_pixel(1005, 200)[0], 255, "glyph");
        assert_eq!(mask.get_pixel(997, 200)[0], 255, "white outline covered");
        assert_eq!(mask.get_pixel(1012, 178)[0], 255);
        assert_eq!(mask.get_pixel(996, 200)[0], 0, "art left alone");
        assert_eq!(mask.get_pixel(1100, 200)[0], 0);
    }
}
