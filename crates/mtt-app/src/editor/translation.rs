//! Translation runs for a chapter (`analysisJob.ts`, `wholePagePipeline.ts`): pages are
//! translated on background threads (optionally in parallel) and applied as they finish.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Instant;

use mtt_core::font_presets::{apply_block_type_font_preset_to_block, ensure_block_type_font_presets};
use mtt_core::library::{PageLayer, resolve_chapter_status};
use mtt_core::model::{PageAnalysisStatus, PageRecord, TranslationBlock};
use mtt_translate::{TranslationConfig, overlay_item_to_block, translate_page};

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
}

pub enum TranslationEvent {
    Started { total: usize, endpoint: String },
    PageStarted { page_id: String },
    Attempt { page_id: String, attempt: u32, error: String },
    PageDone { page_id: String, result: Result<Vec<TranslationBlock>, String> },
    Finished { cancelled: bool, error: Option<String> },
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
    page_ids: Vec<String>,
}

impl TranslationJob {
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

/// `isAnalysisTargetForRun`.
pub fn is_target(page: &PageRecord, mode: RunMode, current_id: &str) -> bool {
    match mode {
        RunMode::CurrentPage => page.id == current_id,
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

        let config = TranslationConfig::from_settings(&services.settings);
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
            page_ids: ids,
        });
        let chapter_id = self.chapter.id.clone();
        let emitter = worker.emitter();
        std::thread::Builder::new()
            .name("mtt-translation".into())
            .spawn(move || {
                let send = |event| emitter.emit(AppEvent::Translation { chapter_id: chapter_id.clone(), event });
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
                                    items
                                        .iter()
                                        .enumerate()
                                        .map(|(i, item)| overlay_item_to_block(item, page_id, *width, *height, i))
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

    pub fn cancel_translation(&mut self) {
        if let Some(job) = &self.translation {
            job.cancel.store(true, Ordering::SeqCst);
            self.status = "번역 취소 중… (진행 중인 요청이 끝나면 멈춥니다)".into();
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
                    let title = if job.mode == RunMode::CurrentPage {
                        "페이지 번역 실패"
                    } else {
                        "번역 실행 실패"
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
                    (None, false) => summarize_warnings(&job.warnings)
                        .unwrap_or("번역 완료 · 문장을 확인한 뒤 인페인트(3)로 원문 글자를 지우세요")
                        .into(),
                };
                return Some(success);
            }
        }
        None
    }

    fn page_name(&self, page_id: &str) -> String {
        self.chapter.pages.iter().find(|page| page.id == page_id).map(|page| page.name.clone()).unwrap_or_default()
    }

    /// Replaces a page's blocks with the translation (block-type presets applied) as one
    /// undo step (`updatePageAfterAnalysis`).
    fn apply_translated_page(&mut self, page_id: &str, result: Result<Vec<TranslationBlock>, String>) {
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
                        .filter(|block| block.source_text.contains("[?]") || block.translated_text.contains("[?]"))
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
            Ok(mut blocks) => {
                for block in &mut blocks {
                    apply_block_type_font_preset_to_block(block, &presets);
                }
                page.blocks = blocks;
                page.block_groups = None;
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

/// Exposed for tests: chapter after applying a translated page.
#[cfg(test)]
pub fn apply_for_test(
    chapter: mtt_core::model::Chapter,
    page_id: &str,
    blocks: Vec<TranslationBlock>,
) -> mtt_core::model::Chapter {
    let mut editor = Editor::new(chapter);
    editor.apply_translated_page(page_id, Ok(blocks));
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
