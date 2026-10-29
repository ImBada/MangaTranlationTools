//! LaMa on LibTorch (MPS on macOS unless `cpu` is given), with RUNTIME the unpacked
//! runtime folder (`<data>/tools/libtorch/…`):
//! `cargo run --release -p mtt-inpaint --example lama_torch -- RUNTIME WEIGHTS bench [cpu]`
//! `… -- RUNTIME WEIGHTS run SOURCE MASK OUTPUT [cpu]` inpaints SOURCE as one window.
//! `… -- RUNTIME WEIGHTS page SOURCE MASK OUTPUT [cpu]` inpaints a page window by window.

use std::time::Instant;

use mtt_inpaint::lama_torch::TorchLama;
use mtt_torch::Device;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cpu = args.last().map(String::as_str) == Some("cpu");
    let started = Instant::now();
    mtt_torch::load(args[0].as_ref()).expect("runtime");
    let device = if !cpu && mtt_torch::mps_available() { Device::Mps } else { Device::Cpu };
    let model = TorchLama::load(args[1].as_ref(), device).expect("model");
    println!("device={:?} load={:.2}s", model.device(), started.elapsed().as_secs_f32());
    let args = &args[1..];
    match args[1].as_str() {
        "bench" => {
            let mut seed = 1u32;
            for size in [384u32, 512, 768, 1024, 1536] {
                let image = image::RgbaImage::from_fn(size, size, |_, _| {
                    seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                    let [r, g, b, _] = seed.to_le_bytes();
                    image::Rgba([r, g, b, 255])
                });
                let mask = image::GrayImage::from_fn(size, size, |x, y| {
                    let inside = (size / 4..size * 3 / 4).contains(&x) && (size / 4..size * 3 / 4).contains(&y);
                    image::Luma([if inside { 255 } else { 0 }])
                });
                model.inpaint(&image, &mask).expect("warm up");
                let started = Instant::now();
                for _ in 0..3 {
                    model.inpaint(&image, &mask).expect("inpaint");
                }
                println!("{size}x{size}: {:.3}s/inference", started.elapsed().as_secs_f32() / 3.0);
            }
        }
        "run" => {
            let source = image::open(&args[2]).expect("source").to_rgba8();
            let mask = image::open(&args[3]).expect("mask").to_luma8();
            let started = Instant::now();
            let result = model.inpaint(&source, &mask).expect("inpaint");
            println!("inpainted in {:.3}s", started.elapsed().as_secs_f32());
            result.save(&args[4]).expect("save");
        }
        "page" => {
            let source = image::open(&args[2]).expect("source").to_rgba8();
            let mask = image::open(&args[3]).expect("mask").to_luma8();
            let started = Instant::now();
            let mut windows = 0;
            let result = mtt_inpaint::windows::inpaint_by_windows(&source, &mask, |image, mask| {
                windows += 1;
                model.inpaint(image, mask)
            })
            .expect("inpaint");
            println!("page in {:.3}s ({windows} windows)", started.elapsed().as_secs_f32());
            result.save(&args[4]).expect("save");
        }
        other => panic!("unknown command {other}"),
    }
}
