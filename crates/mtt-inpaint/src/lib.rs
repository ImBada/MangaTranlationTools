//! Inpainting: engines plus the mask/result layer operations the editor needs
//! (`inpaintEngine.ts`, `libraryImageData.ts`, `inpaintMaskImages.ts`).
//!
//! Masks are single-channel coverage images (0 = keep, 255 = inpaint); results are RGBA
//! layers that are transparent where the source shows through.

pub mod engine;
pub mod lama;
pub mod lama_runtime;
pub mod psd;
pub mod text_mask;

use image::{GrayImage, RgbaImage};

pub use engine::run_inpaint;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Rect {
    /// Clamps a float rect to an image of `width`×`height` (`clampImageRect`).
    pub fn clamped(x: f32, y: f32, w: f32, h: f32, width: u32, height: u32) -> Option<Self> {
        let left = (x.floor().max(0.0) as u32).min(width);
        let top = (y.floor().max(0.0) as u32).min(height);
        let right = ((x + w).ceil().max(0.0) as u32).min(width);
        let bottom = ((y + h).ceil().max(0.0) as u32).min(height);
        (right > left && bottom > top).then_some(Self { x: left, y: top, width: right - left, height: bottom - top })
    }

    pub fn contains(&self, x: u32, y: u32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }
}

/// Square dilation by `radius` (`dilateMask` / `expandMask`).
pub fn dilate(mask: &[u8], width: usize, height: usize, radius: usize) -> Vec<u8> {
    let mut horizontal = vec![0u8; mask.len()];
    let mut output = vec![0u8; mask.len()];
    let r = radius as isize;
    for y in 0..height {
        let row = &mask[y * width..(y + 1) * width];
        let mut active = (0..=radius.min(width.saturating_sub(1))).filter(|x| row[*x] > 0).count() as isize;
        for x in 0..width {
            horizontal[y * width + x] = if active > 0 { 255 } else { 0 };
            let remove = x as isize - r;
            let add = x + radius + 1;
            if remove >= 0 && row[remove as usize] > 0 {
                active -= 1;
            }
            if add < width && row[add] > 0 {
                active += 1;
            }
        }
    }
    for x in 0..width {
        let mut active =
            (0..=radius.min(height.saturating_sub(1))).filter(|y| horizontal[y * width + x] > 0).count() as isize;
        for y in 0..height {
            output[y * width + x] = if active > 0 { 255 } else { 0 };
            let remove = y as isize - r;
            let add = y + radius + 1;
            if remove >= 0 && horizontal[remove as usize * width + x] > 0 {
                active -= 1;
            }
            if add < height && horizontal[add * width + x] > 0 {
                active += 1;
            }
        }
    }
    output
}

/// Edge-normalized box blur (`boxBlurMask`).
pub fn box_blur(mask: &[u8], width: usize, height: usize, radius: usize) -> Vec<u8> {
    let mut horizontal = vec![0f32; mask.len()];
    let mut output = vec![0u8; mask.len()];
    for y in 0..height {
        let row = &mask[y * width..(y + 1) * width];
        let mut sum: f32 = (0..=radius.min(width.saturating_sub(1))).map(|x| f32::from(row[x])).sum();
        for x in 0..width {
            let left = x.saturating_sub(radius);
            let right = (x + radius).min(width - 1);
            horizontal[y * width + x] = sum / (right - left + 1) as f32;
            if x >= radius {
                sum -= f32::from(row[x - radius]);
            }
            if x + radius + 1 < width {
                sum += f32::from(row[x + radius + 1]);
            }
        }
    }
    for x in 0..width {
        let mut sum: f32 = (0..=radius.min(height.saturating_sub(1))).map(|y| horizontal[y * width + x]).sum();
        for y in 0..height {
            let top = y.saturating_sub(radius);
            let bottom = (y + radius).min(height - 1);
            output[y * width + x] = (sum / (bottom - top + 1) as f32).round().clamp(0.0, 255.0) as u8;
            if y >= radius {
                sum -= horizontal[(y - radius) * width + x];
            }
            if y + radius + 1 < height {
                sum += horizontal[(y + radius + 1) * width + x];
            }
        }
    }
    output
}

/// `normalizeInpaintLayerDataUrls`: a fully opaque result is clipped to the mask, and the
/// mask grows to cover every visible result pixel.
pub fn normalize_layers(mask: &mut GrayImage, result: &mut RgbaImage) {
    if result.pixels().all(|p| p[3] == 255) {
        for (pixel, coverage) in result.pixels_mut().zip(mask.pixels()) {
            pixel[3] = if coverage[0] > 0 { 255 } else { 0 };
        }
    }
    for (coverage, pixel) in mask.pixels_mut().zip(result.pixels()) {
        coverage[0] = if coverage[0] > 0 || pixel[3] > 0 { 255 } else { 0 };
    }
}

/// `mergePartialInpaintResult`: patch pixels replace the base where the patch mask
/// covers or the patch has content.
pub fn merge_partial_result(base: Option<&RgbaImage>, patch: &RgbaImage, patch_mask: &GrayImage) -> RgbaImage {
    let mut merged = base.cloned().unwrap_or_else(|| RgbaImage::new(patch.width(), patch.height()));
    for ((target, source), coverage) in merged.pixels_mut().zip(patch.pixels()).zip(patch_mask.pixels()) {
        if coverage[0] == 0 && source[3] == 0 {
            continue;
        }
        *target = *source;
    }
    merged
}

/// Binary union of two masks (`mergeInpaintMaskPixels`).
pub fn merge_masks(base: &mut GrayImage, patch: &GrayImage) {
    for (target, source) in base.pixels_mut().zip(patch.pixels()) {
        target[0] = if target[0] > 0 || source[0] > 0 { 255 } else { 0 };
    }
}

/// Mask restricted to `rect` (`maskDataUrlForSelection`); `None` when nothing is covered.
pub fn mask_for_selection(mask: &GrayImage, rect: Rect) -> Option<GrayImage> {
    let threshold = (0.03 * 255.0) as u8;
    let mut any = false;
    let selected = GrayImage::from_fn(mask.width(), mask.height(), |x, y| {
        let active = rect.contains(x, y) && mask.get_pixel(x, y)[0] > threshold;
        any |= active;
        image::Luma([if active { 255 } else { 0 }])
    });
    any.then_some(selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dilate_grows_square() {
        let mut mask = vec![0u8; 25];
        mask[12] = 255;
        let out = dilate(&mask, 5, 5, 1);
        let covered: Vec<usize> = (0..25).filter(|i| out[*i] > 0).collect();
        assert_eq!(covered, vec![6, 7, 8, 11, 12, 13, 16, 17, 18]);
    }

    #[test]
    fn box_blur_is_edge_normalized() {
        let mask = vec![255u8; 9];
        assert!(box_blur(&mask, 3, 3, 2).iter().all(|v| *v == 255));
        let mut single = vec![0u8; 9];
        single[4] = 255;
        assert_eq!(box_blur(&single, 3, 3, 1)[4], 28);
    }

    #[test]
    fn normalize_clips_opaque_results_and_extends_mask() {
        let mut mask = GrayImage::from_raw(2, 1, vec![255, 0]).unwrap();
        let mut result = RgbaImage::from_pixel(2, 1, image::Rgba([1, 2, 3, 255]));
        normalize_layers(&mut mask, &mut result);
        assert_eq!(result.get_pixel(1, 0)[3], 0);

        let mut mask = GrayImage::from_raw(2, 1, vec![0, 0]).unwrap();
        let mut result = RgbaImage::new(2, 1);
        result.put_pixel(1, 0, image::Rgba([9, 9, 9, 40]));
        normalize_layers(&mut mask, &mut result);
        assert_eq!(mask.as_raw(), &[0, 255]);
    }

    #[test]
    fn partial_merge_and_selection() {
        let base = RgbaImage::from_pixel(2, 1, image::Rgba([10, 10, 10, 255]));
        let patch = RgbaImage::from_pixel(2, 1, image::Rgba([99, 99, 99, 0]));
        let mask = GrayImage::from_raw(2, 1, vec![255, 0]).unwrap();
        let merged = merge_partial_result(Some(&base), &patch, &mask);
        assert_eq!(merged.get_pixel(0, 0).0, [99, 99, 99, 0]);
        assert_eq!(merged.get_pixel(1, 0).0, [10, 10, 10, 255]);

        let full = GrayImage::from_pixel(4, 4, image::Luma([255]));
        let selected = mask_for_selection(&full, Rect { x: 1, y: 1, width: 2, height: 2 }).unwrap();
        assert_eq!(selected.pixels().filter(|p| p[0] > 0).count(), 4);
        assert!(mask_for_selection(&GrayImage::new(4, 4), Rect { x: 0, y: 0, width: 4, height: 4 }).is_none());
    }

    #[test]
    fn rect_clamping() {
        assert_eq!(Rect::clamped(-5.0, 2.4, 10.0, 3.0, 4, 10), Some(Rect { x: 0, y: 2, width: 4, height: 4 }));
        assert_eq!(Rect::clamped(20.0, 0.0, 5.0, 5.0, 10, 10), None);
    }
}
