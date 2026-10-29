//! Writes a sample inpaint PSD for checking with other readers:
//! `cargo run -p mtt-inpaint --example psd_sample -- OUT.psd [WIDTH HEIGHT]` (a side over
//! 30000 makes a PSB).
use image::RgbaImage;
use mtt_inpaint::psd::{BACKGROUND_LAYER_NAME, ExportLayer, RESULT_LAYER_NAME, TRANSLATION_LAYER_NAME, write_psd};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("OUT.psd");
    let size = |index: usize, default: u32| args.get(index).map_or(default, |arg| arg.parse().expect("a size"));
    let (width, height) = (size(2, 64), size(3, 48));
    let source = RgbaImage::from_fn(width, height, |x, y| image::Rgba([(x * 4) as u8, (y * 5) as u8, 128, 255]));
    let mut result = RgbaImage::new(width, height);
    for y in 10..30.min(height) {
        for x in 20..40.min(width) {
            result.put_pixel(x, y, image::Rgba([255, 255, 255, 255]));
        }
    }
    let text = RgbaImage::from_pixel(width, height, image::Rgba([0, 0, 0, 80]));
    let mut composite = source.clone();
    image::imageops::overlay(&mut composite, &result, 0, 0);
    let bytes = write_psd(
        width,
        height,
        &composite,
        &[
            ExportLayer::new(BACKGROUND_LAYER_NAME, &source, false),
            ExportLayer::new(RESULT_LAYER_NAME, &result, false),
            ExportLayer::new(TRANSLATION_LAYER_NAME, &text, true),
        ],
    );
    std::fs::write(path, bytes).unwrap();
}
