//! Automatic text masks for selected blocks (`drawBlocksOnInpaintMask`): dark source pixels
//! inside each block's detection box (light ones for text on a dark box), minus connected
//! components that look like balloon outlines or panel borders, grown so anti-aliased
//! edges and thin light outlines go too. The same strokes give the source lettering's
//! colors ([`detect_lettering`]). With the text segmentation model, its glyph pixels take
//! the strokes' place (`text_segmentation.rs`).

use image::{GrayImage, RgbaImage};
use mtt_core::geometry::{bbox_to_pixels, parse_hex_color};
use mtt_core::model::{StyleKey, TranslationBlock};

use crate::{Rect, close, dilate, erode};

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
/// A box whose edge is darker than this may hold light text; it does when the light
/// pixels (at least `LIGHT_TEXT_CONTRAST` above the edge) keep `LIGHT_TEXT_MAJORITY` times
/// more glyph pixels than the dark ones.
const DARK_BACKGROUND_LUMA: f32 = 100.0;
const LIGHT_TEXT_CONTRAST: f32 = 50.0;
const LIGHT_TEXT_MAJORITY: usize = 5;
/// Lettering detection looks at glyph-sized strokes (at least `1 / GLYPH_SHARE_OF_LARGEST`
/// of the largest, so screentone dots don't count), at least `MIN_LETTERING_PIXELS` of
/// them. An outline is rings around the glyphs with `OUTLINE_SHARE` of their pixels within
/// `COLOR_TOLERANCE` of one color that differs from the fill by `OUTLINE_CONTRAST` luma,
/// and that stop within `MAX_OUTLINE_PX` (a color that goes on is the background).
const GLYPH_SHARE_OF_LARGEST: usize = 20;
const MIN_LETTERING_PIXELS: usize = 24;
const MAX_OUTLINE_PX: usize = 8;
const OUTLINE_SHARE: f32 = 0.85;
const COLOR_TOLERANCE: u8 = 40;
const OUTLINE_CONTRAST: f32 = 90.0;
/// A box edge (its outer `EDGE_BAND_PX`) is plain when this share of it is within
/// `COLOR_TOLERANCE` of its median.
const EDGE_BAND_PX: u32 = 3;
const PLAIN_EDGE_SHARE: f32 = 0.9;
/// The fill replaces the block's text color when a channel differs by more than this.
const FILL_DIFFERENCE: u8 = 80;

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
/// distance of the boxes and off the text-colored pixels set aside as balloon outlines.
pub fn mask_for_blocks(source: &RgbaImage, blocks: &[&TranslationBlock], base: Option<&GrayImage>) -> GrayImage {
    let (width, height) = source.dimensions();
    let mut mask = base.cloned().unwrap_or_else(|| GrayImage::new(width, height));
    if let Some(area) = block_area(blocks, width, height) {
        paint_block_mask(&mut mask, source, &area, None);
    }
    mask
}

/// The scan boxes of some blocks, how far their strokes grow, and the area the mask can
/// cover: the boxes plus the growth.
pub(crate) struct BlockArea {
    pub(crate) rects: Vec<Rect>,
    pub(crate) radius: usize,
    pub(crate) area: Rect,
}

pub(crate) fn block_area(blocks: &[&TranslationBlock], width: u32, height: u32) -> Option<BlockArea> {
    let rects: Vec<Rect> = blocks.iter().filter_map(|block| scan_rect(block, width, height)).collect();
    let radius = growth_radius(width, height);
    let area = rects.iter().map(|rect| grow(*rect, radius, width, height)).reduce(|a, b| {
        let (x, y) = (a.x.min(b.x), a.y.min(b.y));
        let (right, bottom) = ((a.x + a.width).max(b.x + b.width), (a.y + a.height).max(b.y + b.height));
        Rect { x, y, width: right - x, height: bottom - y }
    })?;
    Some(BlockArea { rects, radius, area })
}

fn grow(rect: Rect, by: usize, width: u32, height: u32) -> Rect {
    let by = by as f32;
    Rect::clamped(
        rect.x as f32 - by,
        rect.y as f32 - by,
        rect.width as f32 + by * 2.0,
        rect.height as f32 + by * 2.0,
        width,
        height,
    )
    .unwrap_or(rect)
}

/// Glyph pixels found by other means (the segmentation model) over a [`BlockArea`]'s area,
/// nonzero = glyph, already grown by `grown_by`.
pub(crate) struct Glyphs<'a> {
    pub(crate) mask: &'a [u8],
    pub(crate) grown_by: usize,
}

/// A box falls back to the luma strokes when `glyphs` has fewer pixels in it than this
/// share of its area (and `MIN_GLYPH_PIXELS`): the model found no text there.
const GLYPH_FALLBACK_SHARE: usize = 1000;
const MIN_GLYPH_PIXELS: usize = 8;

/// Paints the text mask of `area` onto `mask`: the strokes grown to `area.radius` in all
/// and closed, the growth kept within the grown boxes and off the candidates the heuristic
/// set aside as balloon outlines.
///
/// With `glyphs`, the strokes are the glyphs within the grown boxes plus the candidates
/// that touch them (the thin parts the model is unsure of) when the heuristic kept them or
/// they lie inside the box (outlines and borders cross its edge); the other candidates
/// count as outlines. A box the glyphs find nothing in keeps the heuristic's strokes.
pub(crate) fn paint_block_mask(mask: &mut GrayImage, source: &RgbaImage, area: &BlockArea, glyphs: Option<Glyphs>) {
    let BlockArea { rects, radius, area } = area;
    let (area_width, area_height) = (area.width as usize, area.height as usize);
    let area_len = area_width * area_height;
    let mut strokes = vec![0u8; area_len];
    let mut outlines = vec![false; area_len];
    let mut allowed = vec![false; area_len];
    for rect in rects {
        let rw = rect.width as usize;
        let at = |index: usize| {
            let (x, y) = ((rect.x - area.x) as usize + index % rw, (rect.y - area.y) as usize + index / rw);
            y * area_width + x
        };
        let (rh, rect_len) = (rect.height as usize, rw * rect.height as usize);
        let found = rect_strokes(source, *rect);
        let kept = match &glyphs {
            Some(glyphs) => {
                let glyph = |index: usize| glyphs.mask[at(index)] > 0;
                let count = (0..rect_len).filter(|index| glyph(*index)).count();
                if count < (rect_len / GLYPH_FALLBACK_SHARE).max(MIN_GLYPH_PIXELS) {
                    found.strokes.clone()
                } else {
                    attached(&found, rw, rh, glyph)
                }
            }
            None => found.strokes.clone(),
        };
        for (index, (value, candidate)) in kept.iter().zip(&found.candidates).enumerate() {
            if *value > 0 {
                strokes[at(index)] = 255;
            } else if *candidate > 0 {
                outlines[at(index)] = true;
            }
        }
        let grown = grow(*rect, *radius, source.width(), source.height());
        for y in grown.y - area.y..grown.y - area.y + grown.height {
            let start = y as usize * area_width + (grown.x - area.x) as usize;
            allowed[start..start + grown.width as usize].fill(true);
        }
    }
    // The glyphs replace the strokes within the grown boxes.
    let glyph_strokes = glyphs.as_ref().map(|glyphs| {
        let kept: Vec<u8> = glyphs
            .mask
            .iter()
            .zip(&allowed)
            .map(|(value, allowed)| if *value > 0 && *allowed { 255 } else { 0 })
            .collect();
        (kept, glyphs.grown_by)
    });
    let mut grown = dilate(&strokes, area_width, area_height, *radius);
    if let Some((kept, grown_by)) = &glyph_strokes {
        let glyphs_grown = dilate(kept, area_width, area_height, radius.saturating_sub(*grown_by));
        for (target, value) in grown.iter_mut().zip(glyphs_grown) {
            *target |= value;
        }
    }
    let grown = close(&grown, area_width, area_height, *radius);
    for index in 0..area_len {
        // A stroke (or glyph) wins over the same pixel set aside as outline in another box.
        let stroke = strokes[index] > 0 || glyph_strokes.as_ref().is_some_and(|(kept, _)| kept[index] > 0);
        if stroke || (grown[index] > 0 && allowed[index] && !outlines[index]) {
            mask.put_pixel(
                area.x + (index % area_width) as u32,
                area.y + (index / area_width) as u32,
                image::Luma([255]),
            );
        }
    }
}

/// The candidate components of `found` (`width`×`height`) with a pixel where `glyph` holds
/// that are strokes or stay off the box's edge.
pub(crate) fn attached(found: &RectStrokes, width: usize, height: usize, glyph: impl Fn(usize) -> bool) -> Vec<u8> {
    let candidates = &found.candidates;
    let mut visited = vec![false; candidates.len()];
    let mut kept = vec![0u8; candidates.len()];
    for index in 0..candidates.len() {
        if candidates[index] > 0 && !visited[index] {
            let component = collect(candidates, &mut visited, width, height, index);
            // Strokes are whole candidate components.
            let stroke = found.strokes[index] > 0;
            if (stroke || !component.touches_edge) && component.pixels.iter().any(|pixel| glyph(*pixel)) {
                for pixel in component.pixels {
                    kept[pixel] = 255;
                }
            }
        }
    }
    kept
}

/// The source lettering of a block: its fill color and, for text drawn with a contrasting
/// rim (white outlines over art, say), the outline color and how far it reaches past the
/// glyphs. Light text on a dark box also gives the box color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLettering {
    pub fill: [u8; 3],
    pub outline: Option<([u8; 3], u32)>,
    pub dark_background: Option<[u8; 3]>,
}

impl SourceLettering {
    /// Puts the lettering on `block` over its preset values, unlinking only the fields it
    /// changes: the text color when it clearly differs, and the outline when there is one.
    pub fn apply_to(&self, block: &mut TranslationBlock) {
        let hex = |[r, g, b]: [u8; 3]| format!("#{r:02x}{g:02x}{b:02x}");
        let current = block.style.text_color.as_deref().and_then(parse_hex_color);
        let differs = current
            .is_none_or(|(r, g, b)| [r, g, b].iter().zip(self.fill).any(|(a, b)| a.abs_diff(b) > FILL_DIFFERENCE));
        if differs {
            block.style.text_color = Some(hex(self.fill));
            block.links.set(StyleKey::TextColor, Some(false));
        }
        if let Some((color, reach)) = self.outline {
            // The stroke is centred on the glyph edge, so it reaches half its width out.
            block.style.outline_color = Some(hex(color));
            block.style.outline_width_px = Some(f64::from(reach * 2));
            block.links.set(StyleKey::OutlineColor, Some(false));
            block.links.set(StyleKey::OutlineWidthPx, Some(false));
        }
        if let Some(color) = self.dark_background {
            block.background_color = hex(color);
        }
    }
}

/// The lettering inside `block`'s box, `None` when too little text is found.
pub fn detect_lettering(source: &RgbaImage, block: &TranslationBlock) -> Option<SourceLettering> {
    let rect = scan_rect(block, source.width(), source.height())?;
    let found = rect_strokes(source, rect);
    let (rw, rh) = (rect.width as usize, rect.height as usize);
    let strokes = glyph_sized(&found.strokes, rw, rh);
    let count = strokes.iter().filter(|value| **value > 0).count();
    if count < MIN_LETTERING_PIXELS {
        return None;
    }
    let colors_in = |selection: &[u8]| -> Vec<[u8; 3]> {
        selection
            .iter()
            .enumerate()
            .filter(|(_, value)| **value > 0)
            .map(|(index, _)| {
                let p = source.get_pixel(rect.x + (index % rw) as u32, rect.y + (index / rw) as u32).0;
                [p[0], p[1], p[2]]
            })
            .collect()
    };
    // The fill from the stroke cores, without their anti-aliased edges.
    let core = erode(&strokes, rw, rh, 1);
    let core_count = core.iter().filter(|value| **value > 0).count();
    let fill = median_color(&colors_in(if core_count * 5 >= count { &core } else { &strokes }))?;
    // Rings around the strokes, from 2px out (1px is anti-aliasing).
    let mut inner = dilate(&strokes, rw, rh, 1);
    let mut rim: Option<([u8; 3], u32)> = None;
    let mut rim_ends = false;
    for distance in 2..=MAX_OUTLINE_PX {
        let outer = dilate(&strokes, rw, rh, distance);
        let ring: Vec<u8> = outer.iter().zip(&inner).map(|(o, i)| if *o > 0 && *i == 0 { 255 } else { 0 }).collect();
        let colors = colors_in(&ring);
        let color = match rim {
            Some((color, _)) => Some(color),
            None => median_color(&colors).filter(|color| (luma(*color) - luma(fill)).abs() >= OUTLINE_CONTRAST),
        };
        let Some(color) = color.filter(|color| colors.len() >= 8 && share_near(&colors, *color) >= OUTLINE_SHARE)
        else {
            rim_ends = true;
            break;
        };
        rim = Some((color, distance as u32));
        inner = outer;
    }
    // A rim that goes on, or has the color of a plain box edge, is the background.
    let outline =
        rim.filter(|(color, _)| rim_ends && !found.edge.is_some_and(|(edge, plain)| plain && near(*color, edge)));
    Some(SourceLettering {
        fill,
        outline,
        dark_background: found.light.then(|| found.edge.map(|(edge, _)| edge)).flatten(),
    })
}

/// `strokes` without components much smaller than the largest (dots, specks).
fn glyph_sized(strokes: &[u8], width: usize, height: usize) -> Vec<u8> {
    let mut visited = vec![false; strokes.len()];
    let mut components = Vec::new();
    for index in 0..strokes.len() {
        if strokes[index] > 0 && !visited[index] {
            components.push(collect(strokes, &mut visited, width, height, index).pixels);
        }
    }
    let largest = components.iter().map(Vec::len).max().unwrap_or(0);
    let mut kept = vec![0u8; strokes.len()];
    for pixels in components.iter().filter(|pixels| pixels.len() * GLYPH_SHARE_OF_LARGEST >= largest) {
        for pixel in pixels {
            kept[*pixel] = 255;
        }
    }
    kept
}

/// Text pixels in `rect`: the candidates of the text's polarity and the ones kept as
/// glyph strokes (`width * height` each), whether the text is light on a dark box, and
/// the box edge's median color with whether the edge is plain.
pub(crate) struct RectStrokes {
    pub(crate) candidates: Vec<u8>,
    pub(crate) strokes: Vec<u8>,
    pub(crate) light: bool,
    pub(crate) edge: Option<([u8; 3], bool)>,
}

pub(crate) fn rect_strokes(source: &RgbaImage, rect: Rect) -> RectStrokes {
    let (rw, rh) = (rect.width as usize, rect.height as usize);
    let candidates = |keep: &dyn Fn(f32) -> bool| -> Vec<u8> {
        let mut out = vec![0u8; rw * rh];
        for y in 0..rh {
            for x in 0..rw {
                let p = source.get_pixel(rect.x + x as u32, rect.y + y as u32).0;
                if p[3] >= TEXT_MASK_MIN_ALPHA && keep(luma([p[0], p[1], p[2]])) {
                    out[y * rw + x] = 255;
                }
            }
        }
        out
    };
    let keep_text = |candidates: &[u8]| retain_centered_components(&filter_candidates(candidates, rw, rh), rw, rh);
    let count = |mask: &[u8]| mask.iter().filter(|value| **value > 0).count();
    let edge = edge_color(source, rect);
    let dark_candidates = candidates(&|luma| luma <= TEXT_MASK_MAX_LUMA);
    let dark = keep_text(&dark_candidates);
    if let Some((edge_color, _)) = edge
        && luma(edge_color) < DARK_BACKGROUND_LUMA
    {
        let threshold = luma(edge_color) + LIGHT_TEXT_CONTRAST;
        let light_candidates = candidates(&|luma| luma >= threshold);
        let light = keep_text(&light_candidates);
        if count(&dark) * LIGHT_TEXT_MAJORITY < count(&light) {
            return RectStrokes { candidates: light_candidates, strokes: light, light: true, edge };
        }
    }
    RectStrokes { candidates: dark_candidates, strokes: dark, light: false, edge }
}

/// Median color of the `EDGE_BAND_PX` along `rect`'s sides (thick enough to catch
/// screentone dots) and whether the edge is plain.
fn edge_color(source: &RgbaImage, rect: Rect) -> Option<([u8; 3], bool)> {
    let (right, bottom) = (rect.x + rect.width, rect.y + rect.height);
    let colors: Vec<[u8; 3]> = (rect.y..bottom)
        .flat_map(|y| (rect.x..right).map(move |x| (x, y)))
        .filter(|(x, y)| {
            x - rect.x < EDGE_BAND_PX
                || right - x <= EDGE_BAND_PX
                || y - rect.y < EDGE_BAND_PX
                || bottom - y <= EDGE_BAND_PX
        })
        .map(|(x, y)| {
            let p = source.get_pixel(x, y).0;
            [p[0], p[1], p[2]]
        })
        .collect();
    let median = median_color(&colors)?;
    Some((median, share_near(&colors, median) >= PLAIN_EDGE_SHARE))
}

fn luma([r, g, b]: [u8; 3]) -> f32 {
    f32::from(r) * 0.299 + f32::from(g) * 0.587 + f32::from(b) * 0.114
}

pub(crate) fn median_color(colors: &[[u8; 3]]) -> Option<[u8; 3]> {
    if colors.is_empty() {
        return None;
    }
    Some([0, 1, 2].map(|channel| {
        let mut values: Vec<u8> = colors.iter().map(|color| color[channel]).collect();
        let middle = values.len() / 2;
        *values.select_nth_unstable(middle).1
    }))
}

fn near(a: [u8; 3], b: [u8; 3]) -> bool {
    a.iter().zip(b).all(|(a, b)| a.abs_diff(b) <= COLOR_TOLERANCE)
}

fn share_near(colors: &[[u8; 3]], target: [u8; 3]) -> f32 {
    colors.iter().filter(|color| near(**color, target)).count() as f32 / colors.len().max(1) as f32
}

/// Detection box plus a small padding (`textMaskScanRect`).
pub(crate) fn scan_rect(block: &TranslationBlock, width: u32, height: u32) -> Option<Rect> {
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

    fn glyphs(source: &mut RgbaImage, color: [u8; 3]) {
        for x0 in [1000, 1022, 1044] {
            for y in 180..220 {
                for x in x0..x0 + 10 {
                    source.put_pixel(x, y, image::Rgba([color[0], color[1], color[2], 255]));
                }
            }
        }
    }

    #[test]
    fn light_text_on_a_dark_box_is_masked_and_detected() {
        let mut source = RgbaImage::from_pixel(2048, 400, image::Rgba([12, 12, 12, 255]));
        glyphs(&mut source, [250, 250, 250]);
        let block = wide_block();
        let mask = mask_for_blocks(&source, &[&block], None);
        assert_eq!(mask.get_pixel(1005, 200)[0], 255, "white glyph masked");
        assert_eq!(mask.get_pixel(1100, 200)[0], 0);
        let lettering = detect_lettering(&source, &block).unwrap();
        assert_eq!(
            lettering,
            SourceLettering { fill: [250, 250, 250], outline: None, dark_background: Some([12, 12, 12]) }
        );

        let mut styled = TranslationBlock { background_color: "#fffdf5".into(), ..block };
        styled.style.text_color = Some("#111111".into());
        lettering.apply_to(&mut styled);
        assert_eq!(styled.style.text_color.as_deref(), Some("#fafafa"));
        assert_eq!(styled.links.get(StyleKey::TextColor), Some(false));
        assert_eq!(styled.background_color, "#0c0c0c");
    }

    #[test]
    fn dark_text_in_a_balloon_has_no_outline() {
        let mut source = RgbaImage::from_pixel(2048, 400, image::Rgba([255, 255, 255, 255]));
        glyphs(&mut source, [20, 20, 20]);
        let lettering = detect_lettering(&source, &wide_block()).unwrap();
        assert_eq!(lettering, SourceLettering { fill: [20, 20, 20], outline: None, dark_background: None });
        let mut block = wide_block();
        block.style.text_color = Some("#111111".into());
        lettering.apply_to(&mut block);
        assert_eq!(block.style.text_color.as_deref(), Some("#111111"), "close to the preset: kept");
        assert_eq!(block.links.get(StyleKey::TextColor), None);
    }

    #[test]
    fn a_white_rim_over_art_is_an_outline() {
        let mut source = RgbaImage::from_fn(2048, 400, |x, y| {
            let v = if (x / 2 + y / 2) % 2 == 0 { 60 } else { 140 };
            image::Rgba([v, v, v, 255])
        });
        for x0 in [1000, 1022, 1044] {
            paint(&mut source, x0 - 3, 177, 16, 46, 250);
        }
        glyphs(&mut source, [10, 10, 10]);
        let lettering = detect_lettering(&source, &wide_block()).unwrap();
        assert_eq!(lettering.fill, [10, 10, 10]);
        assert_eq!(lettering.outline, Some(([250, 250, 250], 3)));
        let mut block = wide_block();
        lettering.apply_to(&mut block);
        assert_eq!(block.style.outline_width_px, Some(6.0));
        assert_eq!(block.style.outline_color.as_deref(), Some("#fafafa"));
    }

    #[test]
    fn colored_text_sets_the_fill() {
        let mut source = RgbaImage::from_pixel(2048, 400, image::Rgba([255, 255, 255, 255]));
        glyphs(&mut source, [200, 30, 30]);
        let mut block = wide_block();
        block.style.text_color = Some("#111111".into());
        detect_lettering(&source, &block).unwrap().apply_to(&mut block);
        assert_eq!(block.style.text_color.as_deref(), Some("#c81e1e"));
    }
}
