//! Page images sent to the model: the original file plus, in accuracy mode, a grayscale
//! high-contrast assist view (`prepareImageVariants`, `buildEnhancedVariant`).

use std::path::Path;

use base64::Engine;
use mtt_core::image_io::open_image;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VariantRole {
    Original,
    Enhanced,
    /// Close-up of the region with this id.
    Region(u32),
}

/// Context around a region close-up, as a share of the region's size (at least
/// `REGION_MIN_PAD_PX`), and the longest close-up side.
const REGION_PAD_SHARE: f64 = 0.15;
const REGION_MIN_PAD_PX: f64 = 8.0;
const REGION_MAX_LONG_SIDE: u32 = 1024;

#[derive(Debug, Clone)]
pub struct ImageVariant {
    pub role: VariantRole,
    pub data_url: String,
}

pub const ENHANCED_MAX_LONG_SIDE: u32 = 1900;
pub const ENHANCED_CONTRAST: f32 = 1.35;

/// The MIME type to send image `bytes` under as they are, by their header (the extension
/// of `path` when it isn't a known format); `None` for formats that go re-encoded.
fn mime_of(bytes: &[u8], path: &Path) -> Option<&'static str> {
    let format = image::guess_format(bytes).ok().or_else(|| image::ImageFormat::from_path(path).ok());
    match format? {
        image::ImageFormat::Png => Some("image/png"),
        image::ImageFormat::Jpeg => Some("image/jpeg"),
        image::ImageFormat::Gif => Some("image/gif"),
        _ => None,
    }
}

fn data_url(mime: &str, bytes: &[u8]) -> String {
    format!("data:{mime};base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
}

fn png_bytes(image: &image::DynamicImage) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    image.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(bytes)
}

/// Original file as a data URL, typed by what the file holds (downloaded pages are often
/// WebP or PNG named `.jpg`). WebP and other formats go re-encoded as PNG, because several
/// OpenAI-compatible servers reject WebP.
pub fn original_variant(path: &Path) -> Result<ImageVariant, String> {
    let unreadable = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let bytes = std::fs::read(path).map_err(|e| unreadable(&e))?;
    let data_url = match mime_of(&bytes, path) {
        Some(mime) => self::data_url(mime, &bytes),
        None => {
            let image = image::load_from_memory(&bytes).map_err(|e| unreadable(&e))?;
            self::data_url("image/png", &png_bytes(&image)?)
        }
    };
    Ok(ImageVariant { role: VariantRole::Original, data_url })
}

/// Grayscale, contrast-stretched, at most 1900px on the long side, flattened on white.
pub fn enhanced_variant(path: &Path) -> Result<ImageVariant, String> {
    let image = open_image(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let (width, height) = (image.width(), image.height());
    let long_side = width.max(height);
    let image = if long_side > ENHANCED_MAX_LONG_SIDE {
        let scale = ENHANCED_MAX_LONG_SIDE as f32 / long_side as f32;
        image.resize(
            ((width as f32 * scale).round() as u32).max(1),
            ((height as f32 * scale).round() as u32).max(1),
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        image
    };
    let rgba = image.to_rgba8();
    let translation = (1.0 - ENHANCED_CONTRAST) / 2.0 * 255.0;
    let gray = image::GrayImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let p = rgba.get_pixel(x, y).0;
        let alpha = f32::from(p[3]) / 255.0;
        // Flatten on white, then luminance, then the linear contrast stretch.
        let channel = |v: u8| f32::from(v) * alpha + 255.0 * (1.0 - alpha);
        let luma = channel(p[0]) * 0.2126 + channel(p[1]) * 0.7152 + channel(p[2]) * 0.0722;
        image::Luma([(luma * ENHANCED_CONTRAST + translation).round().clamp(0.0, 255.0) as u8])
    });
    Ok(ImageVariant {
        role: VariantRole::Enhanced,
        data_url: data_url("image/png", &png_bytes(&image::DynamicImage::ImageLuma8(gray))?),
    })
}

/// Variants for one request; a failing enhanced view is logged and skipped like the TS app.
pub fn prepare_variants(path: &Path, include_enhanced: bool) -> Result<Vec<ImageVariant>, String> {
    let mut variants = vec![original_variant(path)?];
    if include_enhanced {
        match enhanced_variant(path) {
            Ok(variant) => variants.push(variant),
            Err(error) => log::warn!("enhanced variant unavailable, using original only: {error}"),
        }
    }
    Ok(variants)
}

/// Close-up of `bbox` (0..1000 space) on `page` with a little context, as PNG.
pub fn region_variant(page: &image::RgbaImage, id: u32, bbox: mtt_core::model::BBox) -> Result<ImageVariant, String> {
    let (width, height) = (f64::from(page.width()), f64::from(page.height()));
    let px = mtt_core::geometry::bbox_to_pixels(bbox, width, height);
    let pad = (px.w.max(px.h) * REGION_PAD_SHARE).max(REGION_MIN_PAD_PX);
    let left = (px.x - pad).floor().clamp(0.0, width - 1.0);
    let top = (px.y - pad).floor().clamp(0.0, height - 1.0);
    let right = (px.x + px.w + pad).ceil().clamp(left + 1.0, width);
    let bottom = (px.y + px.h + pad).ceil().clamp(top + 1.0, height);
    let crop = image::imageops::crop_imm(page, left as u32, top as u32, (right - left) as u32, (bottom - top) as u32)
        .to_image();
    let mut crop = image::DynamicImage::ImageRgba8(crop);
    if crop.width().max(crop.height()) > REGION_MAX_LONG_SIDE {
        crop = crop.resize(REGION_MAX_LONG_SIDE, REGION_MAX_LONG_SIDE, image::imageops::FilterType::Lanczos3);
    }
    Ok(ImageVariant { role: VariantRole::Region(id), data_url: data_url("image/png", &png_bytes(&crop)?) })
}

pub fn variant_caption(role: VariantRole, index: usize) -> String {
    match role {
        VariantRole::Region(id) => format!("Image {}: close-up of region {id}.", index + 1),
        VariantRole::Enhanced => {
            format!("Image {}: the same full manga page rendered as grayscale/high-contrast assist view.", index + 1)
        }
        VariantRole::Original => format!("Image {}: the original full manga page.", index + 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variants_are_data_urls_and_enhanced_is_downscaled_gray() {
        let dir = tempfile_dir();
        let path = dir.join("page.png");
        image::RgbaImage::from_fn(3800, 100, |x, _| image::Rgba([if x % 2 == 0 { 0 } else { 255 }, 0, 0, 255]))
            .save(&path)
            .unwrap();
        let variants = prepare_variants(&path, true).unwrap();
        assert_eq!(variants.len(), 2);
        assert!(variants[0].data_url.starts_with("data:image/png;base64,"));
        let encoded = variants[1].data_url.trim_start_matches("data:image/png;base64,");
        let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (1900, 50));
        assert!(matches!(decoded, image::DynamicImage::ImageLuma8(_)));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn originals_are_typed_by_their_content() {
        let dir = tempfile_dir();
        let path = dir.join("mislabeled.jpg");
        let page = image::RgbaImage::from_pixel(4, 3, image::Rgba([10, 20, 30, 255]));
        for (format, mime) in [
            (image::ImageFormat::Png, "image/png"),
            (image::ImageFormat::WebP, "image/png"),
            (image::ImageFormat::Bmp, "image/png"),
            (image::ImageFormat::Jpeg, "image/jpeg"),
        ] {
            image::DynamicImage::ImageRgba8(page.clone()).to_rgb8().save_with_format(&path, format).unwrap();
            let url = original_variant(&path).unwrap().data_url;
            assert!(url.starts_with(&format!("data:{mime};base64,")), "{format:?}: {}", &url[..30]);
            let bytes = base64::engine::general_purpose::STANDARD.decode(url.split_once(',').unwrap().1).unwrap();
            assert_eq!(
                image::load_from_memory(&bytes).map(|image| (image.width(), image.height())).unwrap(),
                (4, 3),
                "{format:?}"
            );
        }
        std::fs::remove_dir_all(dir).ok();
    }

    fn tempfile_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mtt-translate-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
}
