//! Page compositing (`renderPageToPngDataUrl`, group drop shadows).

use image::RgbaImage;
use mtt_core::groups::{ListItem, drop_shadow_effect, drop_shadow_settings, list_items};
use mtt_core::model::{PageRecord, RenderTextDirection, TranslationBlock};
use tiny_skia::{ColorU8, Pixmap, PremultipliedColorU8};

use crate::block::{BlockRaster, BlockRenderOptions, render_block};
use crate::canvas::{Canvas, hex_color};
use crate::effects::gaussian_blur;
use crate::fonts::FontContext;
use crate::layout::Size;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveLayer {
    Output,
    Image,
    Inpaint,
    InpaintResult,
    InpaintMask,
    Overlay,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerVisibility {
    pub image: bool,
    pub inpaint: bool,
    pub inpaint_result: bool,
    pub inpaint_mask: bool,
    pub overlay: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerOpacity {
    pub image: f32,
    pub inpaint: f32,
    pub inpaint_result: f32,
    pub inpaint_mask: f32,
    pub overlay: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageRenderOptions {
    pub visibility: LayerVisibility,
    pub opacity: LayerOpacity,
    pub active_layer: ActiveLayer,
}

impl PageRenderOptions {
    /// `OUTPUT_RENDER_OPTIONS`: what "출력" saves.
    pub const OUTPUT: Self = Self {
        visibility: LayerVisibility {
            image: true,
            inpaint: true,
            inpaint_result: true,
            inpaint_mask: false,
            overlay: true,
        },
        opacity: LayerOpacity { image: 1.0, inpaint: 1.0, inpaint_result: 1.0, inpaint_mask: 1.0, overlay: 1.0 },
        active_layer: ActiveLayer::Output,
    };
}

pub struct PageImages<'a> {
    pub source: Option<&'a RgbaImage>,
    pub inpaint_mask: Option<&'a RgbaImage>,
    pub inpaint_result: Option<&'a RgbaImage>,
}

pub fn page_size(page: &PageRecord) -> Size {
    Size::new(f64::from(page.width.max(1)), f64::from(page.height.max(1)))
}

/// Inpaint mask coverage in 0..=1 (`resolveInpaintMaskRgbaCoverageAlpha`).
pub fn mask_coverage(pixel: &image::Rgba<u8>) -> f32 {
    let [r, g, b, a] = pixel.0;
    let luma = (f32::from(r) * 0.299 + f32::from(g) * 0.587 + f32::from(b) * 0.114) / 255.0;
    f32::from(a) / 255.0 * luma
}

/// Converts a straight-alpha image to a premultiplied pixmap, scaling with nearest
/// neighbour when the size differs (masks are drawn without smoothing).
pub fn image_to_pixmap(image: &RgbaImage, width: u32, height: u32) -> Option<Pixmap> {
    let mut pixmap = Pixmap::new(width, height)?;
    let same_size = image.width() == width && image.height() == height;
    for (index, pixel) in pixmap.pixels_mut().iter_mut().enumerate() {
        let (x, y) = (index as u32 % width, index as u32 / width);
        let source = if same_size {
            image.get_pixel(x, y)
        } else {
            image.get_pixel(
                (u64::from(x) * u64::from(image.width()) / u64::from(width)) as u32,
                (u64::from(y) * u64::from(image.height()) / u64::from(height)) as u32,
            )
        };
        *pixel = ColorU8::from_rgba(source[0], source[1], source[2], source[3]).premultiply();
    }
    Some(pixmap)
}

/// Inpaint result with alpha multiplied by mask coverage (`applyInpaintMaskPixelsToLayer`).
pub fn masked_result_pixmap(result: &RgbaImage, mask: Option<&RgbaImage>, width: u32, height: u32) -> Option<Pixmap> {
    let mut pixmap = image_to_pixmap(result, width, height)?;
    let Some(mask) = mask else {
        return Some(pixmap);
    };
    let same_size = mask.width() == width && mask.height() == height;
    for (index, pixel) in pixmap.pixels_mut().iter_mut().enumerate() {
        let (x, y) = (index as u32 % width, index as u32 / width);
        let mask_pixel = if same_size {
            mask.get_pixel(x, y)
        } else {
            mask.get_pixel(
                (u64::from(x) * u64::from(mask.width()) / u64::from(width)) as u32,
                (u64::from(y) * u64::from(mask.height()) / u64::from(height)) as u32,
            )
        };
        let coverage = mask_coverage(mask_pixel);
        if coverage >= 1.0 {
            continue;
        }
        let straight = pixel.demultiply();
        let alpha = (f32::from(straight.alpha()) * coverage).round() as u8;
        *pixel = ColorU8::from_rgba(straight.red(), straight.green(), straight.blue(), alpha).premultiply();
    }
    Some(pixmap)
}

/// Group drop shadows drawn under the blocks (`drawOverlayBlockGroupEffects`).
pub fn render_group_shadows(ctx: &mut FontContext, page: &PageRecord) -> Vec<BlockRaster> {
    let size = page_size(page);
    let mut rasters = Vec::new();
    for item in list_items(page) {
        let ListItem::Group { group, blocks } = item else {
            continue;
        };
        let Some(effect) = drop_shadow_effect(&group).filter(|effect| effect.enabled) else {
            continue;
        };
        let settings = drop_shadow_settings(Some(effect));
        if settings.opacity <= 0.0 {
            continue;
        }
        let members: Vec<&TranslationBlock> = blocks
            .iter()
            .map(|(_, block)| *block)
            .filter(|block| block.render_direction != RenderTextDirection::Hidden)
            .collect();
        if members.is_empty() {
            continue;
        }
        let Some(mut layer) = Canvas::new(page.width.max(1), page.height.max(1)) else {
            continue;
        };
        for block in members {
            if let Some(raster) = render_block(ctx, size, block, BlockRenderOptions::default()) {
                layer.draw_pixmap_device(&raster.pixmap, raster.x, raster.y, 1.0, None);
            }
        }
        let color = hex_color(&settings.color, 1.0).to_color_u8();
        for pixel in layer.pixmap.pixels_mut() {
            let alpha = pixel.alpha();
            *pixel = ColorU8::from_rgba(color.red(), color.green(), color.blue(), alpha).premultiply();
        }
        gaussian_blur(&mut layer.pixmap, settings.blur_px);
        let angle = settings.angle_deg.to_radians();
        let Some(mut shadow) = Pixmap::new(layer.pixmap.width(), layer.pixmap.height()) else {
            continue;
        };
        shadow.draw_pixmap(
            0,
            0,
            layer.pixmap.as_ref(),
            &tiny_skia::PixmapPaint {
                opacity: settings.opacity as f32,
                quality: tiny_skia::FilterQuality::Bilinear,
                ..Default::default()
            },
            tiny_skia::Transform::from_translate(
                (angle.cos() * settings.distance_px) as f32,
                (angle.sin() * settings.distance_px) as f32,
            ),
            None,
        );
        if let Some(raster) = crop_to_content(shadow) {
            rasters.push(raster);
        }
    }
    rasters
}

/// Shrinks a page-sized pixmap to its non-transparent area.
fn crop_to_content(pixmap: Pixmap) -> Option<BlockRaster> {
    let (width, height) = (pixmap.width(), pixmap.height());
    let pixels = pixmap.pixels();
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (width, height, 0, 0);
    for y in 0..height {
        for x in 0..width {
            if pixels[(y * width + x) as usize].alpha() > 0 {
                min_x = min_x.min(x);
                max_x = max_x.max(x);
                min_y = min_y.min(y);
                max_y = max_y.max(y);
            }
        }
    }
    if min_x > max_x || min_y > max_y {
        return None;
    }
    let (crop_width, crop_height) = (max_x - min_x + 1, max_y - min_y + 1);
    let mut cropped = Pixmap::new(crop_width, crop_height)?;
    let destination: &mut [PremultipliedColorU8] = cropped.pixels_mut();
    for y in 0..crop_height {
        let src_start = ((min_y + y) * width + min_x) as usize;
        let dst_start = (y * crop_width) as usize;
        destination[dst_start..dst_start + crop_width as usize]
            .copy_from_slice(&pixels[src_start..src_start + crop_width as usize]);
    }
    Some(BlockRaster { pixmap: cropped, x: min_x as i32, y: min_y as i32, scale: 1.0, overflow: false })
}

/// All overlay rasters in paint order: group shadows first, then blocks in page order.
pub fn render_overlay_rasters(ctx: &mut FontContext, page: &PageRecord, editing_enabled: bool) -> Vec<BlockRaster> {
    let size = page_size(page);
    let mut rasters = render_group_shadows(ctx, page);
    for block in &page.blocks {
        if let Some(raster) = render_block(ctx, size, block, BlockRenderOptions { editing_enabled, scale: 1.0 }) {
            rasters.push(raster);
        }
    }
    rasters
}

/// Composites a page like the TypeScript renderer and returns straight RGBA.
pub fn render_page(
    ctx: &mut FontContext,
    page: &PageRecord,
    images: &PageImages<'_>,
    options: &PageRenderOptions,
) -> Option<RgbaImage> {
    let (width, height) = (page.width.max(1), page.height.max(1));
    let mut canvas = Canvas::new(width, height)?;
    let visibility = options.visibility;
    let opacity = options.opacity;

    if visibility.image
        && let Some(source) = images.source.and_then(|image| image_to_pixmap(image, width, height))
    {
        canvas.draw_pixmap_device(&source, 0, 0, opacity.image.clamp(0.0, 1.0), None);
    }
    if visibility.inpaint {
        if visibility.inpaint_result
            && let Some(result) = images.inpaint_result
        {
            let mask = if options.active_layer == ActiveLayer::InpaintResult { None } else { images.inpaint_mask };
            if let Some(layer) = masked_result_pixmap(result, mask, width, height) {
                canvas.draw_pixmap_device(
                    &layer,
                    0,
                    0,
                    (opacity.inpaint * opacity.inpaint_result).clamp(0.0, 1.0),
                    None,
                );
            }
        }
        if visibility.inpaint_mask
            && let Some(mask) = images.inpaint_mask.and_then(|image| image_to_pixmap(image, width, height))
        {
            canvas.draw_pixmap_device(&mask, 0, 0, (opacity.inpaint * opacity.inpaint_mask).clamp(0.0, 1.0), None);
        }
    }
    if visibility.overlay {
        let editing = options.active_layer == ActiveLayer::Overlay;
        let rasters = render_overlay_rasters(ctx, page, editing);
        let mut overlay = Canvas::new(width, height)?;
        for raster in &rasters {
            overlay.draw_pixmap_device(&raster.pixmap, raster.x, raster.y, 1.0, None);
        }
        canvas.draw_pixmap_device(&overlay.pixmap, 0, 0, opacity.overlay.clamp(0.0, 1.0), None);
    }
    Some(pixmap_to_image(&canvas.pixmap))
}

pub fn pixmap_to_image(pixmap: &Pixmap) -> RgbaImage {
    let mut image = RgbaImage::new(pixmap.width(), pixmap.height());
    for (out, pixel) in image.pixels_mut().zip(pixmap.pixels()) {
        let color = pixel.demultiply();
        *out = image::Rgba([color.red(), color.green(), color.blue(), color.alpha()]);
    }
    image
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_coverage_uses_luma_times_alpha() {
        assert_eq!(mask_coverage(&image::Rgba([255, 255, 255, 255])), 1.0);
        assert_eq!(mask_coverage(&image::Rgba([0, 0, 0, 255])), 0.0);
        assert!((mask_coverage(&image::Rgba([255, 255, 255, 128])) - 128.0 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn masked_result_hides_uncovered_pixels() {
        let result = RgbaImage::from_pixel(2, 1, image::Rgba([10, 20, 30, 255]));
        let mut mask = RgbaImage::from_pixel(2, 1, image::Rgba([0, 0, 0, 255]));
        mask.put_pixel(1, 0, image::Rgba([255, 255, 255, 255]));
        let pixmap = masked_result_pixmap(&result, Some(&mask), 2, 1).unwrap();
        assert_eq!(pixmap.pixels()[0].alpha(), 0);
        assert_eq!(pixmap.pixels()[1].alpha(), 255);
    }

    #[test]
    fn crop_finds_content_bounds() {
        let mut pixmap = Pixmap::new(10, 10).unwrap();
        pixmap.pixels_mut()[3 * 10 + 4] = ColorU8::from_rgba(0, 0, 0, 255).premultiply();
        pixmap.pixels_mut()[5 * 10 + 6] = ColorU8::from_rgba(0, 0, 0, 255).premultiply();
        let raster = crop_to_content(pixmap).unwrap();
        assert_eq!((raster.x, raster.y, raster.pixmap.width(), raster.pixmap.height()), (4, 3, 3, 3));
    }
}
