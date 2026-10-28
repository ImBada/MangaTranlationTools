//! Data directory layout (`src/server/appPaths.ts`).

use std::path::{Path, PathBuf};

pub const DATA_DIR_ENV: &str = "MANGA_TRANSLATOR_DATA_DIR";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppPaths {
    pub data_root: PathBuf,
    pub settings_path: PathBuf,
    pub library_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub log_file: PathBuf,
    pub models_dir: PathBuf,
    pub tools_dir: PathBuf,
    pub font_preset_backups_dir: PathBuf,
}

impl AppPaths {
    /// Uses `MANGA_TRANSLATOR_DATA_DIR` when set, otherwise `~/Documents/MangaTranslationTools`
    /// (the same default as the Node server, so both apps share one library).
    pub fn resolve() -> Self {
        let from_env =
            std::env::var(DATA_DIR_ENV).ok().map(|value| value.trim().to_owned()).filter(|value| !value.is_empty());
        Self::from_data_root(from_env.map(PathBuf::from).unwrap_or_else(default_data_root))
    }

    pub fn from_data_root(data_root: impl Into<PathBuf>) -> Self {
        let data_root = data_root.into();
        let logs_dir = data_root.join("logs");
        Self {
            settings_path: data_root.join("settings.json"),
            library_dir: data_root.join("library"),
            log_file: logs_dir.join("app.log"),
            logs_dir,
            models_dir: data_root.join("models"),
            tools_dir: data_root.join("tools"),
            font_preset_backups_dir: data_root.join("font-preset-backups"),
            data_root,
        }
    }

    pub fn ensure_writable_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.library_dir)?;
        std::fs::create_dir_all(&self.logs_dir)
    }
}

fn default_data_root() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join("Documents").join("MangaTranslationTools")
}

/// `sanitizeFileBasename`: file stem with characters that are invalid on Windows replaced.
pub fn sanitize_file_basename(value: &str, fallback: &str) -> String {
    let stem = Path::new(value).file_stem().and_then(|stem| stem.to_str()).unwrap_or("");
    let sanitized: String = stem
        .chars()
        .map(|ch| {
            if matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || (ch as u32) < 0x20 {
                '_'
            } else {
                ch
            }
        })
        .collect();
    let trimmed = sanitized.trim();
    if trimmed.is_empty() { fallback.to_owned() } else { trimmed.to_owned() }
}

pub fn is_supported_image_path(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "png" | "jpg" | "jpeg" | "webp"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_node_server() {
        let paths = AppPaths::from_data_root("/data");
        assert_eq!(paths.library_dir, PathBuf::from("/data/library"));
        assert_eq!(paths.log_file, PathBuf::from("/data/logs/app.log"));
        assert_eq!(paths.settings_path, PathBuf::from("/data/settings.json"));
    }

    #[test]
    fn sanitize_basename() {
        assert_eq!(sanitize_file_basename("dir/a:b?.jpg", "x"), "a_b_");
        assert_eq!(sanitize_file_basename(" .png", "fallback"), "fallback");
        assert_eq!(sanitize_file_basename("001.jpg", "x"), "001");
    }
}
