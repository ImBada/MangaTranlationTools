//! Undo/redo. Block edits are recorded as before/after snapshots of one page's blocks;
//! layer edits as before/after pixel patches of the changed rectangle, run-length packed
//! (masks are binary and results transparent outside the mask, so a page-sized inpaint
//! patch shrinks from tens of MB to well under one). The `Editor` methods below record edits
//! as steps (against a checkpoint of the page's blocks) and apply them on undo and redo.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use eframe::egui;
use image::{GrayImage, RgbaImage};
use mtt_core::model::{PageRecord, StyleKey, TranslationBlock, TranslationBlockGroup};

use super::Editor;
use super::assets::PageAssets;

const HISTORY_LIMIT: usize = 200;
/// Packed layer bytes kept across undo and redo; the oldest steps go first past it.
const LAYER_BYTE_BUDGET: usize = 384 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct BlocksSnapshot {
    pub blocks: Vec<TranslationBlock>,
    pub groups: Option<Vec<TranslationBlockGroup>>,
    pub completed: Option<bool>,
}

impl BlocksSnapshot {
    pub fn of(page: &PageRecord) -> Self {
        Self { blocks: page.blocks.clone(), groups: page.block_groups.clone(), completed: page.progress_completed }
    }

    /// Whether `page` still has these blocks (compared in place, without a snapshot).
    pub fn matches(&self, page: &PageRecord) -> bool {
        self.completed == page.progress_completed && self.groups == page.block_groups && self.blocks == page.blocks
    }

    pub fn apply(&self, page: &mut PageRecord) {
        page.blocks = self.blocks.clone();
        page.block_groups = self.groups.clone();
        page.progress_completed = self.completed;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LayerKind {
    Mask,
    Result,
}

impl LayerKind {
    pub fn page_layer(self) -> mtt_core::library::PageLayer {
        match self {
            LayerKind::Mask => mtt_core::library::PageLayer::InpaintMask,
            LayerKind::Result => mtt_core::library::PageLayer::InpaintResult,
        }
    }
}

/// `(x, y, width, height)` in page pixels.
pub type PixelRect = (u32, u32, u32, u32);

/// Run-length packed bytes. Tokens are a varint `n` followed by `n / 2` literal bytes
/// (even `n`) or by one byte repeated `n / 2` times (odd `n`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packed {
    bytes: Vec<u8>,
    len: usize,
}

/// Runs shorter than this stay in a literal (a run token costs up to 6 bytes).
const MIN_RUN: usize = 8;

fn push_varint(out: &mut Vec<u8>, mut value: usize) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn read_varint(bytes: &[u8], at: &mut usize) -> usize {
    let (mut value, mut shift) = (0usize, 0);
    loop {
        let byte = bytes[*at];
        *at += 1;
        value |= usize::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return value;
        }
        shift += 7;
    }
}

impl Packed {
    pub fn pack(data: &[u8]) -> Self {
        let mut bytes = Vec::new();
        let mut literal_start = 0;
        let mut index = 0;
        while index < data.len() {
            let value = data[index];
            let run = data[index..].iter().take_while(|byte| **byte == value).count();
            if run >= MIN_RUN {
                if literal_start < index {
                    push_varint(&mut bytes, (index - literal_start) << 1);
                    bytes.extend_from_slice(&data[literal_start..index]);
                }
                push_varint(&mut bytes, (run << 1) | 1);
                bytes.push(value);
                index += run;
                literal_start = index;
            } else {
                index += run;
            }
        }
        if literal_start < data.len() {
            push_varint(&mut bytes, (data.len() - literal_start) << 1);
            bytes.extend_from_slice(&data[literal_start..]);
        }
        bytes.shrink_to_fit();
        Self { bytes, len: data.len() }
    }

    pub fn unpack(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.len);
        let mut at = 0;
        while at < self.bytes.len() {
            let token = read_varint(&self.bytes, &mut at);
            let count = token >> 1;
            if token & 1 == 1 {
                out.resize(out.len() + count, self.bytes[at]);
                at += 1;
            } else {
                out.extend_from_slice(&self.bytes[at..at + count]);
                at += count;
            }
        }
        out
    }

    /// Memory held.
    pub fn packed_len(&self) -> usize {
        self.bytes.len()
    }
}

/// Pixels of a rectangle, before and after: 1 byte per pixel for masks, 4 (RGBA) for results.
pub struct LayerPatch {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub before: Packed,
    pub after: Packed,
}

impl LayerPatch {
    pub fn new(rect: PixelRect, before: &[u8], after: &[u8]) -> Self {
        let (x, y, width, height) = rect;
        Self { x, y, width, height, before: Packed::pack(before), after: Packed::pack(after) }
    }

    fn packed_len(&self) -> usize {
        self.before.packed_len() + self.after.packed_len()
    }
}

/// What a stroke is about to change, saved tile by tile the first time the stroke reaches
/// it, so pressing a brush doesn't copy the whole layer for its undo step.
pub struct StrokeBackup {
    width: u32,
    height: u32,
    channels: u32,
    tiles: HashMap<(u32, u32), Vec<u8>>,
}

const BACKUP_TILE: u32 = 64;

impl StrokeBackup {
    pub fn new(width: u32, height: u32, channels: u32) -> Self {
        Self { width, height, channels, tiles: HashMap::new() }
    }

    fn tile_rect(&self, tx: u32, ty: u32) -> PixelRect {
        let (x, y) = (tx * BACKUP_TILE, ty * BACKUP_TILE);
        (x, y, BACKUP_TILE.min(self.width - x), BACKUP_TILE.min(self.height - y))
    }

    fn tiles_in(&self, rect: PixelRect) -> impl Iterator<Item = (u32, u32)> + use<> {
        let (x, y, width, height) = rect;
        let right = (x + width).min(self.width);
        let bottom = (y + height).min(self.height);
        let columns = if x < right { x / BACKUP_TILE..(right - 1) / BACKUP_TILE + 1 } else { 0..0 };
        let rows = if y < bottom { y / BACKUP_TILE..(bottom - 1) / BACKUP_TILE + 1 } else { 0..0 };
        rows.flat_map(move |ty| columns.clone().map(move |tx| (tx, ty)))
    }

    /// Saves the tiles of `rect` not saved yet. Call before `data` changes inside `rect`.
    pub fn save(&mut self, data: &[u8], rect: PixelRect) {
        for (tx, ty) in self.tiles_in(rect).collect::<Vec<_>>() {
            if !self.tiles.contains_key(&(tx, ty)) {
                let tile = read_rect(data, self.width, self.channels, self.tile_rect(tx, ty));
                self.tiles.insert((tx, ty), tile);
            }
        }
    }

    /// The pixels of `rect` before the stroke; `data` is the layer now.
    pub fn before(&self, data: &[u8], rect: PixelRect) -> Vec<u8> {
        let (x, y, width, height) = rect;
        let channels = self.channels as usize;
        let mut out = Vec::with_capacity(width as usize * height as usize * channels);
        for row in y..y + height {
            let mut column = x;
            while column < x + width {
                let (tx, ty) = (column / BACKUP_TILE, row / BACKUP_TILE);
                let end = ((tx + 1) * BACKUP_TILE).min(x + width);
                let count = (end - column) as usize * channels;
                match self.tiles.get(&(tx, ty)) {
                    Some(tile) => {
                        let (tile_x, tile_y, tile_width, _) = self.tile_rect(tx, ty);
                        let start = (((row - tile_y) * tile_width + column - tile_x) as usize) * channels;
                        out.extend_from_slice(&tile[start..start + count]);
                    }
                    None => {
                        let start = ((row * self.width + column) as usize) * channels;
                        out.extend_from_slice(&data[start..start + count]);
                    }
                }
                column = end;
            }
        }
        out
    }

    /// Puts back the pixels of `rect` outside `keep`: the stroke only changes the
    /// selection, like Photoshop's.
    pub fn restore_outside(&self, data: &mut [u8], rect: PixelRect, keep: PixelRect) {
        let (x, y, width, height) = rect;
        let (right, bottom) = (x + width, y + height);
        let (keep_left, keep_top) = (keep.0, keep.1);
        let (keep_right, keep_bottom) = (keep.0 + keep.2, keep.1 + keep.3);
        let channels = self.channels as usize;
        for row in y..bottom {
            let spans = if (keep_top..keep_bottom).contains(&row) {
                [(x, keep_left.clamp(x, right)), (keep_right.clamp(x, right), right)]
            } else {
                [(x, right), (right, right)]
            };
            for (from, to) in spans.into_iter().filter(|(from, to)| from < to) {
                let before = self.before(data, (from, row, to - from, 1));
                let start = (row * self.width + from) as usize * channels;
                data[start..start + before.len()].copy_from_slice(&before);
            }
        }
    }
}

pub enum Entry {
    Blocks {
        page_id: String,
        before: BlocksSnapshot,
        after: BlocksSnapshot,
    },
    Layer {
        page_id: String,
        layer: LayerKind,
        patch: LayerPatch,
    },
    /// Several changes on one page undone/redone together (e.g. an inpaint run).
    Batch {
        page_id: String,
        entries: Vec<Entry>,
    },
    /// Font preset edits with every block they changed across the chapter.
    Presets {
        page_id: String,
        before: super::presets::PresetSnapshot,
        after: super::presets::PresetSnapshot,
    },
}

impl Entry {
    /// Packed layer bytes held by this step.
    fn layer_bytes(&self) -> usize {
        match self {
            Entry::Layer { patch, .. } => patch.packed_len(),
            Entry::Batch { entries, .. } => entries.iter().map(Entry::layer_bytes).sum(),
            Entry::Blocks { .. } | Entry::Presets { .. } => 0,
        }
    }

    pub fn page_id(&self) -> &str {
        match self {
            Entry::Blocks { page_id, .. }
            | Entry::Layer { page_id, .. }
            | Entry::Batch { page_id, .. }
            | Entry::Presets { page_id, .. } => page_id,
        }
    }
}

/// Bounding box of the pixels that differ between two same-sized raw buffers.
pub fn changed_rect(
    before: &[u8],
    after: &[u8],
    width: u32,
    height: u32,
    channels: u32,
) -> Option<(u32, u32, u32, u32)> {
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (width, height, 0, 0);
    let stride = (width * channels) as usize;
    for y in 0..height as usize {
        let row = y * stride;
        if before[row..row + stride] == after[row..row + stride] {
            continue;
        }
        for x in 0..width as usize {
            let offset = row + x * channels as usize;
            if before[offset..offset + channels as usize] != after[offset..offset + channels as usize] {
                min_x = min_x.min(x as u32);
                max_x = max_x.max(x as u32);
                min_y = min_y.min(y as u32);
                max_y = max_y.max(y as u32);
            }
        }
    }
    (min_x <= max_x).then(|| (min_x, min_y, max_x - min_x + 1, max_y - min_y + 1))
}

/// Copies a rect out of a raw buffer.
pub fn read_rect(data: &[u8], image_width: u32, channels: u32, rect: PixelRect) -> Vec<u8> {
    let (x, y, width, height) = rect;
    let mut bytes = Vec::with_capacity((width * height * channels) as usize);
    for row in y..y + height {
        let start = ((row * image_width + x) * channels) as usize;
        bytes.extend_from_slice(&data[start..start + (width * channels) as usize]);
    }
    bytes
}

/// Patch for the changed area between two full layers, `None` when identical.
pub fn diff_patch(before: &[u8], after: &[u8], width: u32, height: u32, channels: u32) -> Option<LayerPatch> {
    let rect = changed_rect(before, after, width, height, channels)?;
    Some(LayerPatch::new(rect, &read_rect(before, width, channels, rect), &read_rect(after, width, channels, rect)))
}

/// Selection state recorded with an undo step and restored with it (the TS snapshot's
/// `selectedBlockId` / `editingFontPresetId`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HistoryContext {
    pub selection: Vec<String>,
    pub editing_preset_id: Option<String>,
}

pub type Recorded = (Entry, HistoryContext);

#[derive(Default)]
pub struct History {
    undo: Vec<Recorded>,
    redo: Vec<Recorded>,
    /// Context attached to the next pushed entry (kept current by the editor).
    pub context: HistoryContext,
}

impl History {
    pub fn push(&mut self, entry: Entry) {
        self.undo.push((entry, self.context.clone()));
        if self.undo.len() > HISTORY_LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.enforce_budget();
    }

    /// Drops the oldest steps while the layer patches exceed `LAYER_BYTE_BUDGET` (the
    /// newest step always stays).
    fn enforce_budget(&mut self) {
        let mut total: usize = self.undo.iter().chain(&self.redo).map(|(entry, _)| entry.layer_bytes()).sum();
        let mut drop = 0;
        while total > LAYER_BYTE_BUDGET && drop + 1 < self.undo.len() {
            total -= self.undo[drop].0.layer_bytes();
            drop += 1;
        }
        self.undo.drain(..drop);
    }

    pub fn pop_undo(&mut self) -> Option<Recorded> {
        self.undo.pop()
    }

    pub fn pop_redo(&mut self) -> Option<Recorded> {
        self.redo.pop()
    }

    pub fn push_redo(&mut self, recorded: Recorded) {
        self.redo.push(recorded);
    }

    /// Re-pushes an entry that was redone without clearing the redo stack.
    pub fn push_undone(&mut self, recorded: Recorded) {
        self.undo.push(recorded);
    }

    #[cfg(test)]
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    #[cfg(test)]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Drops entries of a deleted page (they can no longer be applied).
    pub fn forget_page(&mut self, page_id: &str) {
        let keep = |(entry, _): &Recorded| matches!(entry, Entry::Presets { .. }) || entry.page_id() != page_id;
        self.undo.retain(keep);
        self.redo.retain(keep);
    }
}

impl Editor {
    pub(super) fn reset_checkpoint(&mut self) {
        self.checkpoint = self.page().map(|page| (page.id.clone(), BlocksSnapshot::of(page)));
        self.last_text_edit = None;
    }

    /// Records the change since the last checkpoint as one undo step.
    pub fn commit_checkpoint(&mut self) {
        if self.commit_preset_checkpoint() {
            self.reset_checkpoint();
            self.pending_context = None;
            return;
        }
        let Some((page_id, before)) = self.checkpoint.take() else {
            self.reset_checkpoint();
            return;
        };
        if let Some(page) = self.chapter.pages.iter().find(|page| page.id == page_id) {
            let after = BlocksSnapshot::of(page);
            if after != before {
                self.history.push(Entry::Blocks { page_id: page_id.clone(), before, after });
                // The recorded selection belongs to this step only.
                self.pending_context = None;
            }
        }
        self.reset_checkpoint();
    }

    /// Called after every mutation of the current page.
    pub(super) fn touch_page(&mut self) {
        self.capture_history_context();
        self.revision += 1;
        if let Some(page) = self.chapter.pages.get_mut(self.current) {
            page.updated_at = mtt_core::time::now_iso();
            self.dirty_pages.insert(page.id.clone());
        }
        self.mark_chapter_dirty();
    }

    pub(super) fn mark_chapter_dirty(&mut self) {
        self.chapter_dirty = true;
        self.last_change = Some(Instant::now());
    }

    /// Applies `edit` to the current page and commits it as its own undo step.
    pub fn edit_page(&mut self, edit: impl FnOnce(&mut PageRecord)) {
        self.commit_checkpoint();
        if let Some(page) = self.page_mut() {
            edit(page);
        }
        self.touch_page();
        self.commit_checkpoint();
    }

    /// Applies `edit` to every selected block. Continuous edits (sliders, typing) are
    /// merged into one undo step by the checkpoint logic.
    pub fn edit_selected(&mut self, mut edit: impl FnMut(&mut TranslationBlock)) {
        let selection: HashSet<String> = self.selection.iter().cloned().collect();
        if let Some(page) = self.page_mut() {
            for block in page.blocks.iter_mut().filter(|block| selection.contains(&block.id)) {
                edit(block);
            }
        }
        self.last_text_edit = Some(Instant::now());
        self.touch_page();
    }

    /// Edits a style field of the selected blocks; linked fields of assigned blocks edit
    /// the font preset (see `presets.rs`).
    pub fn edit_selected_style(&mut self, _key: StyleKey, edit: impl FnMut(&mut TranslationBlock)) {
        self.edit_style(edit);
    }

    pub fn undo(&mut self, ctx: &egui::Context) {
        self.commit_checkpoint();
        let Some((entry, context)) = self.history.pop_undo() else {
            return;
        };
        if !self.prepare_history_entry(&entry, true) {
            self.history.push_undone((entry, context));
            return;
        }
        self.apply_history_entry(ctx, &entry, true);
        self.restore_history_context(&context);
        self.reset_checkpoint();
        self.pending_context = None;
        self.history.push_redo((entry, context));
    }

    pub fn redo(&mut self, ctx: &egui::Context) {
        self.commit_checkpoint();
        let Some((entry, context)) = self.history.pop_redo() else {
            return;
        };
        if !self.prepare_history_entry(&entry, false) {
            self.history.push_redo((entry, context));
            return;
        }
        self.apply_history_entry(ctx, &entry, false);
        self.restore_history_context(&context);
        self.reset_checkpoint();
        self.pending_context = None;
        self.history.push_undone((entry, context));
    }

    /// Selection and editing preset as they were when the step was recorded.
    fn restore_history_context(&mut self, context: &HistoryContext) {
        let live: HashSet<String> =
            self.page().map(|page| page.blocks.iter().map(|block| block.id.clone()).collect()).unwrap_or_default();
        if self.layer.edits_blocks() {
            self.selection = context.selection.iter().filter(|id| live.contains(*id)).cloned().collect();
        }
        self.editing_preset_id = context.editing_preset_id.clone();
    }

    pub(super) fn current_history_context(&self) -> HistoryContext {
        HistoryContext { selection: self.selection.clone(), editing_preset_id: self.editing_preset_id.clone() }
    }

    #[cfg(test)]
    pub(super) fn history_can_redo_for_test(&self) -> bool {
        self.history.can_redo()
    }

    #[cfg(test)]
    pub(super) fn tick_for_test(&mut self) {
        if self.pending_context.is_none() {
            self.history.context = self.current_history_context();
        }
    }

    /// Remembers the selection before the first change of the next undo step
    /// (`recordTranslationUndoSnapshot` is called before the change in the TS app).
    pub(super) fn capture_history_context(&mut self) {
        if self.pending_context.is_none() {
            let context = self.current_history_context();
            self.history.context = context.clone();
            self.pending_context = Some(context);
        }
    }

    /// Navigates to the entry's page. Returns `false` (entry kept) while layer pixels the
    /// entry needs are still loading, or while an inpaint or PSD import runs: it installs
    /// layers diffed against the ones it started from, so a layer step applied meanwhile
    /// would come back when the job lands.
    fn prepare_history_entry(&mut self, entry: &Entry, undo: bool) -> bool {
        if self.inpaint_busy() && entry_touches_layers(entry) {
            self.status = if undo {
                "인페인트 작업이 끝난 뒤에 레이어 변경을 실행 취소하세요."
            } else {
                "인페인트 작업이 끝난 뒤에 레이어 변경을 다시 실행하세요."
            }
            .into();
            return false;
        }
        let Some(index) = self.chapter.pages.iter().position(|page| page.id == entry.page_id()) else {
            // Chapter-wide preset steps still apply after the page they were made on is gone.
            return matches!(entry, Entry::Presets { .. });
        };
        if index != self.current {
            self.go_to_page(index);
        }
        if entry_touches_layers(entry) && !matches!(self.assets.get(entry.page_id()), Some(PageAssets::Ready(_))) {
            self.status = "페이지 이미지를 불러오는 중입니다. 잠시 후 다시 시도하세요.".into();
            return false;
        }
        true
    }

    fn apply_history_entry(&mut self, ctx: &egui::Context, entry: &Entry, undo: bool) {
        match entry {
            Entry::Blocks { before, after, .. } => {
                let snapshot = if undo { before } else { after };
                if let Some(page) = self.page_mut() {
                    snapshot.apply(page);
                }
                let live: HashSet<String> = snapshot.blocks.iter().map(|block| block.id.clone()).collect();
                self.selection.retain(|id| live.contains(id));
                self.touch_page();
            }
            Entry::Layer { page_id, layer, patch } => {
                let bytes = if undo { patch.before.unpack() } else { patch.after.unpack() };
                self.write_layer_patch(ctx, page_id, *layer, patch, &bytes);
            }
            Entry::Presets { before, after, .. } => {
                self.apply_preset_snapshot(if undo { before } else { after });
            }
            Entry::Batch { entries, .. } => {
                if undo {
                    for entry in entries.iter().rev() {
                        self.apply_history_entry(ctx, entry, true);
                    }
                } else {
                    for entry in entries {
                        self.apply_history_entry(ctx, entry, false);
                    }
                }
            }
        }
    }

    fn write_layer_patch(
        &mut self,
        ctx: &egui::Context,
        page_id: &str,
        layer: LayerKind,
        patch: &LayerPatch,
        bytes: &[u8],
    ) {
        let Some(PageAssets::Ready(loaded)) = self.assets.get_mut(page_id) else {
            self.status = "레이어가 아직 로드되지 않아 되돌릴 수 없습니다.".into();
            return;
        };
        match layer {
            LayerKind::Mask => {
                let mask = loaded.ensure_mask();
                write_patch_gray(mask, patch, bytes);
            }
            LayerKind::Result => {
                let result = loaded.ensure_result();
                write_patch_rgba(result, patch, bytes);
            }
        }
        loaded.refresh_layer_region(ctx, page_id, patch.x, patch.y, patch.width, patch.height);
        self.layer_dirty.insert((page_id.to_owned(), layer), Instant::now());
    }
}

fn entry_touches_layers(entry: &Entry) -> bool {
    match entry {
        Entry::Blocks { .. } | Entry::Presets { .. } => false,
        Entry::Layer { .. } => true,
        Entry::Batch { entries, .. } => entries.iter().any(entry_touches_layers),
    }
}

fn write_patch_gray(image: &mut GrayImage, patch: &LayerPatch, bytes: &[u8]) {
    for row in 0..patch.height {
        for col in 0..patch.width {
            let (x, y) = (patch.x + col, patch.y + row);
            if x < image.width() && y < image.height() {
                image.put_pixel(x, y, image::Luma([bytes[(row * patch.width + col) as usize]]));
            }
        }
    }
}

fn write_patch_rgba(image: &mut RgbaImage, patch: &LayerPatch, bytes: &[u8]) {
    for row in 0..patch.height {
        for col in 0..patch.width {
            let (x, y) = (patch.x + col, patch.y + row);
            if x < image.width() && y < image.height() {
                let offset = ((row * patch.width + col) * 4) as usize;
                image.put_pixel(
                    x,
                    y,
                    image::Rgba([bytes[offset], bytes[offset + 1], bytes[offset + 2], bytes[offset + 3]]),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str) -> Entry {
        let snapshot = BlocksSnapshot { blocks: Vec::new(), groups: None, completed: None };
        Entry::Blocks { page_id: id.into(), before: snapshot.clone(), after: snapshot }
    }

    #[test]
    fn diff_patch_covers_only_changes() {
        let before = vec![0u8; 16];
        let mut after = before.clone();
        after[5] = 9;
        after[10] = 9;
        let patch = diff_patch(&before, &after, 4, 4, 1).unwrap();
        assert_eq!((patch.x, patch.y, patch.width, patch.height), (1, 1, 2, 2));
        assert_eq!(patch.after.unpack(), vec![9, 0, 0, 9]);
        assert!(diff_patch(&before, &before, 4, 4, 1).is_none());
    }

    #[test]
    fn push_clears_redo_and_limit_applies() {
        let mut history = History::default();
        history.push(entry("a"));
        let undone = history.pop_undo().unwrap();
        history.push_redo(undone);
        assert!(history.can_redo());
        history.push(entry("b"));
        assert!(!history.can_redo());
        for _ in 0..(HISTORY_LIMIT + 10) {
            history.push(entry("c"));
        }
        assert_eq!(history.undo.len(), HISTORY_LIMIT);
    }

    #[test]
    fn packing_round_trips_and_shrinks_runs() {
        let mut data = vec![0u8; 100_000];
        data[5..12].copy_from_slice(&[1, 2, 3, 4, 5, 6, 7]);
        data[50_000..50_020].fill(255);
        for sample in [data.clone(), Vec::new(), vec![7], (0..=255).collect::<Vec<u8>>(), vec![3; 9]] {
            assert_eq!(Packed::pack(&sample).unpack(), sample);
        }
        assert!(Packed::pack(&data).packed_len() < 64);
    }

    #[test]
    fn layer_budget_drops_the_oldest_steps() {
        let mut history = History::default();
        // Incompressible bytes: a little over a third of the budget per step.
        let noise: Vec<u8> = (0..LAYER_BYTE_BUDGET / 6 + 1).map(|i| (i * 7919 % 251) as u8).collect();
        for _ in 0..4 {
            let patch = LayerPatch::new((0, 0, noise.len() as u32, 1), &noise, &noise);
            history.push(Entry::Layer { page_id: "p".into(), layer: LayerKind::Mask, patch });
        }
        assert_eq!(history.undo.len(), 2, "two steps fit the budget");
    }

    #[test]
    fn stroke_backup_returns_the_pixels_before_the_stroke() {
        let (width, height) = (150u32, 70u32);
        let original: Vec<u8> = (0..width * height).map(|i| (i % 251) as u8).collect();
        let mut layer = original.clone();
        let mut backup = StrokeBackup::new(width, height, 1);
        for rect in [(10, 10, 5, 5), (60, 30, 80, 40), (149, 69, 1, 1)] {
            backup.save(&layer, rect);
            for row in rect.1..rect.1 + rect.3 {
                for column in rect.0..rect.0 + rect.2 {
                    layer[(row * width + column) as usize] = 0;
                }
            }
        }
        let rect = (0, 0, width, height);
        assert_eq!(backup.before(&layer, rect), original);
        assert_eq!(backup.before(&layer, (62, 20, 70, 30)), read_rect(&original, width, 1, (62, 20, 70, 30)));
    }

    #[test]
    fn stroke_backup_keeps_a_stroke_inside_the_selection() {
        let (width, height) = (100u32, 80u32);
        let mut layer = vec![7u8; (width * height) as usize];
        let mut backup = StrokeBackup::new(width, height, 1);
        let stroke = (0, 0, width, height);
        backup.save(&layer, stroke);
        layer.fill(255);
        backup.restore_outside(&mut layer, stroke, (20, 30, 40, 10));
        for row in 0..height {
            for column in 0..width {
                let inside = (20..60).contains(&column) && (30..40).contains(&row);
                assert_eq!(layer[(row * width + column) as usize], if inside { 255 } else { 7 }, "({column}, {row})");
            }
        }
    }
}
