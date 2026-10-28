//! Long-lived app services: paths, library, settings and the (slow to build) font system.

use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::sync::{Arc, Mutex};

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
    fonts: FontState,
}

impl Services {
    pub fn new(ctx: &eframe::egui::Context, paths: AppPaths) -> Self {
        let settings = AppSettings::load(&paths.settings_path).unwrap_or_else(|error| {
            log::warn!("settings load failed: {error}");
            AppSettings::default()
        });
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
            FontState::Ready(fonts) => Some(Arc::clone(fonts)),
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

    pub fn save_ui_state(&self) {
        if let Err(error) =
            mtt_core::json_io::write_json_atomic(&self.paths.data_root.join("native-ui-state.json"), &self.ui_state)
        {
            log::warn!("ui state save failed: {error}");
        }
    }

    pub fn save_settings(&mut self, settings: AppSettings) -> Result<(), String> {
        match settings.save(&self.paths.settings_path) {
            Ok(saved) => {
                self.settings = saved;
                Ok(())
            }
            Err(error) => Err(error.to_string()),
        }
    }
}
