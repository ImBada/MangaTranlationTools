//! Editor side panels: toolbar, page list, block properties, status bar, find/replace.

use eframe::egui::{self, Color32, RichText};
use mtt_core::font_presets::{
    DEFAULT_ENABLED_FONT_BACKGROUND_OPACITY, DEFAULT_ENABLED_SHADOW_DISTANCE_PX, DEFAULT_OVERLAY_FONT_FAMILY,
    ensure_block_type_font_presets, resolve_font_preset,
};
use mtt_core::model::{
    BlockType, FontRenderingMode, RenderTextDirection, StyleKey, TextAlign, TextDecoration, TextFontStyle,
    TextPosition, TranslationBlock,
};
use mtt_render::effects::FilterKind;

/// `DEFAULT_OVERLAY_FONT_WEIGHT`.
const DEFAULT_FONT_WEIGHT: f64 = 700.0;
/// `DEFAULT_SECONDARY_OUTLINE_WIDTH_PX`.
const DEFAULT_SECONDARY_OUTLINE_WIDTH_PX: f64 = 2.0;

use super::{Editor, Layer};
use crate::services::Services;

#[derive(Default)]
pub struct FindState {
    pub open: bool,
    pub keyword: String,
    pub replacement: String,
    /// Keyword of the last "찾기" (`searchedKeyword`); results follow it, not the field.
    pub searched: Option<String>,
}

/// `FindReplaceMatch`.
struct FindMatch {
    page_index: usize,
    page_name: String,
    block_id: String,
    block_index: usize,
    before: String,
    after: String,
    occurrences: usize,
}

/// `collectFindReplaceMatches`: literal matches in translated text.
fn collect_matches(editor: &Editor, keyword: &str, replacement: &str) -> Vec<FindMatch> {
    if keyword.is_empty() {
        return Vec::new();
    }
    let mut matches = Vec::new();
    for (page_index, page) in editor.chapter.pages.iter().enumerate() {
        for (block_index, block) in page.blocks.iter().enumerate() {
            let occurrences = block.translated_text.matches(keyword).count();
            if occurrences == 0 {
                continue;
            }
            matches.push(FindMatch {
                page_index,
                page_name: page.name.clone(),
                block_id: block.id.clone(),
                block_index,
                before: block.translated_text.clone(),
                after: block.translated_text.replace(keyword, replacement),
                occurrences,
            });
        }
    }
    matches
}

/// `HighlightedText`: marks every occurrence of `highlight`.
fn highlighted(ui: &egui::Ui, text: &str, highlight: &str) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let normal = egui::TextFormat { color: ui.visuals().text_color(), ..Default::default() };
    let marked =
        egui::TextFormat { color: Color32::BLACK, background: Color32::from_rgb(253, 224, 71), ..Default::default() };
    if highlight.is_empty() {
        job.append(text, 0.0, normal);
        return job;
    }
    for (index, part) in text.split(highlight).enumerate() {
        if index > 0 {
            job.append(highlight, 0.0, marked.clone());
        }
        job.append(part, 0.0, normal.clone());
    }
    job
}

/// `TranslationBlockGroupToolSection.tsx`.
pub(super) fn group_settings(editor: &mut Editor, ui: &mut egui::Ui, group: &mtt_core::model::TranslationBlockGroup) {
    ui.horizontal(|ui| {
        ui.label("선택 그룹");
        ui.label(RichText::new(format!("{}개 블록", group.block_ids.len())).strong());
    });
    let Some(page) = editor.page() else { return };
    let rows: Vec<(usize, TranslationBlock)> = group
        .block_ids
        .iter()
        .filter_map(|id| page.blocks.iter().position(|block| &block.id == id).map(|i| (i, page.blocks[i].clone())))
        .collect();
    if !rows.is_empty() {
        ui.horizontal(|ui| {
            ui.label("그룹 블록");
            ui.label(RichText::new(rows.len().to_string()).strong());
        });
        let reorder = rows.len() > 1 && !editor.page_edit_locked();
        let mut dropped: Option<(String, String, mtt_core::library::DropPosition)> = None;
        for (index, block) in &rows {
            let hidden = block.render_direction == RenderTextDirection::Hidden;
            let text = format!(
                "{}  {}{}  {}",
                index + 1,
                block_type_label(block.block_type),
                if hidden { " · 숨김" } else { "" },
                block_preview_text(block)
            );
            let row = crate::dnd::drag_button(ui, egui::Button::new(text.clone()).truncate(), block.id.clone(), &text)
                .on_hover_text(if reorder {
                    "드래그해서 순서 변경, 클릭해서 개별 설정"
                } else {
                    "개별 설정"
                });
            if row.clicked() {
                editor.selection = vec![block.id.clone()];
            }
            if reorder
                && let Some(pointer) = ui.input(|i| i.pointer.hover_pos())
                && row.dnd_hover_payload::<String>().is_some()
            {
                let after = pointer.y > row.rect.center().y;
                let y = if after { row.rect.bottom() } else { row.rect.top() };
                ui.painter().hline(row.rect.x_range(), y, (2.0, crate::theme::ACCENT));
                if let Some(source) = row.dnd_release_payload::<String>() {
                    let position = if after {
                        mtt_core::library::DropPosition::After
                    } else {
                        mtt_core::library::DropPosition::Before
                    };
                    dropped = Some(((*source).clone(), block.id.clone(), position));
                }
            }
        }
        if let Some((source, target, position)) = dropped {
            let next = mtt_core::library::reorder_by_target(&group.block_ids, &source, &target, position);
            editor.reorder_group_blocks(&group.id, &next);
        }
    }
    let effect = mtt_core::groups::drop_shadow_effect(group);
    let mut enabled = effect.is_some_and(|e| e.enabled);
    let mut settings = mtt_core::groups::drop_shadow_settings(effect);
    ui.separator();
    let mut changed = ui.checkbox(&mut enabled, "그림자").changed();
    egui::Grid::new("group-shadow").num_columns(2).show(ui, |ui| {
        ui.label("색");
        if let Some(hex) = super::color::color_field(editor, ui, "group-shadow", &settings.color) {
            settings.color = hex;
            changed = true;
        }
        ui.end_row();
        for (label, value, range) in [
            ("불투명도", &mut settings.opacity, 0.0..=1.0),
            ("흐림", &mut settings.blur_px, 0.0..=80.0),
            ("각도", &mut settings.angle_deg, -360.0..=360.0),
            ("거리", &mut settings.distance_px, 0.0..=80.0),
        ] {
            ui.label(label);
            changed |= ui.add(egui::DragValue::new(value).range(range).speed(0.5)).changed();
            ui.end_row();
        }
    });
    if changed {
        editor.set_group_drop_shadow(enabled, &settings);
    }
}

/// `resolveBlockPreviewText`: translated text, else source, whitespace collapsed.
pub(super) fn block_preview_text(block: &TranslationBlock) -> String {
    let text = if block.translated_text.is_empty() { &block.source_text } else { &block.translated_text };
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() { "빈 블록".into() } else { collapsed }
}

/// `BLOCK_TYPE_LABELS` (`lib/blockDisplay.ts`).
pub(super) fn block_type_label(value: BlockType) -> &'static str {
    match value {
        BlockType::Speech => "말풍선",
        BlockType::Sfx => "효과음",
        BlockType::Caption => "자막",
        BlockType::Other => "기타",
    }
}

/// Rename state of a preset tag (`renamingFontPresetId`).
#[derive(Clone, Default)]
struct TagRename {
    id: String,
    name: String,
}

/// The preset panel under the font controls (`font-preset-panel`).
/// `manage` adds the backup/restore row (the preset tab).
pub(super) fn preset_panel(
    editor: &mut Editor,
    ui: &mut egui::Ui,
    first: &TranslationBlock,
    services: &mut Services,
    manage: bool,
) {
    let presets = ensure_block_type_font_presets(editor.chapter.font_presets.as_deref().unwrap_or(&[]));
    let size_presets = editor.chapter.font_size_presets.clone().unwrap_or_default();
    let favorites = editor.chapter.favorite_font_preset_ids.clone().unwrap_or_default();
    let has_selection = !editor.selection.is_empty();
    // `resolveFontPresetTagActiveId`.
    let active_id = if has_selection {
        first.font_preset_id.clone().filter(|_| first.id != super::presets::PRESET_PROXY_ID)
    } else {
        editor.editing_preset_id.clone()
    };
    let name_id = ui.make_persistent_id("preset-name-input");
    let rename_id = ui.make_persistent_id("preset-tag-rename");
    let mut name_input: String = ui.data_mut(|data| data.get_temp(name_id).unwrap_or_default());
    let mut rename: Option<TagRename> = ui.data_mut(|data| data.get_temp(rename_id));
    let mut select = None;
    let mut delete = None;
    let mut favorite = None;
    let mut commit_rename = None;
    ui.horizontal_wrapped(|ui| {
        let ppp = ui.ctx().pixels_per_point();
        for preset in &presets {
            let active = active_id.as_deref() == Some(preset.id.as_str());
            let renaming = rename.as_ref().is_some_and(|r| r.id == preset.id);
            let favorite_index = favorites.iter().position(|id| id == &preset.id);
            let resolved = resolve_font_preset(preset, &size_presets);
            let texture = (!renaming).then(|| editor.font_previews.preset_tag(ui.ctx(), services, &resolved)).flatten();
            // A tag's size must be known up front so the wrapping row can break before it
            // (a frame of unknown width never wraps and widens the whole panel).
            let content = if renaming {
                egui::vec2(110.0, 20.0)
            } else if let Some(texture) = &texture {
                texture.size_vec2() / ppp
            } else {
                ui.painter()
                    .layout_no_wrap(preset.name.clone(), egui::TextStyle::Body.resolve(ui.style()), Color32::BLACK)
                    .size()
            };
            let spacing = ui.spacing().item_spacing.x;
            let extras = if favorite_index.is_some() { 14.0 + spacing } else { 0.0 } + 18.0 + spacing;
            let tag_size = egui::vec2(content.x + extras + 16.0, content.y.max(18.0) + 10.0);
            // Tags are paper-coloured samples of the preset's font.
            let fill = if active { Color32::from_rgb(214, 228, 255) } else { Color32::from_gray(232) };
            // Same stroke width either way: a frame's stroke takes room, so selecting a tag
            // must not resize it and shift the others.
            let stroke = egui::Stroke::new(2.0, if active { crate::theme::ACCENT } else { Color32::TRANSPARENT });
            ui.allocate_ui_with_layout(tag_size, egui::Layout::left_to_right(egui::Align::Center), |ui| {
                egui::Frame::new()
                    .fill(fill)
                    .stroke(stroke)
                    .corner_radius(6.0)
                    .inner_margin(egui::Margin::symmetric(6, 3))
                    .show(ui, |ui| {
                        if renaming {
                            let edit = rename.as_mut().expect("checked");
                            let response = ui.add(egui::TextEdit::singleline(&mut edit.name).desired_width(110.0));
                            response.request_focus();
                            editor.text_editing = true;
                            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                                commit_rename = Some(None);
                            } else if response.lost_focus() {
                                commit_rename = Some(Some(edit.clone()));
                            }
                            return;
                        }
                        let hint = if active {
                            format!("{} 이름 변경", preset.name)
                        } else if has_selection {
                            format!("{} 적용", preset.name)
                        } else {
                            format!("{} 편집", preset.name)
                        };
                        let response = match &texture {
                            Some(texture) => ui.add(
                                egui::Image::new((texture.id(), texture.size_vec2() / ppp)).sense(egui::Sense::click()),
                            ),
                            None => ui.add(
                                egui::Label::new(RichText::new(&preset.name).color(Color32::BLACK))
                                    .sense(egui::Sense::click()),
                            ),
                        }
                        .on_hover_text(hint);
                        if response.clicked() {
                            if active {
                                rename = Some(TagRename { id: preset.id.clone(), name: preset.name.clone() });
                            } else {
                                select = Some(preset.clone());
                            }
                        }
                        if response.secondary_clicked() {
                            favorite = Some(preset.id.clone());
                        }
                        if let Some(index) = favorite_index {
                            ui.label(
                                RichText::new((index + 1).to_string())
                                    .small()
                                    .strong()
                                    .color(Color32::from_rgb(217, 119, 6)),
                            );
                        }
                        let close = egui::Button::new(RichText::new("×").color(Color32::from_gray(90))).frame(false);
                        if ui.add(close).on_hover_text(format!("{} 삭제", preset.name)).clicked() {
                            delete = Some(preset.clone());
                        }
                    });
            });
        }
    });
    ui.horizontal(|ui| {
        if has_selection
            && first.font_preset_id.is_some()
            && ui.button("프리셋 해제").on_hover_text("지금 모양은 그대로 두고 프리셋 연결만 끊습니다").clicked()
        {
            editor.clear_selected_preset();
        }
        if !has_selection && editor.editing_preset_id.is_some() && ui.button("편집 끝내기").clicked() {
            editor.editing_preset_id = None;
        }
    });
    let can_create = has_selection || editor.editing_preset_id.is_some();
    ui.horizontal(|ui| {
        let response = ui.add_enabled(
            can_create,
            egui::TextEdit::singleline(&mut name_input)
                .hint_text("새 프리셋 이름")
                .desired_width((ui.available_width() - 150.0).max(70.0)),
        );
        if response.has_focus() {
            editor.text_editing = true;
        }
        let hint = if has_selection {
            "선택한 블록의 스타일로 새 프리셋을 만들고 연결합니다"
        } else {
            "편집 중인 프리셋을 복제합니다"
        };
        if ui
            .add_enabled(can_create, egui::Button::new("새 프리셋으로 저장"))
            .on_hover_text(hint)
            .on_disabled_hover_text("블록이나 프리셋을 먼저 선택하세요")
            .clicked()
        {
            match editor.create_preset_from_selection(&name_input) {
                Ok(()) => name_input.clear(),
                Err(error) => editor.status = error,
            }
        }
    });
    if manage {
        ui.horizontal(|ui| {
            if ui.button("백업…").on_hover_text("프리셋을 파일로 내보내기").clicked() {
                editor.preset_backup_dialog = Some(super::PresetBackupDialog::backup());
            }
            if ui.button("복원…").on_hover_text("백업 파일에서 프리셋 가져오기").clicked() {
                editor.preset_backup_dialog = Some(super::PresetBackupDialog::restore());
            }
        });
    }
    if let Some(preset) = select {
        editor.select_preset(&preset);
    }
    if let Some(id) = favorite {
        editor.toggle_favorite(&id);
    }
    if let Some(preset) = delete
        && crate::theme::confirm(
            "폰트 프리셋 삭제",
            &format!("\"{}\" 프리셋을 삭제할까요?", preset.name),
            "이 프리셋이 적용된 텍스트 블록에서는 프리셋 연결만 해제됩니다.",
        )
    {
        editor.delete_preset(&preset.id);
    }
    match commit_rename {
        Some(Some(edit)) => {
            let unchanged = presets.iter().any(|p| p.id == edit.id && p.name == edit.name.trim());
            if !unchanged && let Err(error) = editor.rename_preset(&edit.id, &edit.name) {
                editor.status = error;
            }
            rename = None;
        }
        Some(None) => rename = None,
        None => {}
    }
    ui.data_mut(|data| {
        data.insert_temp(name_id, name_input);
        match rename {
            Some(rename) => {
                data.insert_temp(rename_id, rename);
            }
            None => data.remove::<TagRename>(rename_id),
        }
    });
}

/// 🔗 toggle for a field of a block assigned to a font preset.
fn link_toggle(editor: &mut Editor, ui: &mut egui::Ui, first: &TranslationBlock, key: StyleKey) {
    if first.font_preset_id.is_none() || first.id == super::presets::PRESET_PROXY_ID {
        return;
    }
    let linked = mtt_core::font_presets::is_value_linked(first, key);
    let text = if linked { RichText::new("🔗") } else { RichText::new("🔗").weak().strikethrough() };
    let hint = if linked {
        "프리셋과 연결됨: 수정하면 같은 프리셋의 모든 블록이 바뀝니다. 눌러서 해제"
    } else {
        "이 블록만의 값. 눌러서 프리셋 값으로 다시 연결"
    };
    if ui.selectable_label(linked, text).on_hover_text(hint).clicked() {
        editor.toggle_link(key);
    }
}

/// Size preset picker under the size field (`FontToolSection` 크기 프리셋 select and +/- button).
fn size_preset_row(editor: &mut Editor, ui: &mut egui::Ui, first: &TranslationBlock) {
    let sizes = editor.chapter.font_size_presets.clone().unwrap_or_default();
    let active = editor.active_size_preset_id(first);
    let enabled = Editor::size_preset_controls_enabled(first);
    ui.label("크기 프리셋");
    ui.add_enabled_ui(enabled, |ui| {
        ui.horizontal(|ui| {
            let label = |preset: &mtt_core::model::FontSizePreset| format!("{} · {}px", preset.name, preset.font_size_px);
            let selected_text =
                active.as_deref().and_then(|id| sizes.iter().find(|p| p.id == id)).map(label).unwrap_or("개별 지정".into());
            let mut chosen: Option<Option<String>> = None;
            egui::ComboBox::from_id_salt("font-size-preset").width(110.0).truncate().selected_text(selected_text).show_ui(ui, |ui| {
                if ui.selectable_label(active.is_none(), "개별 지정").clicked() {
                    chosen = Some(None);
                }
                for preset in &sizes {
                    if ui.selectable_label(active.as_deref() == Some(preset.id.as_str()), label(preset)).clicked() {
                        chosen = Some(Some(preset.id.clone()));
                    }
                }
            });
            if let Some(choice) = chosen
                && choice != active
            {
                editor.select_size_preset(choice.as_deref());
            }
            match &active {
                Some(id) => {
                    if ui.small_button("-").on_hover_text("선택한 크기 프리셋 삭제").clicked() {
                        let name = sizes.iter().find(|p| &p.id == id).map(|p| p.name.clone());
                        let question = match &name {
                            Some(name) => format!("\"{name}\" 크기 프리셋을 삭제할까요?"),
                            None => "선택한 크기 프리셋을 삭제할까요?".into(),
                        };
                        let confirmed = rfd::MessageDialog::new()
                            .set_title("폰트 크기 프리셋 삭제")
                            .set_description(format!(
                                "{question}\n\n이 크기 프리셋을 참조하는 폰트 프리셋에서는 크기 프리셋 연결만 해제됩니다."
                            ))
                            .set_buttons(rfd::MessageButtons::OkCancel)
                            .show();
                        if confirmed == rfd::MessageDialogResult::Ok {
                            editor.delete_size_preset(id);
                        }
                    }
                }
                None => {
                    if ui.small_button("+").on_hover_text("현재 크기 저장").clicked() {
                        editor.create_size_preset_from_selection();
                    }
                }
            }
        });
    })
    .response
    .on_disabled_hover_text("프리셋이 지정되고 크기가 프리셋에 연결된 블록에서 쓸 수 있습니다.");
    ui.end_row();
}

fn color_row(
    editor: &mut Editor,
    ui: &mut egui::Ui,
    first: &TranslationBlock,
    label: &str,
    key: StyleKey,
    hex: &str,
    set: impl Fn(&mut TranslationBlock, String),
) {
    ui.label(label);
    if let Some(hex) = super::color::color_field(editor, ui, &format!("style-{key:?}"), hex) {
        editor.edit_selected_style(key, |block| set(block, hex.clone()));
    }
    link_toggle(editor, ui, first, key);
    ui.end_row();
}

/// `FONT_WEIGHT_LABELS`.
fn weight_label(weight: u16) -> String {
    match weight {
        100 => "Thin 100".into(),
        200 => "Extra Light 200".into(),
        300 => "Light 300".into(),
        400 => "Regular 400".into(),
        500 => "Medium 500".into(),
        600 => "Semi Bold 600".into(),
        700 => "Bold 700".into(),
        800 => "Extra Bold 800".into(),
        900 => "Black 900".into(),
        other => format!("Weight {other}"),
    }
}

/// One 🔗 for several fields (`renderFontPresetLinkGroupButton`).
fn link_group_toggle(editor: &mut Editor, ui: &mut egui::Ui, first: &TranslationBlock, keys: &[StyleKey]) {
    if first.font_preset_id.is_none() || first.id == super::presets::PRESET_PROXY_ID {
        return;
    }
    let linked = keys.iter().all(|key| mtt_core::font_presets::is_value_linked(first, *key));
    let text = if linked { RichText::new("🔗") } else { RichText::new("🔗").weak().strikethrough() };
    if ui.selectable_label(linked, text).on_hover_text("스타일 프리셋 연결").clicked() {
        editor.toggle_link_group(keys);
    }
}

/// Number field with TS range/step/suffix and its 🔗.
#[allow(clippy::too_many_arguments)]
fn stepped_row(
    editor: &mut Editor,
    ui: &mut egui::Ui,
    first: &TranslationBlock,
    label: &str,
    key: StyleKey,
    value: f64,
    range: std::ops::RangeInclusive<f64>,
    step: f64,
    suffix: &str,
    set: impl Fn(&mut TranslationBlock, f64),
) {
    ui.label(label);
    let mut current = value;
    let drag = egui::DragValue::new(&mut current).range(range).speed(step).suffix(suffix);
    let drag = if step < 1.0 { drag.fixed_decimals(if step < 0.1 { 2 } else { 1 }) } else { drag };
    if ui.add(drag).changed() {
        editor.edit_selected_style(key, |block| set(block, current));
    }
    link_toggle(editor, ui, first, key);
    ui.end_row();
}

pub(super) fn font_section(editor: &mut Editor, ui: &mut egui::Ui, first: &TranslationBlock, services: &mut Services) {
    let style = &first.style;
    egui::CollapsingHeader::new("글꼴").default_open(true).show(ui, |ui| {
        ui.label("서체");
        let current = style.font_family.clone().unwrap_or_else(|| DEFAULT_OVERLAY_FONT_FAMILY.to_owned());
        let mut text_editing = editor.text_editing;
        let chosen = super::font_picker::picker(ui, services, &mut editor.font_previews, &current, &mut text_editing);
        editor.text_editing = text_editing;
        if let Some(option) = chosen {
            editor.edit_style(|block| block.style.font_family = Some(option.value.clone()));
        }
        let weights = services
            .fonts()
            .and_then(|fonts| fonts.lock().ok().and_then(|fonts| fonts.family_weights(&current)))
            .map(|(weights, _)| weights)
            .unwrap_or_default();
        egui::Grid::new("font-grid").num_columns(3).show(ui, |ui| {
            ui.label("스타일");
            ui.horizontal(|ui| {
                let weight = style.font_weight.unwrap_or(DEFAULT_FONT_WEIGHT);
                // B toggles 400 ↔ the default weight at the 600 threshold.
                if ui.selectable_label(weight >= 600.0, RichText::new("B").strong()).on_hover_text("볼드").clicked() {
                    let next = if weight >= 600.0 { 400.0 } else { DEFAULT_FONT_WEIGHT };
                    editor.edit_selected_style(StyleKey::FontWeight, |b| b.style.font_weight = Some(next));
                }
                let italic = style.font_style.unwrap_or_default() == TextFontStyle::Italic;
                if ui.selectable_label(italic, RichText::new("I").italics()).on_hover_text("이탤릭").clicked() {
                    let value = if italic { TextFontStyle::Normal } else { TextFontStyle::Italic };
                    editor.edit_selected_style(StyleKey::FontStyle, |b| b.style.font_style = Some(value));
                }
                let underline = style.text_decoration.unwrap_or_default() == TextDecoration::Underline;
                if ui.selectable_label(underline, RichText::new("U").underline()).on_hover_text("언더라인").clicked()
                {
                    let value = if underline { TextDecoration::None } else { TextDecoration::Underline };
                    editor.edit_selected_style(StyleKey::TextDecoration, |b| b.style.text_decoration = Some(value));
                }
            });
            link_group_toggle(
                editor,
                ui,
                first,
                &[StyleKey::FontWeight, StyleKey::FontStyle, StyleKey::TextDecoration],
            );
            ui.end_row();
            if weights.len() > 1 {
                ui.label("굵기");
                let weight = style.font_weight.unwrap_or(DEFAULT_FONT_WEIGHT);
                let current_weight = weight.round() as u16;
                let mut options = weights.clone();
                if !options.contains(&current_weight) {
                    options.push(current_weight);
                    options.sort_unstable();
                }
                egui::ComboBox::from_id_salt("font-weight")
                    .width(130.0)
                    .truncate()
                    .selected_text(weight_label(current_weight))
                    .show_ui(ui, |ui| {
                        for option in options {
                            if ui.selectable_label(option == current_weight, weight_label(option)).clicked() {
                                editor.edit_selected_style(StyleKey::FontWeight, |b| {
                                    b.style.font_weight = Some(f64::from(option))
                                });
                            }
                        }
                    });
                ui.end_row();
            }

            ui.label("글자 렌더링");
            let mode = style.font_rendering_mode.unwrap_or_default();
            let label = |mode: FontRenderingMode| match mode {
                FontRenderingMode::Hard => "선명하게 (하드 가장자리)",
                _ => "부드럽게",
            };
            egui::ComboBox::from_id_salt("font-rendering-mode")
                .width(130.0)
                .truncate()
                .selected_text(label(mode))
                .show_ui(ui, |ui| {
                    for option in [FontRenderingMode::Smooth, FontRenderingMode::Hard] {
                        if ui.selectable_label(mode == option, label(option)).clicked() && mode != option {
                            editor.edit_selected_style(StyleKey::FontRenderingMode, |b| {
                                b.style.font_rendering_mode = Some(option)
                            });
                        }
                    }
                });
            link_toggle(editor, ui, first, StyleKey::FontRenderingMode);
            ui.end_row();

            stepped_row(
                editor,
                ui,
                first,
                "크기",
                StyleKey::FontSizePx,
                first.font_size_px(),
                8.0..=999.0,
                1.0,
                "px",
                |b, v| b.style.font_size_px = Some(v),
            );
            size_preset_row(editor, ui, first);
            stepped_row(
                editor,
                ui,
                first,
                "줄 간격",
                StyleKey::LineHeight,
                first.line_height(),
                0.8..=2.0,
                0.05,
                "배",
                |b, v| b.style.line_height = Some(v),
            );
            stepped_row(
                editor,
                ui,
                first,
                "글자 간격",
                StyleKey::LetterSpacingPx,
                style.letter_spacing_px.unwrap_or(0.0),
                -10.0..=30.0,
                0.5,
                "px",
                |b, v| b.style.letter_spacing_px = Some(v),
            );
            color_row(editor, ui, first, "글자색", StyleKey::TextColor, first.text_color(), |b, v| {
                b.style.text_color = Some(v)
            });

            ui.label("정렬");
            let mut align = first.text_align;
            ui.horizontal(|ui| {
                ui.selectable_value(&mut align, TextAlign::Left, "좌").on_hover_text("왼쪽 정렬");
                ui.selectable_value(&mut align, TextAlign::Center, "중").on_hover_text("가운데 정렬");
                ui.selectable_value(&mut align, TextAlign::Right, "우").on_hover_text("오른쪽 정렬");
            });
            if align != first.text_align {
                editor.edit_style(|b| b.text_align = align);
            }
            ui.end_row();

            ui.label("위치");
            let mut position = first.text_position.unwrap_or_default();
            egui::Grid::new("text-position").spacing([2.0, 2.0]).show(ui, |ui| {
                for row in [
                    [
                        (TextPosition::TopLeft, "상단 좌측"),
                        (TextPosition::Top, "상단 중앙"),
                        (TextPosition::TopRight, "상단 우측"),
                    ],
                    [
                        (TextPosition::Left, "중앙 좌측"),
                        (TextPosition::Center, "중앙"),
                        (TextPosition::Right, "중앙 우측"),
                    ],
                    [
                        (TextPosition::BottomLeft, "하단 좌측"),
                        (TextPosition::Bottom, "하단 중앙"),
                        (TextPosition::BottomRight, "하단 우측"),
                    ],
                ] {
                    for (value, hint) in row {
                        let marker = if position == value { "●" } else { "○" };
                        ui.selectable_value(&mut position, value, marker).on_hover_text(hint);
                    }
                    ui.end_row();
                }
            });
            if Some(position) != first.text_position
                && !(first.text_position.is_none() && position == TextPosition::Center)
            {
                editor.edit_selected(|b| b.text_position = Some(position));
            }
            ui.end_row();

            ui.label("자동 맞춤");
            let mut auto_fit = style.auto_fit_text.unwrap_or(true);
            if ui.checkbox(&mut auto_fit, "").changed() {
                editor.edit_selected_style(StyleKey::AutoFitText, |b| b.style.auto_fit_text = Some(auto_fit));
            }
            link_toggle(editor, ui, first, StyleKey::AutoFitText);
            ui.end_row();
        });
    });
}

/// "고급 설정" draft (`FontCharacterOverrideModal`).
pub struct CharacterOverrideDialog {
    preset_name: String,
    drafts: Vec<mtt_core::model::FontCharacterOverride>,
    new_character: String,
    new_family: String,
    error: Option<String>,
}

/// `normalizeCharacterOverrideCharacter`: the first character after trimming.
fn first_character(value: &str) -> String {
    value.trim().chars().next().map(String::from).unwrap_or_default()
}

/// Opens "고급 설정" for the font controls' preset.
pub(super) fn open_character_overrides(editor: &mut Editor) {
    let Some(first) = editor.selected_blocks().into_iter().next().or_else(|| editor.preset_proxy_block()) else {
        return;
    };
    let preset_name = first
        .font_preset_id
        .as_deref()
        .and_then(|id| editor.chapter.font_presets.iter().flatten().find(|p| p.id == id))
        .map(|p| p.name.clone())
        .unwrap_or_default();
    let family = first.style.font_family.clone().unwrap_or_else(|| DEFAULT_OVERLAY_FONT_FAMILY.to_owned());
    editor.character_dialog = Some(CharacterOverrideDialog {
        preset_name,
        drafts: mtt_core::font_presets::normalize_character_font_overrides(
            &first.style.character_font_overrides.clone().unwrap_or_default(),
        ),
        new_character: String::new(),
        new_family: family,
        error: None,
    });
}

pub fn character_override_window(editor: &mut Editor, ui: &mut egui::Ui, services: &mut Services) {
    let Some(mut dialog) = editor.character_dialog.take() else { return };
    let mut open = true;
    let mut close = false;
    let mut save = false;
    egui::Window::new("고급 설정").open(&mut open).collapsible(false).default_width(460.0).show(ui.ctx(), |ui| {
        ui.horizontal(|ui| {
            ui.label("프리셋");
            ui.label(RichText::new(&dialog.preset_name).strong());
        });
        ui.separator();
        let mut text_editing = editor.text_editing;
        ui.horizontal(|ui| {
            ui.label("문자");
            let response = ui.add(egui::TextEdit::singleline(&mut dialog.new_character).desired_width(36.0));
            dialog.new_character = first_character(&dialog.new_character);
            if response.has_focus() {
                text_editing = true;
            }
            let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            ui.label("폰트");
            if let Some(option) = super::font_picker::picker_with_id(
                ui,
                services,
                &mut editor.font_previews,
                &dialog.new_family,
                &mut text_editing,
                "override-new-family",
            ) {
                dialog.new_family = option.value;
            }
            if ui.button("추가").clicked() || enter {
                let character = first_character(&dialog.new_character);
                if character.is_empty() {
                    dialog.error = Some("문자를 한 글자 입력하세요.".into());
                } else {
                    dialog.drafts.retain(|entry| entry.character != character);
                    dialog.drafts.push(mtt_core::model::FontCharacterOverride {
                        character,
                        font_family: dialog.new_family.clone(),
                    });
                    dialog.new_character.clear();
                    dialog.error = None;
                }
            }
        });
        if let Some(error) = &dialog.error {
            ui.colored_label(crate::theme::DANGER, error);
        }
        ui.separator();
        if dialog.drafts.is_empty() {
            ui.label(RichText::new("등록된 커스텀 문자가 없습니다.").weak());
        }
        let mut remove = None;
        for (index, entry) in dialog.drafts.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                ui.label("문자");
                let response = ui.add(egui::TextEdit::singleline(&mut entry.character).desired_width(36.0));
                entry.character = first_character(&entry.character);
                if response.has_focus() {
                    text_editing = true;
                }
                ui.label(RichText::new(&entry.character).size(20.0));
                ui.label("폰트");
                if let Some(option) = super::font_picker::picker_with_id(
                    ui,
                    services,
                    &mut editor.font_previews,
                    &entry.font_family,
                    &mut text_editing,
                    &format!("override-family-{index}"),
                ) {
                    entry.font_family = option.value;
                }
                if ui.small_button("삭제").clicked() {
                    remove = Some(index);
                }
            });
        }
        if let Some(index) = remove {
            dialog.drafts.remove(index);
            dialog.error = None;
        }
        editor.text_editing = text_editing;
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("취소").clicked() {
                close = true;
            }
            if ui.button("저장").clicked() {
                save = true;
            }
        });
    });
    if save {
        let overrides = mtt_core::font_presets::normalize_character_font_overrides(&dialog.drafts);
        let value = (!overrides.is_empty()).then_some(overrides);
        editor.edit_style(|block| block.style.character_font_overrides = value.clone());
        close = true;
    }
    if open && !close {
        editor.character_dialog = Some(dialog);
    }
}

/// Slider row with a percentage label (`font-range-field`).
#[allow(clippy::too_many_arguments)]
fn percent_row(
    editor: &mut Editor,
    ui: &mut egui::Ui,
    first: &TranslationBlock,
    label: &str,
    key: StyleKey,
    value: f64,
    range: std::ops::RangeInclusive<f64>,
    set: impl Fn(&mut TranslationBlock, f64),
) {
    ui.label(label);
    let mut current = value;
    let slider = egui::Slider::new(&mut current, range).custom_formatter(|v, _| format!("{:.0}%", v * 100.0));
    if ui.add(slider).changed() {
        editor.edit_selected_style(key, |block| set(block, current));
    }
    link_toggle(editor, ui, first, key);
    ui.end_row();
}

/// Two-choice switch like the TS "1개/2개", "OFF/ON" buttons. Returns the new value.
fn switch(ui: &mut egui::Ui, value: bool, off: &str, on: &str) -> Option<bool> {
    let mut next = None;
    ui.horizontal(|ui| {
        if ui.selectable_label(!value, off).clicked() && value {
            next = Some(false);
        }
        if ui.selectable_label(value, on).clicked() && !value {
            next = Some(true);
        }
    });
    next
}

/// `FontOutlineControls.tsx`.
pub(super) fn outline_section(editor: &mut Editor, ui: &mut egui::Ui, first: &TranslationBlock) {
    let style = &first.style;
    egui::CollapsingHeader::new("외곽선").show(ui, |ui| {
        egui::Grid::new("outline-grid").num_columns(3).show(ui, |ui| {
            let secondary = mtt_render::layout::secondary_outline_enabled(first);
            ui.label("외곽선");
            if let Some(enabled) = switch(ui, secondary, "1개", "2개") {
                let color = style.secondary_outline_color.clone();
                let width = style.secondary_outline_width_px;
                editor.edit_selected_style(StyleKey::SecondaryOutlineEnabled, |b| {
                    b.style.secondary_outline_enabled = Some(enabled);
                    if enabled {
                        // `enableSecondaryOutline`: white, 2px unless a width is already set.
                        b.style.secondary_outline_color = Some(color.clone().unwrap_or_else(|| "#ffffff".into()));
                        b.style.secondary_outline_width_px =
                            Some(width.filter(|w| *w > 0.0).unwrap_or(DEFAULT_SECONDARY_OUTLINE_WIDTH_PX));
                    }
                });
            }
            link_toggle(editor, ui, first, StyleKey::SecondaryOutlineEnabled);
            ui.end_row();
            let prefix = if secondary { "1차 " } else { "" };
            color_row(
                editor,
                ui,
                first,
                &format!("{prefix}외곽선 색"),
                StyleKey::OutlineColor,
                style.outline_color.as_deref().unwrap_or("#000000"),
                |b, v| b.style.outline_color = Some(v),
            );
            stepped_row(
                editor,
                ui,
                first,
                &format!("{prefix}외곽선 두께"),
                StyleKey::OutlineWidthPx,
                style.outline_width_px.unwrap_or(0.0),
                0.0..=24.0,
                0.5,
                "px",
                |b, v| b.style.outline_width_px = Some(v),
            );
            if secondary {
                color_row(
                    editor,
                    ui,
                    first,
                    "2차 외곽선 색",
                    StyleKey::SecondaryOutlineColor,
                    style.secondary_outline_color.as_deref().unwrap_or("#ffffff"),
                    |b, v| b.style.secondary_outline_color = Some(v),
                );
                stepped_row(
                    editor,
                    ui,
                    first,
                    "2차 외곽선 두께",
                    StyleKey::SecondaryOutlineWidthPx,
                    style.secondary_outline_width_px.unwrap_or(0.0),
                    0.0..=24.0,
                    0.5,
                    "px",
                    |b, v| b.style.secondary_outline_width_px = Some(v),
                );
            }
        });
    });
}

/// `FontShadowControls.tsx`.
pub(super) fn shadow_section(editor: &mut Editor, ui: &mut egui::Ui, first: &TranslationBlock) {
    let style = &first.style;
    egui::CollapsingHeader::new("그림자").show(ui, |ui| {
        egui::Grid::new("shadow-grid").num_columns(3).show(ui, |ui| {
            let active = style.shadow_enabled.unwrap_or_else(|| {
                style.shadow_distance_px.unwrap_or(0.0) > 0.0 || style.shadow_blur_px.unwrap_or(0.0) > 0.0
            });
            ui.label("그림자");
            if let Some(enabled) = switch(ui, active, "OFF", "ON") {
                let distance = style.shadow_distance_px.unwrap_or(0.0);
                editor.edit_selected_style(StyleKey::ShadowEnabled, |b| {
                    b.style.shadow_enabled = Some(enabled);
                    if enabled && distance <= 0.0 {
                        b.style.shadow_distance_px = Some(DEFAULT_ENABLED_SHADOW_DISTANCE_PX);
                    }
                });
            }
            link_toggle(editor, ui, first, StyleKey::ShadowEnabled);
            ui.end_row();
            if !active {
                return;
            }
            color_row(
                editor,
                ui,
                first,
                "그림자 색",
                StyleKey::ShadowColor,
                style.shadow_color.as_deref().unwrap_or("#000000"),
                |b, v| b.style.shadow_color = Some(v),
            );
            percent_row(
                editor,
                ui,
                first,
                "불투명도",
                StyleKey::ShadowOpacity,
                style.shadow_opacity.unwrap_or(1.0),
                0.0..=1.0,
                |b, v| b.style.shadow_opacity = Some(v),
            );
            stepped_row(
                editor,
                ui,
                first,
                "흐림",
                StyleKey::ShadowBlurPx,
                style.shadow_blur_px.unwrap_or(0.0),
                0.0..=80.0,
                0.5,
                "px",
                |b, v| b.style.shadow_blur_px = Some(v),
            );
            stepped_row(
                editor,
                ui,
                first,
                "각도",
                StyleKey::ShadowAngleDeg,
                style.shadow_angle_deg.unwrap_or(45.0),
                -360.0..=360.0,
                1.0,
                "도",
                |b, v| b.style.shadow_angle_deg = Some(v),
            );
            stepped_row(
                editor,
                ui,
                first,
                "거리",
                StyleKey::ShadowDistancePx,
                style.shadow_distance_px.unwrap_or(0.0),
                0.0..=80.0,
                0.5,
                "px",
                |b, v| b.style.shadow_distance_px = Some(v),
            );
        });
    });
}

/// Screentone fill and font background (`font-screentone-field`, `font-background-field`).
pub(super) fn background_section(editor: &mut Editor, ui: &mut egui::Ui, first: &TranslationBlock) {
    let style = &first.style;
    egui::CollapsingHeader::new("스크린톤 · 글자 배경").show(ui, |ui| {
        egui::Grid::new("background-grid").num_columns(3).show(ui, |ui| {
            let screentone = style.screentone_fill_enabled.unwrap_or(false);
            ui.label("스크린톤 채우기");
            ui.horizontal(|ui| {
                if let Some(enabled) = switch(ui, screentone, "OFF", "ON") {
                    editor.edit_selected_style(StyleKey::ScreentoneFillEnabled, |b| {
                        b.style.screentone_fill_enabled = Some(enabled)
                    });
                }
                if screentone {
                    let antialias = style.screentone_fill_antialias.unwrap_or(true);
                    if ui.selectable_label(antialias, "안티").on_hover_text("스크린톤 안티앨리어싱").clicked()
                    {
                        editor.edit_selected_style(StyleKey::ScreentoneFillAntialias, |b| {
                            b.style.screentone_fill_antialias = Some(!antialias)
                        });
                    }
                    link_toggle(editor, ui, first, StyleKey::ScreentoneFillAntialias);
                }
            });
            link_toggle(editor, ui, first, StyleKey::ScreentoneFillEnabled);
            ui.end_row();
            if screentone {
                percent_row(
                    editor,
                    ui,
                    first,
                    "강도",
                    StyleKey::ScreentoneFillIntensity,
                    style.screentone_fill_intensity.unwrap_or(0.55),
                    0.05..=1.0,
                    |b, v| b.style.screentone_fill_intensity = Some(v),
                );
                percent_row(
                    editor,
                    ui,
                    first,
                    "밀도",
                    StyleKey::ScreentoneFillDensity,
                    style.screentone_fill_density.unwrap_or(0.55),
                    0.05..=1.0,
                    |b, v| b.style.screentone_fill_density = Some(v),
                );
            }

            let background = mtt_render::layout::font_background_enabled(first);
            ui.label("글자 배경");
            if let Some(enabled) = switch(ui, background, "OFF", "ON") {
                let opacity = mtt_render::layout::font_background_opacity(first);
                editor.edit_selected_style(StyleKey::FontBackgroundEnabled, |b| {
                    b.style.font_background_enabled = Some(enabled);
                    if enabled && opacity <= 0.0 {
                        b.style.font_background_opacity = Some(DEFAULT_ENABLED_FONT_BACKGROUND_OPACITY);
                    }
                });
            }
            link_toggle(editor, ui, first, StyleKey::FontBackgroundEnabled);
            ui.end_row();
            if background {
                color_row(
                    editor,
                    ui,
                    first,
                    "배경색",
                    StyleKey::FontBackgroundColor,
                    style.font_background_color.as_deref().unwrap_or("#ffffff"),
                    |b, v| b.style.font_background_color = Some(v),
                );
                percent_row(
                    editor,
                    ui,
                    first,
                    "크기",
                    StyleKey::FontBackgroundScale,
                    mtt_render::layout::font_background_scale(first),
                    0.8..=2.0,
                    |b, v| b.style.font_background_scale = Some(v),
                );
                stepped_row(
                    editor,
                    ui,
                    first,
                    "라운드",
                    StyleKey::FontBackgroundRadiusPx,
                    mtt_render::layout::font_background_radius_px(first),
                    0.0..=96.0,
                    1.0,
                    "px",
                    |b, v| b.style.font_background_radius_px = Some(v),
                );
                percent_row(
                    editor,
                    ui,
                    first,
                    "투명도",
                    StyleKey::FontBackgroundOpacity,
                    mtt_render::layout::font_background_opacity(first),
                    0.0..=1.0,
                    |b, v| b.style.font_background_opacity = Some(v),
                );
            }
        });
    });
}

/// `FontFilterEffectControls.tsx`: child 🔗 only while the master link is on.
pub(super) fn effect_section(editor: &mut Editor, ui: &mut egui::Ui, first: &TranslationBlock) {
    egui::CollapsingHeader::new("필터 효과").show(ui, |ui| {
        let enabled = first.style.filter_effect_enabled.unwrap_or(false);
        let master_linked = mtt_core::font_presets::is_value_linked(first, StyleKey::FilterEffectEnabled);
        egui::Grid::new("filter-grid").num_columns(3).show(ui, |ui| {
            ui.label("필터 효과");
            if let Some(next) = switch(ui, enabled, "OFF", "ON") {
                let has_child = FilterKind::ALL
                    .iter()
                    .any(|kind| kind.values(first).0 == Some(true) && kind.intensity(first) > 0.0);
                let zigzag = enabled_intensity(FilterKind::Zigzag.values(first).1);
                editor.edit_selected_style(StyleKey::FilterEffectEnabled, |b| {
                    b.style.filter_effect_enabled = Some(next);
                    // `buildEnabledFontFilterEffectSettingPatch`.
                    if next && !has_child {
                        b.style.filter_zigzag_enabled = Some(true);
                        b.style.filter_zigzag_intensity = Some(zigzag);
                    }
                });
            }
            link_toggle(editor, ui, first, StyleKey::FilterEffectEnabled);
            ui.end_row();
            if !enabled {
                return;
            }
            for kind in FilterKind::ALL {
                let (enabled_key, intensity_key) = filter_keys(kind);
                let on = kind.values(first).0.unwrap_or(false);
                ui.label(kind.label());
                if ui.selectable_label(on, if on { "ON" } else { "OFF" }).clicked() {
                    let intensity = enabled_intensity(kind.values(first).1);
                    editor.edit_selected_style(enabled_key, |b| {
                        let value = if on { kind.intensity(b) } else { intensity };
                        set_filter(b, kind, !on, value)
                    });
                }
                if master_linked {
                    link_toggle(editor, ui, first, enabled_key);
                }
                ui.end_row();
                if on {
                    ui.label(format!("{} 강도", kind.label()));
                    let mut value = kind.intensity(first);
                    let slider =
                        egui::Slider::new(&mut value, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0));
                    if ui.add(slider).changed() {
                        editor.edit_selected_style(intensity_key, |b| set_filter(b, kind, true, value));
                    }
                    if master_linked {
                        link_toggle(editor, ui, first, intensity_key);
                    }
                    ui.end_row();
                }
            }
        });
    });
}

/// `resolveEnabledFontFilterIntensity`: the stored intensity, or the 0.35 default when unset/0.
fn enabled_intensity(intensity: Option<f64>) -> f64 {
    intensity.filter(|value| value.is_finite() && *value > 0.0).map_or(0.35, |value| value.clamp(0.0, 1.0))
}

fn filter_keys(kind: FilterKind) -> (StyleKey, StyleKey) {
    match kind {
        FilterKind::Zigzag => (StyleKey::FilterZigzagEnabled, StyleKey::FilterZigzagIntensity),
        FilterKind::Wave => (StyleKey::FilterWaveEnabled, StyleKey::FilterWaveIntensity),
        FilterKind::Glitch => (StyleKey::FilterGlitchEnabled, StyleKey::FilterGlitchIntensity),
        FilterKind::Pixelate => (StyleKey::FilterPixelateEnabled, StyleKey::FilterPixelateIntensity),
        FilterKind::Blur => (StyleKey::FilterBlurEnabled, StyleKey::FilterBlurIntensity),
        FilterKind::Noise => (StyleKey::FilterNoiseEnabled, StyleKey::FilterNoiseIntensity),
    }
}

fn set_filter(block: &mut TranslationBlock, kind: FilterKind, on: bool, value: f64) {
    let style = &mut block.style;
    let (enabled, intensity) = match kind {
        FilterKind::Zigzag => (&mut style.filter_zigzag_enabled, &mut style.filter_zigzag_intensity),
        FilterKind::Wave => (&mut style.filter_wave_enabled, &mut style.filter_wave_intensity),
        FilterKind::Glitch => (&mut style.filter_glitch_enabled, &mut style.filter_glitch_intensity),
        FilterKind::Pixelate => (&mut style.filter_pixelate_enabled, &mut style.filter_pixelate_intensity),
        FilterKind::Blur => (&mut style.filter_blur_enabled, &mut style.filter_blur_intensity),
        FilterKind::Noise => (&mut style.filter_noise_enabled, &mut style.filter_noise_intensity),
    };
    *enabled = Some(on);
    *intensity = Some(value);
}

/// `FontPresetBackupModal`: save the chapter's preset lists or restore a saved backup.
/// `FontPresetBackupModal.tsx`.
pub fn preset_backup_window(editor: &mut Editor, ui: &mut egui::Ui, services: &mut Services) {
    let Some(mut dialog) = editor.preset_backup_dialog.take() else { return };
    if dialog.backups.is_none() {
        let listed =
            mtt_core::preset_backups::list(&services.paths).map_err(|_| "백업 목록을 불러오지 못했습니다.".to_owned());
        if let Ok(backups) = &listed
            && !dialog.selected.as_ref().is_some_and(|id| backups.iter().any(|b| &b.id == id))
        {
            dialog.selected = backups.first().map(|backup| backup.id.clone());
        }
        dialog.backups = Some(listed);
    }
    let mut open = true;
    let mut close = false;
    let title = if dialog.restore { "폰트 프리셋 복원" } else { "폰트 프리셋 백업" };
    egui::Window::new(title).open(&mut open).collapsible(false).default_width(420.0).show(ui.ctx(), |ui| {
        let mut save = false;
        if !dialog.restore {
            ui.label("백업 이름");
            let response = ui.add(egui::TextEdit::singleline(&mut dialog.name).desired_width(f32::INFINITY));
            if response.has_focus() {
                editor.text_editing = true;
            }
            if response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                save = true;
            }
        }
        let mut delete = None;
        match &dialog.backups {
            Some(Ok(backups)) if backups.is_empty() => {
                ui.label(RichText::new("저장된 백업이 없습니다.").weak());
            }
            Some(Ok(backups)) => {
                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    for backup in backups {
                        ui.horizontal(|ui| {
                            let created = chrono::DateTime::parse_from_rfc3339(&backup.created_at)
                                .map(|time| super::report::ko_kr_timestamp(&time.with_timezone(&chrono::Local)))
                                .unwrap_or_else(|_| backup.created_at.clone());
                            let selected = dialog.selected.as_deref() == Some(backup.id.as_str());
                            let label = RichText::new(format!(
                                "{}\n{created} · 폰트 프리셋 {}개 · 크기 프리셋 {}개",
                                backup.name, backup.font_preset_count, backup.font_size_preset_count
                            ));
                            if ui.selectable_label(selected, label).clicked() {
                                dialog.selected = Some(backup.id.clone());
                            }
                            if ui.small_button("삭제").on_hover_text("백업 삭제").clicked() {
                                delete = Some(backup.clone());
                            }
                        });
                    }
                });
            }
            Some(Err(error)) => {
                ui.colored_label(crate::theme::DANGER, error);
            }
            None => {}
        }
        if let Some(backup) = delete
            && crate::theme::confirm(
                "폰트 프리셋 백업 삭제",
                &format!("\"{}\" 백업을 삭제할까요?", backup.name),
                "삭제한 백업은 복원할 수 없습니다.",
            )
        {
            if let Err(error) = mtt_core::preset_backups::delete(&services.paths, &backup.id) {
                log::error!("backup delete failed: {error}");
                dialog.error = Some("폰트 프리셋 백업 삭제에 실패했습니다.".into());
            }
            dialog.backups = None;
        }
        if let Some(error) = &dialog.error {
            ui.colored_label(crate::theme::DANGER, error);
        }
        ui.horizontal(|ui| {
            if ui.button("취소").clicked() {
                close = true;
            }
            if dialog.restore {
                if ui.button("복원").clicked() {
                    let backup = dialog
                        .backups
                        .as_ref()
                        .and_then(|b| b.as_ref().ok())
                        .and_then(|backups| backups.iter().find(|b| Some(&b.id) == dialog.selected.as_ref()))
                        .cloned();
                    match backup {
                        Some(backup) => {
                            editor.restore_preset_backup(&backup);
                            editor.status = "폰트 프리셋 백업을 복원했습니다.".into();
                            close = true;
                        }
                        None => dialog.error = Some("복원할 백업을 선택하세요.".into()),
                    }
                }
            } else if ui.button("저장").clicked() {
                save = true;
            }
        });
        if save {
            let name = dialog.name.trim().to_owned();
            if name.is_empty() {
                dialog.error = Some("백업 이름을 입력하세요.".into());
            } else {
                // The TS app backs up the chapter's stored list as is.
                let presets = editor.chapter.font_presets.clone().unwrap_or_default();
                let sizes = editor.chapter.font_size_presets.clone().unwrap_or_default();
                match mtt_core::preset_backups::create(&services.paths, &name, &presets, &sizes) {
                    Ok(backup) => {
                        editor.status = "폰트 프리셋 백업을 저장했습니다.".into();
                        dialog.name = super::default_backup_name();
                        dialog.selected = Some(backup.id);
                        dialog.backups = None;
                        dialog.error = None;
                    }
                    Err(error) => {
                        log::error!("backup save failed: {error}");
                        dialog.error = Some("폰트 프리셋 백업 저장에 실패했습니다.".into());
                    }
                }
            }
        }
    });
    if open && !close {
        editor.preset_backup_dialog = Some(dialog);
    }
}

/// `FindReplaceModal.tsx`.
pub fn find_replace_window(editor: &mut Editor, ui: &mut egui::Ui) {
    if !editor.find.open {
        return;
    }
    let job_active = editor.translation_running();
    let mut open = true;
    let mut close = false;
    let mut replace_all = None;
    let mut replace_one = None;
    let mut focus = None;
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        close = true;
    }
    egui::Window::new("찾아바꾸기").open(&mut open).default_width(480.0).collapsible(false).show(ui.ctx(), |ui| {
        ui.label("찾을 키워드");
        let keyword_response = ui.add(egui::TextEdit::singleline(&mut editor.find.keyword).hint_text("검색어"));
        if editor.find.searched.is_none() && editor.find.keyword.is_empty() && !keyword_response.has_focus() {
            keyword_response.request_focus();
        }
        ui.label("바꿀 내용");
        let replace_response = ui.add(egui::TextEdit::singleline(&mut editor.find.replacement).hint_text("새 텍스트"));
        if keyword_response.has_focus() || replace_response.has_focus() {
            editor.text_editing = true;
        }
        let enter = (keyword_response.lost_focus() || replace_response.lost_focus())
            && ui.input(|i| i.key_pressed(egui::Key::Enter));
        let can_find = !editor.find.keyword.is_empty();
        let searched = editor.find.searched.clone();
        let matches =
            searched.as_deref().map(|k| collect_matches(editor, k, &editor.find.replacement)).unwrap_or_default();
        let changeable = matches.iter().filter(|m| m.before != m.after).count();
        ui.horizontal(|ui| {
            if (ui.add_enabled(can_find, egui::Button::new("찾기")).clicked() || enter) && can_find {
                editor.find.searched = Some(editor.find.keyword.clone());
            }
            let can_replace = searched.is_some() && changeable > 0 && !job_active;
            if ui.add_enabled(can_replace, egui::Button::new("모두 바꾸기")).clicked()
                && let Some(keyword) = &searched
            {
                replace_all = Some(keyword.clone());
            }
        });
        if job_active {
            ui.label(RichText::new("번역 작업이 진행 중이라 바꾸기를 사용할 수 없습니다.").weak().small());
        }
        let Some(keyword) = searched else { return };
        ui.separator();
        let total: usize = matches.iter().map(|m| m.occurrences).sum();
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{}개 문장", matches.len())).strong());
            ui.label(format!("{total}개 일치"));
        });
        if matches.is_empty() {
            ui.label(RichText::new("일치하는 문장이 없습니다.").weak());
            return;
        }
        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
            for found in &matches {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&found.page_name).strong());
                        ui.label(format!("블록 {} · {}회", found.block_index + 1, found.occurrences));
                    });
                    ui.columns(2, |columns| {
                        columns[0].label(RichText::new("수정 전").small().weak());
                        columns[0].label(highlighted(&columns[0], &found.before, &keyword));
                        columns[1].label(RichText::new("수정 후").small().weak());
                        let replacement = editor.find.replacement.clone();
                        columns[1].label(highlighted(&columns[1], &found.after, &replacement));
                    });
                    ui.horizontal(|ui| {
                        if ui.button("보기").clicked() {
                            focus = Some((found.page_index, found.block_id.clone()));
                        }
                        let disabled = job_active || found.before == found.after;
                        if ui.add_enabled(!disabled, egui::Button::new("바꾸기")).clicked() {
                            replace_one = Some((found.page_index, found.block_id.clone(), keyword.clone()));
                        }
                    });
                });
            }
        });
    });
    if let Some((index, block_id)) = focus {
        focus_match(editor, index, &block_id);
    }
    if let Some(keyword) = replace_all {
        replace_all_matches(editor, &keyword);
    }
    if let Some((index, block_id, keyword)) = replace_one {
        replace_single_match(editor, index, &block_id, &keyword);
    }
    editor.find.open = open && !close;
}

/// `focusFindReplaceMatch`.
fn focus_match(editor: &mut Editor, page_index: usize, block_id: &str) {
    editor.set_layer(Layer::Overlay);
    editor.go_to_page(page_index);
    editor.selection = vec![block_id.to_owned()];
}

/// `replaceAllMatches`: every page in one undo step.
fn replace_all_matches(editor: &mut Editor, keyword: &str) {
    let replacement = editor.find.replacement.clone();
    let changed = collect_matches(editor, keyword, &replacement).iter().filter(|m| m.before != m.after).count();
    if changed == 0 {
        editor.status = "바꿀 문장이 없습니다.".into();
        return;
    }
    editor.replace_in_all_pages(keyword, &replacement);
    editor.status = format!("{changed}개 문장을 바꿨습니다.");
}

/// `replaceSingleMatch`.
fn replace_single_match(editor: &mut Editor, page_index: usize, block_id: &str, keyword: &str) {
    let replacement = editor.find.replacement.clone();
    let Some(block) =
        editor.chapter.pages.get(page_index).and_then(|page| page.blocks.iter().find(|b| b.id == block_id))
    else {
        editor.status = "바꿀 문장을 찾을 수 없습니다.".into();
        return;
    };
    let next = block.translated_text.replace(keyword, &replacement);
    if next == block.translated_text {
        editor.status = "변경할 내용이 없습니다.".into();
        return;
    }
    focus_match(editor, page_index, block_id);
    editor.edit_page(|page| {
        if let Some(block) = page.blocks.iter_mut().find(|b| b.id == block_id) {
            block.translated_text = next.clone();
        }
    });
    editor.status = "1개 문장을 바꿨습니다.".into();
}
