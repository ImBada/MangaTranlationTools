//! Writes a sample inpaint PSD for checking with other readers.
use image::RgbaImage;
use mtt_inpaint::psd::{BACKGROUND_LAYER_NAME, ExportLayer, RESULT_LAYER_NAME, TRANSLATION_LAYER_NAME, write_psd};

fn main() {
    let path = std::env::args().nth(1).expect("OUT.psd");
    let source = RgbaImage::from_fn(64, 48, |x, y| image::Rgba([(x * 4) as u8, (y * 5) as u8, 128, 255]));
    let mut result = RgbaImage::new(64, 48);
    for y in 10..30 {
        for x in 20..40 {
            result.put_pixel(x, y, image::Rgba([255, 255, 255, 255]));
        }
    }
    let text = RgbaImage::from_pixel(64, 48, image::Rgba([0, 0, 0, 80]));
    let mut composite = source.clone();
    image::imageops::overlay(&mut composite, &result, 0, 0);
    let bytes = write_psd(
        64,
        48,
        &composite,
        &[
            ExportLayer::new(BACKGROUND_LAYER_NAME, &source, false),
            ExportLayer::new(RESULT_LAYER_NAME, &result, false),
            ExportLayer::new(TRANSLATION_LAYER_NAME, &text, true),
        ],
    );
    std::fs::write(&path, bytes).unwrap();
}
