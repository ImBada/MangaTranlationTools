//! One-off jobs the editor runs off the UI thread: PSD export and import, and exporting
//! the current page or every page.

use std::path::PathBuf;
use std::sync::{Arc, PoisonError};
use std::time::Instant;

use eframe::egui;
use image::RgbaImage;
use mtt_core::model::{InpaintEngine, InpaintStatus};
use mtt_render::page::{PageImages, PageRenderOptions};

use super::assets::{self, PageAssets};
use super::history::{Entry, LayerKind};
use super::inpaint::{ComputedInpaint, InpaintOutcome};
use super::persistence::encode_png;
use super::{Editor, Layer, layer_ops};
use crate::app::AppEvent;
use crate::services::Services;
use crate::worker::{Worker, catch_panic};

/// Builds the imported layers on the worker (see `Editor::psd_import_job`).
trait PsdImportJob: FnOnce(&egui::Context) -> Result<InpaintOutcome, String> + Send + 'static {}
impl<F: FnOnce(&egui::Context) -> Result<InpaintOutcome, String> + Send + 'static> PsdImportJob for F {}

/// One-off jobs the editor runs on the worker.
pub enum EditorJob {
    PageExported(Result<PathBuf, String>),
    PsdExported(Result<PathBuf, String>),
    PsdImported { page_id: String, quick: bool, result: Result<InpaintOutcome, String> },
}

impl EditorJob {
    /// What a job of a chapter the user has left reports on the screen now in front, and
    /// whether it failed. An import has no page to go to any more.
    pub fn left_behind_message(self) -> Option<(String, bool)> {
        match self {
            EditorJob::PageExported(Ok(path)) => {
                Some((format!("이전 화의 페이지를 출력했습니다: {}", file_name(&path)), false))
            }
            EditorJob::PageExported(Err(error)) => Some((format!("이전 화의 페이지 출력 실패: {error}"), true)),
            EditorJob::PsdExported(Ok(path)) => {
                Some((format!("이전 화의 인페인트 PSD를 내보냈습니다: {}", file_name(&path)), false))
            }
            EditorJob::PsdExported(Err(error)) => Some((format!("이전 화의 PSD 내보내기 실패: {error}"), true)),
            EditorJob::PsdImported { .. } => None,
        }
    }
}

fn file_name(path: &std::path::Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |name| name.to_string_lossy().into_owned())
}

impl Editor {
    /// Exports source, masked result, the rendered translation and its editable text as a
    /// layered PSD. PSD bytes of the current page (`exportInpaintPsd`).
    #[cfg(test)]
    pub fn render_psd(&mut self, services: &mut Services) -> Result<Vec<u8>, String> {
        self.psd_export_job(services)?()
    }

    /// Builds the PSD on whatever thread runs the returned job, from shared copies of the
    /// page's layers.
    pub(super) fn psd_export_job(
        &mut self,
        services: &mut Services,
    ) -> Result<impl FnOnce() -> Result<Vec<u8>, String> + Send + 'static, String> {
        use mtt_inpaint::psd::{
            BACKGROUND_LAYER_NAME, ExportLayer, GROUP_END_NAME, RESULT_LAYER_NAME, TEXT_GROUP_NAME,
            TRANSLATION_LAYER_NAME, write_psd,
        };
        let page = self.page().cloned().ok_or("페이지가 없습니다.")?;
        let fonts = services.fonts().ok_or("폰트를 불러오는 중입니다.")?;
        let loaded = self.loaded_page().ok_or("페이지 이미지가 아직 로드되지 않았습니다.")?;
        let (width, height) = (loaded.width(), loaded.height());
        let (source, mask, result) = (Arc::clone(&loaded.source), loaded.mask.clone(), loaded.result.clone());
        Ok(move || {
            let mut source = (*source).clone();
            source.pixels_mut().for_each(|p| p[3] = 255);
            let mut result = result.as_deref().cloned().unwrap_or_else(|| RgbaImage::new(width, height));
            if let Some(mask) = &mask {
                for (pixel, covered) in result.pixels_mut().zip(mask.iter()) {
                    if *covered == 0 {
                        pixel[3] = 0;
                    }
                }
            }
            let mut composite = source.clone();
            image::imageops::overlay(&mut composite, &result, 0, 0);
            let (translation, texts) = {
                let mut fonts = fonts.lock().unwrap_or_else(PoisonError::into_inner);
                let options = PageRenderOptions {
                    visibility: mtt_render::page::LayerVisibility {
                        image: false,
                        inpaint: false,
                        inpaint_result: false,
                        inpaint_mask: false,
                        overlay: true,
                    },
                    ..PageRenderOptions::OUTPUT
                };
                let translation = mtt_render::page::render_page(
                    &mut fonts,
                    &page,
                    &PageImages { source: None, inpaint_mask: None, inpaint_result: None },
                    &options,
                );
                (translation, layer_ops::psd_text_layers(&mut fonts, &page))
            };
            let mut layers = vec![
                ExportLayer::new(BACKGROUND_LAYER_NAME, &source, false),
                ExportLayer::new(RESULT_LAYER_NAME, &result, false),
            ];
            if let Some(translation) = &translation {
                layers.push(ExportLayer::new(TRANSLATION_LAYER_NAME, translation, true));
            }
            // Editable text in a hidden, closed group on top.
            let empty = RgbaImage::new(0, 0);
            if !texts.is_empty() {
                layers.push(ExportLayer { section: Some(3), ..ExportLayer::new(GROUP_END_NAME, &empty, false) });
                layers.extend(texts.iter().map(|text| ExportLayer {
                    left: text.left,
                    top: text.top,
                    text: Some(&text.text),
                    ..ExportLayer::new(&text.name, &text.raster, false)
                }));
                layers.push(ExportLayer { section: Some(2), ..ExportLayer::new(TEXT_GROUP_NAME, &empty, true) });
            }
            Ok(write_psd(width, height, &composite, &layers))
        })
    }

    /// Replaces the result and mask with the visible non-background layers of a PSD.
    #[cfg(test)]
    pub fn import_psd(&mut self, ctx: &egui::Context, path: &std::path::Path) -> Result<(), String> {
        let (page_id, job) = self.psd_import_job(path)?;
        let outcome = job(ctx)?;
        self.apply_psd_layers(&page_id, outcome)
    }

    /// Reads the PSD and builds the new layers (and their undo patches and textures) on the
    /// thread that runs the returned job.
    fn psd_import_job(&self, path: &std::path::Path) -> Result<(String, impl PsdImportJob), String> {
        let page_id = self.page().map(|page| page.id.clone()).ok_or("페이지가 없습니다.")?;
        let Some(PageAssets::Ready(loaded)) = self.assets.get(&page_id) else {
            return Err("페이지 이미지가 아직 로드되지 않았습니다.".into());
        };
        let (width, height) = (loaded.width(), loaded.height());
        let (before_mask, before_result) = (loaded.mask.clone(), loaded.result.clone());
        let path = path.to_owned();
        let job_page = page_id.clone();
        Ok((page_id, move |ctx: &egui::Context| {
            let bytes = std::fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
            let document = mtt_inpaint::psd::read_psd(&bytes)?;
            let (result, mask) = mtt_inpaint::psd::import_inpaint_layers(&document, width, height)?;
            let computed = ComputedInpaint {
                before_mask,
                before_result,
                mask,
                result,
                engine: InpaintEngine::LocalFillFallback,
                // What the import left out (blend modes, vector masks …), for the status.
                message: mtt_inpaint::psd::import_warning(&document).unwrap_or_default(),
            };
            Ok(computed.finish(ctx, &job_page))
        }))
    }

    fn apply_psd_layers(&mut self, page_id: &str, outcome: InpaintOutcome) -> Result<(), String> {
        let Some(PageAssets::Ready(loaded)) = self.assets.get_mut(page_id) else {
            return Err("PSD를 적용할 페이지가 더 이상 열려 있지 않습니다.".into());
        };
        let entries: Vec<Entry> = outcome
            .patches
            .into_iter()
            .map(|(layer, patch)| Entry::Layer { page_id: page_id.to_owned(), layer, patch })
            .collect();
        loaded.install_layers(outcome.layers);
        if !entries.is_empty() {
            self.history.push(Entry::Batch { page_id: page_id.to_owned(), entries });
        }
        let now = Instant::now();
        self.layer_dirty.insert((page_id.to_owned(), LayerKind::Mask), now);
        self.layer_dirty.insert((page_id.to_owned(), LayerKind::Result), now);
        if let Some(page) = self.chapter.pages.iter_mut().find(|page| page.id == page_id) {
            page.inpaint_status = Some(InpaintStatus::Completed);
        }
        self.mark_chapter_dirty();
        Ok(())
    }

    /// Starts a PSD import of `path` for the current page; `keep_as` is where to keep a copy
    /// (`-last-import.psd`), `quick` picks the status message.
    pub(super) fn start_psd_import(
        &mut self,
        worker: &Worker<AppEvent>,
        path: &std::path::Path,
        keep_as: PathBuf,
        quick: bool,
    ) {
        let (page_id, job) = match self.psd_import_job(path) {
            Ok(job) => job,
            Err(error) => {
                self.status = error;
                return;
            }
        };
        self.psd_import_running = Some(page_id.clone());
        self.status = "PSD를 불러오는 중…".into();
        let chapter_id = self.chapter.id.clone();
        let source = path.to_owned();
        let ctx = worker.ctx().clone();
        worker.spawn(move || {
            let result = catch_panic(|| job(&ctx)).and_then(|outcome| outcome).and_then(|outcome| {
                if let Some(parent) = keep_as.parent() {
                    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                }
                if source != keep_as {
                    std::fs::copy(&source, &keep_as).map_err(|error| error.to_string())?;
                }
                Ok(outcome)
            });
            AppEvent::EditorJob { chapter_id, job: EditorJob::PsdImported { page_id, quick, result } }
        });
    }

    // ---- export ----------------------------------------------------------------------

    /// Renders the current page like "출력" and writes it to `renders/` (automation).
    pub fn export_current(&mut self, services: &mut Services) -> Result<std::path::PathBuf, String> {
        self.page_export_job(services)?()
    }

    /// "이 페이지 출력" on the worker.
    pub fn export_current_in_background(&mut self, services: &mut Services, worker: &Worker<AppEvent>) {
        match self.page_export_job(services) {
            Ok(job) => {
                self.page_export_running = true;
                self.status = "이 페이지를 출력하는 중…".into();
                let chapter_id = self.chapter.id.clone();
                worker.spawn(move || {
                    let result = catch_panic(job).and_then(|result| result);
                    AppEvent::EditorJob { chapter_id, job: EditorJob::PageExported(result) }
                });
            }
            Err(error) => self.status = format!("출력 실패: {error}"),
        }
    }

    /// Renders and writes the page from shared copies of its layers, on any thread. The
    /// editor's fonts are held only while rendering; the canvas skips its update meanwhile.
    fn page_export_job(
        &mut self,
        services: &mut Services,
    ) -> Result<impl FnOnce() -> Result<std::path::PathBuf, String> + Send + 'static, String> {
        let page = self.page().cloned().ok_or("페이지가 없습니다.")?;
        let fonts = services.fonts().ok_or("폰트를 불러오는 중입니다.")?;
        let loaded = self.loaded_page().ok_or("페이지 이미지가 아직 로드되지 않았습니다.")?;
        let (source, mask, result) = (Arc::clone(&loaded.source), loaded.mask.clone(), loaded.result.clone());
        let path = services.library.render_output_path(&self.chapter, &page);
        Ok(move || {
            let images =
                PageImages { source: Some(&source), inpaint_mask: mask.as_deref(), inpaint_result: result.as_deref() };
            let rendered = {
                let mut fonts = fonts.lock().unwrap_or_else(PoisonError::into_inner);
                mtt_render::page::render_page(&mut fonts, &page, &images, &PageRenderOptions::OUTPUT)
                    .ok_or("렌더링 실패")?
            };
            let bytes = encode_png(&rendered)?;
            mtt_core::json_io::write_bytes_atomic(&path, &bytes).map_err(|error| error.to_string())?;
            Ok(path)
        })
    }

    /// A finished editor job (see `EditorJob`).
    pub fn on_editor_job(&mut self, chapter_id: &str, job: EditorJob, services: &Services) {
        if chapter_id != self.chapter.id {
            return;
        }
        match job {
            EditorJob::PageExported(result) => {
                self.page_export_running = false;
                self.status = match result {
                    Ok(path) => {
                        format!("이 페이지를 출력했습니다: {} (출력 폴더 열기는 검수 · 출력 모드)", file_name(&path))
                    }
                    Err(error) => format!("출력 실패: {error}"),
                };
            }
            EditorJob::PsdExported(result) => {
                self.page_export_running = false;
                self.status = match result {
                    Ok(path) => format!("인페인트 PSD를 내보냈습니다: {}", file_name(&path)),
                    Err(error) => error,
                };
                self.refresh_psd_files(services, true);
            }
            EditorJob::PsdImported { page_id, quick, result } => {
                self.psd_import_running = None;
                let applied = result.and_then(|outcome| {
                    let warning = outcome.message.clone();
                    self.apply_psd_layers(&page_id, outcome).map(|()| warning)
                });
                self.status = match applied {
                    Ok(warning) => {
                        let done = if quick {
                            "마지막으로 내보낸 PSD를 퀵로드했습니다."
                        } else {
                            "PSD에서 인페인트 결과와 마스크를 가져왔습니다."
                        };
                        if warning.is_empty() { done.into() } else { format!("{done} {warning}") }
                    }
                    Err(error) => error,
                };
                self.refresh_psd_files(services, true);
                if self.page().is_some_and(|page| page.id == page_id) && self.layer != Layer::InpaintResult {
                    self.set_layer(Layer::InpaintResult);
                }
            }
        }
    }

    /// Renders every page in the background (decoding images there too), reporting
    /// `전체 출력 {i}/{n}` progress. With `cbz` the pages also go, in order, into
    /// `{work} {chapter}.cbz` in the downloads folder.
    pub fn export_all(&mut self, services: &mut Services, worker: &Worker<AppEvent>, cbz: bool) -> Result<(), String> {
        let fonts = services.fonts().ok_or("폰트를 불러오는 중입니다.")?;
        // Its own font context: rendering every page must not hold the editor's.
        let mut fonts = fonts.lock().unwrap_or_else(PoisonError::into_inner).fork();
        self.flush(services)?;
        let library = services.library.clone();
        let chapter = self.chapter.clone();
        let emitter = worker.emitter();
        let total = chapter.pages.len();
        let title = format!("{} {}", self.work_title, chapter.title).trim().to_owned();
        let archive_name =
            cbz.then(|| format!("{}.cbz", layer_ops::sanitize_psd_basename(&format!("{title}.cbz"), &chapter.id)));
        self.export_progress = Some((0, total));
        std::thread::spawn(move || {
            let mut written = 0usize;
            let mut errors = Vec::new();
            let mut archived = Vec::new();
            for (index, page) in chapter.pages.iter().enumerate() {
                emitter.emit(AppEvent::ExportProgress { chapter_id: chapter.id.clone(), current: index, total });
                // A panic fails its page, not the rest of the run.
                let rendered = catch_panic(|| {
                    let load = assets::decode_page(&library, &chapter, page);
                    let source = load.source?;
                    let images = PageImages {
                        source: Some(&source),
                        inpaint_mask: load.mask.as_ref(),
                        inpaint_result: load.result.as_ref(),
                    };
                    let rendered = mtt_render::page::render_page(&mut fonts, page, &images, &PageRenderOptions::OUTPUT)
                        .ok_or_else(|| format!("{}: 렌더링 실패", page.name))?;
                    encode_png(&rendered).map_err(|error| format!("{}: {error}", page.name))
                })
                .map_err(|error| format!("{}: {error}", page.name))
                .and_then(|bytes| bytes);
                let bytes = match rendered {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        errors.push(error);
                        continue;
                    }
                };
                let path = library.render_output_path(&chapter, page);
                match mtt_core::json_io::write_bytes_atomic(&path, &bytes) {
                    Ok(()) => written += 1,
                    Err(error) => errors.push(format!("{}: {error}", page.name)),
                }
                if archive_name.is_some() {
                    archived.push(bytes);
                }
            }
            let archive = archive_name.map(|name| {
                mtt_core::cbz::cbz_bytes(&archived).and_then(|bytes| layer_ops::write_unique_download(&name, &bytes))
            });
            emitter.emit(AppEvent::ExportFinished {
                chapter_id: chapter.id.clone(),
                chapter_title: title,
                written,
                errors,
                archive,
            });
        });
        Ok(())
    }
}
