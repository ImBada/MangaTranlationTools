//! Page and layer images read by their content rather than their extension: downloaded
//! manga often has WebP or PNG data in a `.jpg` file, and the TS app imported such files
//! under the name they came with.

use std::path::Path;

use image::{DynamicImage, ImageFormat, ImageReader, ImageResult};

/// `image::open`, picking the decoder from the file's first bytes (the extension only
/// when they don't match a known format).
pub fn open_image(path: &Path) -> ImageResult<DynamicImage> {
    ImageReader::open(path)?.with_guessed_format()?.decode()
}

/// The format the file holds and its size, read from the header only.
pub fn probe_image(path: &Path) -> ImageResult<(Option<ImageFormat>, (u32, u32))> {
    let reader = ImageReader::open(path)?.with_guessed_format()?;
    let format = reader.format();
    Ok((format, reader.into_dimensions()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mislabeled_files_open_by_content() {
        let dir = tempfile::tempdir().unwrap();
        for format in [ImageFormat::Png, ImageFormat::WebP] {
            let path = dir.path().join("page.jpg");
            image::RgbaImage::from_pixel(3, 2, image::Rgba([1, 2, 3, 255])).save_with_format(&path, format).unwrap();
            assert!(image::open(&path).is_err(), "the extension alone picks the JPEG decoder");
            assert_eq!(open_image(&path).unwrap().to_rgba8().get_pixel(2, 1).0, [1, 2, 3, 255]);
            assert_eq!(probe_image(&path).unwrap(), (Some(format), (3, 2)));
        }
        std::fs::write(dir.path().join("broken.png"), b"<html>not found</html>").unwrap();
        assert!(probe_image(&dir.path().join("broken.png")).is_err());
    }
}
