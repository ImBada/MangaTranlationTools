//! Prints native text measurements for `{"font": css-font, "t": text}` JSON lines on stdin,
//! for comparison with Chrome's `measureText`.

use std::io::BufRead;

use mtt_render::FontContext;

fn main() {
    let mut ctx = FontContext::new();
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        let value: serde_json::Value = serde_json::from_str(&line).unwrap();
        let font = value["font"].as_str().unwrap();
        let text = value["t"].as_str().unwrap();
        let (weight, rest) = font.split_once(' ').unwrap();
        let (size, family) = rest.split_once("px ").unwrap();
        let size: f64 = size.parse().unwrap();
        let spec = ctx.resolve_spec(family, weight.parse().unwrap());
        let run = ctx.shape(text, &spec, &[]);
        let metrics = ctx.vertical_metrics(&spec);
        let ink = ctx.ink_bounds(text, &spec);
        println!(
            "{:?} w={:.3} chrome={} | topToBase={:.3} chrome={} | inkAsc={:.3} chrome={} | spec={:?}",
            text,
            f64::from(run.advance_em) * size,
            value["w"],
            f64::from(metrics.top_to_baseline_em) * size,
            value["topToBase"],
            ink.map(|i| f64::from(i.ascent_em) * size).unwrap_or(f64::NAN),
            value["inkAsc"],
            spec.family,
        );
    }
}
