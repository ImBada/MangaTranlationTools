//! Whole-layer and range operations on the inpaint layers, plus the PSD file workflow
//! (`useInpaintSelectionActions.ts`, `useInpaintPsdActions.ts`, `inpaintRequests.ts`).

use std::path::{Path, PathBuf};
use std::time::Instant;

use eframe::egui;
use mtt_core::paths::sanitize_file_basename;

use super::assets::{LoadedPage, PageAssets};
use super::history::{Entry, LayerKind, LayerPatch, read_rect};
use super::{Editor, Layer};
use crate::services::Services;

/// `-last-export.json` beside the inpaint layers (`ExportedInpaintPsdMeta`).
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportedPsdMeta {
    exported_at: String,
    filename: String,
    path: String,
}

/// Cached PSD file state of the current page (read from disk when the page changes).
#[derive(Default)]
pub struct PsdFiles {
    page_id: String,
    pub last_export: Option<PathBuf>,
    pub last_import: Option<(PathBuf, String)>,
}

impl Editor {
    pub fn current_mask_has_pixels(&self) -> bool {
        self.loaded_page().is_some_and(LoadedPage::mask_has_pixels)
    }

    pub fn current_result_exists(&self) -> bool {
        self.loaded_page().is_some_and(LoadedPage::result_has_pixels)
    }

    /// The range in page pixels clamped to the image: `(x, y, width, height)`.
    fn range_pixels(&self, width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
        let range = self.range?;
        let x = (range.min.x.floor().max(0.0) as u32).min(width);
        let y = (range.min.y.floor().max(0.0) as u32).min(height);
        let right = (range.max.x.ceil().max(0.0) as u32).min(width);
        let bottom = (range.max.y.ceil().max(0.0) as u32).min(height);
        (right > x && bottom > y).then(|| (x, y, right - x, bottom - y))
    }

    /// Applies `edit` to one layer of the current page as a single undo step. Returns
    /// whether any pixel changed.
    fn edit_current_layer(
        &mut self,
        ctx: &egui::Context,
        kind: LayerKind,
        rect: Option<(u32, u32, u32, u32)>,
        edit: impl FnOnce(&mut [u8], u32, u32, (u32, u32, u32, u32)),
    ) -> bool {
        let Some(page_id) = self.page().map(|page| page.id.clone()) else { return false };
        let Some(PageAssets::Ready(loaded)) = self.assets.get_mut(&page_id) else { return false };
        let (width, height) = (loaded.width(), loaded.height());
        let rect = rect.unwrap_or((0, 0, width, height));
        let channels = match kind {
            LayerKind::Mask => 1,
            LayerKind::Result => 4,
        };
        let data: &mut [u8] = match kind {
            LayerKind::Mask => loaded.ensure_mask(),
            LayerKind::Result => loaded.ensure_result(),
        };
        let before = read_rect(data, width, channels, rect);
        edit(data, width, channels, rect);
        let after = read_rect(data, width, channels, rect);
        if before == after {
            return false;
        }
        loaded.refresh_layer_region(ctx, &page_id, rect.0, rect.1, rect.2, rect.3);
        let patch = LayerPatch::new(rect, &before, &after);
        self.history.push(Entry::Layer { page_id: page_id.clone(), layer: kind, patch });
        self.layer_dirty.insert((page_id, kind), Instant::now());
        true
    }

    /// "인페인트 마스크 비우기" / "인페인트 결과 비우기".
    pub fn clear_layer(&mut self, ctx: &egui::Context, kind: LayerKind) {
        self.edit_current_layer(ctx, kind, None, |data, _, _, _| data.fill(0));
    }

    /// Result layer "선택 범위 채우기" (brush color) and Delete inside the range.
    pub fn fill_range_on_result(&mut self, ctx: &egui::Context, fill: bool) -> bool {
        let Some((width, height)) = self.loaded_page().map(|page| (page.width(), page.height())) else { return false };
        let Some(rect) = self.range_pixels(width, height) else { return false };
        let [r, g, b] = self.result_brush.color;
        let changed = self.edit_current_layer(ctx, LayerKind::Result, Some(rect), |data, image_width, _, rect| {
            for y in rect.1..rect.1 + rect.3 {
                for x in rect.0..rect.0 + rect.2 {
                    let offset = ((y * image_width + x) * 4) as usize;
                    data[offset..offset + 4].copy_from_slice(&if fill { [r, g, b, 255] } else { [0; 4] });
                }
            }
        });
        if changed {
            self.status = if fill {
                "선택 범위를 인페인트 결과 색상으로 채웠습니다.".into()
            } else {
                "선택 범위의 인페인트 결과를 지웠습니다.".into()
            };
        }
        changed
    }

    // ---- PSD ---------------------------------------------------------------------------

    fn psd_paths(&self, services: &Services, page_id: &str) -> (PathBuf, PathBuf) {
        let dir = services.library.chapter_dir(&self.chapter.work_id, &self.chapter.id).join("inpaint");
        let base = sanitize_file_basename(page_id, page_id);
        (dir.join(format!("{base}-last-export.json")), dir.join(format!("{base}-last-import.psd")))
    }

    /// Re-reads `-last-export.json` and `-last-import.psd` when the page changed.
    fn refresh_psd_files(&mut self, services: &Services, force: bool) {
        let Some(page_id) = self.page().map(|page| page.id.clone()) else {
            self.psd_files = PsdFiles::default();
            return;
        };
        if !force && self.psd_files.page_id == page_id {
            return;
        }
        let (meta_path, import_path) = self.psd_paths(services, &page_id);
        let last_export = std::fs::read(&meta_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<ExportedPsdMeta>(&bytes).ok())
            .filter(|meta| {
                Path::new(&meta.path).file_name().is_some_and(|name| name.to_string_lossy() == meta.filename)
                    && !meta.exported_at.is_empty()
            })
            .map(|meta| PathBuf::from(meta.path))
            .filter(|path| path.is_file());
        let last_import = std::fs::metadata(&import_path).ok().and_then(|meta| meta.modified().ok()).map(|modified| {
            let local: chrono::DateTime<chrono::Local> = modified.into();
            (
                import_path.clone(),
                local.format("%y. %-m. %-d. %p %-I:%M").to_string().replace("AM", "오전").replace("PM", "오후"),
            )
        });
        self.psd_files = PsdFiles { page_id, last_export, last_import };
    }

    pub fn last_exported_psd(&mut self, services: &Services) -> Option<PathBuf> {
        self.refresh_psd_files(services, false);
        self.psd_files.last_export.clone()
    }

    pub fn last_imported_psd(&mut self, services: &Services) -> Option<(PathBuf, String)> {
        self.refresh_psd_files(services, false);
        self.psd_files.last_import.clone()
    }

    /// "PSD 내보내기": writes `{page}-inpaint.psd` straight into Downloads (` (n)` when the
    /// name is taken) and remembers it for quick load.
    pub fn export_psd_to_downloads(&mut self, services: &mut Services) {
        let Some(page) = self.page().cloned() else { return };
        let filename = format!("{}-inpaint.psd", sanitize_psd_basename(&page.name, &page.id));
        let result = (|| -> Result<PathBuf, String> {
            let bytes = self.render_psd(services)?;
            let path = write_unique_download(&filename, &bytes)?;
            let (meta_path, _) = self.psd_paths(services, &page.id);
            let meta = ExportedPsdMeta {
                exported_at: mtt_core::time::now_iso(),
                filename: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                path: path.to_string_lossy().into_owned(),
            };
            mtt_core::json_io::write_json_atomic(&meta_path, &meta).map_err(|error| error.to_string())?;
            Ok(path)
        })();
        self.status = match result {
            Ok(path) => format!(
                "인페인트 PSD를 내보냈습니다: {}",
                path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()
            ),
            Err(error) => error,
        };
        self.refresh_psd_files(services, true);
    }

    /// "PSD 가져오기": imports the layers and keeps a copy as `-last-import.psd`.
    pub fn import_psd_file(&mut self, ctx: &egui::Context, services: &Services, path: &Path) {
        self.status = match self.import_psd_and_keep(ctx, services, path) {
            Ok(()) => "PSD에서 인페인트 결과와 마스크를 가져왔습니다.".into(),
            Err(error) => error,
        };
    }

    /// "퀵로드": re-imports the PSD exported last from this page.
    pub fn quick_load_psd(&mut self, ctx: &egui::Context, services: &Services, path: &Path) {
        if !path.is_file() {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            self.status = format!("마지막으로 내보낸 PSD 파일을 찾지 못했습니다: {name}");
            return;
        }
        self.status = match self.import_psd_and_keep(ctx, services, path) {
            Ok(()) => "마지막으로 내보낸 PSD를 퀵로드했습니다.".into(),
            Err(error) => error,
        };
    }

    fn import_psd_and_keep(&mut self, ctx: &egui::Context, services: &Services, path: &Path) -> Result<(), String> {
        self.import_psd(ctx, path)?;
        let Some(page_id) = self.page().map(|page| page.id.clone()) else { return Ok(()) };
        let (_, import_path) = self.psd_paths(services, &page_id);
        if let Some(parent) = import_path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::copy(path, &import_path).map_err(|error| error.to_string())?;
        self.refresh_psd_files(services, true);
        if self.layer != Layer::InpaintResult {
            self.set_layer(Layer::InpaintResult);
        }
        Ok(())
    }

    /// "마지막으로 사용한 PSD 내려받기" (saved as `last-imported-inpaint.psd`).
    pub fn download_last_imported_psd(&mut self, path: &Path) {
        let result = std::fs::read(path)
            .map_err(|error| error.to_string())
            .and_then(|bytes| write_unique_download("last-imported-inpaint.psd", &bytes));
        self.status = match result {
            Ok(_) => "마지막으로 사용한 PSD를 내려받았습니다.".into(),
            Err(error) => format!("마지막으로 사용한 PSD 내려받기에 실패했습니다: {error}"),
        };
    }
}

/// `sanitizePsdFileBasename`.
fn sanitize_psd_basename(value: &str, fallback: &str) -> String {
    let without_extension = match value.rfind('.') {
        Some(index) if index > 0 && !value[index + 1..].contains(['/', '\\']) => &value[..index],
        _ => value,
    };
    let cleaned: String = without_extension
        .chars()
        .map(|c| {
            if matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || (c as u32) < 0x20 { '_' } else { c }
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() { fallback.to_owned() } else { trimmed.to_owned() }
}

/// `MANGA_TRANSLATOR_DOWNLOADS_DIR`, else `~/Downloads`.
fn downloads_dir() -> Option<PathBuf> {
    std::env::var("MANGA_TRANSLATOR_DOWNLOADS_DIR")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|home| PathBuf::from(home).join("Downloads"))
        })
}

/// `writeUniqueDownloadFile`: writes into the downloads folder without overwriting.
pub fn write_unique_download(filename: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    let dir = downloads_dir().ok_or("다운로드 폴더를 찾을 수 없습니다.")?;
    write_unique_file(&dir, filename, bytes)
}

/// Adds ` (n)` before the extension until the name is free.
fn write_unique_file(dir: &Path, filename: &str, bytes: &[u8]) -> Result<PathBuf, String> {
    use std::io::Write;
    std::fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    let name = Path::new(filename).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let (stem, extension) = match name.rfind('.') {
        Some(index) if index > 0 => (name[..index].to_owned(), name[index..].to_owned()),
        _ => (name.clone(), String::new()),
    };
    for index in 0.. {
        let candidate = dir.join(if index == 0 { name.clone() } else { format!("{stem} ({index}){extension}") });
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&candidate) {
            Ok(mut file) => {
                if let Err(error) = file.write_all(bytes) {
                    let _ = std::fs::remove_file(&candidate);
                    return Err(error.to_string());
                }
                return Ok(candidate);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
    unreachable!("the loop only ends by returning")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn psd_names_and_unique_downloads() {
        assert_eq!(sanitize_psd_basename("001.png", "p"), "001");
        assert_eq!(sanitize_psd_basename("a:b?.jpg", "p"), "a_b_");
        assert_eq!(sanitize_psd_basename("  ", "p"), "p");
        let dir = tempfile::tempdir().unwrap();
        let first = write_unique_file(dir.path(), "x-inpaint.psd", b"1").unwrap();
        let second = write_unique_file(dir.path(), "x-inpaint.psd", b"2").unwrap();
        assert_eq!(first.file_name().unwrap(), "x-inpaint.psd");
        assert_eq!(second.file_name().unwrap(), "x-inpaint (1).psd");
    }
}
