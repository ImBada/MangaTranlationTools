//! Undo/redo. Block edits are recorded as before/after snapshots of one page's blocks;
//! layer edits as before/after pixel patches of the stroked rectangle.

use mtt_core::model::{PageRecord, TranslationBlock, TranslationBlockGroup};

const HISTORY_LIMIT: usize = 200;

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

/// Raw pixels of a rectangle: 1 byte per pixel for masks, 4 (RGBA) for results.
pub struct LayerPatch {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub before: Vec<u8>,
    pub after: Vec<u8>,
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
pub fn read_rect(data: &[u8], image_width: u32, channels: u32, rect: (u32, u32, u32, u32)) -> Vec<u8> {
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
    Some(LayerPatch {
        x: rect.0,
        y: rect.1,
        width: rect.2,
        height: rect.3,
        before: read_rect(before, width, channels, rect),
        after: read_rect(after, width, channels, rect),
    })
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
        assert_eq!(patch.after, vec![9, 0, 0, 9]);
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
}
