//! The stage "텍스트 블록" list (`StageTextBlockList` in `WorkspaceOverlays.tsx`): blocks in
//! output order with groups collapsed under a "그룹" header, Shift range / ⌘ toggle
//! selection, and drag reordering of page units (blocks or whole groups) and of blocks
//! inside a group.

use eframe::egui::{self, RichText};
use mtt_core::groups::{ListItem, list_items};
use mtt_core::library::{DropPosition, reorder_by_target};
use mtt_core::model::{PageRecord, RenderTextDirection, TranslationBlock};

use super::Editor;
use super::panels::{block_preview_text, block_type_label};
use super::widgets::{self, Host, Widget};
use crate::theme;

/// What a row drags: a page unit (`block:` / `group:` id) or a block within its group.
#[derive(Clone, PartialEq, Eq)]
enum DragItem {
    Page(String),
    Group { group_id: String, block_id: String },
}

fn block_unit(id: &str) -> String {
    format!("block:{id}")
}

fn group_unit(id: &str) -> String {
    format!("group:{id}")
}

/// Page units in order with the block ids each one covers.
fn page_units(page: &PageRecord) -> Vec<(String, Vec<String>)> {
    list_items(page)
        .into_iter()
        .map(|item| match item {
            ListItem::Block { block, .. } => (block_unit(&block.id), vec![block.id.clone()]),
            ListItem::Group { group, blocks } => {
                (group_unit(&group.id), blocks.iter().map(|(_, block)| block.id.clone()).collect())
            }
        })
        .collect()
}

impl Editor {
    /// `updateSelectedPageBlockOrder`: new output order for the page's blocks.
    pub fn reorder_page_blocks(&mut self, ids: &[String]) {
        let Some(page) = self.page() else { return };
        let current: Vec<String> = page.blocks.iter().map(|block| block.id.clone()).collect();
        let mut sorted_current = current.clone();
        let mut sorted_next = ids.to_vec();
        sorted_current.sort();
        sorted_next.sort();
        if sorted_current != sorted_next || current == ids {
            return;
        }
        self.edit_page(|page| {
            let mut blocks = std::mem::take(&mut page.blocks);
            page.blocks = ids
                .iter()
                .filter_map(|id| blocks.iter().position(|block| &block.id == id).map(|index| blocks.remove(index)))
                .collect();
        });
        self.status = "텍스트 블록 출력 순서를 변경했습니다.".into();
    }

    /// `updateBlockGroupOrder`: order inside a group, mirrored in the page block order.
    pub fn reorder_group_blocks(&mut self, group_id: &str, ids: &[String]) {
        let Some(page) = self.page() else { return };
        let Some(group) = mtt_core::groups::valid_groups(page).into_iter().find(|group| group.id == group_id) else {
            return;
        };
        let mut a = group.block_ids.clone();
        let mut b = ids.to_vec();
        a.sort();
        b.sort();
        if a != b || group.block_ids == ids {
            return;
        }
        let now = mtt_core::time::now_iso();
        self.edit_page(|page| {
            if let Some(groups) = &mut page.block_groups
                && let Some(group) = groups.iter_mut().find(|group| group.id == group_id)
            {
                group.block_ids = ids.to_vec();
                group.updated_at = now.clone();
            }
            // `resolveTranslationBlocksAfterGroupReordering`: group slots keep their places.
            let reordered: Vec<TranslationBlock> =
                ids.iter().filter_map(|id| page.blocks.iter().find(|block| &block.id == id).cloned()).collect();
            let mut next = reordered.into_iter();
            for block in page.blocks.iter_mut() {
                if ids.contains(&block.id)
                    && let Some(replacement) = next.next()
                {
                    *block = replacement;
                }
            }
        });
        self.status = "그룹 블록 순서를 변경했습니다.".into();
    }

    /// Row click selection (`selectBlock`).
    fn list_select(&mut self, block_id: &str, modifiers: egui::Modifiers) {
        let Some(page) = self.page() else { return };
        let order: Vec<String> = page.blocks.iter().map(|block| block.id.clone()).collect();
        let unit = mtt_core::groups::group_block_ids(page, block_id).unwrap_or_else(|| vec![block_id.to_owned()]);
        // `selectionAnchorIdRef` follows single selections made anywhere (canvas too).
        if self.selection.len() <= 1 {
            self.list_anchor = self.selection.first().cloned();
        }
        let anchor = self
            .list_anchor
            .clone()
            .filter(|id| order.contains(id))
            .or_else(|| self.selection.first().cloned().filter(|id| order.contains(id)));
        if modifiers.shift {
            // `resolveTranslationBlockRangeSelection`.
            let Some(target) = order.iter().position(|id| id == block_id) else { return };
            let range: Vec<String> = match anchor.as_ref().and_then(|a| order.iter().position(|id| id == a)) {
                Some(start) => order[start.min(target)..=start.max(target)].to_vec(),
                None => vec![block_id.to_owned()],
            };
            let mut next = if modifiers.command { self.selection.clone() } else { Vec::new() };
            for id in range {
                if !next.contains(&id) {
                    next.push(id);
                }
            }
            self.list_anchor = Some(anchor.unwrap_or_else(|| block_id.to_owned()));
            self.selection = next;
            return;
        }
        self.list_anchor = Some(block_id.to_owned());
        if modifiers.command {
            // `resolveToggledTranslationBlockIds` with the whole unit.
            let all_selected = unit.iter().all(|id| self.selection.contains(id));
            if all_selected {
                self.selection.retain(|id| !unit.contains(id));
            } else {
                for id in unit {
                    if !self.selection.contains(&id) {
                        self.selection.push(id);
                    }
                }
            }
            return;
        }
        self.select_block_or_group(block_id);
    }
}

/// The outline card at the top of the translate inspector; the list scrolls inside
/// `max_height`.
/// The page's text blocks in output order (`Widget::Blocks`): select, group and reorder.
pub fn blocks_widget(editor: &mut Editor, ui: &mut egui::Ui, host: Host) {
    let Some(page) = editor.page().cloned() else { return };
    // In the inspector the list takes a slice of the panel and can fold away; as a window
    // it fills whatever size it was given.
    let collapsible = host == Host::Panel;
    let collapsed = collapsible && editor.block_list_collapsed;
    let list_height = match host {
        Host::Panel => (ui.available_height() * 0.3).clamp(110.0, 280.0),
        Host::Window => (ui.available_height() - 64.0).max(60.0),
    };
    let locked = editor.page_edit_locked();
    let units = page_units(&page);
    let page_reorder = !locked && units.len() > 1;
    let mut select: Option<(String, egui::Modifiers)> = None;
    let mut page_drop: Option<(String, String, DropPosition)> = None;
    let mut group_drop: Option<(String, String, String, DropPosition)> = None;
    let mut group_action = None;
    let mut toggle_collapsed = false;
    let title = format!("텍스트 블록 · {}개", page.blocks.len());
    let (_, toggle) = widgets::frame(
        ui,
        host,
        Widget::Blocks,
        &title,
        |ui| {
            if collapsible {
                let arrow = if collapsed { "펼치기" } else { "접기" };
                if ui.add(egui::Button::new(RichText::new(arrow).small()).frame_when_inactive(false)).clicked() {
                    toggle_collapsed = true;
                }
            }
            let group = mtt_core::groups::selected_group(&page, &editor.selection);
            let any_grouped = editor.selection.iter().any(|id| mtt_core::groups::group_block_ids(&page, id).is_some());
            if group.is_some() || (any_grouped && editor.selection.len() >= 2) {
                let tip = if locked {
                    "잠긴 페이지에서는 그룹을 해제할 수 없음"
                } else {
                    "선택한 텍스트 블록 그룹 해제"
                };
                if ui.add_enabled(!locked, egui::Button::new("그룹 해제").small()).on_hover_text(tip).clicked() {
                    group_action = Some(false);
                }
            } else if editor.selection.len() >= 2 {
                let tip = if locked {
                    "잠긴 페이지에서는 그룹화할 수 없음"
                } else {
                    "선택한 텍스트 블록 그룹화"
                };
                if ui.add_enabled(!locked, egui::Button::new("그룹으로 묶기").small()).on_hover_text(tip).clicked()
                {
                    group_action = Some(true);
                }
            }
        },
        |ui| {
            if collapsed {
                return;
            }
            egui::ScrollArea::vertical()
                .id_salt("block-outline")
                .max_height(list_height)
                .auto_shrink([false, host == Host::Panel])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    if page.blocks.is_empty() && locked {
                        theme::hint(ui, "AI 번역이 끝나면 여기에 블록이 나옵니다.");
                    } else if page.blocks.is_empty() {
                        theme::hint(ui, "텍스트 블록이 없습니다. AI 번역을 돌리거나 '블록 추가'로 만드세요.");
                    }
                    for item in list_items(&page) {
                        match item {
                            ListItem::Block { block, index } => {
                                let drag = DragItem::Page(block_unit(&block.id));
                                if let Some((target, position)) =
                                    block_row(ui, editor, block, index, &drag, page_reorder, &mut select)
                                    && let DragItem::Page(source) = target
                                {
                                    page_drop = Some((source, block_unit(&block.id), position));
                                }
                            }
                            ListItem::Group { group, blocks } => {
                                let selected = blocks.iter().all(|(_, block)| editor.selection.contains(&block.id));
                                let drag = DragItem::Page(group_unit(&group.id));
                                let label = RichText::new(format!("그룹 · {}개", blocks.len())).strong();
                                let header = crate::dnd::drag_button(
                                    ui,
                                    egui::Button::selectable(selected, label)
                                        .min_size(egui::vec2(ui.available_width(), 0.0)),
                                    drag.clone(),
                                    "그룹",
                                );
                                let header = if page_reorder {
                                    header.on_hover_text("드래그해서 그룹 순서 변경")
                                } else {
                                    header
                                };
                                if header.clicked()
                                    && let Some((_, first)) = blocks.first()
                                {
                                    select = Some((first.id.clone(), ui.input(|i| i.modifiers)));
                                }
                                if page_reorder
                                    && let Some((source, position)) = drop_on(ui, &header)
                                    && let DragItem::Page(source) = source
                                {
                                    page_drop = Some((source, group_unit(&group.id), position));
                                }
                                let inner_reorder = !locked && blocks.len() > 1;
                                ui.indent(("group-items", &group.id), |ui| {
                                    for (index, block) in &blocks {
                                        let drag =
                                            DragItem::Group { group_id: group.id.clone(), block_id: block.id.clone() };
                                        if let Some((source, position)) =
                                            block_row(ui, editor, block, *index, &drag, inner_reorder, &mut select)
                                            && let DragItem::Group { group_id, block_id } = source
                                            && group_id == group.id
                                        {
                                            group_drop = Some((group_id, block_id, block.id.clone(), position));
                                        }
                                    }
                                });
                            }
                        }
                    }
                });
            theme::hint(
                ui,
                &format!(
                    "{} / {} 클릭으로 여러 개 선택, 드래그로 출력 순서 변경",
                    crate::shortcuts::keys("⇧"),
                    crate::shortcuts::keys("⌘")
                ),
            );
        },
    );
    widgets::toggled(editor, Widget::Blocks, host, toggle);
    if toggle_collapsed {
        editor.block_list_collapsed = !editor.block_list_collapsed;
    }
    if let Some((id, modifiers)) = select {
        editor.list_select(&id, modifiers);
    }
    if let Some((source, target, position)) = page_drop {
        let order: Vec<String> = units.iter().map(|(id, _)| id.clone()).collect();
        let next_units = reorder_by_target(&order, &source, &target, position);
        let ids: Vec<String> = next_units
            .iter()
            .flat_map(|unit| units.iter().find(|(id, _)| id == unit).map(|(_, ids)| ids.clone()).unwrap_or_default())
            .collect();
        editor.reorder_page_blocks(&ids);
    }
    if let Some((group_id, source, target, position)) = group_drop
        && let Some(group) = mtt_core::groups::valid_groups(&page).into_iter().find(|group| group.id == group_id)
    {
        let next = reorder_by_target(&group.block_ids, &source, &target, position);
        editor.reorder_group_blocks(&group_id, &next);
    }
    match group_action {
        Some(true) => {
            let count = editor.selection.len();
            editor.group_selected();
            editor.status = format!("{count}개 텍스트 블록을 그룹으로 묶었습니다.");
        }
        Some(false) => {
            let count = editor.selection.len();
            editor.ungroup_selected();
            editor.status = format!("{count}개 텍스트 블록 그룹을 해제했습니다.");
        }
        None => {}
    }
}

/// One block row; returns a drop onto it as `(dragged item, before/after)`.
fn block_row(
    ui: &mut egui::Ui,
    editor: &Editor,
    block: &TranslationBlock,
    index: usize,
    drag: &DragItem,
    reorder: bool,
    select: &mut Option<(String, egui::Modifiers)>,
) -> Option<(DragItem, DropPosition)> {
    let selected = editor.selection.contains(&block.id);
    let hidden = block.render_direction == RenderTextDirection::Hidden;
    let meta = format!("{}{}", block_type_label(block.block_type), if hidden { " · 숨김" } else { "" });
    let label = format!("{:>2}  {}", index + 1, block_preview_text(block));
    let text = if hidden { RichText::new(label).color(theme::TEXT_WEAK) } else { RichText::new(label) };
    let button = egui::Button::selectable(selected, text)
        .right_text(RichText::new(meta).small().color(theme::TEXT_WEAK))
        .truncate()
        .min_size(egui::vec2(ui.available_width(), 0.0));
    let response = if reorder {
        let ghost = block_preview_text(block);
        let row = crate::dnd::drag_button(ui, button, drag.clone(), &ghost).on_hover_text("드래그해서 순서 변경");
        if row.clicked() {
            *select = Some((block.id.clone(), ui.input(|i| i.modifiers)));
        }
        row
    } else {
        let response = ui.add(button);
        if response.clicked() {
            *select = Some((block.id.clone(), ui.input(|i| i.modifiers)));
        }
        response
    };
    if reorder { drop_on(ui, &response) } else { None }
}

/// Drop indicator and release handling (`before` above the row's middle, else `after`).
fn drop_on(ui: &egui::Ui, response: &egui::Response) -> Option<(DragItem, DropPosition)> {
    let hover = response.dnd_hover_payload::<DragItem>()?;
    let pointer = ui.input(|i| i.pointer.hover_pos())?;
    let position = if pointer.y > response.rect.center().y { DropPosition::After } else { DropPosition::Before };
    let y = if position == DropPosition::After { response.rect.bottom() } else { response.rect.top() };
    ui.painter().hline(response.rect.x_range(), y, (2.0, theme::ACCENT));
    let released = response.dnd_release_payload::<DragItem>()?;
    let _ = hover;
    Some(((*released).clone(), position))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Layer;
    use mtt_core::model::{BBox, Chapter, TranslationBlockGroup};

    fn editor() -> Editor {
        let block = |id: &str| TranslationBlock {
            id: id.into(),
            bbox: BBox::new(0.0, 0.0, 10.0, 10.0),
            ..TranslationBlock::default()
        };
        let page = PageRecord {
            id: "p".into(),
            width: 100,
            height: 100,
            blocks: vec![block("a"), block("b"), block("c"), block("d")],
            block_groups: Some(vec![TranslationBlockGroup {
                id: "g".into(),
                block_ids: vec!["b".into(), "c".into()],
                ..Default::default()
            }]),
            ..PageRecord::default()
        };
        let mut editor = Editor::new(Chapter {
            id: "c".into(),
            page_order: vec!["p".into()],
            pages: vec![page],
            ..Chapter::default()
        });
        editor.set_layer(Layer::Overlay);
        editor
    }

    fn order(editor: &Editor) -> Vec<String> {
        editor.page().unwrap().blocks.iter().map(|block| block.id.clone()).collect()
    }

    #[test]
    fn page_units_move_groups_whole_and_undo() {
        let ctx = egui::Context::default();
        let mut editor = editor();
        let units = page_units(editor.page().unwrap());
        assert_eq!(units.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(), ["block:a", "group:g", "block:d"]);
        let ids: Vec<String> = ["d", "b", "c", "a"].map(String::from).to_vec();
        editor.reorder_page_blocks(&ids);
        assert_eq!(order(&editor), ids);
        editor.undo(&ctx);
        assert_eq!(order(&editor), ["a", "b", "c", "d"]);
    }

    #[test]
    fn group_reorder_keeps_slots_and_updates_group() {
        let mut editor = editor();
        editor.reorder_group_blocks("g", &["c".into(), "b".into()]);
        assert_eq!(order(&editor), ["a", "c", "b", "d"]);
        let groups = editor.page().unwrap().block_groups.clone().unwrap();
        assert_eq!(groups[0].block_ids, ["c", "b"]);
    }

    #[test]
    fn list_selection_ranges_and_toggles_units() {
        let mut editor = editor();
        editor.list_select("a", egui::Modifiers::NONE);
        assert_eq!(editor.selection, ["a"]);
        editor.list_select("c", egui::Modifiers::SHIFT);
        assert_eq!(editor.selection, ["a", "b", "c"]);
        editor.list_select("b", egui::Modifiers::COMMAND);
        assert_eq!(editor.selection, ["a"], "⌘ toggles the whole group off");
    }
}
