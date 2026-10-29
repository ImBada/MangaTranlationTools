//! Long-lived app services: paths, library, settings and the (slow to build) font system.

use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mtt_core::library::Library;
use mtt_core::paths::AppPaths;
use mtt_core::settings::AppSettings;
use mtt_render::FontContext;

pub type SharedFonts = Arc<Mutex<FontContext>>;

/// A font family picked recently (`RECENT_FONT_FAMILY_STORAGE_KEY`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RecentFont {
    pub label: String,
    pub value: String,
}

/// Small UI state the TS app keeps in localStorage, stored as `native-ui-state.json`.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiState {
    #[serde(default)]
    pub recent_font_families: Vec<RecentFont>,
    /// Editor widgets by key (`editor::widgets`): docked or floating, and the window's
    /// last place.
    #[serde(default)]
    pub widgets: std::collections::BTreeMap<String, SavedWidget>,
    /// Order of the selection cards in the inspector (`editor::widgets::Card` keys).
    #[serde(default)]
    pub inspector_order: Vec<String>,
    /// The canvas rect the widget windows were last placed against, so they keep their
    /// distance to its edges when the app window has another size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget_canvas: Option<[f32; 4]>,
}

#[derive(Debug, Default, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedWidget {
    #[serde(default)]
    pub floating: bool,
    /// `[x, y, width, height]` of the floating window in points.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rect: Option<[f32; 4]>,
}

/// `RECENT_FONT_FAMILY_LIMIT`.
const RECENT_FONT_LIMIT: usize = 5;

/// How long the UI state has to stay unchanged before it is written (a window resize or
/// widget drag changes it every frame).
const UI_STATE_SAVE_DELAY: Duration = Duration::from_millis(500);

enum FontState {
    Loading(Receiver<FontContext>),
    Ready(SharedFonts),
    Failed,
}

pub struct Services {
    pub paths: AppPaths,
    pub library: Library,
    pub settings: AppSettings,
    /// Local URL the result report's "이동" buttons call (see `editor::report`).
    pub report_focus_url: Option<String>,
    pub ui_state: UiState,
    /// When `ui_state` last changed without being written.
    ui_state_changed: Option<Instant>,
    fonts: FontState,
}

impl Services {
    pub fn new(ctx: &eframe::egui::Context, paths: AppPaths) -> Self {
        let settings = AppSettings::load(&paths.settings_path).unwrap_or_else(|error| {
            log::warn!("settings load failed: {error}");
            AppSettings::default()
        });
        crate::textures::set_smooth_magnification(settings.zoom_smoothing);
        let (sender, receiver) = channel();
        let repaint = ctx.clone();
        std::thread::Builder::new()
            .name("mtt-fonts".into())
            .spawn(move || {
                let started = std::time::Instant::now();
                let fonts = FontContext::new();
                log::info!("font system ready in {:?} ({} families)", started.elapsed(), fonts.system_families().len());
                let _ = sender.send(fonts);
                repaint.request_repaint();
            })
            .expect("spawn font loader");
        let ui_state = std::fs::read(paths.data_root.join("native-ui-state.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        Self {
            library: Library::new(&paths.library_dir),
            paths,
            settings,
            report_focus_url: None,
            ui_state,
            ui_state_changed: None,
            fonts: FontState::Loading(receiver),
        }
    }

    /// The font system once it has finished loading.
    pub fn fonts(&mut self) -> Option<SharedFonts> {
        if let FontState::Loading(receiver) = &self.fonts {
            self.fonts = match receiver.try_recv() {
                Ok(fonts) => FontState::Ready(Arc::new(Mutex::new(fonts))),
                Err(TryRecvError::Empty) => return None,
                Err(TryRecvError::Disconnected) => FontState::Failed,
            };
        }
        match &self.fonts {
            FontState::Ready(fonts) => {
                // A job that panicked mid-render poisons the lock; the font context is still
                // usable, and the canvas would otherwise never re-render a block again.
                if fonts.is_poisoned() {
                    log::warn!("recovering the font lock after a panic");
                    fonts.clear_poison();
                }
                Some(Arc::clone(fonts))
            }
            _ => None,
        }
    }

    pub fn fonts_failed(&self) -> bool {
        matches!(self.fonts, FontState::Failed)
    }

    /// `selectFontFamily`: moves the font to the front of the recent list (max 5).
    pub fn remember_font(&mut self, label: &str, value: &str) {
        let recent = &mut self.ui_state.recent_font_families;
        recent.retain(|font| font.value != value);
        recent.insert(0, RecentFont { label: label.to_owned(), value: value.to_owned() });
        recent.truncate(RECENT_FONT_LIMIT);
        self.save_ui_state();
    }

    /// Marks the UI state for saving; `tick_ui_state` writes it once it settles.
    pub fn save_ui_state(&mut self) {
        self.ui_state_changed = Some(Instant::now());
    }

    /// Writes the UI state when it has not changed for a moment and no drag is going on.
    /// Call once per frame.
    pub fn tick_ui_state(&mut self, ctx: &eframe::egui::Context) {
        let Some(changed) = self.ui_state_changed else { return };
        // The release of a drag runs a frame of its own.
        if ctx.input(|input| input.pointer.any_down()) {
            return;
        }
        let left = UI_STATE_SAVE_DELAY.saturating_sub(changed.elapsed());
        if left.is_zero() {
            self.flush_ui_state();
        } else {
            ctx.request_repaint_after(left);
        }
    }

    /// Writes pending UI state now (on exit).
    pub fn flush_ui_state(&mut self) {
        if self.ui_state_changed.take().is_none() {
            return;
        }
        // Not worth a disk flush: losing the last window placement on a crash is harmless.
        let path = self.paths.data_root.join("native-ui-state.json");
        if let Err(error) = mtt_core::json_io::write_json_replace(&path, &self.ui_state) {
            log::warn!("ui state save failed: {error}");
        }
    }

    pub fn save_settings(&mut self, settings: AppSettings) -> Result<(), String> {
        match settings.save(&self.paths.settings_path) {
            Ok(saved) => {
                crate::textures::set_smooth_magnification(saved.zoom_smoothing);
                self.settings = saved;
                Ok(())
            }
            Err(error) => Err(error.to_string()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panic_while_holding_the_fonts_does_not_lock_them_for_good() {
        let dir = tempfile::tempdir().unwrap();
        let mut services = Services::new(&eframe::egui::Context::default(), AppPaths::from_data_root(dir.path()));
        let started = Instant::now();
        let fonts = loop {
            if let Some(fonts) = services.fonts() {
                break fonts;
            }
            assert!(started.elapsed() < Duration::from_secs(20), "font system loads");
            std::thread::sleep(Duration::from_millis(20));
        };
        let held = Arc::clone(&fonts);
        let job = std::thread::spawn(move || {
            let _fonts = held.lock().unwrap();
            panic!("render failed");
        });
        assert!(job.join().is_err());
        assert!(fonts.is_poisoned());
        assert!(services.fonts().unwrap().lock().is_ok(), "the next frame gets the fonts again");
    }
}
