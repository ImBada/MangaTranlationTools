//! Font preset editing (`useFontPresetEditing.tsx`).
//!
//! Editing a style field of a block assigned to a preset changes the *preset* when that
//! field is linked (and every block linked to it across the chapter follows); unlinked
//! fields change only the block. Links are toggled per field.

use std::collections::HashSet;

use mtt_core::font_presets::{
    apply_font_preset_to_block, apply_font_preset_values, ensure_block_type_font_presets, is_value_linked,
    normalize_character_font_overrides, relink_keys, resolve_font_preset,
};
use mtt_core::model::{FontPreset, FontSizePreset, StyleKey, TranslationBlock};

use super::Editor;
use super::history::Entry;

/// `MAX_FAVORITE_FONT_PRESETS`.
const MAX_FAVORITE_PRESETS: usize = 5;

/// Id of the stand-in block used while editing a preset without a selection.
pub const PRESET_PROXY_ID: &str = "__font-preset-proxy__";

/// Chapter-wide state touched by preset edits, for undo.
#[derive(Debug, Clone, PartialEq)]
pub struct PresetSnapshot {
    pub font_presets: Option<Vec<FontPreset>>,
    pub font_size_presets: Option<Vec<FontSizePreset>>,
    pub favorites: Option<Vec<String>>,
    /// `(page id, blocks)` for the pages included in the entry.
    pub pages: Vec<(String, Vec<TranslationBlock>)>,
}

impl Editor {
    pub(super) fn preset_snapshot(&self) -> PresetSnapshot {
        PresetSnapshot {
            font_presets: self.chapter.font_presets.clone(),
            font_size_presets: self.chapter.font_size_presets.clone(),
            favorites: self.chapter.favorite_font_preset_ids.clone(),
            pages: self.chapter.pages.iter().map(|page| (page.id.clone(), page.blocks.clone())).collect(),
        }
    }

    pub(super) fn apply_preset_snapshot(&mut self, snapshot: &PresetSnapshot) {
        self.chapter.font_presets = snapshot.font_presets.clone();
        self.chapter.font_size_presets = snapshot.font_size_presets.clone();
        self.chapter.favorite_font_preset_ids = snapshot.favorites.clone();
        for (page_id, blocks) in &snapshot.pages {
            if let Some(page) = self.chapter.pages.iter_mut().find(|page| &page.id == page_id) {
                page.blocks = blocks.clone();
                self.dirty_pages.insert(page_id.clone());
            }
        }
        let live: HashSet<String> =
            self.page().map(|page| page.blocks.iter().map(|b| b.id.clone()).collect()).unwrap_or_default();
        self.selection.retain(|id| live.contains(id));
        self.revision += 1;
        self.mark_chapter_dirty();
    }

    /// Starts a chapter-wide undo step (merged until the next commit).
    fn begin_preset_edit(&mut self) {
        if self.preset_checkpoint.is_none() {
            self.commit_checkpoint();
            self.capture_history_context();
            self.preset_checkpoint = Some(self.preset_snapshot());
        }
    }

    /// Closes a pending preset edit; returns `true` when one was open.
    pub(super) fn commit_preset_checkpoint(&mut self) -> bool {
        let Some(before) = self.preset_checkpoint.take() else { return false };
        let after = self.preset_snapshot();
        let changed: Vec<usize> = (0..after.pages.len())
            .filter(|index| before.pages.get(*index).map(|(_, blocks)| blocks) != Some(&after.pages[*index].1))
            .collect();
        let pick = |snapshot: &PresetSnapshot| PresetSnapshot {
            pages: changed.iter().filter_map(|index| snapshot.pages.get(*index).cloned()).collect(),
            ..snapshot.clone()
        };
        let presets_changed = before.font_presets != after.font_presets
            || before.font_size_presets != after.font_size_presets
            || before.favorites != after.favorites;
        if presets_changed || !changed.is_empty() {
            let page_id = self.page().map(|page| page.id.clone()).unwrap_or_default();
            self.history.push(Entry::Presets { page_id, before: pick(&before), after: pick(&after) });
        }
        true
    }

    /// Without a selection, the preset being edited stands in as a fully linked block so
    /// the style panel edits the preset itself.
    pub fn preset_proxy_block(&self) -> Option<TranslationBlock> {
        if !self.selection.is_empty() {
            return None;
        }
        let id = self.editing_preset_id.as_deref()?;
        let preset = self.presets().into_iter().find(|p| p.id == id)?;
        let resolved = resolve_font_preset(&preset, self.chapter.font_size_presets.as_deref().unwrap_or(&[]));
        Some(TranslationBlock {
            id: PRESET_PROXY_ID.into(),
            font_preset_id: Some(preset.id),
            style: resolved.style,
            ..TranslationBlock::default()
        })
    }

    /// The selected blocks, or the preset proxy when nothing is selected.
    fn style_targets(&self) -> Vec<TranslationBlock> {
        if self.selection.is_empty() { self.preset_proxy_block().into_iter().collect() } else { self.selected_blocks() }
    }

    /// `selectFontPreset`: remembers the preset for editing and applies it to the selection.
    pub fn select_preset(&mut self, preset: &FontPreset) {
        self.editing_preset_id = Some(preset.id.clone());
        let resolved = resolve_font_preset(preset, self.chapter.font_size_presets.as_deref().unwrap_or(&[]));
        self.edit_selected(|block| apply_font_preset_to_block(block, &resolved));
    }

    /// `clearSelectedBlockFontPreset`: unassigns the preset, keeping the current look.
    pub fn clear_selected_preset(&mut self) {
        self.edit_selected(|block| {
            block.font_preset_id = None;
            block.links = Default::default();
        });
    }

    fn presets(&self) -> Vec<FontPreset> {
        ensure_block_type_font_presets(self.chapter.font_presets.as_deref().unwrap_or(&[]))
    }

    /// Re-applies preset `id` to every block linked to it (`updateAssignedFontPreset`).
    fn sync_preset_blocks(&mut self, id: &str) {
        let Some(preset) =
            self.chapter.font_presets.as_ref().and_then(|presets| presets.iter().find(|p| p.id == id)).cloned()
        else {
            return;
        };
        let resolved = resolve_font_preset(&preset, self.chapter.font_size_presets.as_deref().unwrap_or(&[]));
        let now = mtt_core::time::now_iso();
        for page in &mut self.chapter.pages {
            let mut touched = false;
            for block in page.blocks.iter_mut().filter(|block| block.font_preset_id.as_deref() == Some(id)) {
                let before = block.style.clone();
                apply_font_preset_values(block, &resolved, false);
                touched |= block.style != before;
            }
            if touched {
                page.updated_at = now.clone();
                self.dirty_pages.insert(page.id.clone());
            }
        }
        self.revision += 1;
        self.mark_chapter_dirty();
    }

    /// Edits style fields of the selected blocks with preset routing: linked fields of
    /// assigned blocks go to the preset, everything else to the block.
    pub fn edit_style(&mut self, mut edit: impl FnMut(&mut TranslationBlock)) {
        struct Direct {
            id: String,
            edited: TranslationBlock,
            keys: Vec<StyleKey>,
            family: bool,
            overrides: bool,
        }
        let selected = self.style_targets();
        let presets = self.presets();
        let mut preset_targets: Vec<(String, mtt_core::model::TextStyle)> = Vec::new();
        let mut directs: Vec<Direct> = Vec::new();
        for block in &selected {
            let mut edited = block.clone();
            edit(&mut edited);
            let preset = block.font_preset_id.as_deref().and_then(|id| presets.iter().find(|p| p.id == id));
            let mut preset_style = preset.map(|p| p.style.clone());
            let mut keys = Vec::new();
            for key in StyleKey::ALL {
                if style_value_eq(&edited, block, *key) {
                    continue;
                }
                match (&mut preset_style, is_value_linked(block, *key)) {
                    (Some(style), true) => style.copy_key_from(&edited.style, *key),
                    _ => keys.push(*key),
                }
            }
            let family_changed = edited.style.font_family != block.style.font_family;
            let overrides_changed = edited.style.character_font_overrides != block.style.character_font_overrides;
            let (family, overrides) = match &mut preset_style {
                // The family and per-character fonts always belong to the preset.
                Some(style) => {
                    if family_changed {
                        style.font_family = edited.style.font_family.clone();
                    }
                    if overrides_changed {
                        style.character_font_overrides = edited
                            .style
                            .character_font_overrides
                            .as_ref()
                            .map(|o| normalize_character_font_overrides(o))
                            .filter(|o| !o.is_empty());
                    }
                    (false, false)
                }
                None => (family_changed, overrides_changed),
            };
            if let (Some(preset), Some(style)) = (preset, preset_style)
                && style != preset.style
                && !preset_targets.iter().any(|(id, _)| id == &preset.id)
            {
                preset_targets.push((preset.id.clone(), style));
            }
            let other = edited.text_align != block.text_align || edited.text_position != block.text_position;
            if !keys.is_empty() || family || overrides || other {
                directs.push(Direct { id: block.id.clone(), edited, keys, family, overrides });
            }
        }
        if !preset_targets.is_empty() {
            self.begin_preset_edit();
            let mut synced = Vec::new();
            for (id, style) in preset_targets {
                synced.extend(self.update_preset_style(&id, &style));
            }
            synced.sort();
            synced.dedup();
            for id in synced {
                self.sync_preset_blocks(&id);
            }
        }
        if !directs.is_empty() {
            self.edit_selected(|block| {
                let Some(direct) = directs.iter().find(|d| d.id == block.id) else { return };
                for key in &direct.keys {
                    block.style.copy_key_from(&direct.edited.style, *key);
                }
                if direct.family {
                    block.style.font_family = direct.edited.style.font_family.clone();
                }
                if direct.overrides {
                    block.style.character_font_overrides = direct.edited.style.character_font_overrides.clone();
                }
                block.text_align = direct.edited.text_align;
                block.text_position = direct.edited.text_position;
            });
        }
    }

    /// Writes `style` into preset `id`. A size change on a preset bound to a size preset
    /// updates that size preset and every preset using it. Returns presets to re-sync.
    fn update_preset_style(&mut self, id: &str, style: &mtt_core::model::TextStyle) -> Vec<String> {
        let mut presets = self.presets();
        let Some(current) = presets.iter().find(|p| p.id == id).cloned() else { return Vec::new() };
        let mut affected = vec![id.to_owned()];
        let mut size_presets = self.chapter.font_size_presets.clone().unwrap_or_default();
        let size_changed = style.font_size_px != current.style.font_size_px;
        let bound_size = current.font_size_preset_id.clone().filter(|sid| size_presets.iter().any(|p| &p.id == sid));
        if let Some(preset) = presets.iter_mut().find(|p| p.id == id) {
            preset.style = style.clone();
            if size_changed && bound_size.is_none() {
                preset.font_size_preset_id = None;
            }
        }
        if size_changed && let Some(size_id) = bound_size {
            if let Some(size_preset) = size_presets.iter_mut().find(|p| p.id == size_id) {
                size_preset.font_size_px = style.font_size_px.unwrap_or(size_preset.font_size_px);
            }
            for preset in presets.iter_mut().filter(|p| p.font_size_preset_id.as_deref() == Some(size_id.as_str())) {
                preset.style.font_size_px = style.font_size_px;
                affected.push(preset.id.clone());
            }
            self.chapter.font_size_presets = Some(size_presets);
        }
        self.chapter.font_presets = Some(presets);
        affected
    }

    /// Toggles one field's preset link on the selected blocks (`toggleSelectedBlockFontPresetLink`).
    pub fn toggle_link(&mut self, key: StyleKey) {
        let Some(first) = self.selected_blocks().into_iter().find(|block| block.font_preset_id.is_some()) else {
            return;
        };
        let relink = !is_value_linked(&first, key);
        self.set_links(&[key], relink);
    }

    /// `toggleSelectedBlockFontPresetLinkGroup`: one 🔗 for several fields (B/I/U).
    pub fn toggle_link_group(&mut self, keys: &[StyleKey]) {
        let Some(first) = self.selected_blocks().into_iter().find(|block| block.font_preset_id.is_some()) else {
            return;
        };
        let linked = keys.iter().all(|key| is_value_linked(&first, *key));
        self.set_links(keys, !linked);
    }

    /// Links (copying the preset value) or unlinks `keys` on the selected blocks.
    fn set_links(&mut self, keys: &[StyleKey], relink: bool) {
        let presets = self.presets();
        let size_presets = self.chapter.font_size_presets.clone().unwrap_or_default();
        self.edit_selected(|block| {
            let Some(preset) = block.font_preset_id.as_deref().and_then(|id| presets.iter().find(|p| p.id == id))
            else {
                return;
            };
            let resolved = resolve_font_preset(preset, &size_presets);
            for key in keys {
                for k in relink_keys(*key) {
                    if relink {
                        block.links.set(k, Some(true));
                        block.style.copy_key_from(&resolved.style, k);
                    } else {
                        block.links.set(k, Some(false));
                    }
                }
            }
        });
    }

    /// Creates a preset from the first selected block's style and assigns it.
    pub fn create_preset_from_selection(&mut self, name: &str) -> Result<(), String> {
        let presets = self.presets();
        let name = match name.trim() {
            "" => format!("프리셋 {}", self.chapter.font_presets.as_ref().map_or(0, Vec::len) + 1),
            name => name.to_owned(),
        };
        if presets.iter().any(|p| p.name == name) {
            return Err("이미 있는 프리셋 이름입니다.".into());
        }
        // The primary (first clicked) block is both the style source and the one assigned.
        let primary = self.selection.first().and_then(|id| self.selected_blocks().into_iter().find(|b| &b.id == id));
        let Some(block) = primary.or_else(|| self.preset_proxy_block()) else {
            return Err("블록을 선택하세요.".into());
        };
        let preset = FontPreset {
            id: format!("font-preset-{}-{}", chrono_millis(), &uuid::Uuid::new_v4().simple().to_string()[..6]),
            name,
            style: mtt_core::font_presets::normalize_font_preset(&FontPreset {
                style: block.style.clone(),
                ..FontPreset::default()
            })
            .style,
            font_size_preset_id: self.active_size_preset_id(&block),
            ..FontPreset::default()
        };
        self.begin_preset_edit();
        let mut next = presets;
        next.push(preset.clone());
        self.chapter.font_presets = Some(next);
        self.editing_preset_id = Some(preset.id.clone());
        if let Some(first) = self.selection.first().cloned() {
            // Like the TS app, only the primary selected block takes the new preset.
            if let Some(block) = self.page_mut().and_then(|page| page.blocks.iter_mut().find(|b| b.id == first)) {
                apply_font_preset_to_block(block, &preset);
            }
            self.touch_page();
        } else {
            self.revision += 1;
            self.mark_chapter_dirty();
        }
        Ok(())
    }

    pub fn rename_preset(&mut self, id: &str, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("프리셋 이름을 입력하세요.".into());
        }
        let mut presets = self.presets();
        if presets.iter().any(|p| p.name == name && p.id != id) {
            return Err("이미 있는 프리셋 이름입니다.".into());
        }
        self.begin_preset_edit();
        if let Some(preset) = presets.iter_mut().find(|p| p.id == id) {
            preset.name = name.to_owned();
        }
        self.chapter.font_presets = Some(presets);
        self.mark_chapter_dirty();
        Ok(())
    }

    /// Literal replace of translated text on every page as one chapter-wide undo step.
    pub fn replace_in_all_pages(&mut self, keyword: &str, replacement: &str) {
        self.begin_preset_edit();
        let now = mtt_core::time::now_iso();
        for page in &mut self.chapter.pages {
            let mut changed = false;
            for block in page.blocks.iter_mut().filter(|block| block.translated_text.contains(keyword)) {
                block.translated_text = block.translated_text.replace(keyword, replacement);
                changed = true;
            }
            if changed {
                page.updated_at = now.clone();
                self.dirty_pages.insert(page.id.clone());
            }
        }
        self.revision += 1;
        self.mark_chapter_dirty();
        self.commit_checkpoint();
    }

    /// `activeFontSizePresetId`: the size preset behind the block's linked font size.
    pub fn active_size_preset_id(&self, block: &TranslationBlock) -> Option<String> {
        if !is_value_linked(block, StyleKey::FontSizePx) {
            return None;
        }
        let id = block.font_preset_id.as_deref()?;
        let size_id = self.presets().into_iter().find(|p| p.id == id)?.font_size_preset_id?;
        self.chapter.font_size_presets.as_deref()?.iter().any(|p| p.id == size_id).then_some(size_id)
    }

    /// Size preset controls only apply to a preset-assigned block with a linked size.
    pub fn size_preset_controls_enabled(block: &TranslationBlock) -> bool {
        block.font_preset_id.is_some() && is_value_linked(block, StyleKey::FontSizePx)
    }

    /// `selectFontSizePreset`: binds (or unbinds with `None`) the block's font preset to a
    /// size preset; every block linked to the font preset follows.
    pub fn select_size_preset(&mut self, size_id: Option<&str>) {
        let Some(block) = self.style_targets().into_iter().next() else { return };
        let Some(preset_id) = block.font_preset_id.clone().filter(|_| Self::size_preset_controls_enabled(&block))
        else {
            return;
        };
        let sizes = self.chapter.font_size_presets.clone().unwrap_or_default();
        let size = size_id.and_then(|id| sizes.iter().find(|p| p.id == id));
        if size_id.is_some() && size.is_none() {
            return;
        }
        let font_size_px = size.map(|p| p.font_size_px).unwrap_or_else(|| block.font_size_px());
        self.begin_preset_edit();
        let mut presets = self.presets();
        if let Some(preset) = presets.iter_mut().find(|p| p.id == preset_id) {
            preset.font_size_preset_id = size.map(|p| p.id.clone());
            preset.style.font_size_px = Some(font_size_px);
        }
        self.chapter.font_presets = Some(presets);
        self.sync_preset_blocks(&preset_id);
    }

    /// `createFontSizePresetFromCurrentFontSize`: saves the current size as the next
    /// lettered size preset and binds the block's font preset to it.
    pub fn create_size_preset_from_selection(&mut self) {
        let Some(block) = self.style_targets().into_iter().next() else { return };
        if !Self::size_preset_controls_enabled(&block) {
            return;
        }
        let mut sizes = self.chapter.font_size_presets.clone().unwrap_or_default();
        let preset = FontSizePreset {
            id: format!("font-size-preset-{}-{}", chrono_millis(), &uuid::Uuid::new_v4().simple().to_string()[..6]),
            name: next_size_preset_name(&sizes),
            font_size_px: block.font_size_px(),
            ..FontSizePreset::default()
        };
        self.begin_preset_edit();
        let mut presets = self.presets();
        if let Some(font_preset) = presets.iter_mut().find(|p| Some(&p.id) == block.font_preset_id.as_ref()) {
            font_preset.font_size_preset_id = Some(preset.id.clone());
            font_preset.style.font_size_px = Some(preset.font_size_px);
        }
        sizes.push(preset);
        self.chapter.font_size_presets = Some(sizes);
        self.chapter.font_presets = Some(presets);
        self.revision += 1;
        self.mark_chapter_dirty();
    }

    /// `deleteFontSizePreset`: font presets using it keep their size but lose the binding.
    pub fn delete_size_preset(&mut self, size_id: &str) {
        self.begin_preset_edit();
        let mut sizes = self.chapter.font_size_presets.clone().unwrap_or_default();
        sizes.retain(|p| p.id != size_id);
        let mut presets = self.presets();
        for preset in presets.iter_mut().filter(|p| p.font_size_preset_id.as_deref() == Some(size_id)) {
            preset.font_size_preset_id = None;
        }
        self.chapter.font_size_presets = Some(sizes);
        self.chapter.font_presets = Some(presets);
        self.revision += 1;
        self.mark_chapter_dirty();
    }

    /// Deletes a custom preset; blocks keep their current look but lose the assignment.
    pub fn delete_preset(&mut self, id: &str) {
        self.begin_preset_edit();
        if self.editing_preset_id.as_deref() == Some(id) {
            self.editing_preset_id = None;
        }
        let presets: Vec<FontPreset> = self.presets().into_iter().filter(|p| p.id != id).collect();
        self.chapter.font_presets = Some(presets);
        if let Some(favorites) = &mut self.chapter.favorite_font_preset_ids {
            favorites.retain(|favorite| favorite != id);
        }
        for page in &mut self.chapter.pages {
            for block in page.blocks.iter_mut().filter(|b| b.font_preset_id.as_deref() == Some(id)) {
                block.font_preset_id = None;
                block.links = Default::default();
                self.dirty_pages.insert(page.id.clone());
            }
        }
        self.revision += 1;
        self.mark_chapter_dirty();
    }

    /// `applyFontPresetListBackupToChapter`: replaces the preset lists; blocks assigned to
    /// a restored preset take its linked values, blocks whose preset is gone are unassigned.
    pub fn restore_preset_backup(&mut self, backup: &mtt_core::preset_backups::PresetBackup) {
        self.begin_preset_edit();
        let presets = backup.font_presets.clone();
        let sizes = backup.font_size_presets.clone();
        let resolved: Vec<FontPreset> = presets.iter().map(|p| resolve_font_preset(p, &sizes)).collect();
        self.chapter.favorite_font_preset_ids = self
            .chapter
            .favorite_font_preset_ids
            .take()
            .map(|favorites| favorites.into_iter().filter(|id| presets.iter().any(|p| &p.id == id)).collect());
        self.chapter.font_presets = Some(presets);
        self.chapter.font_size_presets = Some(sizes);
        let now = mtt_core::time::now_iso();
        for page in &mut self.chapter.pages {
            let mut changed = false;
            for block in page.blocks.iter_mut().filter(|block| block.font_preset_id.is_some()) {
                changed = true;
                match resolved.iter().find(|p| Some(&p.id) == block.font_preset_id.as_ref()) {
                    Some(preset) => apply_font_preset_values(block, preset, false),
                    None => {
                        block.font_preset_id = None;
                        block.links = Default::default();
                    }
                }
            }
            if changed {
                page.updated_at = now.clone();
                self.dirty_pages.insert(page.id.clone());
            }
        }
        self.revision += 1;
        self.mark_chapter_dirty();
    }

    /// Right-click on a tag (`toggleFavoriteFontPreset`): at most five favourites.
    pub fn toggle_favorite(&mut self, id: &str) {
        let favorites = self.chapter.favorite_font_preset_ids.clone().unwrap_or_default();
        if !favorites.iter().any(|f| f == id) && favorites.len() >= MAX_FAVORITE_PRESETS {
            self.status = "즐겨찾기 태그는 최대 5개까지 지정할 수 있습니다.".into();
            return;
        }
        let favorites = self.chapter.favorite_font_preset_ids.get_or_insert_with(Vec::new);
        if let Some(index) = favorites.iter().position(|f| f == id) {
            favorites.remove(index);
        } else {
            favorites.push(id.to_owned());
        }
        self.mark_chapter_dirty();
    }
}

fn style_value_eq(a: &TranslationBlock, b: &TranslationBlock, key: StyleKey) -> bool {
    let mut probe = a.style.clone();
    probe.copy_key_from(&b.style, key);
    probe == a.style
}

/// `buildAlphabeticPresetName`: A, B, … Z, AA, AB, …
fn alphabetic_preset_name(index: usize) -> String {
    let mut remaining = index as i64;
    let mut name = String::new();
    loop {
        name.insert(0, char::from(b'A' + (remaining % 26) as u8));
        remaining = remaining / 26 - 1;
        if remaining < 0 {
            return name;
        }
    }
}

/// `buildNextFontSizePresetName`.
fn next_size_preset_name(sizes: &[FontSizePreset]) -> String {
    let used: HashSet<&str> = sizes.iter().map(|p| p.name.trim()).collect();
    (0..).map(alphabetic_preset_name).find(|name| !used.contains(name.as_str())).unwrap_or_default()
}

fn chrono_millis() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    use mtt_core::model::{BBox, Chapter, PageRecord};

    fn editor_with_linked_blocks() -> Editor {
        let preset = mtt_core::font_presets::create_default_font_preset(
            "font-preset-speech",
            mtt_core::model::BlockType::Speech,
        );
        let make = |id: &str| {
            let mut block = TranslationBlock {
                id: id.into(),
                bbox: BBox::new(0.0, 0.0, 100.0, 100.0),
                ..TranslationBlock::default()
            };
            apply_font_preset_to_block(&mut block, &preset);
            block
        };
        let pages = vec![
            PageRecord { id: "p1".into(), blocks: vec![make("a"), make("b")], ..PageRecord::default() },
            PageRecord { id: "p2".into(), blocks: vec![make("c")], ..PageRecord::default() },
        ];
        let chapter = Chapter {
            id: "c".into(),
            page_order: vec!["p1".into(), "p2".into()],
            pages,
            font_presets: Some(vec![preset]),
            ..Chapter::default()
        };
        let mut editor = Editor::new(chapter);
        editor.selection = vec!["a".into()];
        editor
    }

    #[test]
    fn linked_edit_updates_preset_and_all_linked_blocks() {
        let ctx = eframe::egui::Context::default();
        let mut editor = editor_with_linked_blocks();
        editor.edit_style(|block| block.style.font_size_px = Some(40.0));
        let size =
            |editor: &Editor, page: usize, block: usize| editor.chapter.pages[page].blocks[block].style.font_size_px;
        assert_eq!(size(&editor, 0, 1), Some(40.0), "same page follows");
        assert_eq!(size(&editor, 1, 0), Some(40.0), "other page follows");
        let preset = editor
            .chapter
            .font_presets
            .as_ref()
            .unwrap()
            .iter()
            .find(|p| p.id == "font-preset-speech")
            .unwrap()
            .clone();
        assert_eq!(preset.style.font_size_px, Some(40.0));

        editor.commit_checkpoint();
        editor.undo(&ctx);
        assert_eq!(size(&editor, 1, 0), Some(24.0), "one undo reverts every page");
    }

    #[test]
    fn unlinked_field_changes_only_the_block() {
        let mut editor = editor_with_linked_blocks();
        editor.toggle_link(StyleKey::TextColor);
        editor.edit_style(|block| block.style.text_color = Some("#ff0000".into()));
        assert_eq!(editor.chapter.pages[0].blocks[0].style.text_color.as_deref(), Some("#ff0000"));
        assert_eq!(editor.chapter.pages[0].blocks[1].style.text_color.as_deref(), Some("#111111"));
        editor.toggle_link(StyleKey::TextColor);
        assert_eq!(
            editor.chapter.pages[0].blocks[0].style.text_color.as_deref(),
            Some("#111111"),
            "relink restores preset value"
        );
    }

    #[test]
    fn restore_backup_reapplies_and_unassigns() {
        let mut editor = editor_with_linked_blocks();
        let mut speech = editor.chapter.font_presets.as_ref().unwrap()[0].clone();
        speech.style.text_color = Some("#123456".into());
        let backup = mtt_core::preset_backups::PresetBackup { font_presets: vec![speech], ..Default::default() };
        editor.restore_preset_backup(&backup);
        assert_eq!(editor.chapter.pages[1].blocks[0].style.text_color.as_deref(), Some("#123456"));

        let empty = mtt_core::preset_backups::PresetBackup::default();
        editor.restore_preset_backup(&empty);
        assert!(editor.chapter.pages[0].blocks[0].font_preset_id.is_none());
        assert_eq!(editor.chapter.pages[0].blocks[0].links, Default::default());
    }

    #[test]
    fn create_rename_delete_presets() {
        let mut editor = editor_with_linked_blocks();
        editor.edit_style(|_| {});
        editor.create_preset_from_selection("독백").unwrap();
        let id = editor.chapter.pages[0].blocks[0].font_preset_id.clone().unwrap();
        assert_ne!(id, "font-preset-speech");
        assert!(editor.create_preset_from_selection("독백").is_err());
        editor.rename_preset(&id, "속마음").unwrap();
        editor.toggle_favorite(&id);
        assert_eq!(editor.chapter.favorite_font_preset_ids.as_deref(), Some(&[id.clone()][..]));
        editor.delete_preset(&id);
        assert!(editor.chapter.pages[0].blocks[0].font_preset_id.is_none());
        assert!(editor.chapter.favorite_font_preset_ids.as_ref().unwrap().is_empty());
        // Like the TS app, a block-type preset can be deleted; it comes back with defaults.
        editor.delete_preset("font-preset-speech");
        assert!(editor.presets().iter().any(|p| p.id == "font-preset-speech"));
        assert!(editor.chapter.pages[1].blocks[0].font_preset_id.is_none());
    }

    #[test]
    fn size_preset_names_follow_ts_lettering() {
        assert_eq!(alphabetic_preset_name(0), "A");
        assert_eq!(alphabetic_preset_name(25), "Z");
        assert_eq!(alphabetic_preset_name(26), "AA");
        assert_eq!(alphabetic_preset_name(27), "AB");
        let sizes = ["A", "B"].map(|name| FontSizePreset { name: name.into(), ..Default::default() });
        assert_eq!(next_size_preset_name(&sizes), "C");
        assert_eq!(next_size_preset_name(&[]), "A");
    }

    #[test]
    fn size_presets_bind_sync_and_unbind() {
        let mut editor = editor_with_linked_blocks();
        editor.create_size_preset_from_selection();
        let size_id = editor.chapter.font_size_presets.as_ref().unwrap()[0].id.clone();
        assert_eq!(editor.chapter.font_size_presets.as_ref().unwrap()[0].name, "A");
        let block = editor.chapter.pages[0].blocks[0].clone();
        assert_eq!(editor.active_size_preset_id(&block).as_deref(), Some(size_id.as_str()));

        // Editing the size of a bound preset edits the size preset.
        editor.edit_style(|block| block.style.font_size_px = Some(30.0));
        assert_eq!(editor.chapter.font_size_presets.as_ref().unwrap()[0].font_size_px, 30.0);

        // Unbinding keeps the size; picking another size preset applies its size everywhere.
        editor.select_size_preset(None);
        assert_eq!(editor.active_size_preset_id(&editor.chapter.pages[0].blocks[0].clone()), None);
        editor.chapter.font_size_presets.as_mut().unwrap().push(FontSizePreset {
            id: "big".into(),
            name: "B".into(),
            font_size_px: 50.0,
            ..Default::default()
        });
        editor.select_size_preset(Some("big"));
        assert_eq!(editor.chapter.pages[1].blocks[0].style.font_size_px, Some(50.0));

        editor.delete_size_preset("big");
        let speech = editor.presets().into_iter().find(|p| p.id == "font-preset-speech").unwrap();
        assert_eq!(speech.font_size_preset_id, None);
        assert_eq!(speech.style.font_size_px, Some(50.0), "deleting keeps the size");
    }

    #[test]
    fn replace_all_is_one_undo_step() {
        let ctx = eframe::egui::Context::default();
        let mut editor = editor_with_linked_blocks();
        for page in &mut editor.chapter.pages {
            for block in &mut page.blocks {
                block.translated_text = "안녕 안녕".into();
            }
        }
        editor.reset_checkpoint();
        editor.replace_in_all_pages("안녕", "잘가");
        assert_eq!(editor.chapter.pages[1].blocks[0].translated_text, "잘가 잘가");
        editor.undo(&ctx);
        assert_eq!(editor.chapter.pages[0].blocks[1].translated_text, "안녕 안녕");
        assert_eq!(editor.chapter.pages[1].blocks[0].translated_text, "안녕 안녕", "one undo reverts every page");
    }
}
