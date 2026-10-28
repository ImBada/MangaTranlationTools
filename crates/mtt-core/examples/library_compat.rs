//! Read-only compatibility check against an existing library.
//!
//! `cargo run -p mtt-core --example library_compat [data-dir]`
//!
//! Parses every `chapter.json`/`work.json`, re-serializes it and reports any value that would
//! change or disappear. Nothing is written.

use std::time::Instant;

use mtt_core::json_io::read_json;
use mtt_core::library::Library;
use mtt_core::model::{Chapter, Work};
use mtt_core::paths::AppPaths;
use serde_json::Value;

fn main() {
    let paths = std::env::args().nth(1).map(AppPaths::from_data_root).unwrap_or_else(AppPaths::resolve);
    let library = Library::new(&paths.library_dir);
    let mut failures = 0usize;
    let mut chapters = 0usize;
    let mut blocks = 0usize;

    let index = library.read_index().expect("index.json");
    for work_id in &index.work_order {
        let work_path = library.work_file(work_id);
        failures += check::<Work>(&work_path);
        let Some(work) = library.read_work(work_id).expect("work.json") else {
            continue;
        };
        for chapter_id in &work.chapter_order {
            let path = library.chapter_file(work_id, chapter_id);
            let started = Instant::now();
            failures += check::<Chapter>(&path);
            let chapter: Chapter = read_json(&path).expect("chapter");
            chapters += 1;
            blocks += chapter.pages.iter().map(|page| page.blocks.len()).sum::<usize>();
            let missing_images = chapter
                .pages
                .iter()
                .filter(|page| {
                    library.resolve_page_layer_path(&chapter, page, mtt_core::library::PageLayer::Source).is_none()
                })
                .count();
            println!(
                "{} / {}: {} pages, {} missing source images, parsed+checked in {:?}",
                work.title,
                chapter.title,
                chapter.pages.len(),
                missing_images,
                started.elapsed()
            );
        }
    }
    println!("checked {chapters} chapters, {blocks} blocks, {failures} differences");
    if failures > 0 {
        std::process::exit(1);
    }
}

fn check<T: serde::de::DeserializeOwned + serde::Serialize>(path: &std::path::Path) -> usize {
    let original: Value = read_json(path).expect("raw json");
    let typed: T = serde_json::from_value(original.clone()).expect("typed parse");
    let round_trip = serde_json::to_value(&typed).expect("serialize");
    let mut diffs = Vec::new();
    diff("", &normalize(&original), &normalize(&round_trip), &mut diffs);
    for line in diffs.iter().take(20) {
        println!("  DIFF {}: {line}", path.display());
    }
    diffs.len()
}

/// Drops `null` object members and turns numbers into f64 so `24` == `24.0`.
fn normalize(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            Value::Object(map.iter().filter(|(_, v)| !v.is_null()).map(|(k, v)| (k.clone(), normalize(v))).collect())
        }
        Value::Array(items) => Value::Array(items.iter().map(normalize).collect()),
        Value::Number(number) => serde_json::json!(number.as_f64()),
        other => other.clone(),
    }
}

fn diff(path: &str, left: &Value, right: &Value, out: &mut Vec<String>) {
    match (left, right) {
        (Value::Object(a), Value::Object(b)) => {
            for (key, value) in a {
                match b.get(key) {
                    Some(other) => diff(&format!("{path}.{key}"), value, other, out),
                    None => out.push(format!("{path}.{key} missing after round trip")),
                }
            }
            for key in b.keys().filter(|key| !a.contains_key(*key)) {
                out.push(format!("{path}.{key} added by round trip: {}", b[key]));
            }
        }
        (Value::Array(a), Value::Array(b)) if a.len() == b.len() => {
            for (index, (x, y)) in a.iter().zip(b).enumerate() {
                diff(&format!("{path}[{index}]"), x, y, out);
            }
        }
        _ if left != right => out.push(format!("{path}: {left} -> {right}")),
        _ => {}
    }
}
