//! `cargo run --release -p mtt-inpaint --example inpaint_file -- SOURCE MASK OUT_PREFIX [lama]`
//! Runs the local fill engine (and LaMa when requested) and writes composited previews.

use mtt_core::model::{InpaintEngine, InpaintSettings};
use mtt_core::paths::AppPaths;
use mtt_inpaint::lama::Lama;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let source = image::open(&args[0]).expect("source").to_rgba8();
    let mask_rgba = image::open(&args[1]).expect("mask").to_rgba8();
    let mask = image::GrayImage::from_fn(source.width(), source.height(), |x, y| {
        let p = mask_rgba.get_pixel(x.min(mask_rgba.width() - 1), y.min(mask_rgba.height() - 1));
        let coverage = f32::from(p[3]) / 255.0
            * (f32::from(p[0]) * 0.299 + f32::from(p[1]) * 0.587 + f32::from(p[2]) * 0.114)
            / 255.0;
        image::Luma([if coverage > 0.0 { 255 } else { 0 }])
    });
    let mut engines = vec![(InpaintEngine::LocalFillFallback, None)];
    if args.get(3).map(String::as_str) == Some("lama") {
        engines.push((InpaintEngine::Lama, Lama::detect(&AppPaths::resolve())));
    }
    for (engine, lama) in engines {
        let started = std::time::Instant::now();
        match mtt_inpaint::run_inpaint(&source, &mask, engine, &InpaintSettings::default(), lama.as_ref()) {
            Ok(result) => {
                let mut composite = source.clone();
                image::imageops::overlay(&mut composite, &result, 0, 0);
                let path = format!("{}-{}.png", args[2], engine.as_str());
                composite.save(&path).unwrap();
                println!("{} in {:?} -> {path}", engine.as_str(), started.elapsed());
            }
            Err(error) => println!("{} failed: {error}", engine.as_str()),
        }
    }
}
