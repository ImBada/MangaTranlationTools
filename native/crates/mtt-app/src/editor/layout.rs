//! Editor frame laid out like the TS app (`AppLayout.tsx`): context bar on top, page list on
//! the left, the per-layer tool rail and the right rail (layers + block panel) on the right.

use eframe::egui::{self, Color32, RichText};
use mtt_core::model::{BlockType, InpaintEngine, PageAnalysisStatus, RenderTextDirection};

use super::panels;
use super::result_tools::{RESULT_BRUSH_MAX, RESULT_BRUSH_MIN, ResultTool};
use super::{Editor, EditorAction, InpaintScope, Layer, MASK_BRUSH_MAX, MASK_BRUSH_MIN, Tool, engine_label};
use crate::app::AppEvent;
use crate::library_view::ImportMode;
use crate::services::Services;
use crate::worker::Worker;

/// `window.mangaApi.confirm`: the TS app uses the system confirm dialog.
pub fn confirm(title: &str, message: &str, detail: &str) -> bool {
    rfd::MessageDialog::new()
        .set_title(title)
        .set_description(format!("{message}\n\n{detail}"))
        .set_buttons(rfd::MessageButtons::OkCancel)
        .show()
        == rfd::MessageDialogResult::Ok
}

// ---- context bar (`ContextBar.tsx`) ---------------------------------------------------

pub fn context_bar(
    editor: &mut Editor,
    ui: &mut egui::Ui,
    services: &mut Services,
    worker: &Worker<AppEvent>,
) -> Option<EditorAction> {
    let mut action = None;
    let job_active = editor.translation_running();
    egui::Panel::top("editor-context-bar").show(ui, |ui| {
        ui.horizontal(|ui| {
            library_popover(editor, ui, services, &mut action);
            if ui.small_button("닫기").on_hover_text("보관함 화면으로 돌아갑니다").clicked() {
                action = Some(EditorAction::Close);
            }
            ui.separator();
            ui.label(RichText::new("가져오기").weak().small());
            for (mode, label) in [
                (ImportMode::Images, "이미지"),
                (ImportMode::Folder, "폴더"),
                (ImportMode::Zip, "압축파일"),
                (ImportMode::ZipFolder, "일괄 번역"),
            ] {
                if ui.add_enabled(!job_active, egui::Button::new(label)).clicked() {
                    action = Some(EditorAction::Import(mode));
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("설정").clicked() {
                    action = Some(EditorAction::OpenSettings);
                }
                status_indicator(editor, ui);
                if job_active && ui.button(RichText::new("취소").color(Color32::LIGHT_RED)).clicked() {
                    editor.cancel_translation();
                }
                if let Some(job) = &editor.translation {
                    ui.add(egui::ProgressBar::new(job.progress()).desired_width(180.0).text(format!(
                        "{} {}/{}",
                        job.phase_label(),
                        job.done,
                        job.total
                    )))
                    .on_hover_text(format!("{}\n{}", job.endpoint, job.last_message));
                }
                let has_pages = editor.page_count() > 0;
                let render_busy = editor.export_progress.is_some();
                let all_label = match editor.export_progress {
                    Some((current, total)) => format!("전체 출력 {current}/{total}"),
                    None => "전체 페이지 출력".into(),
                };
                if ui.add_enabled(has_pages && !job_active && !render_busy, egui::Button::new(all_label)).clicked() {
                    editor.status = match editor.export_all(services, worker) {
                        Ok(()) => "전체 페이지 출력 시작".into(),
                        Err(error) => format!("출력 실패: {error}"),
                    };
                }
                if ui.add_enabled(has_pages && !job_active && !render_busy, egui::Button::new("페이지 출력")).clicked()
                {
                    editor.status = match editor.export_current(services) {
                        Ok(path) => format!("페이지 렌더 저장: {}", path.display()),
                        Err(error) => format!("출력 실패: {error}"),
                    };
                }
                let inpaint_disabled =
                    editor.chapter.pages.iter().all(|page| page.blocks.is_empty() || page.is_completed())
                        || job_active
                        || editor.inpaint_busy();
                let inpaint_label =
                    if editor.inpaint_busy() { "전체 인페인트 중" } else { "전체 인페인트" };
                if ui.add_enabled(!inpaint_disabled, egui::Button::new(inpaint_label)).clicked() {
                    editor.start_inpaint_all_pages();
                }
                use super::translation::RunMode;
                if ui.add_enabled(has_pages && !job_active, egui::Button::new("전체 번역 (AI)")).clicked() {
                    editor.start_translation(RunMode::All, services, worker);
                }
                let primary =
                    egui::Button::new(RichText::new("계속 번역 (AI)").strong()).fill(ui.visuals().selection.bg_fill);
                if ui.add_enabled(has_pages && !job_active, primary).clicked() {
                    editor.start_translation(RunMode::Pending, services, worker);
                }
                ui.separator();
                ui.label(RichText::new(format!("{}p", editor.page_count())).weak());
                ui.label(RichText::new(&editor.chapter.title).strong()).on_hover_text(&editor.chapter.title);
            });
        });
    });
    action
}

/// "보관함 {n}": the library tree as a popover; picking a chapter opens it in place.
fn library_popover(editor: &mut Editor, ui: &mut egui::Ui, services: &mut Services, action: &mut Option<EditorAction>) {
    let count = editor.library_chapter_count;
    let response = ui.menu_button(format!("보관함 {count}"), |ui| {
        if editor.library_cache.is_none() {
            let works = services.library.list().unwrap_or_default();
            editor.library_chapter_count = works.iter().map(|work| work.chapters.len()).sum();
            editor.library_cache = Some(works);
        }
        let job_active = editor.translation_running();
        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
            ui.set_min_width(280.0);
            for work in editor.library_cache.as_deref().unwrap_or(&[]) {
                ui.label(RichText::new(&work.work.title).strong());
                for chapter in &work.chapters {
                    let current = chapter.id == editor.chapter.id;
                    let label = format!(
                        "  {} · {}페이지 · {}",
                        chapter.title,
                        chapter.page_count,
                        crate::library_view::status_label(chapter.status)
                    );
                    if ui.add_enabled(!job_active || current, egui::Button::selectable(current, label)).clicked() {
                        *action = Some(EditorAction::OpenChapter(chapter.id.clone()));
                        ui.close();
                    }
                }
            }
        });
    });
    if response.inner.is_none() {
        // Re-read the library the next time the popover opens.
        editor.library_cache = None;
    }
}

/// `context-status-indicator`: job progress or the save state as a coloured dot.
fn status_indicator(editor: &Editor, ui: &mut egui::Ui) {
    let (color, label) = if editor.notifications.has_failures() {
        (Color32::from_rgb(239, 68, 68), editor.save_state_label().to_owned())
    } else if let Some(job) = &editor.translation {
        (Color32::from_rgb(59, 130, 246), job.phase_label().to_owned())
    } else if editor.notifications.save_flash_active() {
        (Color32::from_rgb(34, 197, 94), "저장 완료".to_owned())
    } else if editor.has_unsaved_changes() {
        (Color32::from_rgb(245, 158, 11), "저장되지 않은 변경 있음".to_owned())
    } else {
        (Color32::from_rgb(148, 163, 184), "최신 상태".to_owned())
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 5.0, color);
    response.on_hover_text(format!("상태: {label}"));
}

// ---- page list (`PageList.tsx`) -----------------------------------------------------

pub fn page_list(editor: &mut Editor, ui: &mut egui::Ui, services: &mut Services, worker: &Worker<AppEvent>) {
    egui::Panel::left("editor-pages").resizable(true).default_size(210.0).size_range(160.0..=340.0).show(ui, |ui| {
        let completed = editor.chapter.pages.iter().filter(|page| page.is_completed()).count();
        ui.horizontal(|ui| {
            ui.label(RichText::new("페이지").strong());
            ui.label(RichText::new(format!("{completed}/{}", editor.page_count())).weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let next = editor.chapter.pages.get(editor.current + 1).map(|page| page.name.clone());
                let hint = next
                    .as_ref()
                    .map_or("다음 페이지 없음 (F)".to_owned(), |name| format!("다음 페이지: {name} (F)"));
                if ui.add_enabled(next.is_some(), egui::Button::new("→ F")).on_hover_text(hint).clicked() {
                    editor.next_page();
                }
                let previous = editor
                    .current
                    .checked_sub(1)
                    .and_then(|index| editor.chapter.pages.get(index))
                    .map(|page| page.name.clone());
                let hint = previous
                    .as_ref()
                    .map_or("이전 페이지 없음 (D)".to_owned(), |name| format!("이전 페이지: {name} (D)"));
                if ui.add_enabled(previous.is_some(), egui::Button::new("← D")).on_hover_text(hint).clicked() {
                    editor.previous_page();
                }
            });
        });
        ui.separator();
        if editor.page_count() == 0 {
            ui.label(RichText::new("불러온 페이지가 없습니다.").weak());
            return;
        }
        let job_active = editor.translation_running();
        let mut go_to = None;
        let mut toggle = None;
        let mut retranslate = None;
        let mut remove = None;
        let mut reorder: Option<(String, String, mtt_core::library::DropPosition)> = None;
        egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
            for index in 0..editor.page_count() {
                let page = &editor.chapter.pages[index];
                let selected = index == editor.current;
                let row = ui.horizontal(|ui| {
                    let mut done = page.is_completed();
                    if ui.checkbox(&mut done, "").on_hover_text("작업 완료 (₩/`)").changed() {
                        toggle = Some(index);
                    }
                    let label = crate::dnd::drag_button(
                        ui,
                        egui::Button::selectable(selected, &page.name).truncate(),
                        page.id.clone(),
                        &page.name,
                    );
                    if label.clicked() {
                        go_to = Some(index);
                    }
                    if selected && editor.view.fit_requested {
                        label.scroll_to_me(None);
                    }
                    if !selected {
                        let (tone, text) = match page.analysis_status {
                            PageAnalysisStatus::Completed => (Color32::from_rgb(34, 197, 94), "번역 완료"),
                            PageAnalysisStatus::Running => (Color32::from_rgb(59, 130, 246), "번역 진행"),
                            PageAnalysisStatus::Failed => (Color32::from_rgb(239, 68, 68), "번역 실패"),
                            PageAnalysisStatus::Idle => (Color32::GRAY, "번역 대기"),
                        };
                        ui.label(RichText::new("A/가").small().color(tone))
                            .on_hover_text(format!("AI 번역 상태: {text}"));
                    } else {
                        if ui.add_enabled(!job_active, egui::Button::new("↻").small()).on_hover_text("재번역").clicked()
                        {
                            retranslate = Some(index);
                        }
                        if ui.add_enabled(!job_active, egui::Button::new("×").small()).on_hover_text("삭제").clicked()
                        {
                            remove = Some(page.id.clone());
                        }
                    }
                });
                if !job_active
                    && let Some(source) = row.response.dnd_release_payload::<String>()
                    && *source != page.id
                {
                    // `reorderByTarget(pageOrder, source, target)`: before the target row.
                    reorder = Some(((*source).clone(), page.id.clone(), mtt_core::library::DropPosition::Before));
                }
                if row.response.dnd_hover_payload::<String>().is_some() {
                    ui.painter().hline(
                        row.response.rect.x_range(),
                        row.response.rect.bottom(),
                        (2.0, Color32::LIGHT_BLUE),
                    );
                }
            }
        });
        if let Some(index) = toggle {
            editor.toggle_page_completed_at(index);
        }
        if let Some(index) = go_to {
            editor.go_to_page(index);
        }
        if let Some((source, target, position)) = reorder {
            editor.reorder_page(&source, &target, position);
        }
        if retranslate.is_some()
            && confirm(
                "페이지 재번역",
                "정말 재번역 하시겠습니까?",
                "기존 번역 결과와 수정 내용이 이 페이지에서 덮어써집니다.",
            )
        {
            editor.start_translation(super::translation::RunMode::CurrentPage, services, worker);
        }
        if let Some(page_id) = remove
            && confirm("페이지 삭제", "정말 삭제하시겠습니까?", "이 페이지와 해당 번역 결과가 보관함에서 삭제됩니다.")
        {
            editor.delete_page(&page_id, services);
        }
    });
}

// ---- right rail: layers + block panel (`AppRightRail.tsx`) -----------------------------

/// Below this window width the right rail and the tool rail share one column, like the TS
/// `@media (max-width: 1400px)` three-column layout.
const WIDE_LAYOUT_MIN_WIDTH: f32 = 1400.0;

/// Right side: two rails on wide windows, one stacked column otherwise.
pub fn right_side(editor: &mut Editor, ui: &mut egui::Ui, services: &mut Services, worker: &Worker<AppEvent>) {
    if ui.ctx().content_rect().width() >= WIDE_LAYOUT_MIN_WIDTH {
        egui::Panel::right("editor-right-rail").resizable(true).default_size(280.0).size_range(240.0..=360.0).show(
            ui,
            |ui| {
                egui::ScrollArea::vertical().id_salt("right-rail").auto_shrink(false).show(ui, |ui| {
                    layer_panel(editor, ui);
                    ui.separator();
                    block_panel(editor, ui, worker);
                });
            },
        );
        egui::Panel::right("editor-tool-rail").resizable(true).default_size(330.0).size_range(280.0..=400.0).show(
            ui,
            |ui| {
                tool_rail_header(editor, ui);
                egui::ScrollArea::vertical()
                    .id_salt("tool-rail")
                    .auto_shrink(false)
                    .show(ui, |ui| tool_rail_contents(editor, ui, services, worker));
            },
        );
    } else {
        egui::Panel::right("editor-side-column").resizable(true).default_size(330.0).size_range(280.0..=380.0).show(
            ui,
            |ui| {
                egui::ScrollArea::vertical().id_salt("side-column").auto_shrink(false).show(ui, |ui| {
                    layer_panel(editor, ui);
                    ui.separator();
                    block_panel(editor, ui, worker);
                    ui.separator();
                    tool_rail_header(editor, ui);
                    tool_rail_contents(editor, ui, services, worker);
                });
            },
        );
    }
}

/// `LayerPanel.tsx`.
fn layer_panel(editor: &mut Editor, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.label(RichText::new("레이어").strong());
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.checkbox(&mut editor.focus_mode, "FOCUS MODE");
        });
    });
    let active = editor.layer;
    // 1 최종 아웃풋 (view only).
    layer_row(ui, active == Layer::Output, "1 최종 아웃풋", None, None, false, |_| {})
        .then(|| editor.set_layer(Layer::Output));

    // 2 번역 블록, with the page-wide "배경 투명도 편집".
    let mut visible = editor.visibility.overlay;
    let mut opacity =
        if editor.overlay_opacity_edit { editor.page_block_opacity() as f32 } else { editor.opacity.overlay };
    let before = (visible, opacity);
    let mut edit_mode = editor.overlay_opacity_edit;
    if layer_row(ui, active == Layer::Overlay, "2 번역 블록", Some(&mut visible), Some(&mut opacity), false, |ui| {
        ui.checkbox(&mut edit_mode, "배경 투명도 편집");
    }) {
        editor.set_layer(Layer::Overlay);
    }
    if edit_mode != editor.overlay_opacity_edit {
        editor.overlay_opacity_edit = edit_mode;
        if edit_mode {
            editor.opacity.overlay = 1.0;
        }
    } else {
        editor.visibility.overlay = visible;
        if opacity != before.1 {
            if !editor.overlay_opacity_edit {
                editor.opacity.overlay = opacity;
            } else if !editor.page_edit_locked() {
                editor.set_page_block_opacity(f64::from(opacity));
            }
        }
    }

    // Inpaint group and its two children.
    let mut visible = editor.visibility.inpaint;
    let mut opacity = editor.opacity.inpaint;
    let group_active = matches!(active, Layer::Inpaint | Layer::InpaintResult | Layer::InpaintMask);
    if layer_row(ui, group_active, "인페인트 레이어", Some(&mut visible), Some(&mut opacity), false, |_| {}) {
        editor.set_layer(Layer::Inpaint);
    }
    if visible != editor.visibility.inpaint {
        editor.visibility.inpaint = visible;
        if !visible {
            editor.visibility.inpaint_result = false;
            editor.visibility.inpaint_mask = false;
        }
    }
    editor.opacity.inpaint = opacity;
    for (layer, label) in [(Layer::InpaintResult, "3 인페인트 결과"), (Layer::InpaintMask, "4 인페인트 마스크")]
    {
        let (child_visible, child_opacity) = match layer {
            Layer::InpaintResult => (editor.visibility.inpaint_result, editor.opacity.inpaint_result),
            _ => (editor.visibility.inpaint_mask, editor.opacity.inpaint_mask),
        };
        let mut visible = editor.visibility.inpaint && child_visible;
        let mut opacity = child_opacity;
        let shown = visible;
        if layer_row(ui, active == layer, label, Some(&mut visible), Some(&mut opacity), true, |_| {}) {
            editor.set_layer(layer);
        }
        if visible != shown {
            editor.visibility.inpaint |= visible;
            match layer {
                Layer::InpaintResult => editor.visibility.inpaint_result = visible,
                _ => editor.visibility.inpaint_mask = visible,
            }
        }
        match layer {
            Layer::InpaintResult => editor.opacity.inpaint_result = opacity,
            _ => editor.opacity.inpaint_mask = opacity,
        }
    }

    let mut visible = editor.visibility.image;
    let mut opacity = editor.opacity.image;
    if layer_row(ui, active == Layer::Image, "5 원본 이미지", Some(&mut visible), Some(&mut opacity), false, |_| {})
    {
        editor.set_layer(Layer::Image);
    }
    editor.visibility.image = visible;
    editor.opacity.image = opacity;
}

/// `LayerControl.tsx`: returns true when the row was clicked (select the layer).
fn layer_row(
    ui: &mut egui::Ui,
    active: bool,
    label: &str,
    visible: Option<&mut bool>,
    opacity: Option<&mut f32>,
    nested: bool,
    extra: impl FnOnce(&mut egui::Ui),
) -> bool {
    let fill = if active { ui.visuals().selection.bg_fill.gamma_multiply(0.35) } else { Color32::TRANSPARENT };
    let mut clicked = false;
    egui::Frame::new().fill(fill).corner_radius(4.0).inner_margin(egui::Margin::symmetric(6, 3)).show(ui, |ui| {
        if nested {
            ui.add_space(0.0);
        }
        ui.horizontal(|ui| {
            if nested {
                ui.add_space(14.0);
            }
            let view_only = visible.is_none();
            let shown = match visible {
                Some(visible) => {
                    ui.checkbox(visible, "");
                    *visible
                }
                None => true,
            };
            if ui.add(egui::Label::new(RichText::new(label).strong()).sense(egui::Sense::click())).clicked() {
                clicked = true;
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| match (&opacity, view_only) {
                (_, true) => {
                    ui.label(RichText::new("보기").small().weak());
                }
                (Some(opacity), false) => {
                    ui.label(RichText::new(format!("{}%", (**opacity * 100.0).round())).small().weak());
                }
                _ => {}
            });
            let _ = shown;
        });
        extra(ui);
        if let Some(opacity) = opacity {
            ui.horizontal(|ui| {
                if nested {
                    ui.add_space(14.0);
                }
                ui.add(egui::Slider::new(opacity, 0.0..=1.0).show_value(false));
            });
        }
    });
    clicked
}

/// `EditorPanel.tsx`.
fn block_panel(editor: &mut Editor, ui: &mut egui::Ui, worker: &Worker<AppEvent>) {
    let blocks = editor.selected_blocks();
    let disabled = editor.inpaint_busy() || editor.page().is_none() || editor.page_edit_locked();
    if blocks.len() > 1 {
        ui.label(RichText::new("블록").strong());
        ui.label(format!("{}개 블록 선택됨", blocks.len()));
        ui.horizontal(|ui| {
            if ui.add_enabled(!disabled, egui::Button::new("선택된 블록 인페인트 실행")).clicked() {
                editor.start_inpaint(InpaintScope::SelectedBlocks, worker);
            }
            if ui.add_enabled(!disabled, egui::Button::new("삭제")).clicked() {
                editor.delete_selected();
            }
        });
        return;
    }
    let Some(block) = blocks.into_iter().next() else {
        ui.label(RichText::new("블록").strong());
        ui.label(RichText::new("블록을 선택하면 문구와 배치 방향을 바로 조정할 수 있습니다.").weak());
        if ui.add_enabled(!disabled, egui::Button::new("빈 블록 생성")).clicked() {
            editor.create_empty_block();
        }
        let no_blocks = editor.page().is_none_or(|page| page.blocks.is_empty());
        if ui.add_enabled(!disabled && !no_blocks, egui::Button::new("전체 블록 인페인트")).clicked() {
            editor.start_inpaint(InpaintScope::AllBlocks, worker);
        }
        return;
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new("블록").strong());
        let preset_name = block.font_preset_id.as_deref().and_then(|id| {
            editor.chapter.font_presets.as_deref().unwrap_or(&[]).iter().find(|p| p.id == id).map(|p| p.name.clone())
        });
        if let Some(name) = preset_name {
            ui.label(RichText::new(name).small().background_color(ui.visuals().faint_bg_color));
        }
    });
    ui.add_enabled_ui(!disabled, |ui| {
        ui.label("종류");
        let mut block_type = block.block_type;
        egui::ComboBox::from_id_salt("block-type").selected_text(block_type.as_str()).show_ui(ui, |ui| {
            for value in BlockType::ALL {
                ui.selectable_value(&mut block_type, *value, value.as_str());
            }
        });
        if block_type != block.block_type {
            editor.edit_selected(|b| b.block_type = block_type);
        }
        ui.label("한국어");
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
        ui.label("OCR");
        let mut source = block.source_text.clone();
        let response = ui.add(egui::TextEdit::multiline(&mut source).desired_rows(2).desired_width(f32::INFINITY));
        if response.has_focus() {
            editor.text_editing = true;
        }
        if response.changed() {
            editor.edit_selected(|b| b.source_text = source.clone());
        }
        ui.label("방향");
        let mut direction = block.render_direction;
        egui::ComboBox::from_id_salt("block-direction").selected_text(direction.as_str()).show_ui(ui, |ui| {
            for value in [RenderTextDirection::Horizontal, RenderTextDirection::Vertical, RenderTextDirection::Hidden] {
                ui.selectable_value(&mut direction, value, value.as_str());
            }
        });
        if direction != block.render_direction {
            editor.edit_selected(|b| b.render_direction = direction);
        }
        ui.label("회전");
        let rotation = (mtt_core::geometry::resolve_block_rotation_deg(&block) * 10.0).round() / 10.0;
        let mut value = rotation;
        ui.horizontal(|ui| {
            let slider = ui.add(egui::Slider::new(&mut value, -180.0..=180.0).step_by(1.0).show_value(false));
            let number = ui.add(egui::DragValue::new(&mut value).range(-180.0..=180.0).speed(1.0).suffix(" deg"));
            if slider.changed() || number.changed() {
                editor.edit_selected(|b| b.rotation_deg = Some(value));
            }
            if ui.add_enabled(rotation != 0.0, egui::Button::new("초기화")).clicked() {
                editor.edit_selected(|b| b.rotation_deg = Some(0.0));
            }
        });
        ui.horizontal(|ui| {
            ui.label("패딩");
            let auto = block.text_padding_px.is_none();
            if ui.selectable_label(auto, "자동").clicked() && !auto {
                editor.edit_selected(|b| b.text_padding_px = None);
            }
            let mut padding = block.text_padding_px.unwrap_or(0.0);
            if ui.add(egui::DragValue::new(&mut padding).range(0.0..=80.0).suffix("px")).changed() {
                editor.edit_selected(|b| b.text_padding_px = Some(padding));
            }
            ui.label("배경색");
            if let Some(hex) = super::color::color_field(editor, ui, "block-background", &block.background_color) {
                editor.edit_selected(|b| b.background_color = hex.clone());
            }
        });
        ui.horizontal(|ui| {
            if ui.button(RichText::new("인페인트 실행").strong()).clicked() {
                editor.start_inpaint(InpaintScope::SelectedBlocks, worker);
            }
            if ui.button("복제").clicked() {
                editor.duplicate_selected();
            }
            if ui.button(RichText::new("삭제").color(Color32::LIGHT_RED)).clicked() {
                editor.delete_selected();
            }
        });
    });
}

// ---- tool rail (`LayerToolPanel.tsx`) ----------------------------------------------------

/// Tool rail title (`resolveLayerToolTitle`) with the "고급 설정" gear.
fn tool_rail_header(editor: &mut Editor, ui: &mut egui::Ui) {
    let group = editor.page().and_then(|page| mtt_core::groups::selected_group(page, &editor.selection));
    let title = match editor.layer {
        Layer::Overlay if group.is_some() => "그룹 설정",
        Layer::Overlay => "폰트 설정",
        Layer::InpaintMask => "마스크 도구",
        Layer::InpaintResult => "결과 레이어 도구",
        Layer::Output => "최종 아웃풋 도구",
        _ => "도구",
    };
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).strong());
        if editor.layer == Layer::Overlay && group.is_none() {
            // "고급 설정" needs a preset behind the font controls.
            let has_preset = editor
                .selected_blocks()
                .first()
                .map(|block| block.font_preset_id.is_some())
                .unwrap_or_else(|| editor.preset_proxy_block().is_some());
            let hint = if has_preset { "고급 설정" } else { "폰트 프리셋 선택 후 설정 가능" };
            if ui
                .add_enabled(has_preset, egui::Button::new("⚙").small())
                .on_hover_text(hint)
                .on_disabled_hover_text(hint)
                .clicked()
            {
                super::panels::open_character_overrides(editor);
            }
        }
    });
    ui.separator();
}

/// `LayerToolPanel.tsx` body for the active layer.
fn tool_rail_contents(editor: &mut Editor, ui: &mut egui::Ui, services: &mut Services, worker: &Worker<AppEvent>) {
    ui.scope(|ui| {
        if editor.page_edit_locked() && editor.layer != Layer::Output {
            ui.label(RichText::new("번역 중인 페이지는 끝날 때까지 편집할 수 없습니다.").weak().small());
            ui.disable();
        }
        match editor.layer {
            Layer::Output => output_tool_section(editor, ui, services, worker),
            Layer::Overlay => panels::font_tool_section(editor, ui, services),
            Layer::InpaintMask => {
                mask_tool_section(editor, ui, worker);
                engine_line(editor, ui);
            }
            Layer::InpaintResult => {
                result_tool_section(editor, ui, worker);
                engine_line(editor, ui);
            }
            Layer::Inpaint => psd_tool_section(editor, ui, services),
            Layer::Image => {
                ui.label(RichText::new("원본 이미지 레이어에는 사용할 도구가 없습니다.").weak());
            }
        }
    });
}

/// `OutputToolSection.tsx`.
fn output_tool_section(editor: &mut Editor, ui: &mut egui::Ui, services: &mut Services, worker: &Worker<AppEvent>) {
    let block_count: usize = editor.chapter.pages.iter().map(|page| page.blocks.len()).sum();
    let busy = editor.report_running();
    let disabled = block_count == 0 || editor.translation_running() || editor.export_progress.is_some() || busy;
    ui.horizontal(|ui| {
        let label = if busy { "보고서 생성 중" } else { "결과 보고서 생성" };
        if ui.add_enabled(!disabled, egui::Button::new(label)).clicked() {
            editor.start_report(services, worker);
        }
        if ui.add_enabled(editor.last_report.is_some() && !busy, egui::Button::new("마지막 보고서 확인")).clicked()
        {
            editor.open_last_report();
        }
    });
    if let Some(progress) = &editor.report {
        let ratio = if progress.total > 0 { progress.current as f32 / progress.total as f32 } else { 0.0 };
        ui.horizontal(|ui| {
            ui.label(RichText::new(&progress.label).small());
            ui.label(RichText::new(format!("{} / {}", progress.current, progress.total)).small().strong());
        });
        ui.add(egui::ProgressBar::new(ratio.clamp(0.0, 1.0)));
    }
    ui.label(
        RichText::new(
            "현재 화의 텍스트 블록별 원본 범위, 최종 아웃풋, 폰트 프리셋과 설정을 정렬 가능한 보고서 페이지로 엽니다.",
        )
        .weak()
        .small(),
    );
}

/// `InpaintMaskToolSection.tsx`.
fn mask_tool_section(editor: &mut Editor, ui: &mut egui::Ui, worker: &Worker<AppEvent>) {
    let locked = editor.inpaint_busy();
    ui.add_enabled_ui(!locked, |ui| {
        ui.horizontal(|ui| {
            for (tool, label, key) in [
                (Tool::MaskBrush, "브러시", "B"),
                (Tool::MaskEraser, "지우개", "E"),
                (Tool::MaskAutoEraser, "자동 지우개", "Alt+E"),
            ] {
                let active = editor.layer == Layer::InpaintMask && editor.tool == tool;
                if ui.selectable_label(active, format!("{label} {key}")).clicked() {
                    editor.select_shared_inpaint_tool(tool);
                }
            }
        });
        ui.horizontal(|ui| {
            ui.label("브러시 크기");
            ui.add(egui::Slider::new(&mut editor.brush_size, MASK_BRUSH_MIN..=MASK_BRUSH_MAX).suffix("px"));
        });
    });
    let has_range = editor.range.is_some();
    let has_mask = editor.current_mask_has_pixels();
    let ctx = ui.ctx().clone();
    egui::Grid::new("mask-actions").num_columns(2).show(ui, |ui| {
        if ui.add_enabled(has_range, egui::Button::new("선택 해제")).clicked() {
            editor.range = None;
        }
        if ui
            .add_enabled(has_range && editor.tool == Tool::Range && !locked, egui::Button::new("선택 범위 채우기"))
            .clicked()
        {
            editor.fill_range_on_mask(&ctx, true);
        }
        ui.end_row();
        if ui.add_enabled(has_mask && !locked, egui::Button::new("인페인트 마스크 비우기")).clicked() {
            editor.clear_layer(&ctx, super::LayerKind::Mask);
        }
        if ui.add_enabled(has_range && has_mask && !locked, egui::Button::new("선택 범위만 다시 인페인트")).clicked()
        {
            editor.start_inpaint(InpaintScope::Range, worker);
        }
        ui.end_row();
    });
}

/// `InpaintResultToolSection.tsx`.
fn result_tool_section(editor: &mut Editor, ui: &mut egui::Ui, worker: &Worker<AppEvent>) {
    let locked = editor.inpaint_busy();
    ui.add_enabled_ui(!locked, |ui| {
        ui.horizontal_wrapped(|ui| {
            for (tool, label) in [
                (ResultTool::Brush, "브러시 B"),
                (ResultTool::SmartBrush, "스마트 Alt+B"),
                (ResultTool::Eraser, "지우개 E"),
                (ResultTool::Blur, "흐림"),
                (ResultTool::Sharpen, "선명"),
                (ResultTool::Smudge, "뭉개기"),
            ] {
                let active = editor.tool == Tool::Result && editor.result_tool == tool;
                if ui.selectable_label(active, label).clicked() {
                    match tool {
                        ResultTool::Brush => editor.select_shared_inpaint_tool(Tool::MaskBrush),
                        ResultTool::Eraser => editor.select_shared_inpaint_tool(Tool::MaskEraser),
                        other => editor.select_result_edit_tool(other),
                    }
                }
            }
        });
        let tool = editor.result_tool;
        let current = super::color::to_hex(editor.result_brush.color);
        let mut picked = None;
        egui::Grid::new("result-brush-color").num_columns(2).show(ui, |ui| {
            ui.label("색상");
            ui.add_enabled_ui(matches!(tool, ResultTool::Brush | ResultTool::SmartBrush), |ui| {
                picked = super::color::color_field(editor, ui, "result-brush", &current);
            });
            ui.end_row();
        });
        if let Some(rgb) = picked.as_deref().and_then(super::color::parse_hex) {
            editor.result_brush.color = rgb;
        }
        let brush = &mut editor.result_brush;
        egui::Grid::new("result-brush").num_columns(2).show(ui, |ui| {
            ui.label("크기");
            ui.add(egui::Slider::new(&mut brush.size, RESULT_BRUSH_MIN..=RESULT_BRUSH_MAX).suffix("px"));
            ui.end_row();
            ui.label("가장자리");
            ui.add(
                egui::Slider::new(&mut brush.hardness, 0.0..=1.0).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)),
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
    });
    let has_range = editor.range.is_some();
    let has_mask = editor.current_mask_has_pixels();
    let has_result = editor.current_result_exists();
    let ctx = ui.ctx().clone();
    egui::Grid::new("result-actions").num_columns(2).show(ui, |ui| {
        if ui.add_enabled(has_range, egui::Button::new("선택 해제")).clicked() {
            editor.range = None;
        }
        if ui
            .add_enabled(has_range && editor.tool == Tool::Range && !locked, egui::Button::new("선택 범위 채우기"))
            .clicked()
        {
            editor.fill_range_on_result(&ctx, true);
        }
        ui.end_row();
        if ui.add_enabled(has_result && !locked, egui::Button::new("인페인트 결과 비우기")).clicked() {
            editor.clear_layer(&ctx, super::LayerKind::Result);
        }
        if ui.add_enabled(has_mask && !locked, egui::Button::new("마스크 유지하고 인페인트 다시하기")).clicked()
        {
            editor.start_inpaint(InpaintScope::WholeMask, worker);
        }
        ui.end_row();
        if ui.add_enabled(has_range && has_mask && !locked, egui::Button::new("선택 범위만 다시 인페인트")).clicked()
        {
            editor.start_inpaint(InpaintScope::Range, worker);
        }
        ui.end_row();
    });
}

/// Engine line under the inpaint tools (LaMa when prepared, else local fill).
fn engine_line(editor: &mut Editor, ui: &mut egui::Ui) {
    ui.separator();
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
                ui.selectable_value(&mut editor.inpaint_engine, InpaintEngine::LocalFillFallback, "로컬 채우기");
            });
        if editor.inpaint_busy() {
            ui.spinner();
        }
    });
    if editor.lama.is_none() {
        ui.label(
            RichText::new("LaMa: 설정에서 환경 준비와 모델 다운로드를 마치면 자동으로 사용합니다.").weak().small(),
        );
    }
}

/// `InpaintPsdToolSection.tsx`.
fn psd_tool_section(editor: &mut Editor, ui: &mut egui::Ui, services: &mut Services) {
    let busy = editor.inpaint_busy() || editor.page().is_none();
    let ctx = ui.ctx().clone();
    ui.horizontal(|ui| {
        if ui.add_enabled(!busy, egui::Button::new("PSD 내보내기")).clicked() {
            editor.export_psd_to_downloads(services);
        }
        if ui.add_enabled(!busy, egui::Button::new("PSD 가져오기")).clicked()
            && let Some(path) = rfd::FileDialog::new().add_filter("PSD", &["psd", "PSD"]).pick_file()
        {
            editor.import_psd_file(&ctx, services, &path);
        }
        let last_export = editor.last_exported_psd(services);
        let response = ui.add_enabled(!busy && last_export.is_some(), egui::Button::new("퀵로드"));
        let response = match &last_export {
            Some(path) => response.on_hover_text(path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()),
            None => response,
        };
        if response.clicked()
            && let Some(path) = last_export
        {
            editor.quick_load_psd(&ctx, services, &path);
        }
    });
    let last_import = editor.last_imported_psd(services);
    if ui.add_enabled(!busy && last_import.is_some(), egui::Button::new("마지막으로 사용한 PSD 내려받기")).clicked()
        && let Some((path, _)) = &last_import
    {
        editor.download_last_imported_psd(path);
    }
    if let Some((_, label)) = &last_import {
        ui.label(RichText::new(format!("마지막 사용: {label}")).weak().small());
    }
    ui.label(
        RichText::new(
            "글자에 레이어 효과가 들어갈 경우 임포트 전에 스마트 오브젝트로 변환한 후 임포트해야 보이는 그대로 불러올 수 있습니다.",
        )
        .weak()
        .small(),
    );
}
