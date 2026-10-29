//! Whole-layer and range operations on the inpaint layers, plus the PSD file workflow
//! (`useInpaintSelectionActions.ts`, `useInpaintPsdActions.ts`, `inpaintRequests.ts`).

use std::path::{Path, PathBuf};
use std::time::Instant;

use eframe::egui;
use image::RgbaImage;
use mtt_core::geometry::resolve_block_rotation_deg;
use mtt_core::model::{PageRecord, RenderTextDirection, TextAlign, TranslationBlock};
use mtt_core::paths::sanitize_file_basename;
use mtt_inpaint::psd_text::{Justification, TextLayer};
use mtt_render::canvas::hex_color;
use mtt_render::layout::synthetic_italic_skew_x;
use mtt_render::{BlockRenderOptions, BlockText, FontContext};

use super::Editor;
use super::assets::{LoadedPage, PageAssets};
use super::history::{Entry, LayerKind, LayerPatch, read_rect};
use crate::app::AppEvent;
use crate::services::Services;
use crate::worker::Worker;

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

/// `range` in page pixels clamped to the image: `(x, y, width, height)`.
pub(super) fn range_pixels(range: egui::Rect, width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
    let x = (range.min.x.floor().max(0.0) as u32).min(width);
    let y = (range.min.y.floor().max(0.0) as u32).min(height);
    let right = (range.max.x.ceil().max(0.0) as u32).min(width);
    let bottom = (range.max.y.ceil().max(0.0) as u32).min(height);
    (right > x && bottom > y).then(|| (x, y, right - x, bottom - y))
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
        range_pixels(self.range?, width, height)
    }

    /// Where brushes may paint: inside the range while one is shown, like Photoshop.
    pub(super) fn paint_clip(&self, width: u32, height: u32) -> Option<(u32, u32, u32, u32)> {
        if self.mode() != super::Mode::Inpaint {
            return None;
        }
        self.range_pixels(width, height)
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
    pub(super) fn refresh_psd_files(&mut self, services: &Services, force: bool) {
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

    /// "PSD 내보내기": writes `{page}-inpaint.psd` (`.psb` for pages over 30000px) straight
    /// into Downloads (` (n)` when the name is taken) and remembers it for quick load. Built
    /// and written on the worker.
    pub fn export_psd_to_downloads(&mut self, services: &mut Services, worker: &Worker<AppEvent>) {
        let Some(page) = self.page().cloned() else { return };
        let basename = format!("{}-inpaint", sanitize_psd_basename(&page.name, &page.id));
        let job = match self.psd_export_job(services) {
            Ok(job) => job,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        let (meta_path, _) = self.psd_paths(services, &page.id);
        self.page_export_running = true;
        self.status = "PSD를 만드는 중…".into();
        let chapter_id = self.chapter.id.clone();
        worker.spawn(move || {
            let result = crate::worker::catch_panic(job).and_then(|bytes| bytes).and_then(|bytes| {
                let extension = if mtt_inpaint::psd::is_psb(&bytes) { "psb" } else { "psd" };
                let path = write_unique_download(&format!("{basename}.{extension}"), &bytes)?;
                let meta = ExportedPsdMeta {
                    exported_at: mtt_core::time::now_iso(),
                    filename: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                    path: path.to_string_lossy().into_owned(),
                };
                mtt_core::json_io::write_json_atomic(&meta_path, &meta).map_err(|error| error.to_string())?;
                Ok(path)
            });
            AppEvent::EditorJob { chapter_id, job: super::EditorJob::PsdExported(result) }
        });
    }

    /// "PSD 가져오기": imports the layers and keeps a copy as `-last-import.psd`.
    pub fn import_psd_file(&mut self, services: &Services, worker: &Worker<AppEvent>, path: &Path) {
        self.import_psd_and_keep(services, worker, path, false);
    }

    /// "퀵로드": re-imports the PSD exported last from this page.
    pub fn quick_load_psd(&mut self, services: &Services, worker: &Worker<AppEvent>, path: &Path) {
        if !path.is_file() {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            self.status = format!("마지막으로 내보낸 PSD 파일을 찾지 못했습니다: {name}");
            return;
        }
        self.import_psd_and_keep(services, worker, path, true);
    }

    fn import_psd_and_keep(&mut self, services: &Services, worker: &Worker<AppEvent>, path: &Path, quick: bool) {
        let Some(page_id) = self.page().map(|page| page.id.clone()) else { return };
        let (_, keep_as) = self.psd_paths(services, &page_id);
        self.start_psd_import(worker, path, keep_as, quick);
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
pub(super) fn sanitize_psd_basename(value: &str, fallback: &str) -> String {
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

/// One layer of the PSD's editable text group: a block's text laid out like the page
/// render, over the block's own raster.
pub(super) struct PsdText {
    pub name: String,
    pub raster: RgbaImage,
    pub left: i32,
    pub top: i32,
    pub text: TextLayer,
}

/// Text layers of the page's visible blocks with text, bottom to top.
pub(super) fn psd_text_layers(fonts: &mut FontContext, page: &PageRecord) -> Vec<PsdText> {
    let size = mtt_render::page::page_size(page);
    page.blocks
        .iter()
        .filter(|block| !block.display_text().trim().is_empty())
        .filter_map(|block| {
            let text = mtt_render::block_text(fonts, size, block)?;
            let raster = mtt_render::render_block(fonts, size, block, BlockRenderOptions::default())?;
            Some(PsdText {
                name: text_layer_name(&text.lines),
                raster: mtt_render::page::pixmap_to_image(&raster.pixmap),
                left: raster.x,
                top: raster.y,
                text: psd_text_layer(block, &text),
            })
        })
        .collect()
}

/// Photoshop names text layers after their text.
fn text_layer_name(lines: &[String]) -> String {
    const MAX_CHARS: usize = 30;
    let text = lines.iter().flat_map(|line| line.split_whitespace()).collect::<Vec<_>>().join(" ");
    if text.chars().count() <= MAX_CHARS {
        text
    } else {
        format!("{}…", text.chars().take(MAX_CHARS).collect::<String>())
    }
}

/// Photoshop paragraph text for `block`: the drawn lines, in a box that puts them where the
/// render does. Box text is top-aligned and justified within the box, so the box starts at
/// the first line and has the lines' column at its justified edge; it is as wide as the
/// block's text area (wider when the lines are, so they never rewrap) and reaches its
/// bottom. Outlines, shadows, screentone and filters are not text properties in PSD.
fn psd_text_layer(block: &TranslationBlock, text: &BlockText) -> TextLayer {
    let layout = &text.layout;
    let (inner_left, inner_bottom) = (layout.inner.left, layout.inner.top + layout.inner.height);
    let (lines, font_size) = (text.text_rect, text.font_size_px);
    let vertical = block.render_direction == RenderTextDirection::Vertical;
    let (left, width, justification) = if vertical {
        // Columns run leftward from the box's right edge.
        let right = lines.left + lines.width;
        let left = inner_left.min(right - text.line_height_px);
        (left, right - left, Justification::Left)
    } else {
        let width = layout.inner.width.max(lines.width + font_size);
        match block.text_align {
            TextAlign::Left => (lines.left, width, Justification::Left),
            TextAlign::Center => (lines.left + (lines.width - width) / 2.0, width, Justification::Center),
            TextAlign::Right => (lines.left + lines.width - width, width, Justification::Right),
        }
    };
    let top = lines.top;
    // The render rotates about the render rect's center c, which moves the box's corner p
    // to c + R(p - c) = p + (I - R)(c - p) (exactly p unrotated).
    let (sin, cos) = resolve_block_rotation_deg(block).to_radians().sin_cos();
    let (wx, wy) =
        (layout.rect.left + layout.rect.width / 2.0 - left, layout.rect.top + layout.rect.height / 2.0 - top);
    // Vertical characters are an em apart in Photoshop and a line height in the render.
    let spacing =
        if vertical { text.line_height_px - font_size + text.letter_spacing_px } else { text.letter_spacing_px };
    let color = hex_color(block.text_color(), 1.0).to_color_u8();
    TextLayer {
        text: text.lines.join("\n"),
        font: text.font_name.clone(),
        font_size,
        faux_bold: text.synthetic_bold,
        faux_italic: synthetic_italic_skew_x(block) != 0.0,
        leading: text.line_height_px,
        tracking: (spacing / font_size * 1000.0).round() as i32,
        color: [color.red(), color.green(), color.blue(), 255],
        justification,
        vertical,
        box_width: width,
        box_height: (inner_bottom - top).max(lines.height + font_size),
        transform: [cos, sin, -sin, cos, left + (1.0 - cos) * wx + sin * wy, top + (1.0 - cos) * wy - sin * wx],
    }
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

    #[test]
    fn psd_text_layers_follow_the_render() {
        use mtt_core::model::BBox;
        let mut fonts = FontContext::new();
        let block = TranslationBlock {
            id: "b".into(),
            bbox: BBox::new(100.0, 100.0, 300.0, 200.0),
            translated_text: "첫 줄은 조금 길고\n둘째 줄".into(),
            text_align: TextAlign::Left,
            style: mtt_core::model::TextStyle { text_color: Some("#336699".into()), ..Default::default() },
            ..TranslationBlock::default()
        };
        let hidden = TranslationBlock { render_direction: RenderTextDirection::Hidden, ..block.clone() };
        let page =
            PageRecord { width: 1000, height: 1000, blocks: vec![block.clone(), hidden], ..PageRecord::default() };
        let size = mtt_render::page::page_size(&page);
        let layers = psd_text_layers(&mut fonts, &page);
        assert_eq!(layers.len(), 1, "hidden blocks have no text layer");
        let raster = mtt_render::render_block(&mut fonts, size, &block, BlockRenderOptions::default()).unwrap();
        assert_eq!(
            (layers[0].left, layers[0].top, layers[0].raster.width()),
            (raster.x, raster.y, raster.pixmap.width())
        );

        // Left-aligned: the box starts at the first line's em box.
        let drawn = mtt_render::block_text(&mut fonts, size, &block).unwrap();
        let layer = &layers[0].text;
        assert_eq!(layer.text, drawn.lines.join("\n"));
        assert_eq!(layer.font, drawn.font_name);
        assert_eq!((layer.font_size, layer.leading), (drawn.font_size_px, drawn.line_height_px));
        assert_eq!(layer.color, [0x33, 0x66, 0x99, 255]);
        assert_eq!((layer.justification, layer.vertical), (Justification::Left, false));
        assert_eq!(layer.transform, [1.0, 0.0, 0.0, 1.0, drawn.text_rect.left, drawn.text_rect.top]);
        assert!(layer.box_width >= drawn.text_rect.width + drawn.font_size_px);
        assert!(layer.box_height >= drawn.text_rect.height + drawn.font_size_px);

        // Centered: the box is centered on the lines.
        let centered = TranslationBlock { text_align: TextAlign::Center, ..block.clone() };
        let drawn = mtt_render::block_text(&mut fonts, size, &centered).unwrap();
        let layer = psd_text_layer(&centered, &drawn);
        let lines_center = drawn.text_rect.left + drawn.text_rect.width / 2.0;
        assert!((layer.transform[4] + layer.box_width / 2.0 - lines_center).abs() < 1e-9);

        // Rotated: the render rect's center stays put.
        let rotated = TranslationBlock { rotation_deg: Some(30.0), ..block.clone() };
        let drawn = mtt_render::block_text(&mut fonts, size, &rotated).unwrap();
        let [xx, xy, yx, yy, tx, ty] = psd_text_layer(&rotated, &drawn).transform;
        let rect = drawn.layout.rect;
        let (cx, cy) = (rect.left + rect.width / 2.0, rect.top + rect.height / 2.0);
        let (u, v) = (cx - drawn.text_rect.left, cy - drawn.text_rect.top);
        assert!((xx * u + yx * v + tx - cx).abs() < 1e-9 && (xy * u + yy * v + ty - cy).abs() < 1e-9);
        assert!((xy - 30f64.to_radians().sin()).abs() < 1e-12);

        // Vertical: one top-aligned column whose right edge is the box's.
        let vertical = TranslationBlock { render_direction: RenderTextDirection::Vertical, ..block.clone() };
        let drawn = mtt_render::block_text(&mut fonts, size, &vertical).unwrap();
        let layer = psd_text_layer(&vertical, &drawn);
        assert!(layer.vertical && layer.justification == Justification::Left);
        assert_eq!(layer.text, "첫줄은조금길고둘째줄");
        assert!((layer.transform[4] + layer.box_width - (drawn.text_rect.left + drawn.text_rect.width)).abs() < 1e-9);
        let expected = (drawn.line_height_px - drawn.font_size_px) / drawn.font_size_px * 1000.0;
        assert_eq!(layer.tracking, expected.round() as i32);
    }

    #[test]
    fn text_layer_names_are_the_text() {
        assert_eq!(text_layer_name(&["안녕  하세요".into(), "반가워요".into()]), "안녕 하세요 반가워요");
        let long = text_layer_name(&["가".repeat(40)]);
        assert_eq!(long, format!("{}…", "가".repeat(30)));
    }
}
