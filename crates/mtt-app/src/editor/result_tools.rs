//! Inpaint result layer touch-up tools (`InpaintResultCanvas.tsx`, `inpaintResultCanvas.ts`):
//! brush, smart brush, eraser, blur, sharpen, smudge and color picking.

use std::collections::HashMap;
use std::sync::Arc;

use image::{GrayImage, RgbaImage};

pub const RESULT_BRUSH_MIN: f32 = 2.0;
pub const RESULT_BRUSH_MAX: f32 = 128.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultTool {
    Brush,
    /// Spot healing: the stroke marks an area that is inpainted from its surroundings on
    /// release (`InpaintScope::Heal`); it paints nothing itself.
    SmartBrush,
    Eraser,
    Blur,
    Sharpen,
    Smudge,
    ColorPicker,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BrushSettings {
    pub size: f32,
    /// 0 = soft edge, 1 = hard edge.
    pub hardness: f32,
    /// Blur/sharpen/smudge amount.
    pub strength: f32,
    /// Most a single stroke covers, however often it crosses itself (Photoshop's opacity).
    pub opacity: f32,
    /// How fast a stroke builds up to `opacity` where dabs overlap (Photoshop's flow).
    pub flow: f32,
    /// 0..1: how far the brush trails the pointer to steady the line.
    pub smoothing: f32,
    /// Leave the inpaint mask alone. By default brush, smart brush and the blur / sharpen /
    /// smudge tools also cover what they change with the mask so it shows in the output;
    /// with this on they edit the result layer only, showing where the mask already is.
    pub keep_mask: bool,
    /// Soft edges and in-between tones. Off for pages drawn without anti-aliasing: paint
    /// snaps to on/off and blur / sharpen / smudge keep to the colors already there.
    pub anti_alias: bool,
    pub color: [u8; 3],
}

impl Default for BrushSettings {
    fn default() -> Self {
        Self {
            size: 28.0,
            hardness: 0.85,
            strength: 0.55,
            opacity: 1.0,
            flow: 1.0,
            smoothing: 0.1,
            keep_mask: false,
            anti_alias: true,
            color: [255, 255, 255],
        }
    }
}

pub type DirtyRect = (u32, u32, u32, u32);

pub fn union(a: Option<DirtyRect>, b: Option<DirtyRect>) -> Option<DirtyRect> {
    match (a, b) {
        (Some(a), Some(b)) => {
            let (left, top) = (a.0.min(b.0), a.1.min(b.1));
            let (right, bottom) = ((a.0 + a.2).max(b.0 + b.2), (a.1 + a.3).max(b.1 + b.3));
            Some((left, top, right - left, bottom - top))
        }
        (a, b) => a.or(b),
    }
}

/// Dab coverage at `(x, y)`: 1 inside the hard core, a smooth bell out to the radius, and
/// the rim anti-aliased over one pixel so hard brushes don't stair-step.
pub fn brush_alpha(x: f32, y: f32, center: (f32, f32), radius: f32, hardness: f32) -> f32 {
    let distance = (x - center.0).hypot(y - center.1);
    let rim = (radius - distance + 0.5).clamp(0.0, 1.0);
    if rim <= 0.0 {
        return 0.0;
    }
    let hard_radius = radius * hardness.clamp(0.0, 1.0);
    if distance <= hard_radius {
        return rim;
    }
    let progress = ((distance - hard_radius) / (radius - hard_radius).max(1e-3)).clamp(0.0, 1.0);
    // Smoothstep falloff: no crease where the core ends, and it fades out flat.
    (1.0 - progress * progress * (3.0 - 2.0 * progress)) * rim
}

fn bounds(center: (f32, f32), radius: f32, width: u32, height: u32) -> Option<DirtyRect> {
    let x = (center.0 - radius).floor().max(0.0) as u32;
    let y = (center.1 - radius).floor().max(0.0) as u32;
    let right = ((center.0 + radius).ceil().max(0.0) as u32).min(width);
    let bottom = ((center.1 + radius).ceil().max(0.0) as u32).min(height);
    (right > x && bottom > y).then_some((x, y, right - x, bottom - y))
}

fn clamp_byte(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

fn lerp(a: (f32, f32), b: (f32, f32), t: f32) -> (f32, f32) {
    (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
}

fn distance(a: (f32, f32), b: (f32, f32)) -> f32 {
    (b.0 - a.0).hypot(b.1 - a.1)
}

const COVERAGE_TILE: u32 = 64;

/// Least pixels (and share of a window) a color needs to count as one the page is drawn
/// with when an aliased edit snaps to the page's own colors.
const PALETTE_MIN_PIXELS: u32 = 3;
const PALETTE_MIN_SHARE: f32 = 0.001;
const PALETTE_MAX: usize = 16;

/// The layer as it was before the stroke and how much the stroke covers it, per tile.
struct CoverageTile {
    original: Vec<[u8; 4]>,
    coverage: Vec<f32>,
    /// Blur / sharpen: what the output showed before the stroke, the filtered image of it
    /// (premultiplied), and the colors an aliased edit may use.
    shown: Vec<[u8; 4]>,
    filtered: Option<Vec<[f32; 4]>>,
    palette: Vec<[f32; 4]>,
}

/// One drag of a result tool.
///
/// Paint goes through a stroke buffer like Photoshop's: each dab raises a per-pixel
/// coverage toward its own profile (by `flow`) and the layer is recomposited from its
/// pre-stroke pixels at `coverage * opacity`. Overlapping dabs therefore never harden a
/// soft edge or darken where the stroke crosses itself. Dabs sit at an even spacing along
/// the whole stroke, which runs through a lagging "string" (smoothing) and midpoint
/// curves between pointer samples so fast strokes stay round instead of polygonal.
pub struct Stroke {
    /// Page px the brush trails the pointer by before it moves.
    string: f32,
    /// Pen position after smoothing.
    pen: Option<(f32, f32)>,
    /// The last two pen positions the curve went through.
    previous: Option<(f32, f32)>,
    /// Where the drawn path ends (the midpoint the next curve starts from).
    path_end: Option<(f32, f32)>,
    /// Path length since the last dab.
    travelled: f32,
    tiles: HashMap<(u32, u32), CoverageTile>,
    /// Smudge: the paint the brush carries (premultiplied), centered on the last dab.
    smudge: Option<Vec<Option<[f32; 4]>>>,
    /// Snap coverage to on/off (pages drawn without anti-aliasing).
    aliased: bool,
    /// The page image, so blur / sharpen / smudge work on what the output shows.
    source: Option<Arc<RgbaImage>>,
}

impl Stroke {
    /// `string` is the smoothing reach in page pixels.
    pub fn new(string: f32, aliased: bool, source: Option<Arc<RgbaImage>>) -> Self {
        Self {
            source,
            string: string.max(0.0),
            pen: None,
            previous: None,
            path_end: None,
            travelled: 0.0,
            tiles: HashMap::new(),
            smudge: None,
            aliased,
        }
    }

    /// Every pixel `add_point(point)` or `finish` may change, so callers can back it up.
    pub fn reach(
        &self,
        point: Option<(f32, f32)>,
        settings: &BrushSettings,
        width: u32,
        height: u32,
    ) -> Option<DirtyRect> {
        let points = [point, self.pen, self.previous, self.path_end];
        let mut points = points.iter().flatten();
        let first = *points.next()?;
        let (mut min, mut max) = (first, first);
        for &(x, y) in points {
            min = (min.0.min(x), min.1.min(y));
            max = (max.0.max(x), max.1.max(y));
        }
        let radius = settings.size.clamp(RESULT_BRUSH_MIN, RESULT_BRUSH_MAX) / 2.0 + 2.0;
        let x = (min.0 - radius).floor().max(0.0) as u32;
        let y = (min.1 - radius).floor().max(0.0) as u32;
        let right = ((max.0 + radius).ceil().max(0.0) as u32).min(width);
        let bottom = ((max.1 + radius).ceil().max(0.0) as u32).min(height);
        (right > x && bottom > y).then_some((x, y, right - x, bottom - y))
    }

    /// Feeds one pointer sample (page px).
    pub fn add_point(
        &mut self,
        tool: ResultTool,
        result: &mut RgbaImage,
        mask: &mut GrayImage,
        point: (f32, f32),
        settings: &BrushSettings,
    ) -> Option<DirtyRect> {
        let Some(pen) = self.pen else {
            // The first dab lands right where the press happened.
            self.pen = Some(point);
            self.previous = Some(point);
            self.path_end = Some(point);
            return self.dab(tool, result, mask, point, settings);
        };
        // Pulled string: the pen only follows once the pointer is `string` away.
        let gap = distance(pen, point);
        if gap <= self.string {
            return None;
        }
        let pen = lerp(pen, point, (gap - self.string) / gap);
        self.pen = Some(pen);
        self.curve_to(tool, result, mask, pen, settings)
    }

    /// Ends the stroke: the pen catches up with the pointer and the path runs out to it.
    pub fn finish(
        &mut self,
        tool: ResultTool,
        result: &mut RgbaImage,
        mask: &mut GrayImage,
        point: Option<(f32, f32)>,
        settings: &BrushSettings,
    ) -> Option<DirtyRect> {
        let mut dirty = None;
        if let Some(point) = point
            && self.pen.is_some_and(|pen| distance(pen, point) > 0.25)
        {
            self.pen = Some(point);
            dirty = self.curve_to(tool, result, mask, point, settings);
        }
        if let (Some(from), Some(to)) = (self.path_end, self.previous) {
            dirty = union(dirty, self.walk(tool, result, mask, from, to, settings));
            self.path_end = Some(to);
        }
        dirty
    }

    /// Quadratic curve from the last midpoint, bent through the previous pen position, to
    /// the midpoint toward `pen`.
    fn curve_to(
        &mut self,
        tool: ResultTool,
        result: &mut RgbaImage,
        mask: &mut GrayImage,
        pen: (f32, f32),
        settings: &BrushSettings,
    ) -> Option<DirtyRect> {
        let (Some(start), Some(control)) = (self.path_end, self.previous) else { return None };
        let end = lerp(control, pen, 0.5);
        self.previous = Some(pen);
        self.path_end = Some(end);
        let length = distance(start, control) + distance(control, end);
        let pieces = (length / 2.0).ceil().clamp(1.0, 256.0) as u32;
        let mut dirty = None;
        let mut from = start;
        for piece in 1..=pieces {
            let t = piece as f32 / pieces as f32;
            let to = lerp(lerp(start, control, t), lerp(control, end, t), t);
            dirty = union(dirty, self.walk(tool, result, mask, from, to, settings));
            from = to;
        }
        dirty
    }

    /// Lays dabs every `spacing` along a straight piece, carrying the leftover distance on.
    fn walk(
        &mut self,
        tool: ResultTool,
        result: &mut RgbaImage,
        mask: &mut GrayImage,
        from: (f32, f32),
        to: (f32, f32),
        settings: &BrushSettings,
    ) -> Option<DirtyRect> {
        let size = settings.size.clamp(RESULT_BRUSH_MIN, RESULT_BRUSH_MAX);
        let spacing = match tool {
            // Dense dabs keep the stroke-buffer edge free of scallops.
            ResultTool::Brush | ResultTool::Eraser | ResultTool::Blur | ResultTool::Sharpen => (size * 0.08).max(0.5),
            // Smudge carries paint from dab to dab; far-apart dabs leave stripes.
            _ => (size * 0.05).max(1.0),
        };
        let length = distance(from, to);
        let mut dirty = None;
        let mut next = spacing - self.travelled;
        while next <= length {
            dirty = union(dirty, self.dab(tool, result, mask, lerp(from, to, next / length.max(1e-6)), settings));
            next += spacing;
        }
        self.travelled = length - (next - spacing);
        dirty
    }

    fn dab(
        &mut self,
        tool: ResultTool,
        result: &mut RgbaImage,
        mask: &mut GrayImage,
        center: (f32, f32),
        settings: &BrushSettings,
    ) -> Option<DirtyRect> {
        match tool {
            ResultTool::Brush => self.paint_dab(result, mask, center, settings, false),
            ResultTool::Eraser => self.paint_dab(result, mask, center, settings, true),
            ResultTool::Blur | ResultTool::Sharpen => {
                self.filter_dab(result, mask, center, settings, tool == ResultTool::Blur)
            }
            ResultTool::Smudge => self.smudge_dab(result, mask, center, settings),
            ResultTool::SmartBrush | ResultTool::ColorPicker => None,
        }
    }

    fn paint_dab(
        &mut self,
        result: &mut RgbaImage,
        mask: &mut GrayImage,
        center: (f32, f32),
        settings: &BrushSettings,
        erase: bool,
    ) -> Option<DirtyRect> {
        let size = settings.size.clamp(RESULT_BRUSH_MIN, RESULT_BRUSH_MAX);
        let radius = size / 2.0;
        let rect = bounds(center, radius + 0.5, result.width(), result.height())?;
        let flow = settings.flow.clamp(0.01, 1.0);
        let opacity = settings.opacity.clamp(0.0, 1.0);
        let color = settings.color.map(f32::from);
        let aliased = self.aliased;
        let mut changed = false;
        let (right, bottom) = (rect.0 + rect.2, rect.1 + rect.3);
        for tile_y in rect.1 / COVERAGE_TILE..=(bottom - 1) / COVERAGE_TILE {
            for tile_x in rect.0 / COVERAGE_TILE..=(right - 1) / COVERAGE_TILE {
                let (left, top) = (tile_x * COVERAGE_TILE, tile_y * COVERAGE_TILE);
                let tile = self.tile((tile_x, tile_y), result, mask, false);
                for y in rect.1.max(top)..bottom.min(top + COVERAGE_TILE) {
                    for x in rect.0.max(left)..right.min(left + COVERAGE_TILE) {
                        let alpha = brush_alpha(x as f32 + 0.5, y as f32 + 0.5, center, radius, settings.hardness);
                        let index = ((y - top) * COVERAGE_TILE + x - left) as usize;
                        let coverage = &mut tile.coverage[index];
                        if alpha <= *coverage {
                            continue;
                        }
                        *coverage += (alpha - *coverage) * flow;
                        let mut amount = *coverage * opacity;
                        if aliased {
                            amount = if amount >= 0.5 { 1.0 } else { 0.0 };
                        }
                        let original = tile.original[index];
                        let pixel = result.get_pixel_mut(x, y);
                        if erase {
                            pixel[3] = clamp_byte(f32::from(original[3]) * (1.0 - amount));
                        } else {
                            if amount <= 0.0 {
                                continue;
                            }
                            // Source-over in straight alpha, onto the layer as it was before the stroke.
                            let dst_alpha = f32::from(original[3]) / 255.0;
                            let out_alpha = amount + dst_alpha * (1.0 - amount);
                            for channel in 0..3 {
                                let dst = f32::from(original[channel]);
                                pixel[channel] = clamp_byte(
                                    (color[channel] * amount + dst * dst_alpha * (1.0 - amount)) / out_alpha.max(1e-6),
                                );
                            }
                            pixel[3] = clamp_byte(out_alpha * 255.0);
                            // Painted pixels must be covered by the mask to show in the output.
                            if !settings.keep_mask {
                                mask.put_pixel(x, y, image::Luma([255]));
                            }
                        }
                        changed = true;
                    }
                }
            }
        }
        changed.then_some(rect)
    }
}

/// Covers an edited result pixel with the mask (unless `keep_mask`) so the edit shows.
/// Pixels without result content are left out: the mask would only grow over nothing.
fn reveal(mask: &mut GrayImage, x: u32, y: u32, pixel: &image::Rgba<u8>, settings: &BrushSettings) {
    if !settings.keep_mask && pixel[3] > 0 {
        mask.put_pixel(x, y, image::Luma([255]));
    }
}

/// What the output shows at a pixel (the result over the page where the mask covers it),
/// or the result alone without a page.
fn shown_pixel(source: Option<&RgbaImage>, result: &RgbaImage, mask: &GrayImage, x: u32, y: u32) -> [u8; 4] {
    let over = result.get_pixel(x, y).0;
    let Some(source) = source else { return over };
    const FULL: u32 = 255 * 255;
    let base = source.get_pixel(x, y).0;
    let alpha = u32::from(over[3]) * u32::from(mask.get_pixel(x, y)[0]);
    let mut out = [0, 0, 0, 255];
    for c in 0..3 {
        out[c] = ((u32::from(base[c]) * (FULL - alpha) + u32::from(over[c]) * alpha + FULL / 2) / FULL) as u8;
    }
    out
}

/// RGB scaled by alpha, all in 0..255.
fn premultiply(pixel: [u8; 4]) -> [f32; 4] {
    let alpha = f32::from(pixel[3]) / 255.0;
    [f32::from(pixel[0]) * alpha, f32::from(pixel[1]) * alpha, f32::from(pixel[2]) * alpha, f32::from(pixel[3])]
}

fn unpremultiply(pixel: [f32; 4]) -> [u8; 4] {
    let alpha = pixel[3].clamp(0.0, 255.0);
    if alpha < 0.5 {
        return [0; 4];
    }
    let scale = 255.0 / alpha;
    [clamp_byte(pixel[0] * scale), clamp_byte(pixel[1] * scale), clamp_byte(pixel[2] * scale), clamp_byte(alpha)]
}

fn mix(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    std::array::from_fn(|c| a[c] + (b[c] - a[c]) * t)
}

/// The colors `pixels` are drawn with, most common first, leaving out rare ones
/// (anti-aliasing, noise): what an aliased edit snaps to so it adds no in-between tones.
fn palette(pixels: impl Iterator<Item = [u8; 4]>) -> Vec<[f32; 4]> {
    let mut counts: HashMap<[u8; 4], u32> = HashMap::new();
    let mut total = 0u32;
    for pixel in pixels {
        *counts.entry(if pixel[3] == 0 { [0; 4] } else { pixel }).or_default() += 1;
        total += 1;
    }
    let mut colors: Vec<([u8; 4], u32)> = counts.into_iter().collect();
    colors.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let least = PALETTE_MIN_PIXELS.max((total as f32 * PALETTE_MIN_SHARE).ceil() as u32);
    let common = colors.iter().filter(|(_, count)| *count >= least).count().clamp(1, PALETTE_MAX);
    colors.iter().take(common).map(|(color, _)| premultiply(*color)).collect()
}

/// The palette color nearest `value`.
fn snap(value: [f32; 4], palette: &[[f32; 4]]) -> [f32; 4] {
    let distance = |color: &[f32; 4]| (0..4).map(|c| (color[c] - value[c]).powi(2)).sum::<f32>();
    palette.iter().min_by(|a, b| distance(a).total_cmp(&distance(b))).copied().unwrap_or(value)
}

fn gaussian(sigma: f32) -> Vec<f32> {
    let reach = (sigma * 3.0).ceil() as i32;
    let weights: Vec<f32> = (-reach..=reach).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect();
    let sum: f32 = weights.iter().sum();
    weights.into_iter().map(|w| w / sum).collect()
}

impl Stroke {
    fn source_for(&self, result: &RgbaImage) -> Option<Arc<RgbaImage>> {
        self.source.clone().filter(|source| source.dimensions() == result.dimensions())
    }

    /// The tile at `key`, capturing the layer (and with `shown`, the output) before the
    /// stroke changes it.
    fn tile(&mut self, key: (u32, u32), result: &RgbaImage, mask: &GrayImage, shown: bool) -> &mut CoverageTile {
        let source = if shown { self.source_for(result) } else { None };
        self.tiles.entry(key).or_insert_with(|| {
            let (width, height) = result.dimensions();
            let (left, top) = (key.0 * COVERAGE_TILE, key.1 * COVERAGE_TILE);
            let cells = (COVERAGE_TILE * COVERAGE_TILE) as usize;
            let (mut original, mut seen) = (Vec::with_capacity(cells), Vec::new());
            for y in top..top + COVERAGE_TILE {
                for x in left..left + COVERAGE_TILE {
                    let inside = x < width && y < height;
                    original.push(if inside { result.get_pixel(x, y).0 } else { [0; 4] });
                    if shown {
                        seen.push(if inside { shown_pixel(source.as_deref(), result, mask, x, y) } else { [0; 4] });
                    }
                }
            }
            CoverageTile { original, coverage: vec![0.0; cells], shown: seen, filtered: None, palette: Vec::new() }
        })
    }

    /// The output before the stroke at a pixel (clamped to the page).
    fn shown_before(
        &self,
        source: Option<&RgbaImage>,
        result: &RgbaImage,
        mask: &GrayImage,
        x: i64,
        y: i64,
    ) -> [u8; 4] {
        let x = x.clamp(0, i64::from(result.width()) - 1) as u32;
        let y = y.clamp(0, i64::from(result.height()) - 1) as u32;
        match self.tiles.get(&(x / COVERAGE_TILE, y / COVERAGE_TILE)) {
            Some(tile) if !tile.shown.is_empty() => {
                tile.shown[((y % COVERAGE_TILE) * COVERAGE_TILE + x % COVERAGE_TILE) as usize]
            }
            // Tiles the stroke hasn't reached still show what they did before it.
            _ => shown_pixel(source, result, mask, x, y),
        }
    }

    /// Gaussian blur (or an unsharp mask of it) of the pre-stroke output over one tile,
    /// and with `aliased` the colors around it.
    fn filter_tile(
        &self,
        key: (u32, u32),
        result: &RgbaImage,
        mask: &GrayImage,
        settings: &BrushSettings,
        blur: bool,
    ) -> (Vec<[f32; 4]>, Vec<[f32; 4]>) {
        let strength = settings.strength.clamp(0.0, 1.0);
        let sigma = if blur { 0.5 + 3.5 * strength } else { 1.0 };
        let kernel = gaussian(sigma);
        let reach = (kernel.len() / 2) as i64;
        let source = self.source_for(result);
        let (left, top) = (i64::from(key.0 * COVERAGE_TILE), i64::from(key.1 * COVERAGE_TILE));
        let size = i64::from(COVERAGE_TILE);
        let span = (size + 2 * reach) as usize;
        let mut window = Vec::with_capacity(span * span);
        for y in top - reach..top + size + reach {
            for x in left - reach..left + size + reach {
                window.push(self.shown_before(source.as_deref(), result, mask, x, y));
            }
        }
        let palette = if self.aliased { palette(window.iter().copied()) } else { Vec::new() };
        let window: Vec<[f32; 4]> = window.into_iter().map(premultiply).collect();
        // Separable: rows over the whole window height, then columns.
        let tile = size as usize;
        let mut rows = vec![[0f32; 4]; span * tile];
        for y in 0..span {
            for x in 0..tile {
                let mut sum = [0f32; 4];
                for (k, weight) in kernel.iter().enumerate() {
                    let pixel = window[y * span + x + k];
                    (0..4).for_each(|c| sum[c] += pixel[c] * weight);
                }
                rows[y * tile + x] = sum;
            }
        }
        let mut out = Vec::with_capacity(tile * tile);
        for y in 0..tile {
            for x in 0..tile {
                let mut sum = [0f32; 4];
                for (k, weight) in kernel.iter().enumerate() {
                    let pixel = rows[(y + k) * tile + x];
                    (0..4).for_each(|c| sum[c] += pixel[c] * weight);
                }
                if !blur {
                    // Unsharp mask: push away from the blur, alpha unchanged.
                    let amount = 0.4 + 2.6 * strength;
                    let original = window[(y + reach as usize) * span + x + reach as usize];
                    for c in 0..3 {
                        sum[c] = (original[c] + (original[c] - sum[c]) * amount).clamp(0.0, original[3]);
                    }
                    sum[3] = original[3];
                }
                out.push(sum);
            }
        }
        (out, palette)
    }

    /// Blur / sharpen through the stroke buffer: the pre-stroke output, filtered once, shows
    /// through as far as the brush has covered it, so going over a spot again within one
    /// stroke doesn't keep eating into it.
    fn filter_dab(
        &mut self,
        result: &mut RgbaImage,
        mask: &mut GrayImage,
        center: (f32, f32),
        settings: &BrushSettings,
        blur: bool,
    ) -> Option<DirtyRect> {
        let radius = settings.size.clamp(RESULT_BRUSH_MIN, RESULT_BRUSH_MAX) / 2.0;
        let rect = bounds(center, radius + 0.5, result.width(), result.height())?;
        let (right, bottom) = (rect.0 + rect.2, rect.1 + rect.3);
        let aliased = self.aliased;
        let mut changed = false;
        for tile_y in rect.1 / COVERAGE_TILE..=(bottom - 1) / COVERAGE_TILE {
            for tile_x in rect.0 / COVERAGE_TILE..=(right - 1) / COVERAGE_TILE {
                let key = (tile_x, tile_y);
                self.tile(key, result, mask, true);
                if self.tiles[&key].filtered.is_none() {
                    let (filtered, palette) = self.filter_tile(key, result, mask, settings, blur);
                    let tile = self.tiles.get_mut(&key).expect("captured above");
                    tile.filtered = Some(filtered);
                    tile.palette = palette;
                }
                let tile = self.tiles.get_mut(&key).expect("captured above");
                let (left, top) = (tile_x * COVERAGE_TILE, tile_y * COVERAGE_TILE);
                for y in rect.1.max(top)..bottom.min(top + COVERAGE_TILE) {
                    for x in rect.0.max(left)..right.min(left + COVERAGE_TILE) {
                        let alpha = brush_alpha(x as f32 + 0.5, y as f32 + 0.5, center, radius, settings.hardness);
                        let index = ((y - top) * COVERAGE_TILE + x - left) as usize;
                        if alpha <= tile.coverage[index] {
                            continue;
                        }
                        tile.coverage[index] = alpha;
                        let filtered = tile.filtered.as_ref().expect("filtered above")[index];
                        let mut value = mix(premultiply(tile.shown[index]), filtered, alpha);
                        if aliased {
                            value = snap(value, &tile.palette);
                        }
                        let pixel = result.get_pixel_mut(x, y);
                        pixel.0 = unpremultiply(value);
                        reveal(mask, x, y, pixel, settings);
                        changed = true;
                    }
                }
            }
        }
        changed.then_some(rect)
    }

    /// Lays down the paint picked up at the last dab, then picks up what is here now.
    fn smudge_dab(
        &mut self,
        result: &mut RgbaImage,
        mask: &mut GrayImage,
        center: (f32, f32),
        settings: &BrushSettings,
    ) -> Option<DirtyRect> {
        let radius = settings.size.clamp(RESULT_BRUSH_MIN, RESULT_BRUSH_MAX) / 2.0;
        let reach = radius.ceil() as i64;
        let side = (2 * reach + 1) as usize;
        let (cx, cy) = (center.0.floor() as i64, center.1.floor() as i64);
        let (width, height) = (i64::from(result.width()), i64::from(result.height()));
        let source = self.source_for(result);
        let inside = |x: i64, y: i64| x >= 0 && y >= 0 && x < width && y < height;
        let offsets = || (-reach..=reach).flat_map(move |dy| (-reach..=reach).map(move |dx| (dx, dy)));
        let mut dirty = None;
        if let Some(patch) = self.smudge.take() {
            let strength = settings.strength.clamp(0.0, 1.0);
            let palette = if self.aliased {
                let here = offsets()
                    .map(|(dx, dy)| (cx + dx, cy + dy))
                    .filter(|&(x, y)| inside(x, y))
                    .map(|(x, y)| shown_pixel(source.as_deref(), result, mask, x as u32, y as u32));
                palette(here.chain(patch.iter().flatten().map(|pixel| unpremultiply(*pixel))))
            } else {
                Vec::new()
            };
            for (index, (dx, dy)) in offsets().enumerate() {
                let (x, y) = (cx + dx, cy + dy);
                let Some(carried) = patch[index] else { continue };
                if !inside(x, y) {
                    continue;
                }
                let falloff = brush_alpha(x as f32 + 0.5, y as f32 + 0.5, center, radius, settings.hardness);
                if falloff <= 0.0 {
                    continue;
                }
                let (x, y) = (x as u32, y as u32);
                let here = premultiply(shown_pixel(source.as_deref(), result, mask, x, y));
                let mut value = mix(here, carried, falloff * strength);
                if self.aliased {
                    value = snap(value, &palette);
                }
                let pixel = result.get_pixel_mut(x, y);
                pixel.0 = unpremultiply(value);
                reveal(mask, x, y, pixel, settings);
                dirty = union(dirty, Some((x, y, 1, 1)));
            }
        }
        let mut patch = Vec::with_capacity(side * side);
        for (dx, dy) in offsets() {
            let (x, y) = (cx + dx, cy + dy);
            patch.push(
                inside(x, y).then(|| premultiply(shown_pixel(source.as_deref(), result, mask, x as u32, y as u32))),
            );
        }
        self.smudge = Some(patch);
        dirty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draw(
        tool: ResultTool,
        result: &mut RgbaImage,
        mask: &mut GrayImage,
        points: &[(f32, f32)],
        settings: &BrushSettings,
    ) -> Option<DirtyRect> {
        let mut stroke = Stroke::new(0.0, false, None);
        let mut dirty = None;
        for &point in points {
            dirty = union(dirty, stroke.add_point(tool, result, mask, point, settings));
        }
        union(dirty, stroke.finish(tool, result, mask, points.last().copied(), settings))
    }

    #[test]
    fn brush_alpha_profile() {
        assert_eq!(brush_alpha(0.0, 0.0, (0.0, 0.0), 10.0, 0.5), 1.0);
        assert_eq!(brush_alpha(10.0, 0.0, (0.0, 0.0), 10.0, 0.5), 0.0);
        let soft = brush_alpha(8.0, 0.0, (0.0, 0.0), 10.0, 0.5);
        assert!(soft > 0.0 && soft < 1.0);
        assert_eq!(brush_alpha(9.0, 0.0, (0.0, 0.0), 10.0, 1.0), 1.0);
        let rim = brush_alpha(10.0, 0.0, (0.0, 0.0), 10.0, 1.0);
        assert!(rim > 0.0 && rim < 1.0, "hard rim is anti-aliased: {rim}");
    }

    #[test]
    fn brush_paints_and_covers_mask_eraser_clears() {
        let mut result = RgbaImage::new(40, 40);
        let mut mask = GrayImage::new(40, 40);
        let settings = BrushSettings { size: 10.0, hardness: 1.0, color: [200, 10, 10], ..BrushSettings::default() };
        let dirty = draw(ResultTool::Brush, &mut result, &mut mask, &[(10.0, 10.0), (30.0, 10.0)], &settings).unwrap();
        assert_eq!(result.get_pixel(20, 10).0, [200, 10, 10, 255]);
        assert_eq!(mask.get_pixel(20, 10)[0], 255);
        assert!(dirty.0 <= 5 && dirty.0 + dirty.2 >= 35);
        draw(ResultTool::Eraser, &mut result, &mut mask, &[(20.0, 10.0)], &settings);
        assert_eq!(result.get_pixel(20, 10)[3], 0);
        assert_eq!(mask.get_pixel(20, 10)[0], 255, "eraser leaves the mask");
    }

    #[test]
    fn keep_mask_edits_the_result_only() {
        let opaque = |_, _| image::Rgba([90, 90, 90, 255]);
        for tool in [ResultTool::Brush, ResultTool::Blur, ResultTool::Sharpen, ResultTool::Smudge] {
            for keep_mask in [false, true] {
                let settings = BrushSettings { size: 10.0, keep_mask, color: [200, 0, 0], ..BrushSettings::default() };
                let mut result = RgbaImage::from_fn(40, 20, opaque);
                let mut mask = GrayImage::new(40, 20);
                draw(tool, &mut result, &mut mask, &[(10.0, 10.0), (30.0, 10.0)], &settings);
                let covered = mask.get_pixel(20, 10)[0];
                assert_eq!(covered, if keep_mask { 0 } else { 255 }, "{tool:?} keep_mask={keep_mask}");
            }
        }
        // Filters don't grow the mask where the result has nothing to show.
        let settings = BrushSettings { size: 10.0, ..BrushSettings::default() };
        let (mut result, mut mask) = (RgbaImage::new(40, 20), GrayImage::new(40, 20));
        draw(ResultTool::Blur, &mut result, &mut mask, &[(10.0, 10.0), (30.0, 10.0)], &settings);
        assert!(mask.iter().all(|value| *value == 0));
    }

    #[test]
    fn overlapping_dabs_keep_the_soft_edge_and_opacity() {
        let settings = BrushSettings { size: 20.0, hardness: 0.0, color: [0, 0, 0], ..BrushSettings::default() };
        // A stroke crossing itself keeps the edge of a single pass.
        let line = [(10.0, 20.0), (50.0, 20.0)];
        let (mut once, mut mask) = (RgbaImage::new(60, 40), GrayImage::new(60, 40));
        draw(ResultTool::Brush, &mut once, &mut mask, &line, &settings);
        let (mut twice, mut mask) = (RgbaImage::new(60, 40), GrayImage::new(60, 40));
        let back_and_forth = [(10.0, 20.0), (50.0, 20.0), (10.0, 20.0), (50.0, 20.0)];
        draw(ResultTool::Brush, &mut twice, &mut mask, &back_and_forth, &settings);
        let (a, b) = (once.get_pixel(30, 26)[3], twice.get_pixel(30, 26)[3]);
        assert!(a > 0 && a < 255 && a.abs_diff(b) <= 2, "edge {a} vs {b}");

        let half = BrushSettings { opacity: 0.5, hardness: 1.0, ..settings };
        let (mut result, mut mask) = (RgbaImage::new(60, 40), GrayImage::new(60, 40));
        draw(ResultTool::Brush, &mut result, &mut mask, &back_and_forth, &half);
        assert_eq!(result.get_pixel(30, 20)[3], 128, "a stroke never goes past its opacity");
    }

    #[test]
    fn dabs_are_evenly_spaced_across_samples() {
        let settings = BrushSettings { size: 10.0, hardness: 1.0, ..BrushSettings::default() };
        let mut stroke = Stroke::new(0.0, false, None);
        let (mut result, mut mask) = (RgbaImage::new(80, 20), GrayImage::new(80, 20));
        for x in 0..60 {
            stroke.add_point(ResultTool::Brush, &mut result, &mut mask, (10.0 + x as f32 * 0.3, 10.0), &settings);
        }
        assert!(stroke.travelled < 0.8 + 1e-3, "leftover distance carries over: {}", stroke.travelled);
    }

    #[test]
    fn aliased_brush_paints_only_full_pixels() {
        let settings = BrushSettings { size: 9.0, hardness: 0.3, color: [0, 0, 0], ..BrushSettings::default() };
        let mut stroke = Stroke::new(0.0, true, None);
        let (mut result, mut mask) = (RgbaImage::new(30, 30), GrayImage::new(30, 30));
        stroke.add_point(ResultTool::Brush, &mut result, &mut mask, (15.0, 15.0), &settings);
        assert!(result.pixels().all(|pixel| pixel[3] == 0 || pixel[3] == 255));
        assert_eq!(result.get_pixel(15, 15)[3], 255);
    }

    #[test]
    fn smoothing_trails_the_pointer_and_catches_up() {
        let settings = BrushSettings { size: 4.0, hardness: 1.0, color: [0, 0, 0], ..BrushSettings::default() };
        let mut stroke = Stroke::new(10.0, false, None);
        let (mut result, mut mask) = (RgbaImage::new(60, 10), GrayImage::new(60, 10));
        stroke.add_point(ResultTool::Brush, &mut result, &mut mask, (5.0, 5.0), &settings);
        stroke.add_point(ResultTool::Brush, &mut result, &mut mask, (40.0, 5.0), &settings);
        assert_eq!(result.get_pixel(38, 5)[3], 0, "the pen lags behind");
        stroke.finish(ResultTool::Brush, &mut result, &mut mask, Some((40.0, 5.0)), &settings);
        assert_eq!(result.get_pixel(38, 5)[3], 255, "and reaches the pointer on release");
    }

    /// A black-and-white page split at x = 20, no result or mask yet.
    fn split_page() -> (Arc<RgbaImage>, RgbaImage, GrayImage) {
        let source = RgbaImage::from_fn(40, 20, |x, _| {
            if x < 20 { image::Rgba([0, 0, 0, 255]) } else { image::Rgba([255, 255, 255, 255]) }
        });
        (Arc::new(source), RgbaImage::new(40, 20), GrayImage::new(40, 20))
    }

    fn draw_on_page(
        tool: ResultTool,
        source: &Arc<RgbaImage>,
        result: &mut RgbaImage,
        mask: &mut GrayImage,
        points: &[(f32, f32)],
        settings: &BrushSettings,
    ) {
        let mut stroke = Stroke::new(0.0, !settings.anti_alias, Some(Arc::clone(source)));
        for &point in points {
            stroke.add_point(tool, result, mask, point, settings);
        }
        stroke.finish(tool, result, mask, points.last().copied(), settings);
    }

    #[test]
    fn filters_work_on_what_the_page_shows() {
        let (source, mut result, mut mask) = split_page();
        let settings = BrushSettings { size: 12.0, strength: 0.5, ..BrushSettings::default() };
        draw_on_page(ResultTool::Blur, &source, &mut result, &mut mask, &[(20.0, 10.0)], &settings);
        let edge = result.get_pixel(20, 10);
        assert!(edge[3] == 255 && edge[0] > 0 && edge[0] < 255, "page edge blurred: {edge:?}");
        assert_eq!(mask.get_pixel(20, 10)[0], 255, "and shows in the output");

        let (source, mut result, mut mask) = split_page();
        let keep = BrushSettings { keep_mask: true, ..settings };
        draw_on_page(ResultTool::Blur, &source, &mut result, &mut mask, &[(20.0, 10.0)], &keep);
        assert!(mask.iter().all(|value| *value == 0), "keep_mask leaves the mask");
    }

    #[test]
    fn blur_does_not_build_up_within_a_stroke() {
        let settings = BrushSettings { size: 12.0, strength: 0.5, ..BrushSettings::default() };
        let (source, mut once, mut mask) = split_page();
        draw_on_page(ResultTool::Blur, &source, &mut once, &mut mask, &[(20.0, 4.0), (20.0, 16.0)], &settings);
        let (source, mut scrubbed, mut mask) = split_page();
        let back_and_forth = [(20.0, 4.0), (20.0, 16.0), (20.0, 4.0), (20.0, 16.0)];
        draw_on_page(ResultTool::Blur, &source, &mut scrubbed, &mut mask, &back_and_forth, &settings);
        assert_eq!(once.get_pixel(19, 10), scrubbed.get_pixel(19, 10));
    }

    #[test]
    fn sharpen_raises_contrast_at_an_edge() {
        let source = Arc::new(RgbaImage::from_fn(40, 20, |x, _| {
            if x < 20 { image::Rgba([100, 100, 100, 255]) } else { image::Rgba([150, 150, 150, 255]) }
        }));
        let (mut result, mut mask) = (RgbaImage::new(40, 20), GrayImage::new(40, 20));
        let settings = BrushSettings { size: 12.0, strength: 0.5, ..BrushSettings::default() };
        draw_on_page(ResultTool::Sharpen, &source, &mut result, &mut mask, &[(20.0, 10.0)], &settings);
        assert!(result.get_pixel(19, 10)[0] < 100 && result.get_pixel(20, 10)[0] > 150);
        assert_eq!(result.get_pixel(14, 10)[0], 100, "flat areas stay flat");
    }

    #[test]
    fn aliased_filters_keep_to_the_page_colors() {
        let settings = BrushSettings { size: 12.0, strength: 1.0, anti_alias: false, ..BrushSettings::default() };
        for tool in [ResultTool::Blur, ResultTool::Sharpen, ResultTool::Smudge] {
            let (source, mut result, mut mask) = split_page();
            draw_on_page(tool, &source, &mut result, &mut mask, &[(14.0, 10.0), (26.0, 10.0)], &settings);
            let colors: std::collections::HashSet<[u8; 4]> =
                result.pixels().filter(|pixel| pixel[3] > 0).map(|pixel| pixel.0).collect();
            assert!(
                colors.iter().all(|color| *color == [0, 0, 0, 255] || *color == [255, 255, 255, 255]),
                "{tool:?} added tones: {colors:?}"
            );
        }
        // With anti-aliasing the smudge leaves in-between tones.
        let (source, mut result, mut mask) = split_page();
        let soft = BrushSettings { anti_alias: true, ..settings };
        draw_on_page(ResultTool::Smudge, &source, &mut result, &mut mask, &[(14.0, 10.0), (26.0, 10.0)], &soft);
        assert!(result.pixels().any(|pixel| pixel[3] > 0 && pixel[0] > 0 && pixel[0] < 255));
    }

    #[test]
    fn blur_softens_edges_and_smudge_drags_color() {
        let mut result = RgbaImage::from_fn(20, 20, |x, _| {
            if x < 10 { image::Rgba([0, 0, 0, 255]) } else { image::Rgba([255, 255, 255, 255]) }
        });
        let mut mask = GrayImage::new(20, 20);
        let settings = BrushSettings { size: 8.0, hardness: 1.0, strength: 1.0, ..BrushSettings::default() };
        draw(ResultTool::Blur, &mut result, &mut mask, &[(10.0, 10.0)], &settings);
        let edge = result.get_pixel(10, 10)[0];
        assert!(edge > 0 && edge < 255, "edge blurred: {edge}");

        let mut result = RgbaImage::from_fn(30, 10, |x, _| {
            if x < 10 { image::Rgba([0, 0, 0, 255]) } else { image::Rgba([255, 255, 255, 255]) }
        });
        draw(ResultTool::Smudge, &mut result, &mut mask, &[(5.0, 5.0), (12.0, 5.0)], &settings);
        assert!(result.get_pixel(12, 5)[0] < 255, "dark color dragged right");
    }
}
