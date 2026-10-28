//! Renders library pages with the native renderer and compares them with the PNGs the
//! TypeScript app saved in `renders/`. Read-only for the library; output goes to `--out`.
//!
//! `cargo run --release -p mtt-render --example render_compare -- [--data DIR] [--out DIR] [--limit N]`

use std::path::PathBuf;
use std::time::Instant;

use mtt_core::library::{Library, PageLayer};
use mtt_core::paths::AppPaths;
use mtt_render::FontContext;
use mtt_render::page::{PageImages, PageRenderOptions, render_page};

fn main() {
    let mut args = std::env::args().skip(1);
    let mut data = None;
    let mut out = PathBuf::from("/tmp/mtt-render-compare");
    let mut limit = usize::MAX;
    let mut only_with_blocks = true;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--data" => data = args.next().map(PathBuf::from),
            "--out" => out = args.next().map(PathBuf::from).expect("--out DIR"),
            "--limit" => limit = args.next().and_then(|v| v.parse().ok()).expect("--limit N"),
            "--all" => only_with_blocks = false,
            _ => panic!("unknown argument {arg}"),
        }
    }
    let paths = data.map(AppPaths::from_data_root).unwrap_or_else(AppPaths::resolve);
    let library = Library::new(&paths.library_dir);
    std::fs::create_dir_all(&out).unwrap();

    let started = Instant::now();
    let mut ctx = FontContext::new();
    println!("font system loaded in {:?} ({} families)", started.elapsed(), ctx.system_families().len());

    let mut compared = 0;
    let mut total_diff = 0.0;
    for work in library.list().unwrap() {
        for summary in &work.chapters {
            let chapter = library.open_chapter(&summary.id).unwrap();
            for page in chapter.pages.iter().filter(|p| !only_with_blocks || !p.blocks.is_empty()) {
                if compared >= limit {
                    break;
                }
                let reference = library.render_output_path(&chapter, page);
                if !reference.is_file() {
                    continue;
                }
                let load = |layer| {
                    library
                        .resolve_page_layer_path(&chapter, page, layer)
                        .and_then(|p| image::open(p).ok())
                        .map(|i| i.to_rgba8())
                };
                let source = load(PageLayer::Source);
                let mask = load(PageLayer::InpaintMask).map(|mask| mtt_render::page::coverage_mask(&mask));
                let result = load(PageLayer::InpaintResult);
                let started = Instant::now();
                let images = PageImages {
                    source: source.as_ref(),
                    inpaint_mask: mask.as_ref(),
                    inpaint_result: result.as_ref(),
                };
                let rendered = render_page(&mut ctx, page, &images, &PageRenderOptions::OUTPUT).unwrap();
                let elapsed = started.elapsed();
                let expected = image::open(&reference).unwrap().to_rgba8();
                let name =
                    format!("{}-{}", &chapter.id[..8], mtt_core::paths::sanitize_file_basename(&page.name, &page.id));
                rendered.save(out.join(format!("{name}-native.png"))).unwrap();
                let (diff_image, mean) = diff(&rendered, &expected);
                diff_image.save(out.join(format!("{name}-diff.png"))).unwrap();
                std::fs::copy(&reference, out.join(format!("{name}-ts.png"))).unwrap();
                println!("{name}: {} blocks, render {:?}, mean abs diff {:.3}", page.blocks.len(), elapsed, mean);
                compared += 1;
                total_diff += mean;
            }
        }
    }
    if compared > 0 {
        println!("compared {compared} pages, average mean abs diff {:.3}", total_diff / compared as f64);
    }
}

/// Per-pixel max channel difference image and mean absolute channel difference.
fn diff(a: &image::RgbaImage, b: &image::RgbaImage) -> (image::RgbaImage, f64) {
    let (w, h) = (a.width().min(b.width()), a.height().min(b.height()));
    let mut out = image::RgbaImage::new(w, h);
    let mut sum = 0u64;
    for y in 0..h {
        for x in 0..w {
            let (pa, pb) = (a.get_pixel(x, y), b.get_pixel(x, y));
            let d = (0..3).map(|c| pa[c].abs_diff(pb[c])).max().unwrap();
            sum += (0..3).map(|c| u64::from(pa[c].abs_diff(pb[c]))).sum::<u64>();
            let v = 255 - d;
            out.put_pixel(x, y, image::Rgba([255, v, v, 255]));
        }
    }
    (out, sum as f64 / (f64::from(w) * f64::from(h) * 3.0))
}
