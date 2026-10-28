#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! MangaTranslationTools native editor (egui).

mod app;
mod automation;
mod dnd;
mod library_view;
mod logging;
mod services;
mod settings_view;
mod shortcuts;
mod single_instance;
mod textures;
mod theme;
mod update_check;
mod worker;

mod editor;

use eframe::egui;
use mtt_core::paths::AppPaths;

fn main() -> eframe::Result {
    let paths = AppPaths::resolve();
    if let Err(error) = paths.ensure_writable_dirs() {
        startup_error(&format!("데이터 폴더를 만들지 못했습니다: {error}"));
        return Ok(());
    }
    // `MANGA_TRANSLATOR_LOG_PATH` overrides the log file like the TS app.
    let log_path = std::env::var("MANGA_TRANSLATOR_LOG_PATH")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| paths.logs_dir.join("native-app.log"));
    logging::init(&log_path);
    log::info!("starting, data root {}", paths.data_root.display());

    let instance = single_instance::acquire(&paths.data_root);
    let listener = match instance {
        single_instance::Instance::Secondary => {
            log::info!("another instance is running; asked it to come to the front");
            return Ok(());
        }
        single_instance::Instance::Primary { ref listener, .. } => listener.as_ref().and_then(|l| l.try_clone().ok()),
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("MangaTranslationTools")
            .with_inner_size([1440.0, 960.0])
            .with_min_inner_size([1100.0, 720.0]),
        ..eframe::NativeOptions::default()
    };
    let result = eframe::run_native(
        "MangaTranslationTools",
        options,
        Box::new(move |creation| {
            install_ui_fonts(&creation.egui_ctx);
            theme::apply(&creation.egui_ctx);
            if let Some(listener) = listener {
                let ctx = creation.egui_ctx.clone();
                single_instance::serve(listener, move || {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    ctx.request_repaint();
                });
            }
            Ok(Box::new(app::MttApp::new(&creation.egui_ctx, paths)))
        }),
    );
    drop(instance);
    if let Err(error) = &result {
        log::error!("startup failed: {error}");
        startup_error(&error.to_string());
    }
    result
}

/// `showStartupError`.
fn startup_error(message: &str) {
    eprintln!("MangaTranslationTools 시작 실패: {message}");
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("MangaTranslationTools 시작 실패")
        .set_description(message)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

/// `(regular file, face index, semibold file, face index)` per platform.
type UiFontFiles = (&'static str, u32, &'static str, u32);

/// Makes a Hangul-capable system font the primary UI font so Korean and Latin text share
/// one face; other scripts still fall back through eframe's system font provider. Its
/// semibold weight, when present, becomes `theme::SEMIBOLD_FAMILY` for titles.
fn install_ui_fonts(ctx: &egui::Context) {
    let candidates: &[UiFontFiles] = if cfg!(target_os = "macos") {
        let file = "/System/Library/Fonts/AppleSDGothicNeo.ttc";
        &[(file, 0, file, 4)]
    } else if cfg!(target_os = "windows") {
        &[("C:\\Windows\\Fonts\\malgun.ttf", 0, "C:\\Windows\\Fonts\\malgunbd.ttf", 0)]
    } else {
        &[
            (
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                1,
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc",
                1,
            ),
            (
                "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
                1,
                "/usr/share/fonts/noto-cjk/NotoSansCJK-Bold.ttc",
                1,
            ),
        ]
    };
    use egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
    // Loaded once for the whole run; the macOS collection holds both weights, so one copy
    // serves both faces.
    let read = |path: &str| std::fs::read(path).ok().map(|bytes| &*Box::leak(bytes.into_boxed_slice()));
    for &(path, index, bold_path, bold_index) in candidates {
        let Some(bytes) = read(path) else {
            continue;
        };
        let mut data = egui::FontData::from_static(bytes);
        data.index = index;
        let families = [egui::FontFamily::Proportional, egui::FontFamily::Monospace]
            .into_iter()
            .map(|family| InsertFontFamily { family, priority: FontPriority::Highest })
            .collect();
        ctx.add_font(FontInsert { name: "ui-hangul".into(), data, families });
        let bold_bytes = if bold_path == path { Some(bytes) } else { read(bold_path) };
        if let Some(bold_bytes) = bold_bytes {
            let semibold = egui::FontFamily::Name(theme::SEMIBOLD_FAMILY.into());
            let mut data = egui::FontData::from_static(bold_bytes);
            data.index = bold_index;
            let family = InsertFontFamily { family: semibold.clone(), priority: FontPriority::Highest };
            ctx.add_font(FontInsert { name: "ui-hangul-semibold".into(), data, families: vec![family] });
            // Glyphs the semibold face lacks come from the regular one.
            let mut data = egui::FontData::from_static(bytes);
            data.index = index;
            let family = InsertFontFamily { family: semibold, priority: FontPriority::Lowest };
            ctx.add_font(FontInsert { name: "ui-hangul-semibold-fallback".into(), data, families: vec![family] });
            theme::set_semibold_ready();
        }
        return;
    }
    log::warn!("no Hangul UI font found; relying on system font fallback");
}
