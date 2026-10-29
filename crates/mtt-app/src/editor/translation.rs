//! Translation runs for a chapter (`analysisJob.ts`, `wholePagePipeline.ts`): pages are
//! translated on background threads (optionally in parallel) and applied as they finish.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

use mtt_core::font_presets::{apply_block_type_font_preset_to_block, ensure_block_type_font_presets};
use mtt_core::groups::valid_groups;
use mtt_core::library::{PageLayer, resolve_chapter_status};
use mtt_core::model::{BBox, BlockType, PageAnalysisStatus, PageRecord, TranslationBlock};
use mtt_inpaint::bubble::find_balloons;
use mtt_inpaint::text_mask::{SourceLettering, detect_lettering};
use mtt_translate::parser::RegionText;
use mtt_translate::{Region, TranslationConfig, overlay_item_to_block, translate_page, translate_regions};

use super::Editor;
use super::history::{BlocksSnapshot, Entry};
use crate::app::AppEvent;
use crate::services::Services;
use crate::worker::Worker;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    /// Pages not yet translated and not marked done.
    Pending,
    /// Every page not marked done.
    All,
    /// Only the current page.
    CurrentPage,
    /// The selected blocks of the current page, read and translated again in place.
    Blocks,
}

pub enum TranslationEvent {
    Started {
        total: usize,
        endpoint: String,
    },
    PageStarted {
        page_id: String,
    },
    Attempt {
        page_id: String,
        attempt: u32,
        error: String,
    },
    /// The page's blocks with the source lettering found under each.
    PageDone {
        page_id: String,
        result: Result<Vec<(TranslationBlock, Option<SourceLettering>)>, String>,
    },
    /// Source and translation for blocks of a [`RunMode::Blocks`] run, by block id.
    BlocksDone {
        page_id: String,
        result: Result<Vec<(String, RegionText)>, String>,
    },
    Finished {
        cancelled: bool,
        error: Option<String>,
    },
}

pub struct TranslationJob {
    pub cancel: Arc<AtomicBool>,
    pub total: usize,
    pub done: usize,
    pub failed: usize,
    pub started: Instant,
    pub endpoint: String,
    pub last_message: String,
    /// `formatJobLabel` of the latest phase.
    pub phase: String,
    /// Warnings collected like `wholePagePipeline.ts` (`summarizeWarnings` at the end).
    pub warnings: Vec<String>,
    pub attempts: u32,
    pub mode: RunMode,
    /// The user's own blocks left in place on translated pages.
    pub kept_blocks: usize,
    page_ids: Vec<String>,
}

impl TranslationJob {
    /// Whether this is a [`RunMode::Blocks`] run on `page_id`.
    pub fn covers_blocks_of(&self, page_id: &str) -> bool {
        self.mode == RunMode::Blocks && self.page_ids.iter().any(|id| id == page_id)
    }

    pub fn progress(&self) -> f32 {
        if self.total == 0 { 0.0 } else { self.done as f32 / self.total as f32 }
    }

    pub fn phase_label(&self) -> &str {
        &self.phase
    }

    /// 1-based position of a page in this run.
    fn page_number(&self, page_id: &str) -> usize {
        self.page_ids.iter().position(|id| id == page_id).map_or(0, |index| index + 1)
    }
}

/// `summarizeWarnings`.
pub fn summarize_warnings(warnings: &[String]) -> Option<&'static str> {
    if warnings.is_empty() {
        return None;
    }
    let skipped = warnings.iter().any(|warning| warning.contains("건너뜁니다"));
    let uncertain = warnings.iter().any(|warning| warning.contains("불확실한 OCR"));
    Some(match (skipped, uncertain) {
        (true, true) => "일부 페이지를 건너뛰었고 OCR 확인이 필요한 블록도 있습니다.",
        (true, false) => "일부 페이지는 건너뛰고 다음 페이지로 진행했습니다.",
        (false, true) => "일부 블록은 OCR 확인이 더 필요합니다.",
        (false, false) => "중간 경고가 있었지만 작업은 계속 진행되었습니다.",
    })
}

/// What a translation thread sends when it dies of a panic, so the run still ends.
fn panic_finished() -> TranslationEvent {
    let error = "번역 작업이 내부 오류로 중단되었습니다.".to_owned();
    TranslationEvent::Finished { cancelled: false, error: Some(error) }
}

/// `isAnalysisTargetForRun`.
pub fn is_target(page: &PageRecord, mode: RunMode, current_id: &str) -> bool {
    match mode {
        RunMode::CurrentPage => page.id == current_id,
        RunMode::Blocks => false,
        _ if page.is_completed() => false,
        RunMode::All => true,
        RunMode::Pending => page.analysis_status != PageAnalysisStatus::Completed,
    }
}

impl Editor {
    pub fn translation_running(&self) -> bool {
        self.translation.is_some()
    }

    pub fn start_translation(&mut self, mode: RunMode, services: &Services, worker: &Worker<AppEvent>) {
        if self.translation.is_some() {
            self.status = "번역이 이미 실행 중입니다.".into();
            return;
        }
        let current_id = self.page().map(|page| page.id.clone()).unwrap_or_default();
        let targets: Vec<(String, std::path::PathBuf, u32, u32)> = self
            .chapter
            .pages
            .iter()
            .filter(|page| is_target(page, mode, &current_id))
            .filter_map(|page| {
                let path = services.library.resolve_page_layer_path(&self.chapter, page, PageLayer::Source)?;
                Some((page.id.clone(), path, page.width, page.height))
            })
            .collect();
        if targets.is_empty() {
            self.status = "번역할 미완료 페이지가 없습니다.".into();
            return;
        }
        self.commit_checkpoint();
        self.last_run_mode = Some(mode);
        let ids: Vec<String> = targets.iter().map(|(id, ..)| id.clone()).collect();
        for page in self.chapter.pages.iter_mut().filter(|page| ids.contains(&page.id)) {
            page.analysis_status = PageAnalysisStatus::Running;
        }
        self.chapter.status = resolve_chapter_status(&self.chapter.pages);
        self.mark_chapter_dirty();

        let config = self.translation_config(services);
        let cancel = Arc::new(AtomicBool::new(false));
        self.translation = Some(TranslationJob {
            cancel: Arc::clone(&cancel),
            total: targets.len(),
            done: 0,
            failed: 0,
            started: Instant::now(),
            endpoint: config.describe(),
            last_message: "엔드포인트 확인 중…".into(),
            phase: "모델 준비 중".into(),
            warnings: Vec::new(),
            attempts: config.attempts.max(1),
            mode,
            kept_blocks: 0,
            page_ids: ids,
        });
        let chapter_id = self.chapter.id.clone();
        let emitter = worker.emitter();
        std::thread::Builder::new()
            .name("mtt-translation".into())
            .spawn(move || {
                let send = |event| emitter.emit(AppEvent::Translation { chapter_id: chapter_id.clone(), event });
                let _finished = crate::worker::OnPanic::new(|| send(panic_finished()));
                if let Err(error) = mtt_translate::verify_endpoint(&config) {
                    send(TranslationEvent::Finished { cancelled: false, error: Some(error) });
                    return;
                }
                send(TranslationEvent::Started { total: targets.len(), endpoint: config.describe() });
                let next = AtomicUsize::new(0);
                std::thread::scope(|scope| {
                    for _ in 0..config.concurrency.min(targets.len()).max(1) {
                        scope.spawn(|| {
                            loop {
                                if cancel.load(Ordering::SeqCst) {
                                    return;
                                }
                                let index = next.fetch_add(1, Ordering::SeqCst);
                                let Some((page_id, path, width, height)) = targets.get(index) else { return };
                                send(TranslationEvent::PageStarted { page_id: page_id.clone() });
                                let result = translate_page(
                                    &config,
                                    path,
                                    |attempt, error| {
                                        send(TranslationEvent::Attempt {
                                            page_id: page_id.clone(),
                                            attempt,
                                            error: error.to_owned(),
                                        })
                                    },
                                    || cancel.load(Ordering::SeqCst),
                                )
                                .map(|items| {
                                    // The lettering colors come from the page itself.
                                    let source = image::open(path).ok().map(|image| image.to_rgba8());
                                    let blocks: Vec<TranslationBlock> = items
                                        .iter()
                                        .enumerate()
                                        .map(|(i, item)| overlay_item_to_block(item, page_id, *width, *height, i))
                                        .collect();
                                    // Dialogue in a balloon gets the balloon's room, and its lines
                                    // follow the outline.
                                    let dialogue: Vec<usize> =
                                        (0..blocks.len()).filter(|i| blocks[*i].block_type != BlockType::Sfx).collect();
                                    let mut balloons = vec![None; blocks.len()];
                                    if let Some(source) = &source {
                                        let fitted: Vec<TranslationBlock> =
                                            dialogue.iter().map(|i| blocks[*i].clone()).collect();
                                        for (i, balloon) in dialogue.iter().zip(find_balloons(source, &fitted)) {
                                            balloons[*i] = balloon;
                                        }
                                    }
                                    blocks
                                        .into_iter()
                                        .zip(balloons)
                                        .map(|(mut block, balloon)| {
                                            let lettering =
                                                source.as_ref().and_then(|source| detect_lettering(source, &block));
                                            if let Some(balloon) = balloon {
                                                block.render_bbox = Some(balloon.text_area);
                                                block.render_bbox_space = block.bbox_space.clone();
                                                block.text_shape = Some(balloon.shape);
                                            }
                                            (block, lettering)
                                        })
                                        .collect()
                                });
                                if cancel.load(Ordering::SeqCst) && result.is_err() {
                                    return;
                                }
                                send(TranslationEvent::PageDone { page_id: page_id.clone(), result });
                            }
                        });
                    }
                });
                send(TranslationEvent::Finished { cancelled: cancel.load(Ordering::SeqCst), error: None });
            })
            .expect("spawn translation thread");
    }

    /// Reads and translates the selected blocks of the current page again, keeping their
    /// boxes and styles (koharu's "Process Selected Layers").
    pub fn start_block_translation(&mut self, services: &Services, worker: &Worker<AppEvent>) {
        if self.translation.is_some() {
            self.status = "번역이 이미 실행 중입니다.".into();
            return;
        }
        let Some(page) = self.page() else { return };
        let blocks = self.selected_blocks();
        if blocks.is_empty() {
            return;
        }
        let Some(path) = services.library.resolve_page_layer_path(&self.chapter, page, PageLayer::Source) else {
            self.status = "페이지 원본 이미지를 찾지 못했습니다.".into();
            return;
        };
        let page_id = page.id.clone();
        let regions: Vec<Region> =
            blocks.iter().enumerate().map(|(index, block)| Region { id: index as u32 + 1, bbox: block.bbox }).collect();
        let block_ids: Vec<String> = blocks.into_iter().map(|block| block.id).collect();
        self.commit_checkpoint();
        let config = self.translation_config(services);
        let cancel = Arc::new(AtomicBool::new(false));
        self.translation = Some(TranslationJob {
            cancel: Arc::clone(&cancel),
            total: block_ids.len(),
            done: 0,
            failed: 0,
            started: Instant::now(),
            endpoint: config.describe(),
            last_message: "엔드포인트 확인 중…".into(),
            phase: format!("블록 {}개 번역 중", block_ids.len()),
            warnings: Vec::new(),
            attempts: config.attempts.max(1),
            mode: RunMode::Blocks,
            kept_blocks: 0,
            page_ids: vec![page_id.clone()],
        });
        let chapter_id = self.chapter.id.clone();
        let emitter = worker.emitter();
        std::thread::Builder::new()
            .name("mtt-block-translation".into())
            .spawn(move || {
                let send = |event| emitter.emit(AppEvent::Translation { chapter_id: chapter_id.clone(), event });
                let _finished = crate::worker::OnPanic::new(|| send(panic_finished()));
                if let Err(error) = mtt_translate::verify_endpoint(&config) {
                    send(TranslationEvent::Finished { cancelled: false, error: Some(error) });
                    return;
                }
                send(TranslationEvent::Started { total: block_ids.len(), endpoint: config.describe() });
                let result = translate_regions(
                    &config,
                    &path,
                    &regions,
                    |attempt, error| {
                        send(TranslationEvent::Attempt { page_id: page_id.clone(), attempt, error: error.to_owned() })
                    },
                    || cancel.load(Ordering::SeqCst),
                );
                let cancelled = cancel.load(Ordering::SeqCst);
                let error = result.as_ref().err().filter(|_| !cancelled).cloned();
                if let Ok(texts) = result {
                    let texts = texts
                        .into_iter()
                        .filter_map(|text| Some((block_ids.get((text.id as usize).checked_sub(1)?)?.clone(), text)))
                        .collect();
                    send(TranslationEvent::BlocksDone { page_id, result: Ok(texts) });
                }
                send(TranslationEvent::Finished { cancelled, error });
            })
            .expect("spawn block translation thread");
    }

    /// Request settings plus the general instructions and this work's notes.
    fn translation_config(&self, services: &Services) -> TranslationConfig {
        let notes = services
            .library
            .read_work(&self.chapter.work_id)
            .ok()
            .flatten()
            .map(|work| work.translation_notes)
            .unwrap_or_default();
        TranslationConfig::from_settings(&services.settings)
            .with_instructions(&services.settings.translation_instructions, &notes)
    }

    pub fn cancel_translation(&mut self) {
        if let Some(job) = &self.translation {
            job.cancel.store(true, Ordering::SeqCst);
            self.status = "번역 취소 중…".into();
        }
    }

    /// Applies a job event. Returns `Some(success)` when the job finished.
    pub fn on_translation_event(&mut self, chapter_id: &str, event: TranslationEvent) -> Option<bool> {
        if chapter_id != self.chapter.id {
            return None;
        }
        match event {
            TranslationEvent::Started { total, endpoint } => {
                if let Some(job) = &mut self.translation {
                    job.total = total;
                    job.last_message = format!("{endpoint} 연결됨");
                    job.phase = "모델 준비 완료".into();
                }
            }
            TranslationEvent::PageStarted { page_id } => {
                if let Some(job) = &mut self.translation {
                    job.phase = format!("{} / {} 페이지 번역 중", job.page_number(&page_id), job.total);
                }
            }
            TranslationEvent::Attempt { page_id, attempt, error } => {
                let name = self.page_name(&page_id);
                if let Some(job) = &mut self.translation {
                    let message = mtt_translate::client::preview(&error, 120);
                    job.warnings.push(format!("{name}: 시도 {attempt}/{} 실패 - {message}", job.attempts));
                    job.last_message = format!("{name}: 시도 {attempt} 실패 — {message}");
                    if attempt < job.attempts {
                        job.phase = format!(
                            "{} / {} 페이지 재시도 {} / {}",
                            job.page_number(&page_id),
                            job.total,
                            attempt + 1,
                            job.attempts
                        );
                    }
                }
            }
            TranslationEvent::PageDone { page_id, result } => self.apply_translated_page(&page_id, result),
            TranslationEvent::BlocksDone { page_id, result } => self.apply_translated_blocks(&page_id, result),
            TranslationEvent::Finished { cancelled, error } => {
                let job = self.translation.take()?;
                let success = error.is_none() && !cancelled;
                let reset_to = if error.is_some() { PageAnalysisStatus::Failed } else { PageAnalysisStatus::Idle };
                for page in self.chapter.pages.iter_mut() {
                    if job.page_ids.contains(&page.id) && page.analysis_status == PageAnalysisStatus::Running {
                        page.analysis_status = reset_to;
                        if let Some(error) = &error {
                            page.last_error = Some(error.clone());
                        }
                    }
                }
                self.chapter.status = resolve_chapter_status(&self.chapter.pages);
                self.mark_chapter_dirty();
                log::info!(
                    "translation finished in {:.0}s: {} done, {} failed",
                    job.started.elapsed().as_secs_f32(),
                    job.done,
                    job.failed
                );
                if error.is_some() {
                    let title = match job.mode {
                        RunMode::CurrentPage => "페이지 번역 실패",
                        RunMode::Blocks => "블록 번역 실패",
                        _ => "번역 실행 실패",
                    };
                    self.notifications.report(
                        super::notifications::FailureId::AnalysisRun,
                        title,
                        "기존 편집 내용은 유지됩니다. 설정과 모델 상태를 확인한 뒤 다시 실행하세요.",
                    );
                } else {
                    self.notifications.clear(super::notifications::FailureId::AnalysisRun);
                }
                self.status = match (error, cancelled) {
                    (Some(error), _) => error,
                    (None, true) => "작업이 취소됨".into(),
                    (None, false) if job.mode == RunMode::Blocks => job.last_message.clone(),
                    (None, false) => {
                        let summary = summarize_warnings(&job.warnings)
                            .unwrap_or("번역 완료 · 문장을 확인한 뒤 인페인트(3)로 원문 글자를 지우세요");
                        match job.kept_blocks {
                            0 => summary.into(),
                            kept => format!("{summary} · 직접 고치거나 만든 블록 {kept}개는 그대로 뒀습니다"),
                        }
                    }
                };
                return Some(success);
            }
        }
        None
    }

    fn page_name(&self, page_id: &str) -> String {
        self.chapter.pages.iter().find(|page| page.id == page_id).map(|page| page.name.clone()).unwrap_or_default()
    }

    /// Replaces a page's generated blocks with the translation (block-type presets applied)
    /// as one undo step (`updatePageAfterAnalysis`); the user's own blocks stay.
    fn apply_translated_page(
        &mut self,
        page_id: &str,
        result: Result<Vec<(TranslationBlock, Option<SourceLettering>)>, String>,
    ) {
        let name = self.page_name(page_id);
        if let Some(job) = &mut self.translation {
            job.done += 1;
            if result.is_err() {
                job.failed += 1;
            }
            let number = job.page_number(page_id);
            job.last_message = match &result {
                Ok(blocks) => format!("{name}: 블록 {}개", blocks.len()),
                Err(error) => format!("{name}: 실패 — {}", mtt_translate::client::preview(error, 120)),
            };
            match &result {
                Ok(blocks) => {
                    job.phase = format!("{number} / {} 페이지 완료", job.total);
                    // `buildPageWarnings`: "[?]" marks uncertain OCR.
                    let uncertain = blocks
                        .iter()
                        .filter(|(block, _)| block.source_text.contains("[?]") || block.translated_text.contains("[?]"))
                        .count();
                    if uncertain > 0 {
                        job.warnings.push(format!("{name}: 불확실한 OCR 조각이 {uncertain}개 있습니다."));
                    }
                }
                Err(error) => {
                    job.phase = format!("{number} / {} 페이지 건너뜀", job.total);
                    job.warnings.push(format!(
                        "{name}: {}회 재시도 후 실패하여 이 페이지는 건너뜁니다. 마지막 오류: {}",
                        job.attempts,
                        mtt_translate::client::preview(error, 200)
                    ));
                }
            }
        }
        let is_current = self.page().is_some_and(|page| page.id == page_id);
        // Close pending edits (including a chapter-wide preset step) so the result becomes
        // its own undo step.
        self.commit_checkpoint();
        let presets = ensure_block_type_font_presets(self.chapter.font_presets.as_deref().unwrap_or(&[]));
        let Some(page) = self.chapter.pages.iter_mut().find(|page| page.id == page_id) else { return };
        let before = BlocksSnapshot::of(page);
        match result {
            Ok(translated) => {
                let blocks = translated
                    .into_iter()
                    .map(|(mut block, lettering)| {
                        apply_block_type_font_preset_to_block(&mut block, &presets);
                        if let Some(lettering) = lettering {
                            lettering.apply_to(&mut block);
                        }
                        block
                    })
                    .collect();
                let (blocks, kept) = merge_translated_blocks(page_id, &page.blocks, blocks);
                page.blocks = blocks;
                page.block_groups = Some(valid_groups(page)).filter(|groups| !groups.is_empty());
                if let Some(job) = &mut self.translation {
                    job.kept_blocks += kept;
                }
                page.analysis_status = PageAnalysisStatus::Completed;
                page.last_error = None;
                self.chapter.font_presets = Some(presets);
            }
            Err(error) => {
                page.analysis_status = PageAnalysisStatus::Failed;
                page.last_error = Some(error);
            }
        }
        page.updated_at = mtt_core::time::now_iso();
        let after = BlocksSnapshot::of(page);
        if after != before {
            self.history.push(Entry::Blocks { page_id: page_id.to_owned(), before, after });
        }
        self.dirty_pages.insert(page_id.to_owned());
        if is_current {
            self.selection.clear();
            self.reset_checkpoint();
        }
        self.revision += 1;
        self.chapter.status = resolve_chapter_status(&self.chapter.pages);
        self.mark_chapter_dirty();
    }
}

impl Editor {
    /// Puts the source and translation the model read for each block in place, as one undo
    /// step. Blocks the translation made stay generated; the user's own stay the user's.
    fn apply_translated_blocks(&mut self, page_id: &str, result: Result<Vec<(String, RegionText)>, String>) {
        let Ok(texts) = result else { return };
        self.commit_checkpoint();
        let Some(page) = self.chapter.pages.iter_mut().find(|page| page.id == page_id) else { return };
        let before = BlocksSnapshot::of(page);
        let mut changed = 0;
        for (block_id, text) in &texts {
            let Some(block) = page.blocks.iter_mut().find(|block| block.id == *block_id) else { continue };
            let generated = !block.is_user_work(page_id);
            block.source_text = text.jp.clone();
            block.translated_text = text.ko.clone();
            if generated {
                block.mark_generated();
            }
            changed += 1;
        }
        page.updated_at = mtt_core::time::now_iso();
        let after = BlocksSnapshot::of(page);
        if after != before {
            self.history.push(Entry::Blocks { page_id: page_id.to_owned(), before, after });
        }
        self.dirty_pages.insert(page_id.to_owned());
        self.revision += 1;
        self.mark_chapter_dirty();
        if let Some(job) = &mut self.translation {
            job.done = changed;
            let missed = job.total.saturating_sub(changed);
            job.last_message = match missed {
                0 => format!("블록 {changed}개를 다시 번역했습니다"),
                _ => format!("블록 {changed}개를 다시 번역했습니다 · {missed}개는 글자를 찾지 못해 그대로 뒀습니다"),
            };
        }
    }
}

/// Share of the smaller box a fresh block may overlap a kept one by.
const KEPT_OVERLAP: f64 = 0.4;

fn overlap_share(a: BBox, b: BBox) -> f64 {
    let w = (a.x + a.w).min(b.x + b.w) - a.x.max(b.x);
    let h = (a.y + a.h).min(b.y + b.h) - a.y.max(b.y);
    let smaller = (a.w * a.h).min(b.w * b.h);
    if w <= 0.0 || h <= 0.0 || smaller <= 0.0 { 0.0 } else { w * h / smaller }
}

/// The page's blocks after a translation returned `fresh` (presets applied) for a page that
/// had `old`: the user's own blocks stay on top in their order, fresh blocks over them are
/// dropped, and the rest replace the generated blocks, marked as generated. Also returns how
/// many blocks stayed.
fn merge_translated_blocks(
    page_id: &str,
    old: &[TranslationBlock],
    fresh: Vec<TranslationBlock>,
) -> (Vec<TranslationBlock>, usize) {
    let kept: Vec<&TranslationBlock> = old.iter().filter(|block| block.is_user_work(page_id)).collect();
    let mut used: std::collections::HashSet<String> = kept.iter().map(|block| block.id.clone()).collect();
    let mut next = 1;
    let mut blocks = Vec::with_capacity(fresh.len() + kept.len());
    for mut block in fresh {
        if kept.iter().any(|own| overlap_share(own.bbox, block.bbox) >= KEPT_OVERLAP) {
            continue;
        }
        // A kept block may still carry a translation id.
        if used.contains(&block.id) {
            while used.contains(&format!("{page_id}-block-{next}")) {
                next += 1;
            }
            block.id = format!("{page_id}-block-{next}");
        }
        used.insert(block.id.clone());
        block.mark_generated();
        blocks.push(block);
    }
    let count = kept.len();
    blocks.extend(kept.into_iter().cloned());
    (blocks, count)
}

/// Exposed for tests: chapter after applying a translated page.
#[cfg(test)]
pub fn apply_for_test(
    chapter: mtt_core::model::Chapter,
    page_id: &str,
    blocks: Vec<TranslationBlock>,
) -> mtt_core::model::Chapter {
    let mut editor = Editor::new(chapter);
    editor.apply_translated_page(page_id, Ok(blocks.into_iter().map(|block| (block, None)).collect()));
    editor.chapter
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtt_core::model::{BlockType, Chapter};

    fn page(id: &str, status: PageAnalysisStatus, completed: bool) -> PageRecord {
        PageRecord {
            id: id.into(),
            analysis_status: status,
            progress_completed: Some(completed),
            ..PageRecord::default()
        }
    }

    #[test]
    fn run_targets_follow_ts_rules() {
        let done = page("a", PageAnalysisStatus::Completed, false);
        let marked = page("b", PageAnalysisStatus::Idle, true);
        let pending = page("c", PageAnalysisStatus::Failed, false);
        assert!(!is_target(&done, RunMode::Pending, "x"));
        assert!(is_target(&done, RunMode::All, "x"));
        assert!(!is_target(&marked, RunMode::All, "x"));
        assert!(is_target(&marked, RunMode::CurrentPage, "b"), "explicit page ignores the done flag");
        assert!(is_target(&pending, RunMode::Pending, "x"));
    }

    #[test]
    fn translated_page_gets_presets_and_status() {
        let chapter = Chapter {
            id: "c".into(),
            pages: vec![PageRecord { id: "p".into(), width: 100, height: 100, ..PageRecord::default() }],
            page_order: vec!["p".into()],
            ..Chapter::default()
        };
        let block =
            TranslationBlock { id: "p-block-1".into(), block_type: BlockType::Sfx, ..TranslationBlock::default() };
        let chapter = apply_for_test(chapter, "p", vec![block]);
        let page = &chapter.pages[0];
        assert_eq!(page.analysis_status, PageAnalysisStatus::Completed);
        assert_eq!(page.blocks[0].font_preset_id.as_deref(), Some("font-preset-sfx"));
        assert_eq!(chapter.font_presets.as_ref().map(Vec::len), Some(4));
    }

    #[test]
    fn retranslation_keeps_the_users_own_blocks() {
        let at = |id: &str, x: f64, text: &str| TranslationBlock {
            id: id.into(),
            bbox: BBox::new(x, 100.0, 100.0, 100.0),
            translated_text: text.into(),
            ..TranslationBlock::default()
        };
        let mut generated = at("p-block-1", 0.0, "그대로");
        generated.mark_generated();
        let mut edited = at("p-block-2", 200.0, "생성");
        edited.mark_generated();
        edited.translated_text = "직접 고침".into();
        let manual = at("p-block-manual-5", 400.0, "직접 만듦");
        let legacy = at("p-block-3", 600.0, "예전 번역");
        let copy = TranslationBlock { id: "p-block-1-dup-9".into(), ..generated.clone() };
        let chapter = Chapter {
            id: "c".into(),
            pages: vec![PageRecord {
                id: "p".into(),
                width: 1000,
                height: 1000,
                blocks: vec![generated, edited, manual, legacy, copy],
                ..PageRecord::default()
            }],
            page_order: vec!["p".into()],
            ..Chapter::default()
        };
        let fresh = vec![
            at("p-block-1", 0.0, "새 1"),
            at("p-block-2", 210.0, "새 2"), // over the edited block: dropped
            at("p-block-3", 600.0, "새 3"),
            at("p-block-4", 800.0, "새 4"),
        ];
        let chapter = apply_for_test(chapter, "p", fresh);
        let blocks = &chapter.pages[0].blocks;
        let texts: Vec<&str> = blocks.iter().map(|block| block.translated_text.as_str()).collect();
        assert_eq!(texts, ["새 3", "새 4", "직접 고침", "직접 만듦", "그대로"]);
        let ids: std::collections::HashSet<&str> = blocks.iter().map(|block| block.id.as_str()).collect();
        assert_eq!(ids.len(), blocks.len(), "ids stay unique");
        assert!(blocks[0].generated_fingerprint.is_some() && !blocks[0].is_user_work("p"));
        assert!(blocks[2].is_user_work("p"));
    }

    #[test]
    fn block_translation_updates_texts_in_place() {
        let mut generated = TranslationBlock {
            id: "p-block-1".into(),
            bbox: BBox::new(10.0, 10.0, 100.0, 100.0),
            translated_text: "예전".into(),
            ..TranslationBlock::default()
        };
        generated.mark_generated();
        let manual = TranslationBlock {
            id: "p-block-manual-1".into(),
            bbox: BBox::new(300.0, 10.0, 100.0, 100.0),
            ..TranslationBlock::default()
        };
        let chapter = Chapter {
            id: "c".into(),
            pages: vec![PageRecord {
                id: "p".into(),
                width: 1000,
                height: 1000,
                blocks: vec![generated, manual],
                ..PageRecord::default()
            }],
            page_order: vec!["p".into()],
            ..Chapter::default()
        };
        let mut editor = Editor::new(chapter);
        let text = |jp: &str, ko: &str| RegionText { id: 1, jp: jp.into(), ko: ko.into() };
        editor.apply_translated_blocks(
            "p",
            Ok(vec![("p-block-1".into(), text("えっ", "엥?")), ("p-block-manual-1".into(), text("いや", "아니"))]),
        );
        let blocks = &editor.chapter.pages[0].blocks;
        assert_eq!((blocks[0].source_text.as_str(), blocks[0].translated_text.as_str()), ("えっ", "엥?"));
        assert_eq!(blocks[0].bbox, BBox::new(10.0, 10.0, 100.0, 100.0), "box kept");
        assert!(!blocks[0].is_user_work("p"), "still generated");
        assert_eq!(blocks[1].translated_text, "아니");
        assert!(blocks[1].is_user_work("p"), "still the user's");
        editor.undo(&egui::Context::default());
        assert_eq!(editor.chapter.pages[0].blocks[0].translated_text, "예전", "one undo step");
    }

    #[test]
    fn warning_summaries_match_ts() {
        assert_eq!(summarize_warnings(&[]), None);
        let skipped = "p: 5회 재시도 후 실패하여 이 페이지는 건너뜁니다. 마지막 오류: x".to_owned();
        let uncertain = "p: 불확실한 OCR 조각이 2개 있습니다.".to_owned();
        assert_eq!(
            summarize_warnings(std::slice::from_ref(&skipped)),
            Some("일부 페이지는 건너뛰고 다음 페이지로 진행했습니다.")
        );
        assert_eq!(summarize_warnings(std::slice::from_ref(&uncertain)), Some("일부 블록은 OCR 확인이 더 필요합니다."));
        assert_eq!(
            summarize_warnings(&[skipped, uncertain]),
            Some("일부 페이지를 건너뛰었고 OCR 확인이 필요한 블록도 있습니다.")
        );
        assert_eq!(
            summarize_warnings(&["p: 시도 1/5 실패 - e".into()]),
            Some("중간 경고가 있었지만 작업은 계속 진행되었습니다.")
        );
    }
}
