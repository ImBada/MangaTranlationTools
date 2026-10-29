//! Saving: chapter autosaves and inpaint layer writes on the background writer, and the
//! synchronous flush when leaving the editor or quitting.

use std::sync::{Arc, PoisonError};
use std::time::{Duration, Instant};

use image::{GrayImage, RgbaImage};
use mtt_core::library::resolve_chapter_status;

use super::assets::{LoadedPage, PageAssets};
use super::history::LayerKind;
use super::notifications;
use super::{AUTOSAVE_DELAY, Editor, InpaintScope, LAYER_RETRY_BACKOFF_MAX, LAYER_SAVE_DELAY, TEXT_COMMIT_DELAY};
use crate::app::AppEvent;
use crate::services::Services;
use crate::worker::{Worker, catch_panic};

/// Orders the background writer against synchronous flushes: queued writes remember the
/// generation they were queued in and are skipped once a flush has advanced it. A write
/// that panicked poisons the lock; the counter is still valid, so it is recovered.
#[derive(Clone, Default)]
pub struct WriteGeneration(std::sync::Arc<std::sync::Mutex<u64>>);

impl WriteGeneration {
    pub(super) fn current(&self) -> u64 {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs `write` unless a flush happened after `queued_at` (the lock covers the write).
    /// `Ok(false)`: skipped, the flush took the write over.
    pub(super) fn write_if_current(
        &self,
        queued_at: u64,
        write: impl FnOnce() -> Result<(), String>,
    ) -> Result<bool, String> {
        let guard = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if *guard != queued_at {
            return Ok(false);
        }
        let result = write();
        drop(guard);
        result.map(|()| true)
    }

    /// Bumps the generation and runs `flush` while holding the lock.
    pub(super) fn advance_while<T>(&self, flush: impl FnOnce() -> T) -> T {
        let mut guard = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        *guard += 1;
        flush()
    }
}

#[derive(Default)]
pub(super) struct SaveState {
    seq: u64,
    saved_seq: u64,
    in_flight: bool,
    error: Option<String>,
}

impl Editor {
    /// Starts due background saves and closes undo steps. Call once per frame.
    /// `pointer_down` keeps a slider or canvas drag in one undo step.
    pub fn tick(&mut self, services: &mut Services, worker: &Worker<AppEvent>, pointer_down: bool) {
        if self.pending_context.is_none() {
            self.history.context = self.current_history_context();
        }
        self.request_assets(services, worker);
        self.advance_inpaint_queue(worker);
        if std::mem::take(&mut self.heal_requested)
            && let Some(page_id) = self.heal_stroke.as_ref().map(|stroke| stroke.page_id.clone())
        {
            self.start_inpaint_on(&page_id, InpaintScope::Heal, worker);
        }
        let pointer_busy = self.drag.is_some() || pointer_down;
        let text_idle = self.last_text_edit.is_some_and(|at| at.elapsed() > TEXT_COMMIT_DELAY);
        if (!self.text_editing && !pointer_busy) || (self.text_editing && text_idle) {
            self.commit_checkpoint_if_changed();
        }

        let due_layers: Vec<(String, LayerKind)> = self
            .layer_dirty
            .iter()
            .filter(|(key, at)| at.elapsed() >= self.layer_save_delay(key) && !pointer_busy)
            .map(|(key, _)| key.clone())
            .collect();
        for (page_id, layer) in due_layers {
            self.layer_dirty.remove(&(page_id.clone(), layer));
            self.save_layer(services, worker, &page_id, layer);
        }

        if self.chapter_dirty
            && !pointer_busy
            && !self.save.in_flight
            && self.last_change.is_some_and(|at| at.elapsed() >= AUTOSAVE_DELAY)
        {
            self.save_chapter(services, worker);
        }
    }

    /// How long until `tick` has a save or an undo step to close. egui only runs a frame on
    /// input, so without a repaint for this an edit would sit unsaved until the mouse moves.
    /// (A drag's release and a finished save run frames of their own.)
    pub(super) fn next_tick_due(&self) -> Option<Duration> {
        let left = |at: &Instant, delay: Duration| delay.saturating_sub(at.elapsed());
        let layers = self.layer_dirty.iter().map(|(key, at)| left(at, self.layer_save_delay(key)));
        let chapter = self
            .last_change
            .as_ref()
            .filter(|_| self.chapter_dirty && !self.save.in_flight)
            .map(|at| left(at, AUTOSAVE_DELAY));
        let text = self.last_text_edit.as_ref().filter(|_| self.text_editing).map(|at| left(at, TEXT_COMMIT_DELAY));
        // Just past the deadline: `tick` compares with `>`/`>=`.
        layers.chain(chapter).chain(text).min().map(|wait| wait + Duration::from_millis(10))
    }

    fn commit_checkpoint_if_changed(&mut self) {
        if self.preset_checkpoint.is_some() {
            self.commit_checkpoint();
            return;
        }
        let changed = match (&self.checkpoint, self.page()) {
            (Some((page_id, snapshot)), Some(page)) => page_id != &page.id || !snapshot.matches(page),
            _ => true,
        };
        if changed {
            self.commit_checkpoint();
        }
    }

    fn save_chapter(&mut self, services: &Services, worker: &Worker<AppEvent>) {
        self.chapter.updated_at = mtt_core::time::now_iso();
        self.chapter.status = resolve_chapter_status(&self.chapter.pages);
        self.chapter.page_order = self.chapter.pages.iter().map(|page| page.id.clone()).collect();
        self.save.seq += 1;
        self.save.in_flight = true;
        self.chapter_dirty = false;
        self.dirty_pages.clear();
        let seq = self.save.seq;
        // Only the saved copy gets "..."→"…" and "--"→"—" (`normalizeChapterTranslatedText`).
        let mut chapter = self.chapter.clone();
        mtt_core::text::normalize_chapter_translated_text(&mut chapter);
        let library = services.library.clone();
        let generation = self.write_generation.clone();
        let queued_at = generation.current();
        worker.spawn_serial(move || {
            // A synchronous `flush` since queuing wrote newer data; don't overwrite it.
            let result = catch_panic(|| {
                generation.write_if_current(queued_at, || library.write_chapter(&chapter).map_err(|e| e.to_string()))
            })
            .and_then(|written| written);
            let skipped = result == Ok(false);
            AppEvent::ChapterSaved { chapter_id: chapter.id.clone(), seq, error: result.err(), skipped }
        });
    }

    /// `skipped`: a flush wrote the chapter instead and reported its own outcome.
    pub fn on_chapter_saved(&mut self, chapter_id: &str, seq: u64, error: Option<String>, skipped: bool) {
        if chapter_id != self.chapter.id || seq < self.save.saved_seq {
            return;
        }
        self.save.saved_seq = seq;
        self.save.in_flight = seq < self.save.seq;
        if skipped {
            return;
        }
        match error {
            Some(error) => {
                log::error!("chapter save failed: {error}");
                self.save.error = Some(error);
                self.notifications.report(
                    notifications::FailureId::ChapterSave,
                    "자동 저장 실패",
                    "변경 내용은 화면에 남아 있습니다. 네트워크 또는 저장 위치를 확인한 뒤 다시 저장하세요.",
                );
                // Retry on the next tick.
                self.mark_chapter_dirty();
            }
            None => {
                self.save.error = None;
                self.notifications.clear(notifications::FailureId::ChapterSave);
                if !self.save.in_flight {
                    self.notifications.flash_saved();
                }
            }
        }
    }

    /// Unsaved edits (`dirty`): navigation-only changes don't count.
    pub fn has_unsaved_changes(&self) -> bool {
        !self.dirty_pages.is_empty() || !self.layer_dirty.is_empty() || self.save.in_flight
    }

    /// Re-queues layer saves that failed, without waiting out their backoff.
    pub(super) fn retry_layer_saves(&mut self) {
        let due = Instant::now() - LAYER_SAVE_DELAY;
        for (key, failures) in &mut self.layer_failures {
            *failures = 0;
            self.layer_dirty.insert(key.clone(), due);
        }
    }

    /// How long a dirty layer waits before its background save; failed ones back off.
    pub(super) fn layer_save_delay(&self, key: &(String, LayerKind)) -> Duration {
        let failures = self.layer_failures.get(key).copied().unwrap_or(0);
        LAYER_SAVE_DELAY * 2u32.pow(failures.min(LAYER_RETRY_BACKOFF_MAX))
    }

    /// A layer write failed: the layer stays dirty, which keeps its page loaded, makes
    /// `flush` write it, and retries it in the background after a backoff.
    fn layer_save_failed(&mut self, key: (String, LayerKind), error: String) {
        log::error!("layer save failed for {} {:?}: {error}", key.0, key.1);
        self.save.error = Some(error);
        *self.layer_failures.entry(key.clone()).or_default() += 1;
        // An edit made since keeps its own (later) time.
        self.layer_dirty.entry(key).or_insert_with(Instant::now);
        self.notifications.report(
            notifications::FailureId::LayerSave,
            "인페인트 레이어 저장 실패",
            "마스크와 결과 레이어는 현재 화면에 남아 있습니다. 다시 저장을 시도하세요.",
        );
    }

    fn layer_save_succeeded(&mut self, key: &(String, LayerKind)) {
        if self.layer_failures.remove(key).is_some() && self.layer_failures.is_empty() {
            self.notifications.clear(notifications::FailureId::LayerSave);
        }
    }

    fn save_layer(&mut self, services: &Services, worker: &Worker<AppEvent>, page_id: &str, layer: LayerKind) {
        let Some(PageAssets::Ready(loaded)) = self.assets.get(page_id) else {
            return;
        };
        let Some(page_index) = self.chapter.pages.iter().position(|page| page.id == page_id) else {
            return;
        };
        let Some(payload) = LayerImage::of(loaded, layer) else {
            return;
        };
        let path = services.library.inpaint_layer_path(&self.chapter, page_id, layer.page_layer());
        let path_text = path.to_string_lossy().into_owned();
        let page = &mut self.chapter.pages[page_index];
        let field = match layer {
            LayerKind::Mask => &mut page.inpaint_mask_path,
            LayerKind::Result => &mut page.inpaint_result_path,
        };
        if field.as_deref() != Some(path_text.as_str()) {
            *field = Some(path_text);
            self.mark_chapter_dirty();
            // Save the chapter right behind this PNG on the writer, not after the autosave
            // delay: a crash in between would leave the file with no page pointing at it.
            self.last_change = Some(Instant::now() - AUTOSAVE_DELAY);
        }
        self.layer_saves_in_flight += 1;
        self.layers_queued.insert((page_id.to_owned(), layer));
        let page_id = page_id.to_owned();
        let generation = self.write_generation.clone();
        let queued_at = generation.current();
        worker.spawn_serial(move || {
            let result = catch_panic(|| {
                payload.encode().and_then(|bytes| {
                    generation.write_if_current(queued_at, || {
                        mtt_core::json_io::write_bytes_atomic(&path, &bytes).map_err(|error| error.to_string())
                    })
                })
            })
            .and_then(|written| written);
            let skipped = result == Ok(false);
            AppEvent::LayerSaved { page_id, layer, error: result.err(), skipped }
        });
    }

    /// `skipped`: a flush took the write over (and keeps the layer dirty if it failed).
    pub fn on_layer_saved(&mut self, page_id: &str, layer: LayerKind, error: Option<String>, skipped: bool) {
        self.layer_saves_in_flight = self.layer_saves_in_flight.saturating_sub(1);
        // Kept until every queued write is confirmed; rewriting a saved layer is harmless.
        if self.layer_saves_in_flight == 0 {
            self.layers_queued.clear();
        }
        // A write of a deleted page (or of the chapter just left) has nothing to retry.
        if skipped || !self.chapter.pages.iter().any(|page| page.id == page_id) {
            return;
        }
        let key = (page_id.to_owned(), layer);
        if let Some(error) = error {
            self.layer_save_failed(key, error);
        } else {
            self.layer_save_succeeded(&key);
            if self.layer_saves_in_flight == 0 {
                self.notifications.flash_saved();
            }
        }
    }

    /// Synchronously writes pending changes (used when leaving the editor or quitting).
    /// Background saves still queued are made stale so they can't land on top afterwards.
    pub fn flush(&mut self, services: &mut Services) -> Result<(), String> {
        let generation = self.write_generation.clone();
        generation.advance_while(|| self.flush_now(services))
    }

    fn flush_now(&mut self, services: &mut Services) -> Result<(), String> {
        // Text typed into the canvas editor is kept, as if it lost focus.
        self.commit_inline_edit();
        self.commit_checkpoint();
        // Queued background writes are skipped after this flush, so cover them here too.
        let mut pending: Vec<(String, LayerKind)> = self.layer_dirty.drain().map(|(key, _)| key).collect();
        for key in self.layers_queued.drain() {
            if !pending.contains(&key) {
                pending.push(key);
            }
        }
        // Every layer gets its try; the ones that fail stay dirty for the next flush.
        let mut layer_error = None;
        for key in pending {
            match self.write_layer_now(services, &key) {
                Ok(()) => self.layer_save_succeeded(&key),
                Err(error) => {
                    layer_error.get_or_insert_with(|| error.clone());
                    self.layer_save_failed(key, error);
                }
            }
        }
        if self.chapter_dirty || self.save.in_flight {
            self.chapter.updated_at = mtt_core::time::now_iso();
            self.chapter.status = resolve_chapter_status(&self.chapter.pages);
            let mut chapter = self.chapter.clone();
            mtt_core::text::normalize_chapter_translated_text(&mut chapter);
            services.library.write_chapter(&chapter).map_err(|error| error.to_string())?;
            self.chapter_dirty = false;
            self.dirty_pages.clear();
        }
        layer_error.map_or(Ok(()), Err)
    }

    /// Writes one layer PNG now and points the page at it.
    fn write_layer_now(&mut self, services: &Services, (page_id, layer): &(String, LayerKind)) -> Result<(), String> {
        let Some(PageAssets::Ready(loaded)) = self.assets.get(page_id) else { return Ok(()) };
        let Some(payload) = LayerImage::of(loaded, *layer) else { return Ok(()) };
        let path = services.library.inpaint_layer_path(&self.chapter, page_id, layer.page_layer());
        let bytes = payload.encode()?;
        mtt_core::json_io::write_bytes_atomic(&path, &bytes).map_err(|error| error.to_string())?;
        if let Some(page) = self.chapter.pages.iter_mut().find(|page| &page.id == page_id) {
            let text = path.to_string_lossy().into_owned();
            match layer {
                LayerKind::Mask => page.inpaint_mask_path = Some(text),
                LayerKind::Result => page.inpaint_result_path = Some(text),
            }
        }
        self.chapter_dirty = true;
        Ok(())
    }
}

/// Copy of `block` with a new id, shifted by `(dx, dy)` page pixels in normalized space.
pub(super) fn encode_png(image: &RgbaImage) -> Result<Vec<u8>, String> {
    encode_png_raw(image.as_raw(), image.dimensions(), image::ExtendedColorType::Rgba8, false)
}

/// PNG from raw pixels (no copy into a `DynamicImage`). `fast` trades file size for speed:
/// layer autosaves run after every stroke.
fn encode_png_raw(
    data: &[u8],
    (width, height): (u32, u32),
    color: image::ExtendedColorType,
    fast: bool,
) -> Result<Vec<u8>, String> {
    use image::ImageEncoder;
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    let mut bytes = Vec::new();
    let encoder = if fast {
        PngEncoder::new_with_quality(&mut bytes, CompressionType::Fast, FilterType::Adaptive)
    } else {
        PngEncoder::new(&mut bytes)
    };
    encoder.write_image(data, width, height, color).map_err(|error| error.to_string())?;
    Ok(bytes)
}

/// A layer handed to the writer thread: shared with the editor, not copied.
enum LayerImage {
    Mask(Arc<GrayImage>),
    Result(Arc<RgbaImage>),
}

impl LayerImage {
    fn of(loaded: &LoadedPage, layer: LayerKind) -> Option<Self> {
        match layer {
            LayerKind::Mask => loaded.mask.clone().map(Self::Mask),
            LayerKind::Result => loaded.result.clone().map(Self::Result),
        }
    }

    /// Masks are stored as grayscale (both readers also take the older black/white RGBA).
    fn encode(&self) -> Result<Vec<u8>, String> {
        match self {
            Self::Mask(mask) => encode_png_raw(mask.as_raw(), mask.dimensions(), image::ExtendedColorType::L8, true),
            Self::Result(result) => {
                encode_png_raw(result.as_raw(), result.dimensions(), image::ExtendedColorType::Rgba8, true)
            }
        }
    }
}
