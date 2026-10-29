//! Inpaint engines (`inpaintEngine.ts`): mask preparation, the local fill fallback,
//! JPEG artifact cleanup and LaMa (window by window, `windows.rs`).

use std::collections::HashMap;

use image::{GrayImage, RgbaImage};
use mtt_core::model::{InpaintEngine, InpaintSettings};

use crate::lama::Lama;
use crate::{Rect, box_blur, dilate};

const MASK_ACTIVE_THRESHOLD: f32 = 0.03;
const ARTIFACT_CLEANUP_STRENGTH: f32 = 0.9;
const ARTIFACT_CLEANUP_MIN_LUMA: f32 = 190.0;
const ARTIFACT_CLEANUP_SAMPLE_MIN_LUMA: f32 = 225.0;
const ARTIFACT_CLEANUP_MAX_CHROMA: f32 = 24.0;
const ARTIFACT_CLEANUP_MAX_BG_DISTANCE: f32 = 90.0;
const ARTIFACT_CLEANUP_DARK_STROKE_LUMA: f32 = 150.0;
/// Share of the pixels around the mask a color needs to count as one the page is drawn
/// with (paper, ink, flat tones) rather than JPEG noise or anti-aliasing.
const ALIASED_MIN_SHARE: f32 = 0.02;
/// Most flat tones an aliased fill picks from.
const ALIASED_MAX_TONES: usize = 32;
/// A tone fills a pixel only when it makes up this share of the unmasked pixels within
/// `ALIASED_TONE_REACH` cells of `ALIASED_TONE_CELL` px, so a gray from another panel
/// doesn't creep into black-and-white art.
const ALIASED_TONE_CELL: u32 = 8;
const ALIASED_TONE_REACH: usize = 6;
const ALIASED_TONE_MIN_LOCAL_SHARE: u32 = 20; // 1/20 = 5%
const NEIGHBORS: [(i32, i32); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];

/// `normalizeInpaintSettings`.
pub fn normalize_settings(engine: InpaintEngine, settings: &InpaintSettings) -> InpaintSettings {
    InpaintSettings {
        engine,
        padding_px: settings.padding_px.round().max(0.0),
        feather_px: settings.feather_px.round().max(0.0),
        tile_size: settings.tile_size.round().max(128.0),
        artifact_cleanup_px: Some(settings.artifact_cleanup_px.unwrap_or(8.0).round().max(0.0)),
        aliased: settings.aliased,
        extra: settings.extra.clone(),
    }
}

fn luma(r: u8, g: u8, b: u8) -> f32 {
    f32::from(r) * 0.299 + f32::from(g) * 0.587 + f32::from(b) * 0.114
}

fn chroma(r: u8, g: u8, b: u8) -> f32 {
    f32::from(r.max(g).max(b)) - f32::from(r.min(g).min(b))
}

/// Binary threshold, padding dilation and feathering (`prepareMask`). Values are coverage.
pub fn prepare_mask(mask: &GrayImage, settings: &InpaintSettings) -> GrayImage {
    let (width, height) = mask.dimensions();
    let threshold = (MASK_ACTIVE_THRESHOLD * 255.0) as u8;
    let binary: Vec<u8> = mask.as_raw().iter().map(|v| if *v > threshold { 255 } else { 0 }).collect();
    let padding = settings.padding_px as usize;
    let feather = settings.feather_px as usize;
    let dilated = if padding > 0 { dilate(&binary, width as usize, height as usize, padding) } else { binary };
    let coverage = if feather > 0 { box_blur(&dilated, width as usize, height as usize, feather) } else { dilated };
    GrayImage::from_raw(width, height, coverage).expect("mask size")
}

pub fn mask_bounds(mask: &GrayImage) -> Option<Rect> {
    bounds_above(mask, 0)
}

/// Bounds of the pixels above `threshold`, row by row.
fn bounds_above(mask: &GrayImage, threshold: u8) -> Option<Rect> {
    let width = mask.width() as usize;
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (usize::MAX, usize::MAX, 0, 0);
    for (y, row) in mask.as_raw().chunks_exact(width.max(1)).enumerate() {
        let Some(first) = row.iter().position(|value| *value > threshold) else { continue };
        let last = row.iter().rposition(|value| *value > threshold).unwrap_or(first);
        (min_x, max_x) = (min_x.min(first), max_x.max(last));
        (min_y, max_y) = (min_y.min(y), max_y.max(y));
    }
    (min_y != usize::MAX).then(|| Rect {
        x: min_x as u32,
        y: min_y as u32,
        width: (max_x - min_x + 1) as u32,
        height: (max_y - min_y + 1) as u32,
    })
}

/// `prepare_mask` worked out only around the mask, offset to where it sits on the page.
/// The window reaches past the thresholded pixels by the padding plus twice the feather, so
/// the edge-normalized blur inside it equals the full-page one. `None` when nothing is
/// covered.
fn prepare_mask_window(mask: &GrayImage, settings: &InpaintSettings) -> Option<(Rect, GrayImage)> {
    let (width, height) = mask.dimensions();
    let threshold = (MASK_ACTIVE_THRESHOLD * 255.0) as u8;
    let raw = bounds_above(mask, threshold)?;
    let reach = settings.padding_px as u32 + settings.feather_px as u32 * 2 + 1;
    let x = raw.x.saturating_sub(reach);
    let y = raw.y.saturating_sub(reach);
    let right = (raw.x + raw.width + reach).min(width);
    let bottom = (raw.y + raw.height + reach).min(height);
    let window = Rect { x, y, width: right - x, height: bottom - y };
    let crop = image::imageops::crop_imm(mask, window.x, window.y, window.width, window.height).to_image();
    Some((window, prepare_mask(&crop, settings)))
}

/// Context margin around the masked area handed to the engine (`cropRectForBounds`).
pub fn crop_rect_for_bounds(bounds: Rect, width: u32, height: u32, settings: &InpaintSettings) -> Rect {
    let cleanup = settings.artifact_cleanup_px.unwrap_or(0.0);
    let margin = (settings.padding_px + settings.feather_px + cleanup + 32.0)
        .max((settings.tile_size.min(1024.0) * 0.25).round()) as u32;
    let x = bounds.x.saturating_sub(margin);
    let y = bounds.y.saturating_sub(margin);
    let right = (bounds.x + bounds.width + margin).min(width);
    let bottom = (bounds.y + bounds.height + margin).min(height);
    Rect { x, y, width: right - x, height: bottom - y }
}

/// Runs `engine` and returns a full-size result layer that is transparent outside the
/// (prepared) mask, plus optional artifact cleanup around it.
pub fn run_inpaint(
    source: &RgbaImage,
    mask: &GrayImage,
    engine: InpaintEngine,
    settings: &InpaintSettings,
    lama: Option<&Lama>,
) -> Result<RgbaImage, String> {
    run_inpaint_in(source, mask, engine, settings, lama).map(|(result, _)| result)
}

/// `run_inpaint`, also returning the region the result can differ from transparent in
/// (`None`: nowhere), so follow-up steps can stay inside it.
pub fn run_inpaint_in(
    source: &RgbaImage,
    mask: &GrayImage,
    engine: InpaintEngine,
    settings: &InpaintSettings,
    lama: Option<&Lama>,
) -> Result<(RgbaImage, Option<Rect>), String> {
    let settings = normalize_settings(engine, settings);
    let (width, height) = source.dimensions();
    if mask.dimensions() != source.dimensions() {
        return Err(format!("마스크 크기 {:?}가 원본 {:?}와 다릅니다.", mask.dimensions(), source.dimensions()));
    }
    let Some((window, prepared)) = prepare_mask_window(mask, &settings) else {
        return Ok((RgbaImage::new(width, height), None));
    };
    let Some(local) = mask_bounds(&prepared) else {
        return Ok((RgbaImage::new(width, height), None));
    };
    if engine == InpaintEngine::MaskFillFallback {
        let mut image = RgbaImage::from_pixel(width, height, image::Rgba([0, 0, 0, 255]));
        for (x, y, value) in prepared.enumerate_pixels() {
            let v = value[0];
            image.put_pixel(window.x + x, window.y + y, image::Rgba([v, v, v, 255]));
        }
        let full = Rect { x: 0, y: 0, width, height };
        return Ok((image, Some(full)));
    }
    let bounds = Rect { x: window.x + local.x, y: window.y + local.y, ..local };
    let rect = crop_rect_for_bounds(bounds, width, height, &settings);
    let source_crop = image::imageops::crop_imm(source, rect.x, rect.y, rect.width, rect.height).to_image();
    // The prepared mask is zero outside its window.
    let mut mask_crop = GrayImage::new(rect.width, rect.height);
    for (x, y, value) in prepared.enumerate_pixels() {
        let (px, py) = (window.x + x, window.y + y);
        if rect.contains(px, py) {
            mask_crop.put_pixel(px - rect.x, py - rect.y, *value);
        }
    }
    let inpainted = match (engine, lama) {
        (InpaintEngine::Lama, Some(lama)) => lama.inpaint(&source_crop, &mask_crop)?,
        (InpaintEngine::Lama, None) => return Err("LaMa 인페인트 명령이 설정되지 않았습니다.".into()),
        _ => local_fill(&source_crop, &mask_crop),
    };
    if inpainted.dimensions() != source_crop.dimensions() {
        return Err("인페인트 결과 크기가 맞지 않습니다.".into());
    }
    Ok((masked_result_layer(source, &inpainted, &mask_crop, rect, &settings), Some(rect)))
}

/// Fills the mask ring by ring from its border inward with the average of the neighbours
/// that are unmasked or filled by an earlier ring (`localFillInpaint`, which counted the
/// pixels it had just filled in the same pass and so filled everything in one raster scan
/// from the top left). A ring reads only what was there before it, so the fill comes in
/// evenly from every side.
pub fn local_fill(source: &RgbaImage, mask: &GrayImage) -> RgbaImage {
    let (width, height) = source.dimensions();
    let (w, h) = (width as i32, height as i32);
    let total = (width * height) as usize;
    let mut output = source.clone();
    let coverage: Vec<f32> = mask.as_raw().iter().map(|v| f32::from(*v) / 255.0).collect();
    let masked: Vec<bool> = coverage.iter().map(|c| *c > MASK_ACTIVE_THRESHOLD).collect();
    let fallback = average_unmasked_color(source, &masked);
    let neighbors = |index: usize| {
        let (x, y) = ((index % width as usize) as i32, (index / width as usize) as i32);
        NEIGHBORS.iter().filter_map(move |(dx, dy)| {
            let (nx, ny) = (x + dx, y + dy);
            (nx >= 0 && ny >= 0 && nx < w && ny < h).then_some((ny * w + nx) as usize)
        })
    };
    // Unmasked or filled by an earlier ring; and in some ring already.
    let mut filled: Vec<bool> = masked.iter().map(|masked| !masked).collect();
    let mut queued = filled.clone();
    let mut ring: Vec<usize> =
        (0..total).filter(|index| masked[*index] && neighbors(*index).any(|n| filled[n])).collect();
    ring.iter().for_each(|index| queued[*index] = true);
    let mut colors = Vec::new();
    while !ring.is_empty() {
        colors.clear();
        colors.extend(ring.iter().map(|index| {
            let (mut sum, mut count) = ([0f32; 3], 0f32);
            for neighbor in neighbors(*index).filter(|n| filled[*n]) {
                let p = output.as_raw()[neighbor * 4..][..3].iter();
                sum.iter_mut().zip(p).for_each(|(sum, value)| *sum += f32::from(*value));
                count += 1.0;
            }
            sum.map(|sum| sum / count)
        }));
        for (index, color) in ring.iter().zip(&colors) {
            write_color(&mut output, (*index % width as usize) as u32, (*index / width as usize) as u32, *color);
            filled[*index] = true;
        }
        let mut next = Vec::new();
        for neighbor in ring.iter().flat_map(|index| neighbors(*index)) {
            if !queued[neighbor] {
                queued[neighbor] = true;
                next.push(neighbor);
            }
        }
        ring = next;
    }
    for index in 0..total {
        if !masked[index] {
            continue;
        }
        let (x, y) = ((index % width as usize) as u32, (index / width as usize) as u32);
        if !filled[index] {
            write_color(&mut output, x, y, fallback);
        }
        let amount = coverage[index].clamp(0.0, 1.0);
        let src = source.get_pixel(x, y).0;
        let out = output.get_pixel_mut(x, y);
        for channel in 0..3 {
            out[channel] = (f32::from(src[channel]) * (1.0 - amount) + f32::from(out[channel]) * amount).round() as u8;
        }
        out[3] = src[3];
    }
    output
}

fn average_unmasked_color(source: &RgbaImage, masked: &[bool]) -> [f32; 3] {
    let (mut r, mut g, mut b, mut count) = (0f64, 0f64, 0f64, 0f64);
    for (pixel, is_masked) in source.pixels().zip(masked) {
        if *is_masked {
            continue;
        }
        r += f64::from(pixel[0]);
        g += f64::from(pixel[1]);
        b += f64::from(pixel[2]);
        count += 1.0;
    }
    if count == 0.0 { [255.0; 3] } else { [(r / count) as f32, (g / count) as f32, (b / count) as f32] }
}

fn write_color(image: &mut RgbaImage, x: u32, y: u32, color: [f32; 3]) {
    let pixel = image.get_pixel_mut(x, y);
    pixel[0] = color[0].round() as u8;
    pixel[1] = color[1].round() as u8;
    pixel[2] = color[2].round() as u8;
}

/// Places the inpainted crop into a transparent full-size layer (`maskedResultLayer`).
pub fn masked_result_layer(
    source: &RgbaImage,
    inpainted: &RgbaImage,
    mask: &GrayImage,
    rect: Rect,
    settings: &InpaintSettings,
) -> RgbaImage {
    let mut output = RgbaImage::new(source.width(), source.height());
    if settings.aliased {
        let palette = surrounding_palette(source, mask, rect);
        let nearby = NearbyTones::new(source, mask, rect, &palette);
        let mut colors = Vec::with_capacity(palette.len());
        for (x, y, value) in mask.enumerate_pixels() {
            if value[0] == 0 {
                continue;
            }
            colors.clear();
            colors.extend(palette.iter().enumerate().filter(|(i, _)| nearby.allows(x, y, *i)).map(|(_, c)| *c));
            let [r, g, b] = nearest_color(&colors, inpainted.get_pixel(x, y).0);
            output.put_pixel(rect.x + x, rect.y + y, image::Rgba([r, g, b, 255]));
        }
        return output;
    }
    let cleanup_px = settings.artifact_cleanup_px.unwrap_or(0.0).round().max(0.0) as usize;
    let cleanup_mask = (cleanup_px > 0).then(|| {
        let binary: Vec<u8> = mask.as_raw().iter().map(|v| if *v > 0 { 255 } else { 0 }).collect();
        let dilated = dilate(&binary, rect.width as usize, rect.height as usize, cleanup_px);
        box_blur(
            &dilated,
            rect.width as usize,
            rect.height as usize,
            ((cleanup_px as f32 / 2.0).round() as usize).max(1),
        )
    });
    let background = cleanup_mask.as_ref().map(|_| estimate_background(source, mask, rect));
    for y in 0..rect.height {
        for x in 0..rect.width {
            let amount = mask.get_pixel(x, y)[0];
            let (sx, sy) = (rect.x + x, rect.y + y);
            if amount == 0 {
                if let (Some(cleanup), Some(background)) = (&cleanup_mask, background) {
                    let value = cleanup[(y * rect.width + x) as usize];
                    if let Some(pixel) = artifact_cleanup_pixel(source, mask, rect, x, y, value, background) {
                        output.put_pixel(sx, sy, pixel);
                    }
                }
                continue;
            }
            let p = inpainted.get_pixel(x, y).0;
            output.put_pixel(sx, sy, image::Rgba([p[0], p[1], p[2], amount]));
        }
    }
    output
}

/// Colors the fill may use on an aliased page: the most common dark and light colors of
/// the unmasked pixels in `rect` (ink and paper), plus any other color common enough to be
/// a flat tone. The most common rather than the mean color, so JPEG noise and gray tones
/// don't turn white paper gray.
fn surrounding_palette(source: &RgbaImage, mask: &GrayImage, rect: Rect) -> Vec<[u8; 3]> {
    let mut counts: HashMap<[u8; 3], u32> = HashMap::new();
    for (x, y, value) in mask.enumerate_pixels() {
        if value[0] == 0 {
            let p = source.get_pixel(rect.x + x, rect.y + y).0;
            *counts.entry([p[0], p[1], p[2]]).or_default() += 1;
        }
    }
    let total: u32 = counts.values().sum();
    let mut by_count: Vec<([u8; 3], u32)> = counts.into_iter().collect();
    // Most common first; ties by color so the palette doesn't depend on hash order.
    by_count.sort_unstable_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let is_dark = |c: &[u8; 3]| luma(c[0], c[1], c[2]) < 128.0;
    let ink = by_count.iter().find(|(color, _)| is_dark(color)).map_or([0; 3], |(color, _)| *color);
    let paper = by_count.iter().find(|(color, _)| !is_dark(color)).map_or([255; 3], |(color, _)| *color);
    let mut palette = vec![ink, paper];
    let min_count = (total as f32 * ALIASED_MIN_SHARE).max(1.0) as u32;
    for (color, count) in by_count {
        if palette.len() < 2 + ALIASED_MAX_TONES && count >= min_count && !palette.contains(&color) {
            palette.push(color);
        }
    }
    palette
}

/// Which of a `surrounding_palette`'s tones (every entry past ink and paper) are drawn near
/// each cell of the crop.
struct NearbyTones {
    columns: usize,
    /// Per cell, a bit per tone.
    allowed: Vec<u32>,
}

impl NearbyTones {
    fn new(source: &RgbaImage, mask: &GrayImage, rect: Rect, palette: &[[u8; 3]]) -> Self {
        let cell = ALIASED_TONE_CELL;
        let (columns, rows) = (rect.width.div_ceil(cell) as usize, rect.height.div_ceil(cell) as usize);
        let tones = &palette[2.min(palette.len())..];
        // Unmasked pixels per cell, then each tone's pixels per cell.
        let mut unmasked = vec![0u32; columns * rows];
        let mut toned = vec![0u32; columns * rows * tones.len()];
        for (x, y, value) in mask.enumerate_pixels() {
            if value[0] > 0 {
                continue;
            }
            let index = (y / cell) as usize * columns + (x / cell) as usize;
            unmasked[index] += 1;
            let p = source.get_pixel(rect.x + x, rect.y + y).0;
            if let Some(tone) = tones.iter().position(|tone| *tone == [p[0], p[1], p[2]]) {
                toned[index * tones.len() + tone] += 1;
            }
        }
        let reach = ALIASED_TONE_REACH;
        let mut allowed = vec![0u32; columns * rows];
        let mut sums = vec![0u32; tones.len()];
        for row in 0..rows {
            for column in 0..columns {
                sums.fill(0);
                let mut total = 0;
                for r in row.saturating_sub(reach)..(row + reach + 1).min(rows) {
                    for c in column.saturating_sub(reach)..(column + reach + 1).min(columns) {
                        let index = r * columns + c;
                        total += unmasked[index];
                        for (sum, count) in sums.iter_mut().zip(&toned[index * tones.len()..][..tones.len()]) {
                            *sum += count;
                        }
                    }
                }
                for (tone, sum) in sums.iter().enumerate() {
                    if *sum > 0 && sum * ALIASED_TONE_MIN_LOCAL_SHARE >= total {
                        allowed[row * columns + column] |= 1 << tone;
                    }
                }
            }
        }
        Self { columns, allowed }
    }

    /// Whether palette entry `index` may fill crop pixel (`x`, `y`); ink and paper always.
    fn allows(&self, x: u32, y: u32, index: usize) -> bool {
        let cell = (y / ALIASED_TONE_CELL) as usize * self.columns + (x / ALIASED_TONE_CELL) as usize;
        index < 2 || self.allowed[cell] & (1 << (index - 2)) != 0
    }
}

fn nearest_color(palette: &[[u8; 3]], p: [u8; 4]) -> [u8; 3] {
    let distance = |color: &[u8; 3]| -> i32 {
        (0..3).map(|channel| (i32::from(color[channel]) - i32::from(p[channel])).pow(2)).sum()
    };
    palette.iter().copied().min_by_key(distance).unwrap_or([p[0], p[1], p[2]])
}

fn estimate_background(source: &RgbaImage, mask: &GrayImage, rect: Rect) -> [f32; 3] {
    let (mut r, mut g, mut b, mut weight_sum) = (0f32, 0f32, 0f32, 0f32);
    for y in 0..rect.height {
        for x in 0..rect.width {
            if mask.get_pixel(x, y)[0] > 0 {
                continue;
            }
            let p = source.get_pixel(rect.x + x, rect.y + y).0;
            let l = luma(p[0], p[1], p[2]);
            if l < ARTIFACT_CLEANUP_SAMPLE_MIN_LUMA || chroma(p[0], p[1], p[2]) > ARTIFACT_CLEANUP_MAX_CHROMA {
                continue;
            }
            let weight = ((l - ARTIFACT_CLEANUP_SAMPLE_MIN_LUMA) / (255.0 - ARTIFACT_CLEANUP_SAMPLE_MIN_LUMA)).max(0.1);
            r += f32::from(p[0]) * weight;
            g += f32::from(p[1]) * weight;
            b += f32::from(p[2]) * weight;
            weight_sum += weight;
        }
    }
    if weight_sum <= 0.0 { [255.0; 3] } else { [r / weight_sum, g / weight_sum, b / weight_sum] }
}

fn artifact_cleanup_pixel(
    source: &RgbaImage,
    mask: &GrayImage,
    rect: Rect,
    x: u32,
    y: u32,
    amount: u8,
    background: [f32; 3],
) -> Option<image::Rgba<u8>> {
    if amount == 0 {
        return None;
    }
    // Keep pixels next to dark unmasked strokes (line art) untouched.
    for (dx, dy) in NEIGHBORS {
        let (nx, ny) = (x as i32 + dx, y as i32 + dy);
        if nx < 0 || ny < 0 || nx >= rect.width as i32 || ny >= rect.height as i32 {
            continue;
        }
        if mask.get_pixel(nx as u32, ny as u32)[0] > 0 {
            continue;
        }
        let p = source.get_pixel(rect.x + nx as u32, rect.y + ny as u32).0;
        if luma(p[0], p[1], p[2]) < ARTIFACT_CLEANUP_DARK_STROKE_LUMA {
            return None;
        }
    }
    let p = source.get_pixel(rect.x + x, rect.y + y).0;
    let l = luma(p[0], p[1], p[2]);
    if l < ARTIFACT_CLEANUP_MIN_LUMA || chroma(p[0], p[1], p[2]) > ARTIFACT_CLEANUP_MAX_CHROMA {
        return None;
    }
    let distance = ((f32::from(p[0]) - background[0]).powi(2)
        + (f32::from(p[1]) - background[1]).powi(2)
        + (f32::from(p[2]) - background[2]).powi(2))
    .sqrt();
    if distance > ARTIFACT_CLEANUP_MAX_BG_DISTANCE {
        return None;
    }
    let luma_weight = ((l - ARTIFACT_CLEANUP_MIN_LUMA) / (255.0 - ARTIFACT_CLEANUP_MIN_LUMA)).clamp(0.0, 1.0);
    let background_weight = (1.0 - distance / ARTIFACT_CLEANUP_MAX_BG_DISTANCE).clamp(0.0, 1.0);
    let alpha = (f32::from(amount) * ARTIFACT_CLEANUP_STRENGTH * luma_weight.max(background_weight)).round();
    (alpha > 0.0).then(|| {
        image::Rgba([
            background[0].round() as u8,
            background[1].round() as u8,
            background[2].round() as u8,
            alpha as u8,
        ])
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checker_source() -> RgbaImage {
        RgbaImage::from_fn(
            40,
            40,
            |x, _| if x < 20 { image::Rgba([200, 0, 0, 255]) } else { image::Rgba([0, 0, 200, 255]) },
        )
    }

    #[test]
    fn local_fill_propagates_neighbouring_colors() {
        let source = checker_source();
        let mut mask = GrayImage::new(40, 40);
        for y in 15..25 {
            for x in 5..15 {
                mask.put_pixel(x, y, image::Luma([255]));
            }
        }
        let filled = local_fill(&source, &mask);
        let p = filled.get_pixel(10, 20).0;
        assert!(p[0] > 150 && p[2] < 50, "left area filled with red: {p:?}");
        assert_eq!(filled.get_pixel(30, 20), source.get_pixel(30, 20), "unmasked pixels untouched");
    }

    #[test]
    fn local_fill_comes_in_evenly_from_every_side() {
        // White above row 20, black below, and a 20×20 hole across the edge. Filled in one
        // raster scan, the white ran down to row 25 (`[65, 224, 255, 254, 184]`) and left a
        // seam on the last row.
        let source = RgbaImage::from_fn(40, 40, |_, y| {
            let v = if y < 20 { 255 } else { 0 };
            image::Rgba([v, v, v, 255])
        });
        let mut mask = GrayImage::new(40, 40);
        for y in 10..30 {
            for x in 10..30 {
                mask.put_pixel(x, y, image::Luma([255]));
            }
        }
        let filled = local_fill(&source, &mask);
        let value = |x: u32, y: u32| filled.get_pixel(x, y)[0];
        for x in 10..30 {
            for y in 10..18 {
                assert!(value(x, y) > 200, "({x}, {y}) is white: {}", value(x, y));
            }
            for y in 22..30 {
                assert!(value(x, y) < 55, "({x}, {y}) is black: {}", value(x, y));
            }
            for y in 10..30 {
                assert!(value(x, y).abs_diff(value(39 - x, y)) <= 1, "({x}, {y}) mirrors its other side");
            }
        }
    }

    #[test]
    fn run_inpaint_result_is_transparent_outside_mask() {
        let source = checker_source();
        let mut mask = GrayImage::new(40, 40);
        mask.put_pixel(10, 10, image::Luma([255]));
        let settings = InpaintSettings { artifact_cleanup_px: Some(0.0), ..InpaintSettings::default() };
        let result = run_inpaint(&source, &mask, InpaintEngine::LocalFillFallback, &settings, None).unwrap();
        assert_eq!(result.get_pixel(10, 10)[3], 255);
        assert_eq!(result.get_pixel(30, 30)[3], 0);
        let empty =
            run_inpaint(&source, &GrayImage::new(40, 40), InpaintEngine::LocalFillFallback, &settings, None).unwrap();
        assert!(empty.pixels().all(|p| p[3] == 0));
    }

    #[test]
    fn windowed_mask_preparation_matches_the_full_page() {
        let mut mask = GrayImage::new(120, 90);
        for (x, y) in [(40, 30), (41, 30), (70, 60), (3, 2), (60, 45)] {
            mask.put_pixel(x, y, image::Luma([255]));
        }
        mask.put_pixel(90, 80, image::Luma([5])); // below the threshold
        for (padding, feather) in [(0.0, 0.0), (3.0, 0.0), (0.0, 4.0), (2.0, 6.0)] {
            let settings = InpaintSettings { padding_px: padding, feather_px: feather, ..InpaintSettings::default() };
            let full = prepare_mask(&mask, &settings);
            let (window, prepared) = prepare_mask_window(&mask, &settings).unwrap();
            for (x, y, value) in full.enumerate_pixels() {
                let inside = window.contains(x, y);
                let windowed = if inside { prepared.get_pixel(x - window.x, y - window.y)[0] } else { 0 };
                assert_eq!(value[0], windowed, "({x}, {y}) padding {padding} feather {feather}");
            }
        }
    }

    #[test]
    fn crop_margin_matches_ts() {
        let settings = normalize_settings(InpaintEngine::LocalFillFallback, &InpaintSettings::default());
        // max(0 + 0 + 8 + 32, round(1024 * 0.25)) = 256
        let rect = crop_rect_for_bounds(Rect { x: 300, y: 10, width: 10, height: 10 }, 1000, 1000, &settings);
        assert_eq!((rect.x, rect.y, rect.width, rect.height), (44, 0, 522, 276));
    }

    #[test]
    fn aliased_fill_uses_page_colors_with_hard_edges() {
        // 1-bit page: white paper with a black line through the masked area.
        let source = RgbaImage::from_fn(40, 40, |x, _| {
            if x == 20 { image::Rgba([0, 0, 0, 255]) } else { image::Rgba([255, 255, 255, 255]) }
        });
        let mut mask = GrayImage::new(40, 40);
        for y in 15..25 {
            for x in 15..25 {
                mask.put_pixel(x, y, image::Luma([255]));
            }
        }
        let settings =
            InpaintSettings { feather_px: 3.0, artifact_cleanup_px: Some(8.0), aliased: true, ..Default::default() };
        let result = run_inpaint(&source, &mask, InpaintEngine::LocalFillFallback, &settings, None).unwrap();
        for pixel in result.pixels() {
            assert!(pixel[3] == 0 || pixel[3] == 255, "hard alpha: {pixel:?}");
            if pixel[3] == 255 {
                assert!(pixel.0[..3] == [0, 0, 0] || pixel.0[..3] == [255, 255, 255], "page color: {pixel:?}");
            }
        }
        assert_eq!(result.get_pixel(18, 20)[3], 255, "the mask is filled");
    }

    #[test]
    fn aliased_palette_keeps_paper_white_on_noisy_pages() {
        // JPEG-like page: mostly white paper and black ink, a flat gray tone, and noise.
        let source = RgbaImage::from_fn(100, 100, |x, y| {
            let v = match (x, y) {
                (_, 0..10) => 0,
                (_, 10..20) => 167,
                (_, 20..22) => 200 + (x % 50) as u8,
                (_, 22..24) => (x % 40) as u8,
                _ => 255,
            };
            image::Rgba([v, v, v, 255])
        });
        let mask = GrayImage::new(100, 100);
        let palette = surrounding_palette(&source, &mask, Rect { x: 0, y: 0, width: 100, height: 100 });
        assert_eq!(palette, vec![[0; 3], [255; 3], [167; 3]]);
        assert_eq!(nearest_color(&palette, [245, 245, 245, 255]), [255; 3]);
        assert_eq!(nearest_color(&palette, [180, 180, 180, 255]), [167; 3]);
    }

    #[test]
    fn aliased_fill_uses_only_tones_drawn_nearby() {
        // A gray tone panel on the left; black-and-white art on the right.
        let source = RgbaImage::from_fn(400, 100, |x, y| match x {
            0..100 => image::Rgba([167, 167, 167, 255]),
            _ if (x + y) % 2 == 0 => image::Rgba([0, 0, 0, 255]),
            _ => image::Rgba([255, 255, 255, 255]),
        });
        let mut mask = GrayImage::new(400, 100);
        for (x, y) in (300..340).flat_map(|x| (40..60).map(move |y| (x, y))).chain([(50, 50)]) {
            mask.put_pixel(x, y, image::Luma([255]));
        }
        let rect = Rect { x: 0, y: 0, width: 400, height: 100 };
        let palette = surrounding_palette(&source, &mask, rect);
        assert!(palette.contains(&[167; 3]));
        // A mid gray fill: gray in the tone panel, black or white in the art.
        let inpainted = RgbaImage::from_pixel(400, 100, image::Rgba([150, 150, 150, 255]));
        let settings = InpaintSettings { aliased: true, ..Default::default() };
        let result = masked_result_layer(&source, &inpainted, &mask, rect, &settings);
        assert_eq!(result.get_pixel(50, 50).0, [167, 167, 167, 255]);
        assert!(
            (300..340).flat_map(|x| (40..60).map(move |y| (x, y))).all(|(x, y)| result.get_pixel(x, y)[0] != 167),
            "no gray in the black-and-white art"
        );
    }

    #[test]
    fn artifact_cleanup_whitens_light_noise_near_mask() {
        let mut source = RgbaImage::from_pixel(60, 60, image::Rgba([250, 250, 250, 255]));
        source.put_pixel(31, 30, image::Rgba([215, 215, 215, 255]));
        let mut mask = GrayImage::new(60, 60);
        mask.put_pixel(30, 30, image::Luma([255]));
        let settings = InpaintSettings { artifact_cleanup_px: Some(4.0), ..InpaintSettings::default() };
        let result = run_inpaint(&source, &mask, InpaintEngine::LocalFillFallback, &settings, None).unwrap();
        assert!(result.get_pixel(31, 30)[3] > 0, "noise next to the mask is covered");
        assert_eq!(result.get_pixel(5, 5)[3], 0, "far pixels untouched");
    }
}
