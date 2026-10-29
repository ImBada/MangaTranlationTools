//! Undo/redo. Block edits are recorded as before/after snapshots of one page's blocks;
//! layer edits as before/after pixel patches of the changed rectangle, run-length packed
//! (masks are binary and results transparent outside the mask, so a page-sized inpaint
//! patch shrinks from tens of MB to well under one).

use std::collections::HashMap;

use mtt_core::model::{PageRecord, TranslationBlock, TranslationBlockGroup};

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
