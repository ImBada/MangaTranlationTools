//! Inpaint result layer touch-up tools (`InpaintResultCanvas.tsx`, `inpaintResultCanvas.ts`):
//! brush, smart brush, eraser, blur, sharpen, smudge and color picking.

use std::collections::HashMap;

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

/// The layer as it was before the stroke and how much the stroke covers it, per tile.
struct CoverageTile {
    original: Vec<[u8; 4]>,
    coverage: Vec<f32>,
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
    smudge: Option<RgbaImage>,
    /// Snap coverage to on/off (pages drawn without anti-aliasing).
    aliased: bool,
}

impl Stroke {
    /// `string` is the smoothing reach in page pixels.
    pub fn new(string: f32, aliased: bool) -> Self {
        Self {
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
            ResultTool::Brush | ResultTool::Eraser => size * 0.08,
            ResultTool::Smudge => size / 6.0,
            _ => size / 4.0,
        }
        .max(0.5);
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
            ResultTool::Blur | ResultTool::Sharpen => filter_stamp(result, center, settings, tool == ResultTool::Blur),
            ResultTool::Smudge => smudge_stamp(result, center, settings, &mut self.smudge),
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
        let (width, height) = result.dimensions();
        let mut changed = false;
        let (right, bottom) = (rect.0 + rect.2, rect.1 + rect.3);
        for tile_y in rect.1 / COVERAGE_TILE..=(bottom - 1) / COVERAGE_TILE {
            for tile_x in rect.0 / COVERAGE_TILE..=(right - 1) / COVERAGE_TILE {
                let (left, top) = (tile_x * COVERAGE_TILE, tile_y * COVERAGE_TILE);
                let tile = self.tiles.entry((tile_x, tile_y)).or_insert_with(|| {
                    let mut original = Vec::with_capacity((COVERAGE_TILE * COVERAGE_TILE) as usize);
                    for y in top..top + COVERAGE_TILE {
                        for x in left..left + COVERAGE_TILE {
                            original.push(if x < width && y < height { result.get_pixel(x, y).0 } else { [0; 4] });
                        }
                    }
                    CoverageTile { original, coverage: vec![0.0; (COVERAGE_TILE * COVERAGE_TILE) as usize] }
                });
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
                        if self.aliased {
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
                            mask.put_pixel(x, y, image::Luma([255]));
                        }
                        changed = true;
                    }
                }
            }
        }
        changed.then_some(rect)
    }
}

fn filter_stamp(result: &mut RgbaImage, center: (f32, f32), settings: &BrushSettings, blur: bool) -> Option<DirtyRect> {
    let radius = (settings.size / 2.0).round().max(1.0);
    let rect = bounds(center, radius, result.width(), result.height())?;
    let source = image::imageops::crop_imm(result, rect.0, rect.1, rect.2, rect.3).to_image();
    let (w, h) = (rect.2 as i32, rect.3 as i32);
    let read = |x: i32, y: i32| source.get_pixel(x.clamp(0, w - 1) as u32, y.clamp(0, h - 1) as u32).0;
    let strength = settings.strength.clamp(0.0, 1.0);
    for y in 0..h {
        for x in 0..w {
            let alpha = brush_alpha(
                (rect.0 as i32 + x) as f32 + 0.5,
                (rect.1 as i32 + y) as f32 + 0.5,
                center,
                radius,
                settings.hardness,
            ) * strength;
            if alpha <= 0.0 {
                continue;
            }
            let center_pixel = read(x, y);
            let filtered: [f32; 3] = if blur {
                let mut sum = [0f32; 3];
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let p = read(x + dx, y + dy);
                        for c in 0..3 {
                            sum[c] += f32::from(p[c]);
                        }
                    }
                }
                sum.map(|v| v / 9.0)
            } else {
                let neighbours = [read(x - 1, y), read(x + 1, y), read(x, y - 1), read(x, y + 1)];
                let mut out = [0f32; 3];
                for c in 0..3 {
                    let ring: f32 = neighbours.iter().map(|p| f32::from(p[c])).sum();
                    out[c] = f32::from(clamp_byte(f32::from(center_pixel[c]) * 5.0 - ring));
                }
                out
            };
            let target = result.get_pixel_mut(rect.0 + x as u32, rect.1 + y as u32);
            for c in 0..3 {
                let src = f32::from(center_pixel[c]);
                target[c] = clamp_byte(src + (filtered[c] - src) * alpha);
            }
        }
    }
    Some(rect)
}

/// Draws the patch picked up at the previous point here, then picks up a new patch.
fn smudge_stamp(
    result: &mut RgbaImage,
    center: (f32, f32),
    settings: &BrushSettings,
    patch: &mut Option<RgbaImage>,
) -> Option<DirtyRect> {
    let radius = (settings.size / 2.0).round().max(1.0) as i32;
    let (left, top) = (center.0.round() as i32 - radius, center.1.round() as i32 - radius);
    let mut dirty = None;
    if let Some(previous) = patch.as_ref() {
        let strength = settings.strength.clamp(0.0, 1.0);
        for py in 0..previous.height() as i32 {
            for px in 0..previous.width() as i32 {
                let (x, y) = (left + px, top + py);
                if x < 0 || y < 0 || x >= result.width() as i32 || y >= result.height() as i32 {
                    continue;
                }
                let mask = brush_alpha(
                    px as f32 + 0.5,
                    py as f32 + 0.5,
                    (radius as f32, radius as f32),
                    radius as f32,
                    settings.hardness,
                );
                let src = previous.get_pixel(px as u32, py as u32).0;
                let alpha = f32::from(src[3]) / 255.0 * mask * strength;
                if alpha <= 0.0 {
                    continue;
                }
                let dst = result.get_pixel_mut(x as u32, y as u32);
                let dst_alpha = f32::from(dst[3]) / 255.0;
                let out_alpha = alpha + dst_alpha * (1.0 - alpha);
                for c in 0..3 {
                    dst[c] = clamp_byte(
                        (f32::from(src[c]) * alpha + f32::from(dst[c]) * dst_alpha * (1.0 - alpha))
                            / out_alpha.max(1e-6),
                    );
                }
                dst[3] = clamp_byte(out_alpha * 255.0);
            }
        }
        dirty = bounds(center, radius as f32, result.width(), result.height());
    }
    let size = (radius * 2) as u32;
    *patch = Some(RgbaImage::from_fn(size, size, |px, py| {
        let (x, y) = (left + px as i32, top + py as i32);
        if x < 0 || y < 0 || x >= result.width() as i32 || y >= result.height() as i32 {
            image::Rgba([0, 0, 0, 0])
        } else {
            *result.get_pixel(x as u32, y as u32)
        }
    }));
    dirty
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
        let mut stroke = Stroke::new(0.0, false);
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
        let mut stroke = Stroke::new(0.0, false);
        let (mut result, mut mask) = (RgbaImage::new(80, 20), GrayImage::new(80, 20));
        for x in 0..60 {
            stroke.add_point(ResultTool::Brush, &mut result, &mut mask, (10.0 + x as f32 * 0.3, 10.0), &settings);
        }
        assert!(stroke.travelled < 0.8 + 1e-3, "leftover distance carries over: {}", stroke.travelled);
    }

    #[test]
    fn aliased_brush_paints_only_full_pixels() {
        let settings = BrushSettings { size: 9.0, hardness: 0.3, color: [0, 0, 0], ..BrushSettings::default() };
        let mut stroke = Stroke::new(0.0, true);
        let (mut result, mut mask) = (RgbaImage::new(30, 30), GrayImage::new(30, 30));
        stroke.add_point(ResultTool::Brush, &mut result, &mut mask, (15.0, 15.0), &settings);
        assert!(result.pixels().all(|pixel| pixel[3] == 0 || pixel[3] == 255));
        assert_eq!(result.get_pixel(15, 15)[3], 255);
    }

    #[test]
    fn smoothing_trails_the_pointer_and_catches_up() {
        let settings = BrushSettings { size: 4.0, hardness: 1.0, color: [0, 0, 0], ..BrushSettings::default() };
        let mut stroke = Stroke::new(10.0, false);
        let (mut result, mut mask) = (RgbaImage::new(60, 10), GrayImage::new(60, 10));
        stroke.add_point(ResultTool::Brush, &mut result, &mut mask, (5.0, 5.0), &settings);
        stroke.add_point(ResultTool::Brush, &mut result, &mut mask, (40.0, 5.0), &settings);
        assert_eq!(result.get_pixel(38, 5)[3], 0, "the pen lags behind");
        stroke.finish(ResultTool::Brush, &mut result, &mut mask, Some((40.0, 5.0)), &settings);
        assert_eq!(result.get_pixel(38, 5)[3], 255, "and reaches the pointer on release");
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
