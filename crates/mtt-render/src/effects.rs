//! Pixel effects: CSS-like gaussian blur, font filter effects (`fontFilterEffects.ts`) and
//! hard (aliased) text palette snapping (`hardTextRendering.ts`).

use mtt_core::geometry::parse_hex_color;
use mtt_core::model::TranslationBlock;
use tiny_skia::Pixmap;

use crate::canvas::{from_straight_rgba, to_straight_rgba};

/// Approximates CSS `filter: blur(sigma)` with three box blurs (SVG feGaussianBlur rule)
/// on premultiplied pixels. Pixels outside the image count as transparent.
pub fn gaussian_blur(pixmap: &mut Pixmap, sigma: f64) {
    if sigma <= 0.0 {
        return;
    }
    let width = pixmap.width() as usize;
    let height = pixmap.height() as usize;
    let box_size = ((sigma * 3.0 * (2.0 * std::f64::consts::PI).sqrt() / 4.0) + 0.5).floor().max(1.0) as usize;
    // Three passes per axis; even box sizes alternate left/right bias as in the SVG spec.
    let passes: [(usize, usize); 3] = if box_size % 2 == 1 {
        let r = box_size / 2;
        [(r, r); 3]
    } else {
        let r = box_size / 2;
        [(r, r.saturating_sub(1)), (r.saturating_sub(1), r), (r, r)]
    };
    let mut scratch = vec![0u8; pixmap.data().len()];
    // Six passes ping-pong between the two buffers and end back in the pixmap.
    let (mut src, mut dst): (&mut [u8], &mut [u8]) = (pixmap.data_mut(), &mut scratch);
    for (left, right) in passes {
        box_blur_horizontal(src, dst, width, left, right);
        std::mem::swap(&mut src, &mut dst);
    }
    for (top, bottom) in passes {
        box_blur_vertical(src, dst, width, height, top, bottom);
        std::mem::swap(&mut src, &mut dst);
    }
}

fn box_blur_horizontal(src: &[u8], dst: &mut [u8], width: usize, left: usize, right: usize) {
    if width == 0 {
        return;
    }
    let window = (left + right + 1) as u32;
    for (src_row, dst_row) in src.chunks_exact(width * 4).zip(dst.chunks_exact_mut(width * 4)) {
        let pixel = |x: usize| &src_row[x * 4..x * 4 + 4];
        let mut sum = [0u32; 4];
        for x in 0..=right.min(width - 1) {
            for (total, value) in sum.iter_mut().zip(pixel(x)) {
                *total += u32::from(*value);
            }
        }
        for x in 0..width {
            for (out, total) in dst_row[x * 4..x * 4 + 4].iter_mut().zip(&sum) {
                *out = ((total + window / 2) / window) as u8;
            }
            let add = x + right + 1;
            if add < width {
                for (total, value) in sum.iter_mut().zip(pixel(add)) {
                    *total += u32::from(*value);
                }
            }
            if x >= left {
                for (total, value) in sum.iter_mut().zip(pixel(x - left)) {
                    *total -= u32::from(*value);
                }
            }
        }
    }
}

/// Row by row with a running sum per column and channel (cache friendly, unlike walking
/// down each column).
fn box_blur_vertical(src: &[u8], dst: &mut [u8], width: usize, height: usize, top: usize, bottom: usize) {
    if width == 0 || height == 0 {
        return;
    }
    let stride = width * 4;
    let window = (top + bottom + 1) as u32;
    let row = |y: usize| &src[y * stride..(y + 1) * stride];
    let mut sums = vec![0u32; stride];
    for y in 0..=bottom.min(height - 1) {
        for (total, value) in sums.iter_mut().zip(row(y)) {
            *total += u32::from(*value);
        }
    }
    for (y, dst_row) in dst.chunks_exact_mut(stride).enumerate() {
        for (out, total) in dst_row.iter_mut().zip(&sums) {
            *out = ((total + window / 2) / window) as u8;
        }
        let add = y + bottom + 1;
        if add < height {
            for (total, value) in sums.iter_mut().zip(row(add)) {
                *total += u32::from(*value);
            }
        }
        if y >= top {
            for (total, value) in sums.iter_mut().zip(row(y - top)) {
                *total -= u32::from(*value);
            }
        }
    }
}

const ZIGZAG_DISPLACEMENT_RATIO: f64 = 0.18;
const WAVE_DISPLACEMENT_RATIO: f64 = 0.16;
const GLITCH_DISPLACEMENT_RATIO: f64 = 0.14;
const GLITCH_CHANNEL_SPLIT_RATIO: f64 = 0.055;
const PIXELATE_MIN_BLOCK_SIZE_PX: f64 = 2.0;
const PIXELATE_BLOCK_RATIO: f64 = 0.26;
const BLUR_RADIUS_RATIO: f64 = 0.08;
const ZIGZAG_TREMBLE_BANDS_PER_FONT_SIZE: f64 = 12.0;
const ZIGZAG_VERTICAL_EDGE_FADE_RATIO: f64 = 0.06;
const ZIGZAG_TREMBLE_ALTERNATE_WEIGHT: f64 = 0.62;
const ZIGZAG_TREMBLE_NOISE_WEIGHT: f64 = 0.34;
const ZIGZAG_TREMBLE_MICRO_SHAKE: f64 = 0.14;
const NOISE_COLOR_RANGE: f64 = 48.0;
const NOISE_ALPHA_RANGE: f64 = 0.14;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterKind {
    Zigzag,
    Wave,
    Glitch,
    Pixelate,
    Blur,
    Noise,
}

impl FilterKind {
    pub const ALL: [FilterKind; 6] = [
        FilterKind::Zigzag,
        FilterKind::Wave,
        FilterKind::Glitch,
        FilterKind::Pixelate,
        FilterKind::Blur,
        FilterKind::Noise,
    ];

    pub fn label(self) -> &'static str {
        match self {
            FilterKind::Zigzag => "지그재그",
            FilterKind::Wave => "물결",
            FilterKind::Glitch => "글리치",
            FilterKind::Pixelate => "픽셀화",
            FilterKind::Blur => "번짐",
            FilterKind::Noise => "노이즈",
        }
    }

    /// `(enabled, intensity)` stored on the block.
    pub fn values(self, block: &TranslationBlock) -> (Option<bool>, Option<f64>) {
        let s = &block.style;
        match self {
            FilterKind::Zigzag => (s.filter_zigzag_enabled, s.filter_zigzag_intensity),
            FilterKind::Wave => (s.filter_wave_enabled, s.filter_wave_intensity),
            FilterKind::Glitch => (s.filter_glitch_enabled, s.filter_glitch_intensity),
            FilterKind::Pixelate => (s.filter_pixelate_enabled, s.filter_pixelate_intensity),
            FilterKind::Blur => (s.filter_blur_enabled, s.filter_blur_intensity),
            FilterKind::Noise => (s.filter_noise_enabled, s.filter_noise_intensity),
        }
    }

    pub fn intensity(self, block: &TranslationBlock) -> f64 {
        resolve_filter_intensity(self.values(block).1)
    }

    pub fn is_active(self, block: &TranslationBlock) -> bool {
        block.style.filter_effect_enabled == Some(true)
            && self.values(block).0 == Some(true)
            && self.intensity(block) > 0.0
    }

    fn margin_px(self, font_size: f64, intensity: f64) -> f64 {
        let fs = filter_font_size(font_size);
        match self {
            FilterKind::Zigzag => (fs * ZIGZAG_DISPLACEMENT_RATIO * intensity * 2.0 + 2.0).ceil(),
            FilterKind::Wave => (fs * WAVE_DISPLACEMENT_RATIO * intensity * 2.0 + 2.0).ceil(),
            FilterKind::Glitch => {
                ((fs * GLITCH_DISPLACEMENT_RATIO * intensity + fs * GLITCH_CHANNEL_SPLIT_RATIO * intensity) * 2.0 + 2.0)
                    .ceil()
            }
            FilterKind::Blur => blur_radius_px(font_size, intensity) as f64 * 3.0 + 2.0,
            FilterKind::Pixelate | FilterKind::Noise => 0.0,
        }
    }
}

fn resolve_filter_intensity(value: Option<f64>) -> f64 {
    let value = value.unwrap_or(mtt_core::font_presets::DEFAULT_FILTER_INTENSITY);
    if value.is_finite() { value.clamp(0.0, 1.0) } else { mtt_core::font_presets::DEFAULT_FILTER_INTENSITY }
}

fn filter_font_size(font_size: f64) -> f64 {
    if font_size.is_finite() { font_size.max(1.0) } else { 1.0 }
}

pub fn has_active_filter_effects(block: &TranslationBlock) -> bool {
    FilterKind::ALL.iter().any(|kind| kind.is_active(block))
}

/// `resolveFontFilterEffectMarginPx`.
pub fn filter_effect_margin_px(block: &TranslationBlock, font_size: f64) -> f64 {
    if block.style.filter_effect_enabled != Some(true) {
        return 0.0;
    }
    FilterKind::ALL
        .iter()
        .filter(|kind| kind.is_active(block))
        .map(|kind| kind.margin_px(font_size, kind.intensity(block)))
        .sum::<f64>()
        .ceil()
}

fn blur_radius_px(font_size: f64, intensity: f64) -> usize {
    if intensity > 0.0 {
        (filter_font_size(font_size) * BLUR_RADIUS_RATIO * intensity).round().max(1.0) as usize
    } else {
        0
    }
}

/// Applies every active filter in definition order to straight RGBA pixels of `pixmap`.
pub fn apply_font_filter_effects(pixmap: &mut Pixmap, block: &TranslationBlock, font_size: f64) {
    let active: Vec<FilterKind> = FilterKind::ALL.into_iter().filter(|kind| kind.is_active(block)).collect();
    if active.is_empty() {
        return;
    }
    let width = pixmap.width() as usize;
    let height = pixmap.height() as usize;
    let mut pixels = to_straight_rgba(pixmap);
    let mut changed = false;
    for kind in active {
        let intensity = kind.intensity(block);
        let next = match kind {
            FilterKind::Zigzag => zigzag(&pixels, width, height, intensity, font_size),
            FilterKind::Wave => wave(&pixels, width, intensity, font_size),
            FilterKind::Glitch => glitch(&pixels, width, intensity, font_size),
            FilterKind::Pixelate => pixelate(&pixels, width, height, intensity, font_size),
            FilterKind::Blur => blur_straight(&pixels, width, height, blur_radius_px(font_size, intensity)),
            FilterKind::Noise => noise(&pixels, width, height, intensity, font_size),
        };
        if let Some(next) = next {
            pixels = next;
            changed = true;
        }
    }
    if changed {
        from_straight_rgba(pixmap, &pixels);
    }
}

fn zigzag(source: &[u8], width: usize, height: usize, intensity: f64, font_size: f64) -> Option<Vec<u8>> {
    let displacement = filter_font_size(font_size) * ZIGZAG_DISPLACEMENT_RATIO * intensity;
    (displacement > 0.0).then(|| {
        horizontal_displacement(source, width, |y| {
            zigzag_tremble_offset(y + 0.5, height as f64, font_size) * displacement
        })
    })
}

fn wave(source: &[u8], width: usize, intensity: f64, font_size: f64) -> Option<Vec<u8>> {
    let displacement = filter_font_size(font_size) * WAVE_DISPLACEMENT_RATIO * intensity;
    if displacement <= 0.0 {
        return None;
    }
    let wave_length = (filter_font_size(font_size) * (1.15 - intensity * 0.45)).max(4.0);
    Some(horizontal_displacement(source, width, |y| {
        ((y + 0.5) / wave_length * std::f64::consts::TAU).sin() * displacement
    }))
}

fn glitch(source: &[u8], width: usize, intensity: f64, font_size: f64) -> Option<Vec<u8>> {
    let fs = filter_font_size(font_size);
    let displacement = fs * GLITCH_DISPLACEMENT_RATIO * intensity;
    let split = fs * GLITCH_CHANNEL_SPLIT_RATIO * intensity;
    if intensity <= 0.0 || (displacement <= 0.0 && split <= 0.0) {
        return None;
    }
    let mut output = vec![0u8; source.len()];
    for (y, (src, out)) in source.chunks_exact(width * 4).zip(output.chunks_exact_mut(width * 4)).enumerate() {
        let row_offset = glitch_row_offset(y as f64 + 0.5, font_size, intensity, displacement);
        for (x, pixel) in out.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let fx = x as f64 + row_offset;
            pixel[0] = sample_row(src, width, fx + split, 0).round() as u8;
            pixel[1] = sample_row(src, width, fx, 1).round() as u8;
            pixel[2] = sample_row(src, width, fx - split, 2).round() as u8;
            pixel[3] = sample_row(src, width, fx, 3).round() as u8;
        }
    }
    Some(output)
}

fn pixelate(source: &[u8], width: usize, height: usize, intensity: f64, font_size: f64) -> Option<Vec<u8>> {
    let block = if intensity <= 0.0 {
        1.0
    } else {
        (filter_font_size(font_size) * PIXELATE_BLOCK_RATIO * intensity).round().max(PIXELATE_MIN_BLOCK_SIZE_PX)
    } as usize;
    if block <= 1 {
        return None;
    }
    let mut output = vec![0u8; source.len()];
    for y in 0..height {
        let sy = ((y / block) * block + block / 2).min(height - 1);
        for x in 0..width {
            let sx = ((x / block) * block + block / 2).min(width - 1);
            let (src, dst) = ((sy * width + sx) * 4, (y * width + x) * 4);
            output[dst..dst + 4].copy_from_slice(&source[src..src + 4]);
        }
    }
    Some(output)
}

/// Box blur of straight RGBA in premultiplied f32 with clamped (repeated) edges.
fn blur_straight(source: &[u8], width: usize, height: usize, radius: usize) -> Option<Vec<u8>> {
    if radius == 0 || width == 0 || height == 0 {
        return None;
    }
    let mut premultiplied: Vec<f32> = Vec::with_capacity(source.len());
    for px in source.as_chunks::<4>().0.iter() {
        let alpha = f32::from(px[3]) / 255.0;
        premultiplied.extend_from_slice(&[
            f32::from(px[0]) * alpha,
            f32::from(px[1]) * alpha,
            f32::from(px[2]) * alpha,
            f32::from(px[3]),
        ]);
    }
    let window = (radius * 2 + 1) as f32;
    let reach = radius as isize;
    let clamp_index = |value: isize, max: usize| value.clamp(0, max as isize - 1) as usize;
    let stride = width * 4;
    let mut horizontal = vec![0f32; source.len()];
    for (src, out) in premultiplied.chunks_exact(stride).zip(horizontal.chunks_exact_mut(stride)) {
        let pixel = |x: isize| &src[clamp_index(x, width) * 4..][..4];
        let mut sum = [0f32; 4];
        for offset in -reach..=reach {
            for (total, value) in sum.iter_mut().zip(pixel(offset)) {
                *total += value;
            }
        }
        for x in 0..width {
            for (value, total) in out[x * 4..x * 4 + 4].iter_mut().zip(&sum) {
                *value = total / window;
            }
            if x + 1 < width {
                let (add, remove) = (pixel(x as isize + reach + 1), pixel(x as isize - reach));
                for ((total, add), remove) in sum.iter_mut().zip(add).zip(remove) {
                    *total += add - remove;
                }
            }
        }
    }
    // Vertical pass row by row with a running sum per column (same order of additions per
    // column as walking down it), written back over the premultiplied buffer.
    let row = |y: isize| &horizontal[clamp_index(y, height) * stride..][..stride];
    let mut sums = vec![0f32; stride];
    for offset in -reach..=reach {
        for (total, value) in sums.iter_mut().zip(row(offset)) {
            *total += value;
        }
    }
    let blurred = &mut premultiplied;
    for (y, out) in blurred.chunks_exact_mut(stride).enumerate() {
        for (value, total) in out.iter_mut().zip(&sums) {
            *value = total / window;
        }
        if y + 1 < height {
            let (add, remove) = (row(y as isize + reach + 1), row(y as isize - reach));
            for ((total, add), remove) in sums.iter_mut().zip(add).zip(remove) {
                *total += add - remove;
            }
        }
    }
    let mut output = vec![0u8; source.len()];
    for (out, px) in output.as_chunks_mut::<4>().0.iter_mut().zip(blurred.as_chunks::<4>().0.iter()) {
        let alpha = px[3].clamp(0.0, 255.0);
        out[3] = alpha.round() as u8;
        if alpha <= 0.0 {
            continue;
        }
        let ratio = alpha / 255.0;
        for c in 0..3 {
            out[c] = (px[c] / ratio).clamp(0.0, 255.0).round() as u8;
        }
    }
    Some(output)
}

fn noise(source: &[u8], width: usize, height: usize, intensity: f64, font_size: f64) -> Option<Vec<u8>> {
    if intensity <= 0.0 {
        return None;
    }
    let mut output = source.to_vec();
    let color_amount = NOISE_COLOR_RANGE * intensity;
    let alpha_amount = NOISE_ALPHA_RANGE * intensity;
    let font_noise_scale = filter_font_size(font_size) * 0.37;
    for y in 0..height {
        for x in 0..width {
            let offset = (y * width + x) * 4;
            let alpha = f64::from(source[offset + 3]);
            if alpha <= 0.0 {
                continue;
            }
            let value = signed_noise(x as f64 * 12.9898 + y as f64 * 78.233 + font_noise_scale);
            let color = value * color_amount;
            for c in 0..3 {
                output[offset + c] = clamp_byte(f64::from(source[offset + c]) + color);
            }
            output[offset + 3] = clamp_byte(alpha * (1.0 + value * alpha_amount));
        }
    }
    Some(output)
}

/// Shifts each row by `offset_x(y)` with linear interpolation.
fn horizontal_displacement(source: &[u8], width: usize, offset_x: impl Fn(f64) -> f64) -> Vec<u8> {
    let mut output = vec![0u8; source.len()];
    if width == 0 {
        return output;
    }
    let max_x = width as f64 - 1.0;
    for (y, (src, out)) in source.chunks_exact(width * 4).zip(output.chunks_exact_mut(width * 4)).enumerate() {
        let dx = offset_x(y as f64);
        for (x, pixel) in out.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            let cx = (x as f64 + dx).clamp(0.0, max_x);
            let x0 = cx.floor() as usize;
            let x1 = (x0 + 1).min(width - 1);
            let (left, right) = (&src[x0 * 4..x0 * 4 + 4], &src[x1 * 4..x1 * 4 + 4]);
            // Fully transparent black on both sides interpolates to the zeros already there.
            if left == [0; 4] && right == [0; 4] {
                continue;
            }
            let tx = cx - x0 as f64;
            for ((out, a), b) in pixel.iter_mut().zip(left).zip(right) {
                *out = (f64::from(*a) * (1.0 - tx) + f64::from(*b) * tx).round() as u8;
            }
        }
    }
    output
}

/// Linear sample of one channel within a row (`sampleImageDataBilinear` at a whole-pixel
/// `y`, where the vertical weight is zero).
fn sample_row(row: &[u8], width: usize, x: f64, channel: usize) -> f64 {
    let cx = x.clamp(0.0, width as f64 - 1.0);
    let x0 = cx.floor() as usize;
    let x1 = (x0 + 1).min(width - 1);
    let tx = cx - x0 as f64;
    f64::from(row[x0 * 4 + channel]) * (1.0 - tx) + f64::from(row[x1 * 4 + channel]) * tx
}

fn zigzag_tremble_offset(y: f64, height: f64, font_size: f64) -> f64 {
    let normalized = if height > 1.0 { y / height } else { 0.0 };
    let fade = smoothstep(0.0, ZIGZAG_VERTICAL_EDGE_FADE_RATIO, normalized)
        .min(1.0 - smoothstep(1.0 - ZIGZAG_VERTICAL_EDGE_FADE_RATIO, 1.0, normalized));
    if fade <= 0.0 {
        return 0.0;
    }
    let band_height = (filter_font_size(font_size) / ZIGZAG_TREMBLE_BANDS_PER_FONT_SIZE).max(1.0);
    let position = y / band_height;
    let index = position.floor();
    let mix = smoothstep(0.18, 0.82, position - index);
    let band = zigzag_band_offset(index) * (1.0 - mix) + zigzag_band_offset(index + 1.0) * mix;
    let micro = (position * std::f64::consts::PI * 3.0).sin() * ZIGZAG_TREMBLE_MICRO_SHAKE;
    (band + micro).clamp(-1.0, 1.0) * fade
}

fn zigzag_band_offset(index: f64) -> f64 {
    // JS `%` keeps the sign: -1 % 2 === -1, which is "odd".
    let alternating =
        if index % 2.0 == 0.0 { ZIGZAG_TREMBLE_ALTERNATE_WEIGHT } else { -ZIGZAG_TREMBLE_ALTERNATE_WEIGHT };
    (alternating + signed_noise(index) * ZIGZAG_TREMBLE_NOISE_WEIGHT).clamp(-1.0, 1.0)
}

fn glitch_row_offset(y: f64, font_size: f64, intensity: f64, displacement: f64) -> f64 {
    let band_height = (filter_font_size(font_size) * (0.24 - intensity * 0.08)).max(1.0);
    let index = (y / band_height).floor();
    let band_noise = signed_noise(index * 3.17);
    let gate = if signed_noise(index + 19.31).abs() < 0.22 { 0.35 } else { 1.0 };
    let snapped = (band_noise * 2.0).round() / 2.0;
    (snapped + band_noise * 0.3) * displacement * gate
}

fn smoothstep(edge0: f64, edge1: f64, value: f64) -> f64 {
    if edge0 == edge1 {
        return if value < edge0 { 0.0 } else { 1.0 };
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn signed_noise(value: f64) -> f64 {
    let raw = ((value + 1.0) * 127.1).sin() * 43758.5453123;
    (raw - raw.floor()) * 2.0 - 1.0
}

/// JS `Math.round(clamp(v, 0, 255))` (rounds .5 up).
fn clamp_byte(value: f64) -> u8 {
    (value.clamp(0.0, 255.0) + 0.5).floor() as u8
}

const HARD_TEXT_ALPHA_THRESHOLD: u8 = 128;
const HARD_TEXT_COLOR_DISTANCE_THRESHOLD_SQUARED: i32 = 64 * 64 * 3;

/// `hardenTextPixelData`: snaps antialiased text pixels to the block palette and binary alpha.
pub fn harden_text_pixels(pixmap: &mut Pixmap, block: &TranslationBlock) {
    let width = pixmap.width() as usize;
    let height = pixmap.height() as usize;
    let mut pixels = to_straight_rgba(pixmap);
    let palette = hard_text_palette(block);
    for px in pixels.as_chunks_mut::<4>().0.iter_mut() {
        if px[3] < HARD_TEXT_ALPHA_THRESHOLD {
            px.copy_from_slice(&[0, 0, 0, 0]);
            continue;
        }
        match nearest_palette_color(&palette, px[0], px[1], px[2]) {
            Some((r, g, b)) => px.copy_from_slice(&[r, g, b, 255]),
            None => px.copy_from_slice(&[0, 0, 0, 0]),
        }
    }
    fill_outline_interior_gaps(&mut pixels, width, height, block);
    from_straight_rgba(pixmap, &pixels);
}

fn hard_text_palette(block: &TranslationBlock) -> Vec<(u8, u8, u8)> {
    let colors = [
        Some(block.text_color()),
        Some(block.style.outline_color.as_deref().unwrap_or("#000000")),
        Some(block.style.secondary_outline_color.as_deref().unwrap_or("#ffffff")),
        Some(block.style.shadow_color.as_deref().unwrap_or("#000000")),
        Some("#ffffff"),
    ];
    let mut palette = Vec::new();
    for color in colors.into_iter().flatten().filter_map(parse_hex_color) {
        if !palette.contains(&color) {
            palette.push(color);
        }
    }
    palette
}

fn nearest_palette_color(palette: &[(u8, u8, u8)], r: u8, g: u8, b: u8) -> Option<(u8, u8, u8)> {
    let distance = |c: &(u8, u8, u8)| {
        let d = |a: u8, b: u8| (i32::from(a) - i32::from(b)).pow(2);
        d(c.0, r) + d(c.1, g) + d(c.2, b)
    };
    let nearest = palette.iter().min_by_key(|color| distance(color))?;
    (distance(nearest) <= HARD_TEXT_COLOR_DISTANCE_THRESHOLD_SQUARED).then_some(*nearest)
}

fn fill_outline_interior_gaps(pixels: &mut [u8], width: usize, height: usize, block: &TranslationBlock) {
    if block.style.outline_width_px.unwrap_or(0.0) <= 0.0 || width < 3 || height < 3 {
        return;
    }
    let (Some(outline), Some(text)) = (
        parse_hex_color(block.style.outline_color.as_deref().unwrap_or("#000000")),
        parse_hex_color(block.text_color()),
    ) else {
        return;
    };
    // Per pixel: bit 1 = opaque outline color, bit 2 = opaque text color, 4 = transparent.
    const OUTLINE: u8 = 1;
    const TEXT: u8 = 2;
    const EMPTY: u8 = 4;
    let classes: Vec<u8> = pixels
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| {
            let color = (p[0], p[1], p[2]);
            let opaque = p[3] == 255;
            (if p[3] == 0 { EMPTY } else { 0 })
                | (if opaque && color == outline { OUTLINE } else { 0 })
                | (if opaque && color == text { TEXT } else { 0 })
        })
        .collect();
    for y in 1..height - 1 {
        let (above, here, below) = ((y - 1) * width, y * width, (y + 1) * width);
        for x in 1..width - 1 {
            if classes[here + x] & EMPTY == 0 {
                continue;
            }
            let around = classes[above + x - 1]
                | classes[above + x]
                | classes[above + x + 1]
                | classes[here + x - 1]
                | classes[here + x + 1]
                | classes[below + x - 1]
                | classes[below + x]
                | classes[below + x + 1];
            if around & OUTLINE != 0 && around & TEXT != 0 {
                let o = (here + x) * 4;
                pixels[o..o + 4].copy_from_slice(&[outline.0, outline.1, outline.2, 255]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signed_noise_matches_js() {
        // Math.sin((0 + 1) * 127.1) * 43758.5453123 => fract * 2 - 1
        let raw = (127.1f64).sin() * 43758.5453123;
        assert!((signed_noise(0.0) - ((raw - raw.floor()) * 2.0 - 1.0)).abs() < 1e-12);
        assert!((-1.0..1.0).contains(&signed_noise(12.3)));
    }

    #[test]
    fn blur_spreads_alpha_and_preserves_total_roughly() {
        let mut pixmap = Pixmap::new(21, 21).unwrap();
        pixmap.pixels_mut()[10 * 21 + 10] = tiny_skia::ColorU8::from_rgba(0, 0, 0, 255).premultiply();
        gaussian_blur(&mut pixmap, 2.0);
        let center = pixmap.pixels()[10 * 21 + 10].alpha();
        let near = pixmap.pixels()[10 * 21 + 12].alpha();
        assert!(center > 0 && center < 255 && near > 0, "center {center} near {near}");
    }

    /// The column-walking blur this module used before (reference output).
    fn reference_blur(data: &mut [u8], width: usize, height: usize, sigma: f64) {
        let box_size = ((sigma * 3.0 * (2.0 * std::f64::consts::PI).sqrt() / 4.0) + 0.5).floor().max(1.0) as usize;
        let r = box_size / 2;
        let passes: [(usize, usize); 3] =
            if box_size % 2 == 1 { [(r, r); 3] } else { [(r, r.saturating_sub(1)), (r.saturating_sub(1), r), (r, r)] };
        let mut scratch = vec![0u8; data.len()];
        let at_h = |y: usize, x: usize, c: usize| (y * width + x) * 4 + c;
        for (left, right) in passes {
            let window = (left + right + 1) as u32;
            for y in 0..height {
                for c in 0..4 {
                    let mut sum: u32 = (0..=right.min(width - 1)).map(|x| u32::from(data[at_h(y, x, c)])).sum();
                    for x in 0..width {
                        scratch[at_h(y, x, c)] = ((sum + window / 2) / window) as u8;
                        if x + right + 1 < width {
                            sum += u32::from(data[at_h(y, x + right + 1, c)]);
                        }
                        if x >= left {
                            sum -= u32::from(data[at_h(y, x - left, c)]);
                        }
                    }
                }
            }
            data.copy_from_slice(&scratch);
        }
        for (top, bottom) in passes {
            let window = (top + bottom + 1) as u32;
            for x in 0..width {
                for c in 0..4 {
                    let mut sum: u32 = (0..=bottom.min(height - 1)).map(|y| u32::from(data[at_h(y, x, c)])).sum();
                    for y in 0..height {
                        scratch[at_h(y, x, c)] = ((sum + window / 2) / window) as u8;
                        if y + bottom + 1 < height {
                            sum += u32::from(data[at_h(y + bottom + 1, x, c)]);
                        }
                        if y >= top {
                            sum -= u32::from(data[at_h(y - top, x, c)]);
                        }
                    }
                }
            }
            data.copy_from_slice(&scratch);
        }
    }

    #[test]
    fn blur_matches_the_reference_exactly() {
        for (width, height, sigma) in [(37, 23, 2.0), (16, 40, 5.5), (64, 9, 0.6), (3, 3, 9.0)] {
            let mut pixmap = Pixmap::new(width, height).unwrap();
            for (index, pixel) in pixmap.pixels_mut().iter_mut().enumerate() {
                let alpha = (index * 37 % 256) as u8;
                *pixel = tiny_skia::ColorU8::from_rgba((index * 13 % 256) as u8, 40, 200, alpha).premultiply();
            }
            let mut expected = pixmap.data().to_vec();
            reference_blur(&mut expected, width as usize, height as usize, sigma);
            gaussian_blur(&mut pixmap, sigma);
            assert_eq!(pixmap.data(), expected.as_slice(), "{width}x{height} sigma {sigma}");
        }
    }

    #[test]
    fn harden_snaps_to_palette() {
        let block = TranslationBlock::default();
        let mut pixmap = Pixmap::new(2, 1).unwrap();
        from_straight_rgba(&mut pixmap, &[20, 20, 20, 200, 20, 20, 20, 100]);
        harden_text_pixels(&mut pixmap, &block);
        let out = to_straight_rgba(&pixmap);
        assert_eq!(&out[0..4], &[0x11, 0x11, 0x11, 255], "nearest is the default text color");
        assert_eq!(out[7], 0, "low alpha is cleared");
    }

    #[test]
    fn pixelate_block_size() {
        let block = TranslationBlock {
            style: mtt_core::model::TextStyle {
                filter_effect_enabled: Some(true),
                filter_pixelate_enabled: Some(true),
                filter_pixelate_intensity: Some(0.5),
                ..Default::default()
            },
            ..TranslationBlock::default()
        };
        assert!(FilterKind::Pixelate.is_active(&block));
        assert_eq!(filter_effect_margin_px(&block, 40.0), 0.0);
    }

    // ---- the implementations these replaced, as references ----

    fn reference_glitch(source: &[u8], width: usize, height: usize, intensity: f64, font_size: f64) -> Option<Vec<u8>> {
        let fs = filter_font_size(font_size);
        let displacement = fs * GLITCH_DISPLACEMENT_RATIO * intensity;
        let split = fs * GLITCH_CHANNEL_SPLIT_RATIO * intensity;
        if intensity <= 0.0 || (displacement <= 0.0 && split <= 0.0) {
            return None;
        }
        let mut output = vec![0u8; source.len()];
        for y in 0..height {
            let row_offset = glitch_row_offset(y as f64 + 0.5, font_size, intensity, displacement);
            for x in 0..width {
                let out = (y * width + x) * 4;
                let fx = x as f64 + row_offset;
                output[out] = reference_sample_bilinear(source, width, height, fx + split, y as f64, 0).round() as u8;
                output[out + 1] = reference_sample_bilinear(source, width, height, fx, y as f64, 1).round() as u8;
                output[out + 2] =
                    reference_sample_bilinear(source, width, height, fx - split, y as f64, 2).round() as u8;
                output[out + 3] = reference_sample_bilinear(source, width, height, fx, y as f64, 3).round() as u8;
            }
        }
        Some(output)
    }

    fn reference_blur_straight(source: &[u8], width: usize, height: usize, radius: usize) -> Option<Vec<u8>> {
        if radius == 0 {
            return None;
        }
        let mut premultiplied: Vec<f32> = Vec::with_capacity(source.len());
        for px in source.as_chunks::<4>().0.iter() {
            let alpha = f32::from(px[3]) / 255.0;
            premultiplied.extend_from_slice(&[
                f32::from(px[0]) * alpha,
                f32::from(px[1]) * alpha,
                f32::from(px[2]) * alpha,
                f32::from(px[3]),
            ]);
        }
        let mut horizontal = vec![0f32; source.len()];
        let mut blurred = vec![0f32; source.len()];
        let window = (radius * 2 + 1) as f32;
        let clamp_index = |value: isize, max: usize| value.clamp(0, max as isize - 1) as usize;
        for y in 0..height {
            for c in 0..4 {
                let at = |x: usize| (y * width + x) * 4 + c;
                let mut sum = 0f32;
                for offset in -(radius as isize)..=(radius as isize) {
                    sum += premultiplied[at(clamp_index(offset, width))];
                }
                for x in 0..width {
                    horizontal[at(x)] = sum / window;
                    if x + 1 < width {
                        sum += premultiplied[at(clamp_index(x as isize + radius as isize + 1, width))]
                            - premultiplied[at(clamp_index(x as isize - radius as isize, width))];
                    }
                }
            }
        }
        for x in 0..width {
            for c in 0..4 {
                let at = |y: usize| (y * width + x) * 4 + c;
                let mut sum = 0f32;
                for offset in -(radius as isize)..=(radius as isize) {
                    sum += horizontal[at(clamp_index(offset, height))];
                }
                for y in 0..height {
                    blurred[at(y)] = sum / window;
                    if y + 1 < height {
                        sum += horizontal[at(clamp_index(y as isize + radius as isize + 1, height))]
                            - horizontal[at(clamp_index(y as isize - radius as isize, height))];
                    }
                }
            }
        }
        let mut output = vec![0u8; source.len()];
        for (out, px) in output.as_chunks_mut::<4>().0.iter_mut().zip(blurred.as_chunks::<4>().0.iter()) {
            let alpha = px[3].clamp(0.0, 255.0);
            out[3] = alpha.round() as u8;
            if alpha <= 0.0 {
                continue;
            }
            let ratio = alpha / 255.0;
            for c in 0..3 {
                out[c] = (px[c] / ratio).clamp(0.0, 255.0).round() as u8;
            }
        }
        Some(output)
    }

    fn reference_horizontal_displacement(
        source: &[u8],
        width: usize,
        height: usize,
        offset_x: impl Fn(f64) -> f64,
    ) -> Vec<u8> {
        let mut output = vec![0u8; source.len()];
        for y in 0..height {
            let dx = offset_x(y as f64);
            for x in 0..width {
                let out = (y * width + x) * 4;
                for c in 0..4 {
                    output[out + c] =
                        reference_sample_bilinear(source, width, height, x as f64 + dx, y as f64, c).round() as u8;
                }
            }
        }
        output
    }

    fn reference_sample_bilinear(source: &[u8], width: usize, height: usize, x: f64, y: f64, channel: usize) -> f64 {
        let cx = x.clamp(0.0, width as f64 - 1.0);
        let cy = y.clamp(0.0, height as f64 - 1.0);
        let (x0, y0) = (cx.floor() as usize, cy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(width - 1), (y0 + 1).min(height - 1));
        let (tx, ty) = (cx - x0 as f64, cy - y0 as f64);
        let px = |x: usize, y: usize| f64::from(source[(y * width + x) * 4 + channel]);
        let top = px(x0, y0) * (1.0 - tx) + px(x1, y0) * tx;
        let bottom = px(x0, y1) * (1.0 - tx) + px(x1, y1) * tx;
        top * (1.0 - ty) + bottom * ty
    }

    fn reference_fill_gaps(pixels: &mut [u8], width: usize, height: usize, block: &TranslationBlock) {
        if block.style.outline_width_px.unwrap_or(0.0) <= 0.0 || width < 3 || height < 3 {
            return;
        }
        let (Some(outline), Some(text)) = (
            parse_hex_color(block.style.outline_color.as_deref().unwrap_or("#000000")),
            parse_hex_color(block.text_color()),
        ) else {
            return;
        };
        let source = pixels.to_vec();
        let has_neighbor = |x: usize, y: usize, color: (u8, u8, u8)| {
            for ny in y - 1..=y + 1 {
                for nx in x - 1..=x + 1 {
                    if nx == x && ny == y {
                        continue;
                    }
                    let o = (ny * width + nx) * 4;
                    if source[o + 3] == 255 && (source[o], source[o + 1], source[o + 2]) == color {
                        return true;
                    }
                }
            }
            false
        };
        for y in 1..height - 1 {
            for x in 1..width - 1 {
                let o = (y * width + x) * 4;
                if source[o + 3] != 0 || !has_neighbor(x, y, outline) || !has_neighbor(x, y, text) {
                    continue;
                }
                pixels[o..o + 4].copy_from_slice(&[outline.0, outline.1, outline.2, 255]);
            }
        }
    }

    fn sample_pixels(width: usize, height: usize) -> Vec<u8> {
        (0..width * height)
            .flat_map(|i| {
                let alpha = if i % 7 < 3 { 0 } else { (i * 53 % 256) as u8 };
                let value = if alpha == 0 && i % 2 == 0 { 0 } else { (i * 31 % 256) as u8 };
                [value, (i * 17 % 256) as u8 & if alpha == 0 { 0 } else { 255 }, 90, alpha]
            })
            .collect()
    }

    #[test]
    fn filters_match_their_references() {
        let (width, height) = (41, 29);
        let source = sample_pixels(width, height);
        let offset = |y: f64| (y * 0.37).sin() * 5.3;
        assert_eq!(
            horizontal_displacement(&source, width, offset),
            reference_horizontal_displacement(&source, width, height, offset)
        );
        for intensity in [0.3, 1.0] {
            assert_eq!(
                glitch(&source, width, intensity, 40.0),
                reference_glitch(&source, width, height, intensity, 40.0)
            );
        }
        for radius in [1, 3, 30] {
            assert_eq!(
                blur_straight(&source, width, height, radius),
                reference_blur_straight(&source, width, height, radius)
            );
        }
        let mut block = TranslationBlock::default();
        block.style.outline_width_px = Some(2.0);
        block.style.outline_color = Some("#000000".into());
        let mut hard = vec![0u8; width * height * 4];
        for (i, px) in hard.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            match i % 5 {
                0 => px.copy_from_slice(&[0, 0, 0, 255]),
                1 => px.copy_from_slice(&[0x11, 0x11, 0x11, 255]),
                _ => {}
            }
        }
        let text = parse_hex_color(block.text_color()).unwrap();
        hard[4..8].copy_from_slice(&[text.0, text.1, text.2, 255]);
        let mut expected = hard.clone();
        reference_fill_gaps(&mut expected, width, height, &block);
        fill_outline_interior_gaps(&mut hard, width, height, &block);
        assert_eq!(hard, expected);
    }
}
