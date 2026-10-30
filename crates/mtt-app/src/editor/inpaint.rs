//! Inpaint runs: one scope of a page computed on the worker, or every unfinished page
//! in turn (`applyInpaintAllPages`), and installing the layers they return.

use std::sync::Arc;
use std::time::Instant;

use eframe::egui;
use image::{GrayImage, RgbaImage};
use mtt_core::model::{InpaintEngine, InpaintSettings, InpaintStatus, TranslationBlock};
use mtt_inpaint::lama::Lama;
use mtt_inpaint::text_segmentation::TextSegmentation;

use super::assets::{self, PageAssets};
use super::history::{self, Entry, LayerKind, LayerPatch};
use super::{Editor, notifications};
use crate::app::AppEvent;
use crate::services::Services;
use crate::worker::{Worker, catch_panic};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InpaintScope {
    /// The whole current mask.
    WholeMask,
    /// The mask inside the range selection, merged into the existing result.
    Range,
    /// Text pixels detected inside the selected blocks, the mask grown by `grow_px` (0: as
    /// detected). "넓혀서 다시": the model draws streaks into some textures (dense hatching)
    /// when the mask hugs the glyphs, and fills them cleanly once it reaches past them.
    SelectedBlocks { grow_px: u32 },
    /// "전체 블록 인페인트": a fresh mask from every block's text pixels, replacing the
    /// existing mask and result.
    AllBlocks,
    /// The smart brush's last stroke (`Editor::heal_stroke`), filled from its surroundings
    /// and merged into the existing result (spot healing).
    Heal,
}

/// The smart brush's last stroke: the area to heal on its page.
#[derive(Debug, Clone)]
pub struct HealStroke {
    pub page_id: String,
    pub mask: Arc<GrayImage>,
    /// Fill the result only and leave the inpaint mask as it was (`BrushSettings::keep_mask`).
    pub keep_mask: bool,
}

/// What the "인페인트 실패" card's 재시도 runs again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LastInpaint {
    Page(InpaintScope),
    AllPages,
}

/// "전체 인페인트" over the chapter (`applyInpaintAllPages`).
pub struct InpaintQueue {
    pub pages: std::collections::VecDeque<String>,
    pub total: usize,
}

/// A finished inpaint, ready to swap in: the new layers with their textures and the undo
/// patches, all made on the worker.
pub struct InpaintOutcome {
    pub layers: assets::LayerSet,
    pub patches: Vec<(LayerKind, LayerPatch)>,
    pub engine: InpaintEngine,
    pub message: String,
}

pub(super) struct ComputedInpaint {
    pub(super) before_mask: Option<Arc<GrayImage>>,
    pub(super) before_result: Option<Arc<RgbaImage>>,
    pub(super) mask: GrayImage,
    pub(super) result: RgbaImage,
    pub(super) engine: InpaintEngine,
    pub(super) message: String,
}

impl ComputedInpaint {
    pub(super) fn finish(self, ctx: &egui::Context, page_id: &str) -> InpaintOutcome {
        let (width, height) = self.mask.dimensions();
        let mut patches = Vec::new();
        let mask_before = self.before_mask.as_deref().map_or_else(|| vec![0; self.mask.len()], |m| m.as_raw().clone());
        if let Some(patch) = history::diff_patch(&mask_before, self.mask.as_raw(), width, height, 1) {
            patches.push((LayerKind::Mask, patch));
        }
        drop(mask_before);
        let result_before =
            self.before_result.as_deref().map_or_else(|| vec![0; self.result.len()], |r| r.as_raw().clone());
        if let Some(patch) = history::diff_patch(&result_before, self.result.as_raw(), width, height, 4) {
            patches.push((LayerKind::Result, patch));
        }
        drop(result_before);
        let layers = assets::LayerSet::build(ctx, page_id, self.mask, self.result);
        InpaintOutcome { layers, patches, engine: self.engine, message: self.message }
    }
}

impl Editor {
    /// Detects LaMa (the downloaded model or a configured command) and prefers it when
    /// available, and the text segmentation model for block masks.
    pub fn configure_inpaint(&mut self, services: &Services) {
        self.lama = Lama::detect(&services.paths);
        self.text_segmentation = TextSegmentation::detect(&services.paths);
        self.inpaint_engine = if self.lama.is_some() { InpaintEngine::Lama } else { InpaintEngine::LocalFillFallback };
    }

    /// Turns anti-aliasing off (or back on) for this chapter's next inpaint runs.
    pub fn set_inpaint_aliased(&mut self, aliased: bool) {
        self.chapter.inpaint_aliased = aliased;
        self.result_brush.anti_alias = !aliased;
        self.mark_chapter_dirty();
        self.status = if aliased {
            "이 화는 안티앨리어싱 없이 인페인트합니다."
        } else {
            "이 화는 부드러운 경계로 인페인트합니다."
        }
        .into();
    }

    /// Starts an inpaint run for the current page on a worker thread.
    pub fn start_inpaint(&mut self, scope: InpaintScope, worker: &Worker<AppEvent>) {
        let Some(page_id) = self.page().map(|page| page.id.clone()) else { return };
        self.start_inpaint_on(&page_id, scope, worker);
    }

    pub(super) fn start_inpaint_on(&mut self, page_id: &str, scope: InpaintScope, worker: &Worker<AppEvent>) {
        if self.inpaint_running.is_some() {
            self.status = "인페인트가 이미 실행 중입니다.".into();
            return;
        }
        let Some(page) = self.chapter.pages.iter().find(|page| page.id == page_id).cloned() else { return };
        let Some(PageAssets::Ready(loaded)) = self.assets.get(&page.id) else {
            self.status = "페이지 이미지를 불러오는 중입니다.".into();
            return;
        };
        // Shared, not copied: the job copies a layer only if it changes it.
        let source = Arc::clone(&loaded.source);
        let mask = loaded.mask.clone();
        let result = loaded.result.clone();
        let has_mask = loaded.mask_has_pixels();
        let range = self.range.and_then(|r| {
            mtt_inpaint::Rect::clamped(r.min.x, r.min.y, r.width(), r.height(), source.width(), source.height())
        });
        let blocks: Vec<TranslationBlock> =
            if scope == InpaintScope::AllBlocks { page.blocks.clone() } else { self.selected_blocks() };
        if matches!(scope, InpaintScope::WholeMask | InpaintScope::Range) && !has_mask {
            self.status = "다시 인페인트할 마스크가 없습니다.".into();
            return;
        }
        if scope == InpaintScope::Range && range.is_none() {
            self.status = "부분 인페인트할 범위를 먼저 선택하세요.".into();
            return;
        }
        let heal = self.heal_stroke.clone().filter(|stroke| stroke.page_id == page_id);
        if scope == InpaintScope::Heal && heal.is_none() {
            return;
        }
        if matches!(scope, InpaintScope::SelectedBlocks { .. } | InpaintScope::AllBlocks) && blocks.is_empty() {
            return;
        }
        let engine = if self.inpaint_engine == InpaintEngine::Lama && self.lama.is_none() {
            InpaintEngine::LocalFillFallback
        } else {
            self.inpaint_engine
        };
        let lama = self.lama.clone();
        let segmentation = self.text_segmentation.clone();
        let jpeg = matches!(
            std::path::Path::new(if page.image_path.is_empty() { &page.name } else { &page.image_path })
                .extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext.to_ascii_lowercase())
                .as_deref(),
            Some("jpg" | "jpeg")
        );
        let settings = InpaintSettings {
            engine,
            // Artifact cleanup only helps JPEG compression noise (`resolvePageInpaintSettings`).
            artifact_cleanup_px: Some(if jpeg { 8.0 } else { 0.0 }),
            aliased: self.chapter.inpaint_aliased,
            ..page.inpaint_settings.clone().unwrap_or_default()
        };
        let chapter_id = self.chapter.id.clone();
        let page_id = page.id.clone();
        if self.inpaint_queue.is_none() {
            self.last_inpaint = Some(LastInpaint::Page(scope));
        }
        self.inpaint_running = Some(page_id.clone());
        if let Some(record) = self.chapter.pages.iter_mut().find(|record| record.id == page_id) {
            record.inpaint_status = Some(InpaintStatus::Running);
            record.inpaint_settings.get_or_insert_default().aliased = settings.aliased;
        }
        self.status = format!("인페인트 실행 중… ({})", engine_label(engine));
        let ctx = worker.ctx().clone();
        worker.spawn(move || {
            let outcome = catch_panic(|| {
                compute_inpaint(
                    scope,
                    &source,
                    mask,
                    result,
                    range,
                    heal.as_ref(),
                    &blocks,
                    engine,
                    &settings,
                    lama.as_ref(),
                    segmentation.as_ref(),
                )
                .map(|computed| computed.finish(&ctx, &page_id))
            })
            .and_then(|outcome| outcome);
            AppEvent::InpaintFinished { chapter_id, page_id, outcome }
        });
    }

    pub fn on_inpaint_finished(&mut self, chapter_id: &str, page_id: &str, outcome: Result<InpaintOutcome, String>) {
        if chapter_id != self.chapter.id {
            return;
        }
        self.inpaint_running = None;
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) => {
                log::error!("inpaint failed: {error}");
                let name = self.chapter.pages.iter().find(|page| page.id == page_id).map(|page| page.name.clone());
                self.status = match (&self.inpaint_queue, name) {
                    // The all-pages run stops at the first failure.
                    (Some(_), Some(name)) => format!("{name} 인페인트 실패: {error}"),
                    _ => format!("인페인트 실패: {error}"),
                };
                let (title, message) = match self.last_inpaint {
                    Some(LastInpaint::AllPages) => {
                        ("모든 페이지 인페인트 실패", "완료된 페이지는 유지됩니다. 실패한 페이지부터 다시 실행하세요.")
                    }
                    Some(LastInpaint::Page(InpaintScope::Heal)) => {
                        ("스팟 복구 실패", "기존 결과 레이어는 유지됩니다. 다시 실행하세요.")
                    }
                    Some(LastInpaint::Page(InpaintScope::Range)) => {
                        ("부분 인페인트 실패", "기존 결과 레이어와 선택 범위는 유지됩니다. 다시 실행하세요.")
                    }
                    Some(LastInpaint::Page(InpaintScope::SelectedBlocks { .. })) => {
                        ("선택 블록 인페인트 실패", "기존 결과 레이어와 마스크는 유지됩니다. 다시 실행하세요.")
                    }
                    _ => ("인페인트 실패", "마스크와 편집 상태는 유지됩니다. 설정을 확인한 뒤 다시 실행하세요."),
                };
                self.notifications.report(notifications::FailureId::InpaintRun, title, message);
                self.inpaint_queue = None;
                if let Some(page) = self.chapter.pages.iter_mut().find(|page| page.id == page_id) {
                    page.inpaint_status = Some(InpaintStatus::Failed);
                }
                return;
            }
        };
        let Some(PageAssets::Ready(loaded)) = self.assets.get_mut(page_id) else {
            self.status = "인페인트 결과를 적용할 페이지가 없습니다.".into();
            if let Some(queue) = &mut self.inpaint_queue {
                queue.pages.retain(|id| id != page_id);
                if queue.pages.is_empty() {
                    self.inpaint_queue = None;
                }
            }
            return;
        };
        // Layer edits are locked while an inpaint runs, so the job's diff against the layers
        // it started from is the change to undo.
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
            let mut settings = page.inpaint_settings.clone().unwrap_or_default();
            settings.engine = outcome.engine;
            page.inpaint_settings = Some(settings);
            page.updated_at = mtt_core::time::now_iso();
        }
        self.mark_chapter_dirty();
        self.status = outcome.message;
        self.notifications.clear(notifications::FailureId::InpaintRun);
        if let Some(queue) = &mut self.inpaint_queue
            && queue.pages.front().map(String::as_str) == Some(page_id)
        {
            queue.pages.pop_front();
            let done = queue.total - queue.pages.len();
            let name = self.chapter.pages.iter().find(|page| page.id == page_id).map(|p| p.name.clone());
            self.status = format!("{done}/{} {} 인페인트 완료", queue.total, name.unwrap_or_default());
            if queue.pages.is_empty() {
                self.status = format!("모든 페이지 인페인트 완료: {}p", queue.total);
                self.inpaint_queue = None;
            }
        }
    }

    /// `applyInpaintAllPages`: queues every unfinished page with blocks; `tick` runs them
    /// one by one once each page's images are loaded.
    pub fn start_inpaint_all_pages(&mut self) {
        if self.inpaint_running.is_some() || self.inpaint_queue.is_some() {
            return;
        }
        let pages: std::collections::VecDeque<String> = self
            .chapter
            .pages
            .iter()
            .filter(|page| !page.blocks.is_empty() && !page.is_completed())
            .map(|page| page.id.clone())
            .collect();
        if pages.is_empty() {
            self.status = "인페인트할 미완료 페이지가 없습니다.".into();
            return;
        }
        self.status = format!("모든 페이지 인페인트 시작: {}p", pages.len());
        self.last_inpaint = Some(LastInpaint::AllPages);
        self.inpaint_queue = Some(InpaintQueue { total: pages.len(), pages });
    }

    /// Starts the next queued page once it is decoded.
    pub(super) fn advance_inpaint_queue(&mut self, worker: &Worker<AppEvent>) {
        if self.inpaint_running.is_some() {
            return;
        }
        let Some(page_id) = self.inpaint_queue.as_ref().and_then(|queue| queue.pages.front()).cloned() else {
            return;
        };
        if !self.chapter.pages.iter().any(|page| page.id == page_id) {
            // The page was deleted while queued.
            if let Some(queue) = &mut self.inpaint_queue {
                queue.pages.pop_front();
                if queue.pages.is_empty() {
                    self.status = format!("모든 페이지 인페인트 완료: {}p", queue.total);
                    self.inpaint_queue = None;
                }
            }
            return;
        }
        match self.assets.get(&page_id) {
            Some(PageAssets::Ready(_)) => self.start_inpaint_on(&page_id, InpaintScope::AllBlocks, worker),
            Some(PageAssets::Failed(error)) => {
                let name = self.chapter.pages.iter().find(|page| page.id == page_id).map(|p| p.name.clone());
                self.status = format!("{} 인페인트 실패: {error}", name.unwrap_or_default());
                self.inpaint_queue = None;
            }
            _ => {}
        }
    }

    pub fn inpaint_busy(&self) -> bool {
        self.inpaint_running.is_some() || self.inpaint_queue.is_some() || self.psd_import_running.is_some()
    }
}

pub fn engine_label(engine: InpaintEngine) -> &'static str {
    match engine {
        InpaintEngine::Lama => "LaMa",
        InpaintEngine::MaskFillFallback => "마스크 채우기",
        _ => "로컬 채우기",
    }
}

/// Worker-side inpaint for one scope; returns the new full mask and result layers. Block
/// masks take their glyph pixels from `segmentation` when given.
#[allow(clippy::too_many_arguments)]
pub(super) fn compute_inpaint(
    scope: InpaintScope,
    source: &RgbaImage,
    before_mask: Option<Arc<GrayImage>>,
    before_result: Option<Arc<RgbaImage>>,
    range: Option<mtt_inpaint::Rect>,
    heal: Option<&HealStroke>,
    blocks: &[TranslationBlock],
    engine: InpaintEngine,
    settings: &InpaintSettings,
    lama: Option<&Lama>,
    segmentation: Option<&TextSegmentation>,
) -> Result<ComputedInpaint, String> {
    let started = Instant::now();
    // "넓혀서 다시" grows this run's mask only; the page keeps its own padding.
    let grown;
    let settings = match scope {
        InpaintScope::SelectedBlocks { grow_px } if grow_px > 0 => {
            grown = InpaintSettings { padding_px: settings.padding_px + f64::from(grow_px), ..settings.clone() };
            &grown
        }
        _ => settings,
    };
    let (width, height) = source.dimensions();
    let (mask, result) = (before_mask.as_deref(), before_result.as_deref());
    let has_layers = mask.is_some_and(|m| m.iter().any(|value| *value > 0)) || result.is_some();
    let patch_mask = match scope {
        InpaintScope::WholeMask => {
            let mask = mask.ok_or("다시 인페인트할 마스크가 없습니다.")?;
            if !mask.iter().any(|value| *value > 0) {
                return Err("다시 인페인트할 마스크가 없습니다.".into());
            }
            mask.clone()
        }
        InpaintScope::Range => {
            let base = mask.ok_or("다시 인페인트할 마스크가 없습니다.")?;
            mtt_inpaint::mask_for_selection(base, range.ok_or("부분 인페인트할 범위를 먼저 선택하세요.")?)
                .ok_or("선택 범위 안에 마스크 픽셀이 없습니다.")?
        }
        InpaintScope::Heal => {
            let stroke = &heal.ok_or("복구할 영역이 없습니다.")?.mask;
            if !stroke.iter().any(|value| *value > 0) {
                return Err("복구할 영역이 없습니다.".into());
            }
            (**stroke).clone()
        }
        InpaintScope::SelectedBlocks { .. } | InpaintScope::AllBlocks => {
            let refs: Vec<&TranslationBlock> = blocks.iter().collect();
            let patch = mtt_inpaint::text_segmentation::mask_for_blocks(source, &refs, None, segmentation);
            if !patch.pixels().any(|p| p[0] > 0) {
                return Err("블록에서 지울 글자를 찾지 못했습니다.".into());
            }
            patch
        }
    };
    let mut patch_mask = patch_mask;
    // A partial run sees the page as it shows now, so text already erased next to it
    // isn't copied back into the fill.
    let shown = match (scope, mask, result) {
        (InpaintScope::Range | InpaintScope::SelectedBlocks { .. } | InpaintScope::Heal, Some(mask), Some(result)) => {
            Some(mtt_inpaint::composite_result(source, result, mask))
        }
        _ => None,
    };
    let (mut patch_result, region) =
        mtt_inpaint::run_inpaint_in(shown.as_ref().unwrap_or(source), &patch_mask, engine, settings, lama)?;
    // Outside the engine's region the patch is empty and the masks are binary, so the
    // follow-up steps only look inside it.
    let region = region.unwrap_or(mtt_inpaint::Rect { x: 0, y: 0, width: 0, height: 0 });
    mtt_inpaint::normalize_layers_in(&mut patch_mask, &mut patch_result, region);
    let whole = matches!(scope, InpaintScope::WholeMask | InpaintScope::AllBlocks) || !has_layers;
    let (mask, result) = if whole {
        (patch_mask, patch_result)
    } else {
        let merged_result = mtt_inpaint::merge_partial_result_in(result, &patch_result, &patch_mask, region);
        let mut merged_mask = mask.cloned().unwrap_or_else(|| GrayImage::new(width, height));
        mtt_inpaint::merge_masks_in(&mut merged_mask, &patch_mask, region);
        (merged_mask, merged_result)
    };
    // A heal that keeps the mask changes the result layer only.
    let mask = if scope == InpaintScope::Heal && heal.is_some_and(|stroke| stroke.keep_mask) {
        before_mask.as_deref().cloned().unwrap_or_else(|| GrayImage::new(width, height))
    } else {
        mask
    };
    let local = engine == InpaintEngine::LocalFillFallback;
    let multi = blocks.len() > 1;
    // `useInpaintRunActions` status messages.
    let message = match (scope, local) {
        (InpaintScope::SelectedBlocks { grow_px }, _) if grow_px > 0 => {
            format!("선택 블록을 {grow_px}px 넓혀 다시 인페인트했습니다.")
        }
        (InpaintScope::WholeMask, false) => "현재 마스크 기준으로 인페인트 결과를 다시 저장했습니다.".into(),
        (InpaintScope::AllBlocks, false) => "전체 블록 인페인트 결과를 저장했습니다.".into(),
        (InpaintScope::WholeMask | InpaintScope::AllBlocks, true) => "로컬 인페인트 결과를 저장했습니다.".into(),
        (InpaintScope::Range, false) => "선택 범위만 다시 인페인트했습니다.".into(),
        (InpaintScope::Range, true) => "선택 범위 로컬 인페인트 결과를 저장했습니다.".into(),
        (InpaintScope::Heal, false) => "칠한 곳을 주변으로 메웠습니다.".into(),
        (InpaintScope::Heal, true) => "칠한 곳을 로컬 채우기로 메웠습니다.".into(),
        (InpaintScope::SelectedBlocks { .. }, false) if multi => "선택된 블록만 인페인트했습니다.".into(),
        (InpaintScope::SelectedBlocks { .. }, false) => "선택 블록만 인페인트했습니다.".into(),
        (InpaintScope::SelectedBlocks { .. }, true) if multi => "선택된 블록 로컬 인페인트 결과를 저장했습니다.".into(),
        (InpaintScope::SelectedBlocks { .. }, true) => "선택 블록 로컬 인페인트 결과를 저장했습니다.".into(),
    };
    log::info!("inpaint {scope:?} done with {} in {:.1}s", engine_label(engine), started.elapsed().as_secs_f32());
    Ok(ComputedInpaint { before_mask, before_result, mask, result, engine, message })
}
