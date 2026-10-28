//! Inpaint result layer touch-up tools (`InpaintResultCanvas.tsx`, `inpaintResultCanvas.ts`):
//! brush, smart brush, eraser, blur, sharpen, smudge and color picking.

use image::{GrayImage, RgbaImage};

pub const RESULT_BRUSH_MIN: f32 = 2.0;
pub const RESULT_BRUSH_MAX: f32 = 128.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultTool {
    Brush,
    /// Paints and also extends the mask so the paint shows in the output.
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
    pub color: [u8; 3],
}

impl Default for BrushSettings {
    fn default() -> Self {
        Self { size: 28.0, hardness: 0.85, strength: 0.55, color: [255, 255, 255] }
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

/// `brushMaskAlpha`: 1 inside the hard core, eased falloff to the radius.
pub fn brush_alpha(x: f32, y: f32, center: (f32, f32), radius: f32, hardness: f32) -> f32 {
    let distance = (x - center.0).hypot(y - center.1);
    if distance >= radius {
        return 0.0;
    }
    let hardness = hardness.clamp(0.0, 1.0);
    if hardness >= 1.0 {
        return 1.0;
    }
    let hard_radius = radius * hardness;
    if distance <= hard_radius {
        return 1.0;
    }
    let progress = (distance - hard_radius) / (radius - hard_radius).max(1.0);
    (1.0 - progress).powf(1.0 + (1.0 - hardness) * 2.0)
}

fn bounds(center: (f32, f32), radius: f32, width: u32, height: u32) -> Option<DirtyRect> {
    let x = (center.0 - radius).floor().max(0.0) as u32;
    let y = (center.1 - radius).floor().max(0.0) as u32;
    let right = ((center.0 + radius).ceil().max(0.0) as u32).min(width);
    let bottom = ((center.1 + radius).ceil().max(0.0) as u32).min(height);
    (right > x && bottom > y).then_some((x, y, right - x, bottom - y))
}

/// Every pixel a `stroke` from `from` to `to` may change (with a pixel or two to spare).
pub fn segment_bounds(
    from: (f32, f32),
    to: (f32, f32),
    settings: &BrushSettings,
    width: u32,
    height: u32,
) -> Option<DirtyRect> {
    let radius = settings.size.max(RESULT_BRUSH_MIN) / 2.0 + 2.0;
    let x = (from.0.min(to.0) - radius).floor().max(0.0) as u32;
    let y = (from.1.min(to.1) - radius).floor().max(0.0) as u32;
    let right = ((from.0.max(to.0) + radius).ceil().max(0.0) as u32).min(width);
    let bottom = ((from.1.max(to.1) + radius).ceil().max(0.0) as u32).min(height);
    (right > x && bottom > y).then_some((x, y, right - x, bottom - y))
}

fn clamp_byte(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

/// Stamps along a segment like `drawSegment` (spacing size/4 for paint, size/2 for filters).
pub fn stroke(
    tool: ResultTool,
    result: &mut RgbaImage,
    mask: &mut GrayImage,
    from: (f32, f32),
    to: (f32, f32),
    settings: &BrushSettings,
    smudge: &mut Option<RgbaImage>,
) -> Option<DirtyRect> {
    let size = settings.size.clamp(RESULT_BRUSH_MIN, RESULT_BRUSH_MAX);
    if tool == ResultTool::Smudge {
        return smudge_stamp(result, to, settings, smudge);
    }
    let paint = matches!(tool, ResultTool::Brush | ResultTool::SmartBrush | ResultTool::Eraser);
    let step = (size / if paint { 4.0 } else { 2.0 }).max(1.0);
    let distance = (to.0 - from.0).hypot(to.1 - from.1);
    let steps = (distance / step).ceil().max(1.0) as u32;
    let mut dirty = None;
    for index in 0..=steps {
        let t = index as f32 / steps as f32;
        let point = (from.0 + (to.0 - from.0) * t, from.1 + (to.1 - from.1) * t);
        let touched = match tool {
            ResultTool::Brush | ResultTool::SmartBrush => paint_stamp(result, mask, point, settings, false),
            ResultTool::Eraser => paint_stamp(result, mask, point, settings, true),
            ResultTool::Blur | ResultTool::Sharpen => filter_stamp(result, point, settings, tool == ResultTool::Blur),
            ResultTool::Smudge | ResultTool::ColorPicker => None,
        };
        dirty = union(dirty, touched);
    }
    dirty
}

fn paint_stamp(
    result: &mut RgbaImage,
    mask: &mut GrayImage,
    center: (f32, f32),
    settings: &BrushSettings,
    erase: bool,
) -> Option<DirtyRect> {
    let radius = (settings.size / 2.0).max(0.5);
    let rect = bounds(center, radius, result.width(), result.height())?;
    let mut changed = false;
    for y in rect.1..rect.1 + rect.3 {
        for x in rect.0..rect.0 + rect.2 {
            let alpha = brush_alpha(x as f32 + 0.5, y as f32 + 0.5, center, radius, settings.hardness);
            if alpha <= 0.0 {
                continue;
            }
            let pixel = result.get_pixel_mut(x, y);
            if erase {
                pixel[3] = clamp_byte(f32::from(pixel[3]) * (1.0 - alpha));
            } else {
                // Source-over in straight alpha.
                let dst_alpha = f32::from(pixel[3]) / 255.0;
                let out_alpha = alpha + dst_alpha * (1.0 - alpha);
                for channel in 0..3 {
                    let dst = f32::from(pixel[channel]);
                    let src = f32::from(settings.color[channel]);
                    pixel[channel] = clamp_byte((src * alpha + dst * dst_alpha * (1.0 - alpha)) / out_alpha.max(1e-6));
                }
                pixel[3] = clamp_byte(out_alpha * 255.0);
                // Painted pixels must be covered by the mask to show in the output.
                mask.put_pixel(x, y, image::Luma([255]));
            }
            changed = true;
        }
    }
    changed.then_some(rect)
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

    #[test]
    fn brush_alpha_profile() {
        assert_eq!(brush_alpha(0.0, 0.0, (0.0, 0.0), 10.0, 0.5), 1.0);
        assert_eq!(brush_alpha(10.0, 0.0, (0.0, 0.0), 10.0, 0.5), 0.0);
        let soft = brush_alpha(8.0, 0.0, (0.0, 0.0), 10.0, 0.5);
        assert!(soft > 0.0 && soft < 1.0);
        assert_eq!(brush_alpha(9.9, 0.0, (0.0, 0.0), 10.0, 1.0), 1.0);
    }

    #[test]
    fn brush_paints_and_covers_mask_eraser_clears() {
        let mut result = RgbaImage::new(40, 40);
        let mut mask = GrayImage::new(40, 40);
        let settings = BrushSettings { size: 10.0, hardness: 1.0, color: [200, 10, 10], ..BrushSettings::default() };
        let dirty = stroke(ResultTool::Brush, &mut result, &mut mask, (10.0, 10.0), (30.0, 10.0), &settings, &mut None)
            .unwrap();
        assert_eq!(result.get_pixel(20, 10).0, [200, 10, 10, 255]);
        assert_eq!(mask.get_pixel(20, 10)[0], 255);
        assert!(dirty.0 <= 5 && dirty.0 + dirty.2 >= 35);
        stroke(ResultTool::Eraser, &mut result, &mut mask, (20.0, 10.0), (20.0, 10.0), &settings, &mut None);
        assert_eq!(result.get_pixel(20, 10)[3], 0);
        assert_eq!(mask.get_pixel(20, 10)[0], 255, "eraser leaves the mask");
    }

    #[test]
    fn blur_softens_edges_and_smudge_drags_color() {
        let mut result = RgbaImage::from_fn(20, 20, |x, _| {
            if x < 10 { image::Rgba([0, 0, 0, 255]) } else { image::Rgba([255, 255, 255, 255]) }
        });
        let mut mask = GrayImage::new(20, 20);
        let settings = BrushSettings { size: 8.0, hardness: 1.0, strength: 1.0, ..BrushSettings::default() };
        stroke(ResultTool::Blur, &mut result, &mut mask, (10.0, 10.0), (10.0, 10.0), &settings, &mut None);
        let edge = result.get_pixel(10, 10)[0];
        assert!(edge > 0 && edge < 255, "edge blurred: {edge}");

        let mut result = RgbaImage::from_fn(30, 10, |x, _| {
            if x < 10 { image::Rgba([0, 0, 0, 255]) } else { image::Rgba([255, 255, 255, 255]) }
        });
        let mut patch = None;
        stroke(ResultTool::Smudge, &mut result, &mut mask, (5.0, 5.0), (5.0, 5.0), &settings, &mut patch);
        stroke(ResultTool::Smudge, &mut result, &mut mask, (5.0, 5.0), (12.0, 5.0), &settings, &mut patch);
        assert!(result.get_pixel(12, 5)[0] < 255, "dark color dragged right");
    }
}
