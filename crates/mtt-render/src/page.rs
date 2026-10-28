//! Page compositing (`renderPageToPngDataUrl`, group drop shadows).

use image::{GrayImage, RgbaImage};
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
    /// Inpaint mask as coverage 0..=255 (the editor's masks are binary; see `coverage_mask`
    /// for stored RGBA masks).
    pub inpaint_mask: Option<&'a GrayImage>,
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

/// A stored RGBA mask as the coverage `PageImages::inpaint_mask` takes.
pub fn coverage_mask(image: &RgbaImage) -> GrayImage {
    let pixels = image.pixels().map(|pixel| (mask_coverage(pixel) * 255.0).round() as u8).collect();
    GrayImage::from_raw(image.width(), image.height(), pixels).expect("one byte per pixel")
}

/// For each of `destination` columns (or rows), the nearest-neighbour source one.
fn nearest_map(source: u32, destination: u32) -> Vec<usize> {
    (0..destination).map(|index| (u64::from(index) * u64::from(source) / u64::from(destination)) as usize).collect()
}

/// Fills a `width`×`height` pixmap from an image of any size (nearest neighbour: masks are
/// drawn without smoothing). `pixel` maps a source pixel index to the output pixel.
fn fill_nearest(
    pixmap: &mut Pixmap,
    (source_width, source_height): (u32, u32),
    mut pixel: impl FnMut(usize) -> PremultipliedColorU8,
) {
    let width = pixmap.width();
    let xs = nearest_map(source_width, width);
    let ys = nearest_map(source_height, pixmap.height());
    for (row, source_y) in pixmap.pixels_mut().chunks_exact_mut(width as usize).zip(ys) {
        let row_start = source_y * source_width as usize;
        for (out, source_x) in row.iter_mut().zip(&xs) {
            *out = pixel(row_start + source_x);
        }
    }
}

/// Converts a straight-alpha image to a premultiplied pixmap of the given size.
pub fn image_to_pixmap(image: &RgbaImage, width: u32, height: u32) -> Option<Pixmap> {
    let mut pixmap = Pixmap::new(width, height)?;
    let data = image.as_raw();
    if image.dimensions() == (width, height) {
        for (out, p) in pixmap.pixels_mut().iter_mut().zip(data.as_chunks::<4>().0.iter()) {
            *out = ColorU8::from_rgba(p[0], p[1], p[2], p[3]).premultiply();
        }
    } else {
        fill_nearest(&mut pixmap, image.dimensions(), |index| {
            let p = &data[index * 4..index * 4 + 4];
            ColorU8::from_rgba(p[0], p[1], p[2], p[3]).premultiply()
        });
    }
    Some(pixmap)
}

/// An opaque black-to-white pixmap of the mask (how the mask layer shows).
fn mask_to_pixmap(mask: &GrayImage, width: u32, height: u32) -> Option<Pixmap> {
    let mut pixmap = Pixmap::new(width, height)?;
    let data = mask.as_raw();
    fill_nearest(&mut pixmap, mask.dimensions(), |index| {
        let value = data[index];
        ColorU8::from_rgba(value, value, value, 255).premultiply()
    });
    Some(pixmap)
}

/// Inpaint result with alpha multiplied by mask coverage (`applyInpaintMaskPixelsToLayer`),
/// in one pass straight from the straight-alpha result.
pub fn masked_result_pixmap(result: &RgbaImage, mask: Option<&GrayImage>, width: u32, height: u32) -> Option<Pixmap> {
    let Some(mask) = mask else {
        return image_to_pixmap(result, width, height);
    };
    let mut pixmap = Pixmap::new(width, height)?;
    let (result_data, mask_data) = (result.as_raw(), mask.as_raw());
    let mask_xs = nearest_map(mask.width(), width);
    let mask_ys = nearest_map(mask.height(), height);
    let result_xs = nearest_map(result.width(), width);
    let result_ys = nearest_map(result.height(), height);
    let rows = pixmap.pixels_mut().chunks_exact_mut(width as usize);
    for ((row, mask_y), result_y) in rows.zip(mask_ys).zip(result_ys) {
        let mask_row = &mask_data[mask_y * mask.width() as usize..][..mask.width() as usize];
        let result_row = &result_data[result_y * result.width() as usize * 4..][..result.width() as usize * 4];
        for ((out, mask_x), result_x) in row.iter_mut().zip(&mask_xs).zip(&result_xs) {
            let coverage = u32::from(mask_row[*mask_x]);
            let p = &result_row[result_x * 4..result_x * 4 + 4];
            // Rounds like `(alpha * coverage).round()`.
            let alpha = if coverage == 255 { p[3] } else { ((u32::from(p[3]) * coverage + 127) / 255) as u8 };
            *out = ColorU8::from_rgba(p[0], p[1], p[2], alpha).premultiply();
        }
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
        let members: Vec<BlockRaster> = members
            .into_iter()
            .filter_map(|block| render_block(ctx, size, block, BlockRenderOptions::default()))
            .collect();
        let angle = settings.angle_deg.to_radians();
        let offset = (angle.cos() * settings.distance_px, angle.sin() * settings.distance_px);
        if let Some(raster) = group_shadow(&members, (page.width.max(1), page.height.max(1)), &settings, offset) {
            rasters.push(raster);
        }
    }
    rasters
}

/// One group's drop shadow from its members' rasters. Works on the members' area padded by
/// the blur's reach rather than the whole page: the box blurs have finite support, so the
/// pixels are the same as blurring a page-sized layer.
fn group_shadow(
    members: &[BlockRaster],
    (page_width, page_height): (u32, u32),
    settings: &mtt_core::groups::DropShadowSettings,
    (dx, dy): (f64, f64),
) -> Option<BlockRaster> {
    let left = members.iter().map(|raster| raster.x).min()?;
    let top = members.iter().map(|raster| raster.y).min()?;
    let right = members.iter().map(|raster| raster.x + raster.pixmap.width() as i32).max()?;
    let bottom = members.iter().map(|raster| raster.y + raster.pixmap.height() as i32).max()?;
    let pad = (settings.blur_px.max(0.0) * 3.0).ceil() as i32 + 2;
    let clamp_x = |x: i32| x.clamp(0, page_width as i32);
    let clamp_y = |y: i32| y.clamp(0, page_height as i32);
    let (region_x, region_y) = (clamp_x(left - pad), clamp_y(top - pad));
    let (region_right, region_bottom) = (clamp_x(right + pad), clamp_y(bottom + pad));
    let mut layer = Canvas::new((region_right - region_x).max(1) as u32, (region_bottom - region_y).max(1) as u32)?;
    for raster in members {
        layer.draw_pixmap_device(&raster.pixmap, raster.x - region_x, raster.y - region_y, 1.0, None);
    }
    let color = hex_color(&settings.color, 1.0).to_color_u8();
    for pixel in layer.pixmap.pixels_mut() {
        let alpha = pixel.alpha();
        *pixel = ColorU8::from_rgba(color.red(), color.green(), color.blue(), alpha).premultiply();
    }
    gaussian_blur(&mut layer.pixmap, settings.blur_px);
    // The shifted region, clipped to the page like the page-sized layer was.
    let out_x = clamp_x(region_x + dx.floor() as i32);
    let out_y = clamp_y(region_y + dy.floor() as i32);
    let out_right = clamp_x(region_right + dx.ceil() as i32);
    let out_bottom = clamp_y(region_bottom + dy.ceil() as i32);
    if out_right <= out_x || out_bottom <= out_y {
        return None;
    }
    let mut shadow = Pixmap::new((out_right - out_x) as u32, (out_bottom - out_y) as u32)?;
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
            (f64::from(region_x - out_x) + dx) as f32,
            (f64::from(region_y - out_y) + dy) as f32,
        ),
        None,
    );
    let mut raster = crop_to_content(shadow)?;
    raster.x += out_x;
    raster.y += out_y;
    Some(raster)
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
    let visibility = options.visibility;
    let opacity = options.opacity;

    let source = images.source.filter(|_| visibility.image).and_then(|image| image_to_pixmap(image, width, height));
    let image_opacity = opacity.image.clamp(0.0, 1.0);
    let mut canvas = match source {
        // Drawn at full opacity onto an empty canvas, the source is the canvas.
        Some(source) if image_opacity >= 1.0 => Canvas::from_pixmap(source),
        source => {
            let mut canvas = Canvas::new(width, height)?;
            if let Some(source) = source {
                canvas.draw_pixmap_device(&source, 0, 0, image_opacity, None);
            }
            canvas
        }
    };
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
            && let Some(mask) = images.inpaint_mask.and_then(|mask| mask_to_pixmap(mask, width, height))
        {
            canvas.draw_pixmap_device(&mask, 0, 0, (opacity.inpaint * opacity.inpaint_mask).clamp(0.0, 1.0), None);
        }
    }
    if visibility.overlay {
        let editing = options.active_layer == ActiveLayer::Overlay;
        let rasters = render_overlay_rasters(ctx, page, editing);
        let overlay_opacity = opacity.overlay.clamp(0.0, 1.0);
        if overlay_opacity >= 1.0 {
            // Source-over is associative: no page-sized layer needed at full opacity.
            for raster in &rasters {
                canvas.draw_pixmap_device(&raster.pixmap, raster.x, raster.y, 1.0, None);
            }
        } else {
            let mut overlay = Canvas::new(width, height)?;
            for raster in &rasters {
                overlay.draw_pixmap_device(&raster.pixmap, raster.x, raster.y, 1.0, None);
            }
            canvas.draw_pixmap_device(&overlay.pixmap, 0, 0, overlay_opacity, None);
        }
    }
    Some(pixmap_to_image(&canvas.pixmap))
}

pub fn pixmap_to_image(pixmap: &Pixmap) -> RgbaImage {
    RgbaImage::from_raw(pixmap.width(), pixmap.height(), crate::canvas::to_straight_rgba(pixmap))
        .expect("four bytes per pixel")
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
        let result = RgbaImage::from_pixel(3, 1, image::Rgba([10, 20, 30, 255]));
        let mut mask = coverage_mask(&RgbaImage::from_pixel(3, 1, image::Rgba([0, 0, 0, 255])));
        mask.put_pixel(1, 0, image::Luma([255]));
        mask.put_pixel(2, 0, image::Luma([128]));
        let pixmap = masked_result_pixmap(&result, Some(&mask), 3, 1).unwrap();
        assert_eq!(pixmap.pixels()[0].alpha(), 0);
        assert_eq!(pixmap.pixels()[1].alpha(), 255);
        assert_eq!(pixmap.pixels()[2].alpha(), 128);
    }

    #[test]
    fn images_of_another_size_are_sampled_nearest() {
        let mut image = RgbaImage::new(2, 2);
        image.put_pixel(1, 1, image::Rgba([255, 0, 0, 255]));
        let pixmap = image_to_pixmap(&image, 4, 4).unwrap();
        let red = |x: u32, y: u32| pixmap.pixels()[(y * 4 + x) as usize].red();
        assert_eq!((red(1, 1), red(2, 2), red(3, 3), red(3, 1)), (0, 255, 255, 0));
    }

    #[test]
    fn group_shadow_matches_a_page_sized_layer() {
        let (page_width, page_height) = (120u32, 90u32);
        let raster = |x: i32, y: i32, width: u32, height: u32| {
            let mut pixmap = Pixmap::new(width, height).unwrap();
            for (index, pixel) in pixmap.pixels_mut().iter_mut().enumerate() {
                *pixel = ColorU8::from_rgba(10, 20, 30, (index * 41 % 256) as u8).premultiply();
            }
            BlockRaster { pixmap, x, y, scale: 1.0, overflow: false }
        };
        // One member hangs off the left edge, so page clipping matters too.
        let members = [raster(-6, 30, 20, 14), raster(50, 10, 30, 25)];
        let settings = mtt_core::groups::DropShadowSettings {
            color: "#3366ff".into(),
            opacity: 0.8,
            blur_px: 3.5,
            distance_px: 7.3,
            angle_deg: 35.0,
        };
        let angle = settings.angle_deg.to_radians();
        let offset = (angle.cos() * settings.distance_px, angle.sin() * settings.distance_px);
        // The page-sized version this replaced.
        let mut layer = Canvas::new(page_width, page_height).unwrap();
        for member in &members {
            layer.draw_pixmap_device(&member.pixmap, member.x, member.y, 1.0, None);
        }
        let color = hex_color(&settings.color, 1.0).to_color_u8();
        for pixel in layer.pixmap.pixels_mut() {
            *pixel = ColorU8::from_rgba(color.red(), color.green(), color.blue(), pixel.alpha()).premultiply();
        }
        gaussian_blur(&mut layer.pixmap, settings.blur_px);
        let mut page_shadow = Pixmap::new(page_width, page_height).unwrap();
        let paint = tiny_skia::PixmapPaint {
            opacity: settings.opacity as f32,
            quality: tiny_skia::FilterQuality::Bilinear,
            ..Default::default()
        };
        let shift = tiny_skia::Transform::from_translate(offset.0 as f32, offset.1 as f32);
        page_shadow.draw_pixmap(0, 0, layer.pixmap.as_ref(), &paint, shift, None);
        let expected = crop_to_content(page_shadow).unwrap();

        let shadow = group_shadow(&members, (page_width, page_height), &settings, offset).unwrap();
        assert_eq!((shadow.x, shadow.y), (expected.x, expected.y));
        assert_eq!(shadow.pixmap.data(), expected.pixmap.data());
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
