//! Generates synthetic manga-like pages (panels, balloons, vertical Japanese text) for
//! end-to-end tests: `cargo run -p mtt-render --example make_test_pages -- OUT_DIR [COUNT]`.
//! Also writes `expected.json` with the balloon boxes in 0..1000 space.

use mtt_core::model::{BBox, RenderTextDirection, TextStyle, TranslationBlock};
use mtt_render::block::{BlockRenderOptions, render_block};
use mtt_render::{FontContext, Size};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

const WIDTH: u32 = 1000;
const HEIGHT: u32 = 1400;
const LINES: [&str; 6] = ["こんにちは", "今日はいい天気", "どこへ行くの", "ちょっと待って", "ありがとう", "またね"];

fn main() {
    let mut args = std::env::args().skip(1);
    let out = std::path::PathBuf::from(args.next().expect("OUT_DIR"));
    let count: usize = args.next().and_then(|v| v.parse().ok()).unwrap_or(3);
    std::fs::create_dir_all(&out).unwrap();
    let mut fonts = FontContext::new();
    let mut expected = Vec::new();
    for page in 0..count {
        let mut pixmap = Pixmap::new(WIDTH, HEIGHT).unwrap();
        pixmap.fill(Color::WHITE);
        let black = paint(Color::BLACK);
        // Two panels with a screentone-like gray band.
        for (y, h) in [(40.0, 640.0), (720.0, 640.0)] {
            let rect = Rect::from_xywh(40.0, y, 920.0, h).unwrap();
            let gray = paint(Color::from_rgba8(225, 225, 225, 255));
            pixmap.fill_rect(
                Rect::from_xywh(40.0, y + h - 140.0, 920.0, 140.0).unwrap(),
                &gray,
                Transform::identity(),
                None,
            );
            pixmap.stroke_path(
                &PathBuilder::from_rect(rect),
                &black,
                &Stroke { width: 6.0, ..Stroke::default() },
                Transform::identity(),
                None,
            );
        }
        let mut boxes = Vec::new();
        for (index, (cx, cy)) in
            [(260.0, 300.0), (720.0, 260.0), (300.0, 1000.0), (700.0, 1050.0)].into_iter().enumerate()
        {
            let (rx, ry) = (120.0, 170.0);
            let balloon =
                PathBuilder::from_oval(Rect::from_xywh(cx - rx, cy - ry, rx * 2.0, ry * 2.0).unwrap()).unwrap();
            pixmap.fill_path(&balloon, &paint(Color::WHITE), FillRule::Winding, Transform::identity(), None);
            pixmap.stroke_path(
                &balloon,
                &black,
                &Stroke { width: 4.0, ..Stroke::default() },
                Transform::identity(),
                None,
            );
            let text = LINES[(page + index) % LINES.len()];
            let bbox = BBox::new(
                f64::from(cx - 70.0) / 1000.0 * 1000.0,
                f64::from(cy - 130.0) / 1400.0 * 1000.0,
                140.0,
                260.0 / 1400.0 * 1000.0,
            );
            let block = TranslationBlock {
                id: format!("jp-{index}"),
                bbox,
                translated_text: text.into(),
                render_direction: RenderTextDirection::Vertical,
                style: TextStyle {
                    font_family: Some("\"Hiragino Mincho ProN\", \"Yu Mincho\", serif".into()),
                    font_size_px: Some(30.0),
                    line_height: Some(1.1),
                    font_weight: Some(400.0),
                    text_color: Some("#000000".into()),
                    auto_fit_text: Some(false),
                    ..TextStyle::default()
                },
                ..TranslationBlock::default()
            };
            if let Some(raster) = render_block(
                &mut fonts,
                Size::new(f64::from(WIDTH), f64::from(HEIGHT)),
                &block,
                BlockRenderOptions::default(),
            ) {
                pixmap.draw_pixmap(
                    raster.x,
                    raster.y,
                    raster.pixmap.as_ref(),
                    &tiny_skia::PixmapPaint::default(),
                    Transform::identity(),
                    None,
                );
            }
            boxes.push(serde_json::json!({ "x": bbox.x, "y": bbox.y, "w": bbox.w, "h": bbox.h, "jp": text }));
        }
        let path = out.join(format!("{:03}.png", page + 1));
        pixmap.save_png(&path).unwrap();
        expected.push(serde_json::json!({ "page": path.file_name().unwrap().to_string_lossy(), "boxes": boxes }));
    }
    std::fs::write(out.join("expected.json"), serde_json::to_string_pretty(&expected).unwrap()).unwrap();
    println!("wrote {count} pages to {}", out.display());
}

fn paint(color: Color) -> Paint<'static> {
    Paint { shader: tiny_skia::Shader::SolidColor(color), anti_alias: true, ..Paint::default() }
}
