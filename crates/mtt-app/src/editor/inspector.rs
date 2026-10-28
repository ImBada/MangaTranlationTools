//! The inspector on the right: one panel whose content follows the mode.
//!
//! - 번역: the page's block outline on top; below it the selection — the sentence first,
//!   then preset, placement and style — or the preset tab for editing presets themselves.
//! - 인페인트: run buttons, brush options, range and layer actions, PSD.
//! - 검수 · 출력: page check-off, chapter progress, export and the result report.

use eframe::egui::{self, RichText};
use mtt_core::model::{
    BlockType, InpaintEngine, InpaintStatus, PageAnalysisStatus, RenderTextDirection, TranslationBlock,
};

use super::result_tools::ResultTool;
use super::widgets::{self, Card, CardDrag, Host, Widget};
use super::{Editor, InpaintScope, InspectorTab, Layer, LayerKind, Mode, Tool, block_list, engine_label, panels};
use crate::app::AppEvent;
use crate::services::Services;
use crate::theme;
use crate::worker::Worker;

pub fn inspector(editor: &mut Editor, ui: &mut egui::Ui, services: &mut Services, worker: &Worker<AppEvent>) {
    egui::Panel::right("editor-inspector")
        .resizable(true)
        .default_size(340.0)
        .size_range(300.0..=460.0)
        .frame(theme::side_frame())
        .show(ui, |ui| match editor.mode() {
            Mode::Translate => translate(editor, ui, services, worker),
            Mode::Inpaint => inpaint(editor, ui, services, worker),
            Mode::Review => review(editor, ui, services, worker),
        });
}

/// Notice for a page the running translation will overwrite; returns whether it is locked.
fn locked_notice(editor: &Editor, ui: &mut egui::Ui) -> bool {
    let locked = editor.page_edit_locked();
    if locked {
        egui::Frame::new().fill(theme::WARNING.gamma_multiply(0.15)).corner_radius(6).inner_margin(8).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(
                RichText::new("AI 번역이 이 페이지를 처리하는 중이라 끝날 때까지 편집할 수 없습니다.")
                    .small()
                    .color(theme::WARNING),
            );
        });
        ui.add_space(6.0);
    }
    locked
}

// ---- 번역 ---------------------------------------------------------------------------------

fn translate(editor: &mut Editor, ui: &mut egui::Ui, services: &mut Services, worker: &Worker<AppEvent>) {
    if !editor.selection.is_empty() {
        editor.inspector_tab = InspectorTab::Blocks;
    }
    if let Some(tab) = theme::segmented(
        ui,
        editor.inspector_tab,
        &[
            (InspectorTab::Blocks, "블록", "이 페이지의 텍스트 블록"),
            (InspectorTab::Presets, "프리셋 편집", "화 전체에서 쓰는 글꼴 프리셋 편집"),
        ],
    ) {
        if tab == InspectorTab::Presets {
            if editor.inline_edit.is_some() {
                editor.commit_inline_edit();
            }
            editor.selection.clear();
        }
        editor.inspector_tab = tab;
    }
    ui.add_space(8.0);
    let locked = locked_notice(editor, ui);
    match editor.inspector_tab {
        InspectorTab::Blocks => {
            if !editor.widgets.floating(Widget::Blocks) {
                block_list::blocks_widget(editor, ui, Host::Panel);
            }
            egui::ScrollArea::vertical().id_salt("block-inspector").auto_shrink(false).show(ui, |ui| {
                ui.add_enabled_ui(!locked, |ui| selection(editor, ui, services, worker));
            });
        }
        InspectorTab::Presets => {
            egui::ScrollArea::vertical().id_salt("preset-inspector").auto_shrink(false).show(ui, |ui| {
                ui.add_enabled_ui(!locked, |ui| presets_tab(editor, ui, services));
            });
        }
    }
}

fn selection(editor: &mut Editor, ui: &mut egui::Ui, services: &mut Services, worker: &Worker<AppEvent>) {
    let Some(page) = editor.page() else {
        theme::hint(ui, "페이지가 없습니다.");
        return;
    };
    let group = mtt_core::groups::selected_group(page, &editor.selection);
    let blocks = editor.selected_blocks();
    if let Some(group) = group {
        theme::card(ui, &format!("그룹 · {}개 블록", group.block_ids.len()), |ui| {
            selection_actions(editor, ui, worker, blocks.len());
            ui.add_space(4.0);
            panels::group_settings(editor, ui, &group);
        });
        return;
    }
    let (single, first) = match blocks.as_slice() {
        [] => {
            if editor.widgets.floating(Widget::Page) {
                theme::hint(ui, PICK_A_BLOCK);
            } else {
                page_widget(editor, ui, services, worker, Host::Panel);
            }
            return;
        }
        [block] => (true, block),
        [first, ..] => (false, first),
    };
    // Like the TS block panel: text and placement wait for a running inpaint.
    let busy = editor.inpaint_busy();
    if busy && single {
        theme::hint(ui, "인페인트가 끝날 때까지 문장과 배치는 잠깁니다.");
    }
    // The cards in the user's order (each one's `⠿` grip drags it), skipping widgets that
    // float as windows and placement with several blocks selected.
    let mut drawn = Vec::new();
    for card in editor.widgets.cards().to_vec() {
        let top = ui.cursor().min.y;
        match card {
            Card::Block if !editor.widgets.floating(Widget::Block) => block_widget(editor, ui, worker, Host::Panel),
            Card::Presets if !editor.widgets.floating(Widget::Presets) => {
                presets_widget(editor, ui, services, Host::Panel)
            }
            Card::Placement if single => {
                ui.add_enabled_ui(!busy, |ui| placement_card(editor, ui, first));
            }
            Card::Style => style_card(editor, ui, first, services),
            _ => continue,
        }
        if let Some(grip) = ui.ctx().read_response(card.grip_id()) {
            crate::dnd::drag_source(ui, grip, CardDrag(card), card.label());
        }
        drawn.push((card, egui::Rect::from_x_y_ranges(ui.max_rect().x_range(), top..=ui.cursor().min.y)));
    }
    reorder_cards(editor, ui, &drawn);
}

/// Drop target for a dragged card: a line where it would land, moved there on release.
/// Holding the card near the top or bottom of the panel scrolls it.
fn reorder_cards(editor: &mut Editor, ui: &egui::Ui, cards: &[(Card, egui::Rect)]) {
    let Some(dragged) = egui::DragAndDrop::payload::<CardDrag>(ui.ctx()) else { return };
    let Some(pointer) = ui.input(|i| i.pointer.interact_pos()) else { return };
    let visible = ui.clip_rect();
    if !visible.x_range().contains(pointer.x) {
        return;
    }
    let edge = 36.0;
    let scroll = if pointer.y < visible.top() + edge {
        12.0
    } else if pointer.y > visible.bottom() - edge {
        -12.0
    } else {
        0.0
    };
    if scroll != 0.0 {
        ui.scroll_with_delta(egui::vec2(0.0, scroll));
        ui.ctx().request_repaint();
    }
    // Over a card: before or after it by which half; past the ends: first or last.
    let (Some(first), Some(last)) = (cards.first(), cards.last()) else { return };
    let (target, rect, after) = if pointer.y < first.1.top() {
        (first.0, first.1, false)
    } else if pointer.y > last.1.bottom() {
        (last.0, last.1, true)
    } else {
        let Some(&(target, rect)) = cards.iter().find(|(_, rect)| rect.y_range().contains(pointer.y)) else { return };
        (target, rect, pointer.y > rect.center().y)
    };
    // Each card is followed by 8pt of space; the line sits in that gap.
    let y = if after { rect.bottom() - 4.0 } else { rect.top() - 4.0 };
    ui.painter().hline(rect.x_range(), y, egui::Stroke::new(2.0, theme::ACCENT));
    if ui.input(|i| i.pointer.any_released()) {
        editor.widgets.move_card(dragged.0, target, after);
    }
}

const PICK_A_BLOCK: &str =
    "블록을 누르면 문장과 스타일을 고칠 수 있습니다. Tab / Shift+Tab으로 블록을 차례로 넘깁니다.";

/// "이 페이지" (`Widget::Page`): the page's state and what can be done to it in this mode.
/// Docked it shows in 번역 with nothing selected and at the top of 검수 · 출력.
pub(super) fn page_widget(
    editor: &mut Editor,
    ui: &mut egui::Ui,
    services: &mut Services,
    worker: &Worker<AppEvent>,
    host: Host,
) {
    // The few fields shown, not a copy of the page and its blocks every frame.
    let Some((name, analysis_status, inpaint_status, block_count, completed)) = editor.page().map(|page| {
        (page.name.clone(), page.analysis_status, page.inpaint_status, page.blocks.len(), page.is_completed())
    }) else {
        return;
    };
    let mode = editor.mode();
    let busy = editor.inpaint_busy();
    let locked = editor.page_edit_locked();
    let export_busy = editor.translation_running() || editor.export_progress.is_some() || editor.page_export_running;
    let title = format!("이 페이지 · {}", name);
    let mut add_block = false;
    let mut inpaint = false;
    let mut toggle_completed = false;
    let mut export = false;
    let (_, toggle) = widgets::frame(
        ui,
        host,
        Widget::Page,
        &title,
        |_| {},
        |ui| {
            if host == Host::Panel && mode == Mode::Translate {
                theme::hint(ui, PICK_A_BLOCK);
                ui.add_space(4.0);
            }
            ui.horizontal_wrapped(|ui| {
                let (translate, color) = match analysis_status {
                    PageAnalysisStatus::Completed => ("번역됨", theme::SUCCESS),
                    PageAnalysisStatus::Running => ("번역 중", theme::ACCENT),
                    PageAnalysisStatus::Failed => ("번역 실패", theme::DANGER),
                    PageAnalysisStatus::Idle => ("번역 전", theme::IDLE),
                };
                theme::pill(ui, translate, color);
                let (inpainted, color) = match inpaint_status {
                    Some(InpaintStatus::Completed) => ("인페인트됨", theme::SUCCESS),
                    Some(InpaintStatus::Running) => ("인페인트 중", theme::ACCENT),
                    Some(InpaintStatus::Failed) => ("인페인트 실패", theme::DANGER),
                    _ => ("인페인트 전", theme::IDLE),
                };
                theme::pill(ui, inpainted, color);
                theme::pill(ui, &format!("블록 {}개", block_count), theme::IDLE);
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                if mode == Mode::Translate {
                    add_block = ui.add_enabled(!busy && !locked, egui::Button::new("블록 추가")).clicked();
                }
                if mode != Mode::Review {
                    inpaint = ui
                        .add_enabled(
                            !busy && !locked && block_count > 0,
                            theme::icon_button(theme::icon::BANDAIDS, "이 페이지 인페인트"),
                        )
                        .on_hover_text("이 페이지 블록 안의 원문 글자를 모두 지웁니다")
                        .clicked();
                }
                let button = if completed {
                    egui::Button::new((RichText::new("검수 완료 해제"), theme::key_hint("₩", false)))
                } else if mode == Mode::Review {
                    egui::Button::new((
                        theme::icon(theme::icon::CHECK).color(egui::Color32::WHITE),
                        RichText::new("검수 완료로 표시").strong().color(egui::Color32::WHITE),
                        theme::key_hint("₩", true),
                    ))
                    .fill(theme::ACCENT_DIM)
                } else {
                    egui::Button::new((theme::icon(theme::icon::CHECK), "검수 완료", theme::key_hint("₩", false)))
                };
                toggle_completed = ui.add(button).on_hover_text("` 도 됨").clicked();
                export = ui
                    .add_enabled(!export_busy, theme::icon_button(theme::icon::EXPORT, "이 페이지 출력"))
                    .on_hover_text("PNG로 저장")
                    .clicked();
            });
            if completed && mode == Mode::Review {
                theme::hint(ui, "검수 완료한 페이지는 AI 번역과 모든 페이지 인페인트에서 건너뜁니다.");
            }
        },
    );
    if add_block {
        editor.create_empty_block();
    }
    if inpaint {
        editor.start_inpaint(InpaintScope::AllBlocks, worker);
    }
    if toggle_completed {
        editor.toggle_page_completed();
    }
    if export {
        editor.export_current_in_background(services, worker);
    }
    widgets::toggled(editor, Widget::Page, host, toggle);
}

/// "블록" (`Widget::Block`): the selected block's text and actions, or with several
/// selected what applies to all of them. Groups stay in the inspector (group settings).
pub(super) fn block_widget(editor: &mut Editor, ui: &mut egui::Ui, worker: &Worker<AppEvent>, host: Host) {
    let Some(page) = editor.page() else { return };
    let grouped = mtt_core::groups::selected_group(page, &editor.selection).is_some();
    let total = page.blocks.len();
    let blocks = editor.selected_blocks();
    let locked = editor.page_edit_locked();
    let toggle = match blocks.as_slice() {
        [block] if !grouped => {
            // Like the TS block panel: the text waits for a running inpaint.
            let enabled = !locked && !editor.inpaint_busy();
            sentence_card(editor, ui, block, total, worker, host, enabled)
        }
        [_, _, ..] if !grouped => {
            let title = format!("{}개 블록 선택", blocks.len());
            widgets::frame(
                ui,
                host,
                Widget::Block,
                &title,
                |_| {},
                |ui| {
                    ui.add_enabled_ui(!locked, |ui| {
                        selection_actions(editor, ui, worker, blocks.len());
                        theme::hint(ui, "프리셋과 스타일은 선택한 블록 모두에 적용됩니다.");
                    });
                },
            )
            .1
        }
        _ => {
            let note = if grouped {
                "그룹은 오른쪽 그룹 카드에서 고칩니다."
            } else {
                "블록을 고르면 여기서 문장을 고칩니다."
            };
            widgets::frame(ui, host, Widget::Block, "블록", |_| {}, |ui| theme::hint(ui, note)).1
        }
    };
    widgets::toggled(editor, Widget::Block, host, toggle);
}

/// Translation first: the text the reader sees, then the OCR source, with the block's
/// actions right under it. Returns the widget button press.
fn sentence_card(
    editor: &mut Editor,
    ui: &mut egui::Ui,
    block: &TranslationBlock,
    total: usize,
    worker: &Worker<AppEvent>,
    host: Host,
    enabled: bool,
) -> Option<egui::Pos2> {
    let index = editor.page().and_then(|page| page.blocks.iter().position(|b| b.id == block.id)).unwrap_or(0);
    let mut step = None;
    let title = format!("블록 {} / {}", index + 1, total);
    let (_, toggle) = widgets::frame(
        ui,
        host,
        Widget::Block,
        &title,
        |ui| {
            if ui
                .add_enabled(
                    index + 1 < total,
                    egui::Button::new((theme::icon(theme::icon::CARET_RIGHT), theme::key_hint("Tab", false))),
                )
                .on_hover_text("다음 블록 (Tab)")
                .clicked()
            {
                step = Some(1);
            }
            if ui
                .add_enabled(
                    index > 0,
                    egui::Button::new((theme::icon(theme::icon::CARET_LEFT), theme::key_hint("⇧Tab", false))),
                )
                .on_hover_text("이전 블록 (Shift+Tab)")
                .clicked()
            {
                step = Some(-1);
            }
        },
        |ui| {
            ui.add_enabled_ui(enabled, |ui| sentence_fields(editor, ui, block, worker));
        },
    );
    if let Some(step) = step {
        editor.select_adjacent_block(step);
    }
    toggle
}

fn sentence_fields(editor: &mut Editor, ui: &mut egui::Ui, block: &TranslationBlock, worker: &Worker<AppEvent>) {
    ui.label(RichText::new("번역").small().color(theme::TEXT_WEAK));
    let mut translated = block.translated_text.clone();
    let response = ui.add(egui::TextEdit::multiline(&mut translated).desired_rows(3).desired_width(f32::INFINITY));
    if editor.focus_text_requested {
        response.request_focus();
        editor.focus_text_requested = false;
    }
    if response.has_focus() {
        editor.text_editing = true;
    }
    if response.changed() {
        editor.edit_selected(|b| b.translated_text = translated.clone());
    }
    ui.add_space(2.0);
    ui.label(RichText::new("원문 (OCR)").small().color(theme::TEXT_WEAK));
    let mut source = block.source_text.clone();
    let response = ui.add(
        egui::TextEdit::multiline(&mut source)
            .desired_rows(2)
            .desired_width(f32::INFINITY)
            .text_color(theme::TEXT_WEAK),
    );
    if response.has_focus() {
        editor.text_editing = true;
    }
    if response.changed() {
        editor.edit_selected(|b| b.source_text = source.clone());
    }
    ui.add_space(4.0);
    selection_actions(editor, ui, worker, 1);
}

/// The preset palette for the selected blocks (`Widget::Presets`).
pub(super) fn presets_widget(editor: &mut Editor, ui: &mut egui::Ui, services: &mut Services, host: Host) {
    let grouped = editor.page().and_then(|page| mtt_core::groups::selected_group(page, &editor.selection)).is_some();
    // Groups keep their own settings, and the preset tab edits presets instead.
    let first = editor
        .page()
        .and_then(|page| page.blocks.iter().find(|block| editor.selection.contains(&block.id)))
        .filter(|_| !grouped && editor.inspector_tab == InspectorTab::Blocks)
        .cloned();
    let has_preset = first.as_ref().is_some_and(|block| block.font_preset_id.is_some());
    let locked = editor.page_edit_locked();
    let mut overrides = false;
    let (_, toggle) = widgets::frame(
        ui,
        host,
        Widget::Presets,
        "프리셋",
        |ui| {
            overrides = ui
                .add_enabled(has_preset, egui::Button::new(RichText::new("글자별 설정").small()))
                .on_hover_text("특정 글자만 다른 글꼴로 바꾸기")
                .on_disabled_hover_text("프리셋이 연결된 블록에서 쓸 수 있습니다")
                .clicked();
        },
        |ui| match &first {
            Some(block) => {
                ui.add_enabled_ui(!locked, |ui| {
                    panels::preset_panel(editor, ui, block, services, false);
                    theme::hint(ui, "누르면 적용, 적용된 프리셋을 다시 누르면 이름 변경, 우클릭은 즐겨찾기");
                });
            }
            None if grouped => theme::hint(ui, "그룹은 오른쪽 그룹 설정에서 꾸밉니다."),
            None => theme::hint(ui, "블록을 고르면 누르는 프리셋이 바로 적용됩니다."),
        },
    );
    if overrides {
        panels::open_character_overrides(editor);
    }
    widgets::toggled(editor, Widget::Presets, host, toggle);
}

/// Block type, writing direction, rotation, padding and background.
fn placement_card(editor: &mut Editor, ui: &mut egui::Ui, block: &TranslationBlock) {
    theme::card_full(
        ui,
        "배치",
        Some(Card::Placement.grip_id()),
        |_| {},
        |ui| {
            egui::Grid::new("block-placement").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label("종류");
                let mut block_type = block.block_type;
                egui::ComboBox::from_id_salt("block-type").selected_text(panels::block_type_label(block_type)).show_ui(
                    ui,
                    |ui| {
                        for value in BlockType::ALL {
                            ui.selectable_value(&mut block_type, *value, panels::block_type_label(*value));
                        }
                    },
                );
                if block_type != block.block_type {
                    editor.edit_selected(|b| b.block_type = block_type);
                }
                ui.end_row();

                ui.label("방향");
                let mut direction = block.render_direction;
                ui.horizontal(|ui| {
                    for (value, label) in [
                        (RenderTextDirection::Horizontal, "가로"),
                        (RenderTextDirection::Vertical, "세로"),
                        (RenderTextDirection::Hidden, "숨김"),
                    ] {
                        ui.selectable_value(&mut direction, value, label);
                    }
                });
                if direction != block.render_direction {
                    editor.edit_selected(|b| b.render_direction = direction);
                }
                ui.end_row();

                ui.label("회전");
                let rotation = (mtt_core::geometry::resolve_block_rotation_deg(block) * 10.0).round() / 10.0;
                let mut value = rotation;
                ui.horizontal(|ui| {
                    let number = ui.add(egui::DragValue::new(&mut value).range(-180.0..=180.0).speed(1.0).suffix("°"));
                    if number.changed() {
                        editor.edit_selected(|b| b.rotation_deg = Some(value));
                    }
                    if rotation != 0.0 && ui.button("초기화").clicked() {
                        editor.edit_selected(|b| b.rotation_deg = Some(0.0));
                    }
                });
                ui.end_row();

                ui.label("안쪽 여백");
                ui.horizontal(|ui| {
                    let auto = block.text_padding_px.is_none();
                    if ui.selectable_label(auto, "자동").clicked() && !auto {
                        editor.edit_selected(|b| b.text_padding_px = None);
                    }
                    let mut padding = block.text_padding_px.unwrap_or(0.0);
                    if ui.add(egui::DragValue::new(&mut padding).range(0.0..=80.0).suffix("px")).changed() {
                        editor.edit_selected(|b| b.text_padding_px = Some(padding));
                    }
                });
                ui.end_row();

                ui.label("블록 배경");
                if let Some(hex) = super::color::color_field(editor, ui, "block-background", &block.background_color) {
                    editor.edit_selected(|b| b.background_color = hex.clone());
                }
                ui.end_row();
            });
        },
    );
}

fn style_card(editor: &mut Editor, ui: &mut egui::Ui, first: &TranslationBlock, services: &mut Services) {
    theme::card_full(
        ui,
        "스타일",
        Some(Card::Style.grip_id()),
        |_| {},
        |ui| {
            if first.font_preset_id.is_some() {
                theme::hint(ui, "🔗 표시 값은 프리셋과 연결되어 같은 프리셋의 블록이 함께 바뀝니다.");
            }
            panels::font_section(editor, ui, first, services);
            panels::outline_section(editor, ui, first);
            panels::shadow_section(editor, ui, first);
            panels::background_section(editor, ui, first);
            panels::effect_section(editor, ui, first);
        },
    );
}

fn selection_actions(editor: &mut Editor, ui: &mut egui::Ui, worker: &Worker<AppEvent>, count: usize) {
    let busy = editor.inpaint_busy();
    ui.horizontal_wrapped(|ui| {
        let label = "인페인트";
        if ui
            .add_enabled(!busy, theme::icon_button(theme::icon::BANDAIDS, label))
            .on_hover_text(if count > 1 {
                "선택한 블록들 안의 원문 글자를 지웁니다"
            } else {
                "블록 안의 원문 글자를 지웁니다"
            })
            .clicked()
        {
            editor.start_inpaint(InpaintScope::SelectedBlocks, worker);
        }
        if ui
            .add_enabled(!busy, theme::icon_button_key(theme::icon::COPY, "복제", "⌘D"))
            .on_hover_text("블록 복제")
            .clicked()
        {
            editor.duplicate_selected();
        }
        let delete = egui::Button::new((
            theme::icon(theme::icon::TRASH).color(theme::DANGER),
            RichText::new("삭제").color(theme::DANGER),
            theme::key_hint("⌫", false),
        ));
        if ui.add_enabled(!busy, delete).on_hover_text("블록 삭제").clicked() {
            editor.delete_selected();
        }
    });
}

/// The preset tab: presets of the chapter, edited through a stand-in block.
fn presets_tab(editor: &mut Editor, ui: &mut egui::Ui, services: &mut Services) {
    let proxy = editor.preset_proxy_block();
    let editing_name = proxy.as_ref().and_then(|block| {
        let id = block.font_preset_id.as_deref()?;
        editor
            .chapter
            .font_presets
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .find(|p| p.id == id)
            .map(|p| super::presets::display_name(&p.name).to_owned())
    });
    theme::card(ui, "글꼴 프리셋", |ui| {
        theme::hint(ui, "프리셋을 누르면 아래에서 편집합니다. 바꾼 값은 이 프리셋이 연결된 모든 블록에 반영됩니다.");
        panels::preset_panel(editor, ui, proxy.as_ref().unwrap_or(&TranslationBlock::default()), services, true);
    });
    let Some(proxy) = proxy else { return };
    let title = format!("편집 중 · {}", editing_name.unwrap_or_default());
    let mut overrides = false;
    theme::card_with(
        ui,
        &title,
        |ui| overrides = ui.button(RichText::new("글자별 설정").small()).clicked(),
        |ui| {
            panels::font_section(editor, ui, &proxy, services);
            panels::outline_section(editor, ui, &proxy);
            panels::shadow_section(editor, ui, &proxy);
            panels::background_section(editor, ui, &proxy);
            panels::effect_section(editor, ui, &proxy);
        },
    );
    if overrides {
        panels::open_character_overrides(editor);
    }
}

// ---- 인페인트 -------------------------------------------------------------------------------

fn inpaint(editor: &mut Editor, ui: &mut egui::Ui, services: &mut Services, worker: &Worker<AppEvent>) {
    ui.label(RichText::new("인페인트").heading());
    theme::hint(ui, "원문 글자를 지우고, 남은 자국은 마스크나 결과 레이어에서 직접 다듬습니다.");
    ui.add_space(6.0);
    let locked = locked_notice(editor, ui);
    egui::ScrollArea::vertical().id_salt("inpaint-inspector").auto_shrink(false).show(ui, |ui| {
        ui.add_enabled_ui(!locked, |ui| {
            run_card(editor, ui, worker);
            brush_card(editor, ui);
            range_card(editor, ui, worker);
            layers_card(editor, ui);
            psd_card(editor, ui, services, worker);
        });
    });
}

fn run_card(editor: &mut Editor, ui: &mut egui::Ui, worker: &Worker<AppEvent>) {
    let busy = editor.inpaint_busy();
    let has_blocks = editor.page().is_some_and(|page| !page.blocks.is_empty());
    let has_mask = editor.current_mask_has_pixels();
    theme::card_with(
        ui,
        "실행",
        |ui| {
            if busy {
                ui.spinner();
            }
        },
        |ui| {
            if ui
                .add_enabled(!busy && has_blocks, theme::primary_icon(theme::icon::BANDAIDS, "이 페이지 인페인트"))
                .on_hover_text("블록 안의 원문 글자로 마스크를 새로 만들고 지웁니다 (기존 마스크·결과는 바뀜)")
                .clicked()
            {
                editor.start_inpaint(InpaintScope::AllBlocks, worker);
            }
            if ui
                .add_enabled(!busy && has_mask, egui::Button::new("마스크대로 다시 인페인트"))
                .on_hover_text("직접 칠한 마스크 영역 전체를 다시 인페인트합니다")
                .clicked()
            {
                editor.start_inpaint(InpaintScope::WholeMask, worker);
            }
            let chapter_ready = editor.chapter.pages.iter().any(|page| !page.blocks.is_empty() && !page.is_completed());
            if ui
                .add_enabled(
                    !busy && chapter_ready && !editor.translation_running(),
                    egui::Button::new("모든 페이지 인페인트"),
                )
                .on_hover_text("검수 완료하지 않은 모든 페이지를 차례로 인페인트합니다")
                .clicked()
            {
                editor.start_inpaint_all_pages();
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("엔진").small());
                let lama_available = editor.lama.is_some();
                egui::ComboBox::from_id_salt("inpaint-engine-combo")
                    .selected_text(engine_label(editor.inpaint_engine))
                    .show_ui(ui, |ui| {
                        ui.add_enabled_ui(lama_available, |ui| {
                            ui.selectable_value(&mut editor.inpaint_engine, InpaintEngine::Lama, "LaMa")
                                .on_disabled_hover_text("LaMa 런타임과 모델이 준비되지 않았습니다.");
                        });
                        ui.selectable_value(
                            &mut editor.inpaint_engine,
                            InpaintEngine::LocalFillFallback,
                            "로컬 채우기",
                        );
                    });
            });
            if editor.lama.is_none() {
                theme::hint(ui, "LaMa를 쓰려면 설정 › 인페인트에서 환경 준비와 모델 다운로드를 마치세요.");
            }
        },
    );
}

fn brush_card(editor: &mut Editor, ui: &mut egui::Ui) {
    let locked = editor.inpaint_busy();
    if editor.source_view() {
        return;
    }
    if editor.inpaint_layer == Layer::InpaintMask {
        if !matches!(editor.tool, Tool::MaskBrush | Tool::MaskEraser | Tool::MaskAutoEraser) {
            return;
        }
        theme::card(ui, "마스크 브러시", |ui| {
            ui.add_enabled_ui(!locked, |ui| {
                ui.horizontal(|ui| {
                    ui.label("크기");
                    ui.add(
                        egui::Slider::new(&mut editor.brush_size, super::MASK_BRUSH_MIN..=super::MASK_BRUSH_MAX)
                            .suffix("px"),
                    );
                });
            });
            theme::hint(ui, "[ / ] 로 크기 조절");
        });
        return;
    }
    if editor.tool != Tool::Result {
        return;
    }
    let tool = editor.result_tool;
    theme::card(ui, "결과 브러시", |ui| {
        ui.add_enabled_ui(!locked, |ui| {
            let current = super::color::to_hex(editor.result_brush.color);
            let mut picked = None;
            egui::Grid::new("result-brush").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                ui.label("색상");
                ui.add_enabled_ui(matches!(tool, ResultTool::Brush | ResultTool::SmartBrush), |ui| {
                    picked = super::color::color_field(editor, ui, "result-brush", &current);
                });
                ui.end_row();
                let brush = &mut editor.result_brush;
                ui.label("크기");
                ui.add(
                    egui::Slider::new(
                        &mut brush.size,
                        super::result_tools::RESULT_BRUSH_MIN..=super::result_tools::RESULT_BRUSH_MAX,
                    )
                    .suffix("px"),
                );
                ui.end_row();
                ui.label("가장자리");
                ui.add(
                    egui::Slider::new(&mut brush.hardness, 0.0..=1.0)
                        .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                );
                ui.end_row();
                ui.label("강도");
                ui.add_enabled(
                    !matches!(tool, ResultTool::Brush | ResultTool::SmartBrush | ResultTool::Eraser),
                    egui::Slider::new(&mut brush.strength, 0.05..=1.0)
                        .custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
                );
                ui.end_row();
            });
            if let Some(rgb) = picked.as_deref().and_then(super::color::parse_hex) {
                editor.result_brush.color = rgb;
            }
        });
    });
}

fn range_card(editor: &mut Editor, ui: &mut egui::Ui, worker: &Worker<AppEvent>) {
    if editor.range.is_none() && editor.tool != Tool::Range {
        return;
    }
    let locked = editor.inpaint_busy();
    let has_range = editor.range.is_some();
    let has_mask = editor.current_mask_has_pixels();
    let ctx = ui.ctx().clone();
    theme::card(ui, "선택 범위", |ui| {
        if !has_range {
            theme::hint(ui, "캔버스에서 드래그해 범위를 고르세요. Shift+드래그로 더합니다.");
            return;
        }
        ui.horizontal_wrapped(|ui| {
            let fill_label = if editor.inpaint_layer == Layer::InpaintMask {
                "마스크로 채우기"
            } else {
                "브러시 색으로 채우기"
            };
            if ui.add_enabled(editor.tool == Tool::Range && !locked, egui::Button::new(fill_label)).clicked() {
                if editor.inpaint_layer == Layer::InpaintMask {
                    editor.fill_range_on_mask(&ctx, true);
                } else {
                    editor.fill_range_on_result(&ctx, true);
                }
            }
            if ui
                .add_enabled(has_mask && !locked, egui::Button::new("범위만 다시 인페인트"))
                .on_hover_text("범위 안의 마스크만 다시 인페인트합니다")
                .clicked()
            {
                editor.start_inpaint(InpaintScope::Range, worker);
            }
            let mask_layer = editor.inpaint_layer == Layer::InpaintMask;
            if ui
                .add_enabled(editor.tool == Tool::Range && !locked, egui::Button::new("범위 안 비우기"))
                .on_hover_text(if mask_layer {
                    "범위 안 마스크 지우기 (Delete)"
                } else {
                    "범위 안 결과 지우기"
                })
                .clicked()
            {
                let changed = if mask_layer {
                    editor.fill_range_on_mask(&ctx, false)
                } else {
                    editor.fill_range_on_result(&ctx, false)
                };
                if !changed {
                    editor.status = "선택 범위에서 지울 레이어 내용이 없습니다.".into();
                }
            }
            if ui.button("선택 해제").clicked() {
                editor.range = None;
            }
        });
    });
}

fn layers_card(editor: &mut Editor, ui: &mut egui::Ui) {
    let locked = editor.inpaint_busy();
    let has_mask = editor.current_mask_has_pixels();
    let has_result = editor.current_result_exists();
    let ctx = ui.ctx().clone();
    theme::card(ui, "레이어 비우기", |ui| {
        ui.horizontal(|ui| {
            if ui.add_enabled(has_mask && !locked, egui::Button::new("마스크 비우기")).clicked() {
                editor.clear_layer(&ctx, LayerKind::Mask);
            }
            if ui.add_enabled(has_result && !locked, egui::Button::new("결과 비우기")).clicked() {
                editor.clear_layer(&ctx, LayerKind::Result);
            }
        });
    });
}

fn psd_card(editor: &mut Editor, ui: &mut egui::Ui, services: &mut Services, worker: &Worker<AppEvent>) {
    let busy = editor.inpaint_busy() || editor.page_export_running || editor.page().is_none();
    theme::card(ui, "PSD로 다듬기", |ui| {
        theme::hint(ui, "포토샵에서 결과를 손본 뒤 다시 불러옵니다.");
        ui.horizontal_wrapped(|ui| {
            if ui.add_enabled(!busy, egui::Button::new("내보내기")).on_hover_text("다운로드 폴더에 PSD 저장").clicked()
            {
                editor.export_psd_to_downloads(services, worker);
            }
            if ui.add_enabled(!busy, egui::Button::new("가져오기…")).clicked()
                && let Some(path) = rfd::FileDialog::new().add_filter("PSD", &["psd", "PSD"]).pick_file()
            {
                editor.import_psd_file(services, worker, &path);
            }
            let last_export = editor.last_exported_psd(services);
            let hint = last_export
                .as_ref()
                .and_then(|path| path.file_name())
                .map(|name| format!("마지막으로 내보낸 PSD 다시 불러오기: {}", name.to_string_lossy()))
                .unwrap_or_else(|| "아직 내보낸 PSD가 없습니다".into());
            if ui.add_enabled(!busy && last_export.is_some(), egui::Button::new("퀵로드")).on_hover_text(hint).clicked()
                && let Some(path) = last_export
            {
                editor.quick_load_psd(services, worker, &path);
            }
        });
        let last_import = editor.last_imported_psd(services);
        if let Some((path, label)) = &last_import {
            ui.horizontal(|ui| {
                theme::hint(ui, &format!("마지막 사용: {label}"));
                if ui.add_enabled(!busy, egui::Button::new(RichText::new("내려받기").small())).clicked() {
                    editor.download_last_imported_psd(path);
                }
            });
        }
        theme::hint(ui, "글자에 레이어 효과가 있으면 스마트 오브젝트로 바꾼 뒤 불러와야 보이는 그대로 들어옵니다.");
    });
}

// ---- 검수 · 출력 ----------------------------------------------------------------------------

fn review(editor: &mut Editor, ui: &mut egui::Ui, services: &mut Services, worker: &Worker<AppEvent>) {
    ui.label(RichText::new("검수 · 출력").heading());
    theme::hint(ui, "최종 결과를 넘겨 보며 끝난 페이지를 검수 완료로 표시하고, 이미지로 출력합니다.");
    ui.add_space(6.0);
    egui::ScrollArea::vertical().id_salt("review-inspector").auto_shrink(false).show(ui, |ui| {
        if !editor.widgets.floating(Widget::Page) {
            page_widget(editor, ui, services, worker, Host::Panel);
        }
        chapter_card(editor, ui, services, worker);
        report_card(editor, ui, services, worker);
    });
}

fn chapter_card(editor: &mut Editor, ui: &mut egui::Ui, services: &mut Services, worker: &Worker<AppEvent>) {
    let pages = &editor.chapter.pages;
    let total = pages.len();
    let translated = pages.iter().filter(|p| p.analysis_status == PageAnalysisStatus::Completed).count();
    let inpainted = pages.iter().filter(|p| p.inpaint_status == Some(InpaintStatus::Completed)).count();
    let completed = pages.iter().filter(|p| p.is_completed()).count();
    let job_active = editor.translation_running();
    let render_busy = editor.export_progress.is_some();
    let inpaint_disabled =
        pages.iter().all(|page| page.blocks.is_empty() || page.is_completed()) || job_active || editor.inpaint_busy();
    theme::card(ui, &format!("{} 전체", editor.chapter.title), |ui| {
        for (label, count) in [("번역", translated), ("인페인트", inpainted), ("검수 완료", completed)] {
            ui.horizontal(|ui| {
                // Left-aligned label column, then the bar (green once everything is done).
                let (rect, _) = ui.allocate_exact_size(egui::vec2(64.0, 16.0), egui::Sense::hover());
                ui.painter().text(
                    rect.left_center(),
                    egui::Align2::LEFT_CENTER,
                    label,
                    egui::FontId::proportional(12.0),
                    theme::TEXT,
                );
                let ratio = if total == 0 { 0.0 } else { count as f32 / total as f32 };
                let bar = (ui.available_width() - 64.0).clamp(60.0, 180.0);
                let color = if total > 0 && count == total { theme::SUCCESS } else { theme::ACCENT };
                ui.add(egui::ProgressBar::new(ratio).desired_width(bar).desired_height(6.0).fill(color));
                ui.label(RichText::new(format!("{count}/{total}")).small().color(theme::TEXT_WEAK));
            });
        }
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let label = match editor.export_progress {
                Some((current, total)) => format!("출력 중 {current}/{total}"),
                None => "전체 페이지 출력".into(),
            };
            let unreviewed = total - completed;
            if ui
                .add_enabled(total > 0 && !job_active && !render_busy, theme::primary_icon(theme::icon::EXPORT, label))
                .clicked()
                && (unreviewed == 0
                    || theme::confirm(
                        "전체 페이지 출력",
                        &format!("검수 완료하지 않은 페이지가 {unreviewed}개 있습니다."),
                        "그래도 모든 페이지를 출력할까요?",
                    ))
            {
                editor.status = match editor.export_all(services, worker) {
                    Ok(()) => "전체 페이지 출력 시작".into(),
                    Err(error) => format!("출력 실패: {error}"),
                };
            }
            let inpaint_label = if editor.inpaint_queue.is_some() {
                "모든 페이지 인페인트 중"
            } else {
                "모든 페이지 인페인트"
            };
            if ui
                .add_enabled(!inpaint_disabled, egui::Button::new(inpaint_label))
                .on_hover_text("완료하지 않은 모든 페이지의 블록 글자를 지웁니다")
                .clicked()
            {
                editor.start_inpaint_all_pages();
            }
            if ui.add(theme::icon_button(theme::icon::FOLDER_OPEN, "출력 폴더 열기")).clicked() {
                let folder = services.library.chapter_dir(&editor.chapter.work_id, &editor.chapter.id).join("renders");
                let target = if folder.is_dir() {
                    folder
                } else {
                    services.library.chapter_dir(&editor.chapter.work_id, &editor.chapter.id)
                };
                if let Err(error) = super::report::open_with_system(&target) {
                    editor.status = format!("폴더를 열 수 없습니다: {error}");
                }
            }
        });
    });
}

fn report_card(editor: &mut Editor, ui: &mut egui::Ui, services: &mut Services, worker: &Worker<AppEvent>) {
    let block_count: usize = editor.chapter.pages.iter().map(|page| page.blocks.len()).sum();
    let busy = editor.report_running();
    let disabled = block_count == 0 || editor.translation_running() || editor.export_progress.is_some() || busy;
    theme::card(ui, "결과 보고서", |ui| {
        theme::hint(ui, "블록별 원문 범위, 최종 결과, 프리셋과 설정을 한눈에 비교하는 보고서를 브라우저로 엽니다.");
        ui.horizontal(|ui| {
            let label = if busy { "보고서 만드는 중" } else { "보고서 만들기" };
            if ui.add_enabled(!disabled, theme::icon_button(theme::icon::FILE_TEXT, label)).clicked() {
                editor.start_report(services, worker);
            }
            if ui.add_enabled(editor.last_report.is_some() && !busy, egui::Button::new("마지막 보고서 열기")).clicked()
            {
                editor.open_last_report();
            }
        });
        if let Some(progress) = &editor.report {
            let ratio = if progress.total > 0 { progress.current as f32 / progress.total as f32 } else { 0.0 };
            ui.add(
                egui::ProgressBar::new(ratio.clamp(0.0, 1.0))
                    .text(format!("{} {}/{}", progress.label, progress.current, progress.total)),
            );
        }
    });
}
