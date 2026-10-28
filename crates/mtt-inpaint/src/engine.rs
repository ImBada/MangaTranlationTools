//! Inpaint engines (`inpaintEngine.ts`): mask preparation, the local fill fallback,
//! JPEG artifact cleanup and the external LaMa command.

use image::{GrayImage, RgbaImage};
use mtt_core::model::{InpaintEngine, InpaintSettings};

use crate::lama::LamaCommand;
use crate::{Rect, box_blur, dilate};

const MASK_ACTIVE_THRESHOLD: f32 = 0.03;
const ARTIFACT_CLEANUP_STRENGTH: f32 = 0.9;
const ARTIFACT_CLEANUP_MIN_LUMA: f32 = 190.0;
const ARTIFACT_CLEANUP_SAMPLE_MIN_LUMA: f32 = 225.0;
const ARTIFACT_CLEANUP_MAX_CHROMA: f32 = 24.0;
const ARTIFACT_CLEANUP_MAX_BG_DISTANCE: f32 = 90.0;
const ARTIFACT_CLEANUP_DARK_STROKE_LUMA: f32 = 150.0;
const NEIGHBORS: [(i32, i32); 8] = [(-1, -1), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)];

/// `normalizeInpaintSettings`.
pub fn normalize_settings(engine: InpaintEngine, settings: &InpaintSettings) -> InpaintSettings {
    InpaintSettings {
        engine,
        padding_px: settings.padding_px.round().max(0.0),
        feather_px: settings.feather_px.round().max(0.0),
        tile_size: settings.tile_size.round().max(128.0),
        artifact_cleanup_px: Some(settings.artifact_cleanup_px.unwrap_or(8.0).round().max(0.0)),
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
    let (width, height) = mask.dimensions();
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (width, height, 0, 0);
    let mut any = false;
    for (x, y, value) in mask.enumerate_pixels() {
        if value[0] > 0 {
            any = true;
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    any.then(|| Rect { x: min_x, y: min_y, width: max_x - min_x + 1, height: max_y - min_y + 1 })
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
    lama: Option<&LamaCommand>,
) -> Result<RgbaImage, String> {
    let settings = normalize_settings(engine, settings);
    let (width, height) = source.dimensions();
    if mask.dimensions() != source.dimensions() {
        return Err(format!("마스크 크기 {:?}가 원본 {:?}와 다릅니다.", mask.dimensions(), source.dimensions()));
    }
    let mask = prepare_mask(mask, &settings);
    let Some(bounds) = mask_bounds(&mask) else {
        return Ok(RgbaImage::new(width, height));
    };
    if engine == InpaintEngine::MaskFillFallback {
        return Ok(RgbaImage::from_fn(width, height, |x, y| {
            let v = mask.get_pixel(x, y)[0];
            image::Rgba([v, v, v, 255])
        }));
    }
    let rect = crop_rect_for_bounds(bounds, width, height, &settings);
    let source_crop = image::imageops::crop_imm(source, rect.x, rect.y, rect.width, rect.height).to_image();
    let mask_crop = image::imageops::crop_imm(&mask, rect.x, rect.y, rect.width, rect.height).to_image();
    let inpainted = match (engine, lama) {
        (InpaintEngine::Lama, Some(lama)) => lama.run(&source_crop, &mask_crop)?,
        (InpaintEngine::Lama, None) => return Err("LaMa 인페인트 명령이 설정되지 않았습니다.".into()),
        _ => local_fill(&source_crop, &mask_crop),
    };
    if inpainted.dimensions() != source_crop.dimensions() {
        return Err("인페인트 결과 크기가 맞지 않습니다.".into());
    }
    Ok(masked_result_layer(source, &inpainted, &mask_crop, rect, &settings))
}

/// Breadth-first fill from the mask border inward with the average of known neighbours
/// (`localFillInpaint`).
pub fn local_fill(source: &RgbaImage, mask: &GrayImage) -> RgbaImage {
    let (width, height) = source.dimensions();
    let (w, h) = (width as i32, height as i32);
    let total = (width * height) as usize;
    let mut output = source.clone();
    let coverage: Vec<f32> = mask.as_raw().iter().map(|v| f32::from(*v) / 255.0).collect();
    let masked: Vec<bool> = coverage.iter().map(|c| *c > MASK_ACTIVE_THRESHOLD).collect();
    let mut filled = vec![false; total];
    let mut queue: Vec<usize> = Vec::with_capacity(total);
    let fallback = average_unmasked_color(source, &masked);

    let fill_from_neighbors = |output: &mut RgbaImage, filled: &[bool], index: usize| {
        let (x, y) = ((index % width as usize) as i32, (index / width as usize) as i32);
        let (mut r, mut g, mut b, mut count) = (0f32, 0f32, 0f32, 0f32);
        for (dx, dy) in NEIGHBORS {
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0 || ny < 0 || nx >= w || ny >= h {
                continue;
            }
            let neighbor = (ny * w + nx) as usize;
            if masked[neighbor] && !filled[neighbor] {
                continue;
            }
            let p = output.get_pixel(nx as u32, ny as u32).0;
            r += f32::from(p[0]);
            g += f32::from(p[1]);
            b += f32::from(p[2]);
            count += 1.0;
        }
        let color = if count > 0.0 { [r / count, g / count, b / count] } else { fallback };
        write_color(output, x as u32, y as u32, color);
    };

    for y in 0..h {
        for x in 0..w {
            let index = (y * w + x) as usize;
            if !masked[index] {
                continue;
            }
            let has_neighbor = NEIGHBORS.iter().any(|(dx, dy)| {
                let (nx, ny) = (x + dx, y + dy);
                nx >= 0 && ny >= 0 && nx < w && ny < h && {
                    let neighbor = (ny * w + nx) as usize;
                    !masked[neighbor] || filled[neighbor]
                }
            });
            if !has_neighbor {
                continue;
            }
            fill_from_neighbors(&mut output, &filled, index);
            filled[index] = true;
            queue.push(index);
        }
    }
    let mut head = 0;
    while head < queue.len() {
        let index = queue[head];
        head += 1;
        let (x, y) = ((index % width as usize) as i32, (index / width as usize) as i32);
        for (dx, dy) in NEIGHBORS {
            let (nx, ny) = (x + dx, y + dy);
            if nx < 0 || ny < 0 || nx >= w || ny >= h {
                continue;
            }
            let neighbor = (ny * w + nx) as usize;
            if !masked[neighbor] || filled[neighbor] {
                continue;
            }
            fill_from_neighbors(&mut output, &filled, neighbor);
            filled[neighbor] = true;
            queue.push(neighbor);
        }
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
    fn crop_margin_matches_ts() {
        let settings = normalize_settings(InpaintEngine::LocalFillFallback, &InpaintSettings::default());
        // max(0 + 0 + 8 + 32, round(1024 * 0.25)) = 256
        let rect = crop_rect_for_bounds(Rect { x: 300, y: 10, width: 10, height: 10 }, 1000, 1000, &settings);
        assert_eq!((rect.x, rect.y, rect.width, rect.height), (44, 0, 522, 276));
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
