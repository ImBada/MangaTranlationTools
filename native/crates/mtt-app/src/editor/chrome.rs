//! Editor frame around the canvas: the top bar (navigation, mode, AI translation), the
//! page strip, the canvas toolbar (tools of the mode, view and zoom) and the status bar.
//! The inspector on the right lives in `inspector.rs`.

use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke};
use mtt_core::library::DropPosition;
use mtt_core::model::{InpaintStatus, PageAnalysisStatus, PageRecord};

use super::result_tools::ResultTool;
use super::translation::RunMode;
use super::{Editor, EditorAction, Layer, Mode, Tool};
use crate::app::AppEvent;
use crate::library_view::ImportMode;
use crate::services::Services;
use crate::shortcuts;
use crate::theme::{self, icon};
use crate::worker::Worker;

pub const MODES: [theme::Segment<'static, Mode>; 3] = [
    (Mode::Translate, icon::TRANSLATE, "번역", "2", "문장과 글꼴 편집"),
    (Mode::Inpaint, icon::BANDAIDS, "인페인트", "3·4", "원문 지우기와 보정 (3 결과 · 4 마스크)"),
    (Mode::Review, icon::CHECK_CIRCLE, "검수 · 출력", "1", "최종 결과 확인, 완료 표시, 출력"),
];

/// `text` cut to `max` characters with an ellipsis.
fn ellipsize(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_owned()
    } else {
        format!("{}…", text.chars().take(max.saturating_sub(1)).collect::<String>())
    }
}

// ---- top bar ----------------------------------------------------------------------------

pub fn top_bar(
    editor: &mut Editor,
    ui: &mut egui::Ui,
    services: &mut Services,
    worker: &Worker<AppEvent>,
) -> Option<EditorAction> {
    let mut action = None;
    egui::Panel::top("editor-top-bar").frame(theme::bar_frame(theme::APP_BG, 7)).show(ui, |ui| {
        let bar = ui.max_rect();
        ui.horizontal(|ui| {
            if ui
                .button(theme::icon_text(icon::ARROW_LEFT, "보관함"))
                .on_hover_text("저장하고 보관함으로 돌아갑니다")
                .clicked()
            {
                action = Some(EditorAction::Close);
            }
            if !editor.work_title.is_empty() {
                ui.scope(|ui| {
                    ui.set_max_width(150.0);
                    ui.add(egui::Label::new(RichText::new(&editor.work_title).color(theme::TEXT_WEAK)).truncate())
                        .on_hover_text(&editor.work_title);
                });
                ui.label(RichText::new("›").color(theme::TEXT_WEAK));
            }
            chapter_menu(editor, ui, services, &mut action);
            let left_end = ui.min_rect().right();
            let row = ui.min_rect().y_range();

            let right_start = ui
                .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .add(egui::Button::new(theme::icon(icon::GEAR_SIX).size(18.0)).min_size(egui::vec2(32.0, 28.0)))
                        .on_hover_text("설정: 번역 엔진, LaMa, 단축키")
                        .clicked()
                    {
                        action = Some(EditorAction::OpenSettings);
                    }
                    ai_controls(editor, ui, services, worker);
                    ui.min_rect().left()
                })
                .inner;

            // Mode tabs centred on the bar, or right after the breadcrumb when tight.
            let width_id = ui.scope_id().with("mode-tabs-width");
            let width: f32 = ui.data(|data| data.get_temp(width_id)).unwrap_or(300.0);
            let mut x = bar.center().x - width / 2.0;
            if x < left_end + 16.0 || x + width > right_start - 16.0 {
                x = left_end + 16.0;
            }
            // Never under the right-hand controls: squeeze in from the right instead.
            x = x.min(right_start - 12.0 - width).max(left_end + 4.0);
            let rect = Rect::from_x_y_ranges(x..=x + width + 8.0, row);
            let tabs = ui.scope_builder(
                egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)),
                |ui| theme::segmented_icons(ui, editor.mode(), &MODES),
            );
            let measured = tabs.response.rect.width();
            if (measured - width).abs() > 0.5 {
                ui.data_mut(|data| data.insert_temp(width_id, measured));
                ui.ctx().request_repaint();
            }
            if let Some(mode) = tabs.inner {
                editor.set_mode(mode);
            }
        });
    });
    action
}

/// Chapter title as a menu: every work's chapters, and the import entries.
fn chapter_menu(editor: &mut Editor, ui: &mut egui::Ui, services: &mut Services, action: &mut Option<EditorAction>) {
    let title = RichText::new(ellipsize(&editor.chapter.title, 18)).strong();
    let response = ui.menu_button((title, theme::icon(icon::CARET_DOWN).size(12.0)), |ui| {
        if editor.library_cache.is_none() {
            editor.library_cache = Some(services.library.list().unwrap_or_default());
        }
        let job_active = editor.translation_running();
        ui.set_min_width(300.0);
        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
            for work in editor.library_cache.as_deref().unwrap_or(&[]) {
                ui.label(RichText::new(&work.work.title).small().color(theme::TEXT_WEAK));
                for chapter in &work.chapters {
                    let current = chapter.id == editor.chapter.id;
                    let button = egui::Button::selectable(current, &chapter.title).right_text(
                        RichText::new(format!(
                            "{}/{}p · {}",
                            chapter.completed_page_count,
                            chapter.page_count,
                            crate::library_view::status_label(chapter.status)
                        ))
                        .small()
                        .color(theme::TEXT_WEAK),
                    );
                    let response = ui.add_enabled(!job_active || current, button);
                    let response = if job_active && !current {
                        response.on_disabled_hover_text("번역 중에는 다른 화를 열 수 없습니다")
                    } else {
                        response
                    };
                    if response.clicked() {
                        *action = Some(EditorAction::OpenChapter(chapter.id.clone()));
                        ui.close();
                    }
                }
                ui.add_space(4.0);
            }
        });
        ui.separator();
        ui.add_enabled_ui(!job_active, |ui| {
            ui.menu_button(theme::icon_text(icon::DOWNLOAD_SIMPLE, "새로 가져오기"), |ui| {
                for (mode, label) in [
                    (ImportMode::Images, "이미지 파일…"),
                    (ImportMode::Folder, "폴더…"),
                    (ImportMode::Zip, "압축파일…"),
                    (ImportMode::ZipFolder, "여러 화 일괄 번역…"),
                ] {
                    if ui.button(label).clicked() {
                        *action = Some(EditorAction::Import(mode));
                        ui.close();
                    }
                }
            });
        });
    });
    if response.inner.is_none() {
        // Read the library again the next time the menu opens.
        editor.library_cache = None;
    }
}

/// "AI 번역" (pending pages) with a menu for the other runs, or the running job.
fn ai_controls(editor: &mut Editor, ui: &mut egui::Ui, services: &mut Services, worker: &Worker<AppEvent>) {
    if let Some(job) = &editor.translation {
        let hover = format!("{}\n{}", job.endpoint, job.last_message);
        let text = format!("{} {}/{}", job.phase_label(), job.done, job.total);
        let progress = job.progress();
        if ui
            .add(egui::Button::new((
                theme::icon(icon::MINUS).color(theme::DANGER),
                RichText::new("취소").color(theme::DANGER),
            )))
            .on_hover_text("AI 번역 취소")
            .clicked()
        {
            editor.cancel_translation();
        }
        ui.add(egui::ProgressBar::new(progress).desired_width(170.0).text(text)).on_hover_text(hover);
        return;
    }
    let has_pages = editor.page_count() > 0;
    let mut run = None;
    ui.add_enabled_ui(has_pages, |ui| {
        ui.menu_button(theme::icon(icon::CARET_DOWN).size(13.0), |ui| {
            if ui.button("번역 안 된 페이지 이어서 번역").clicked() {
                run = Some(RunMode::Pending);
            }
            if ui.button("완료 표시 안 된 페이지 모두 다시 번역").clicked() {
                run = Some(RunMode::All);
            }
            ui.separator();
            if ui.button("이 페이지만 다시 번역…").clicked() {
                run = Some(RunMode::CurrentPage);
            }
        })
        .response
        .on_hover_text("번역 범위 선택");
    });
    let primary = egui::Button::new((
        theme::icon(icon::SPARKLE).color(Color32::WHITE),
        RichText::new("AI 번역").strong().color(Color32::WHITE),
    ))
    .fill(theme::ACCENT_DIM);
    if ui.add_enabled(has_pages, primary).on_hover_text("아직 번역하지 않은 페이지를 이어서 번역합니다").clicked()
    {
        run = Some(RunMode::Pending);
    }
    match run {
        Some(RunMode::CurrentPage) => {
            if theme::confirm(
                "페이지 재번역",
                "정말 재번역 하시겠습니까?",
                "기존 번역 결과와 수정 내용이 이 페이지에서 덮어써집니다.",
            ) {
                editor.start_translation(RunMode::CurrentPage, services, worker);
            }
        }
        Some(mode) => editor.start_translation(mode, services, worker),
        None => {}
    }
}

// ---- status bar -------------------------------------------------------------------------

pub fn status_bar(editor: &mut Editor, ui: &mut egui::Ui) {
    egui::Panel::bottom("editor-status-bar").frame(theme::bar_frame(theme::APP_BG, 3)).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let (color, label) = save_state(editor);
            theme::status_dot(ui, color);
            ui.label(RichText::new(label).small());
            ui.separator();
            if let Some(page) = editor.page() {
                ui.label(
                    RichText::new(format!(
                        "{} / {} · {} · {}×{}",
                        editor.current + 1,
                        editor.page_count(),
                        ellipsize(&page.name, 28),
                        page.width,
                        page.height
                    ))
                    .small()
                    .color(theme::TEXT_WEAK),
                )
                .on_hover_text(&page.name);
                ui.separator();
            }
            let left = ui.cursor().left();
            let row = ui.min_rect().y_range();
            let right = ui
                .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let sheet = egui::Button::new((
                        theme::icon(icon::KEYBOARD).size(14.0),
                        RichText::new("단축키").small(),
                        theme::key_hint("?", false),
                    ))
                    .frame_when_inactive(false);
                    if ui.add(sheet).on_hover_text("단축키 전체 보기").clicked() {
                        editor.shortcut_sheet = !editor.shortcut_sheet;
                    }
                    ui.separator();
                    jobs(editor, ui);
                    ui.min_rect().left()
                })
                .inner;
            let rect = Rect::from_x_y_ranges(left..=(right - 8.0).max(left), row);
            ui.scope_builder(
                egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)),
                |ui| status_line(editor, ui),
            );
        });
    });
}

fn save_state(editor: &Editor) -> (Color32, String) {
    if editor.notifications.has_save_failure() {
        (theme::DANGER, "저장 실패".into())
    } else if editor.notifications.save_flash_active() {
        (theme::SUCCESS, "저장 완료".into())
    } else if editor.has_unsaved_changes() {
        (theme::WARNING, "저장 대기".into())
    } else {
        (theme::IDLE, "저장됨".into())
    }
}

/// Latest status line; a fresh one is highlighted. Clicking opens the history.
fn status_line(editor: &mut Editor, ui: &mut egui::Ui) {
    let line = editor.notifications.latest().unwrap_or("").to_owned();
    let fresh = editor.notifications.latest_is_fresh();
    let failed = line.contains("실패") || line.contains("오류");
    let color = if failed {
        theme::DANGER
    } else if fresh {
        theme::TEXT
    } else {
        theme::TEXT_WEAK
    };
    let text = if line.is_empty() {
        RichText::new("기록").small().color(theme::TEXT_WEAK)
    } else {
        RichText::new(line).small().color(color)
    };
    let button =
        egui::Button::new((theme::icon(icon::CLOCK_COUNTER_CLOCKWISE).size(13.0).color(theme::TEXT_WEAK), text))
            .frame_when_inactive(false)
            .truncate();
    let response = ui.add(button).on_hover_text("상태 기록 보기");
    egui::Popup::from_toggle_button_response(&response)
        .align(egui::RectAlign::TOP_START)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_width(360.0);
            ui.label(RichText::new("상태 기록").strong());
            ui.add_space(2.0);
            egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                let lines = editor.notifications.lines();
                if lines.is_empty() {
                    theme::hint(ui, "아직 표시할 상태가 없습니다.");
                }
                for line in lines {
                    ui.label(RichText::new(line).small());
                }
            });
        });
}

/// Background work on the right of the status bar (right-to-left order).
fn jobs(editor: &Editor, ui: &mut egui::Ui) {
    if let Some((current, total)) = editor.export_progress {
        job(ui, &format!("출력 {current}/{total}"), ratio(current, total));
    }
    if let Some(queue) = &editor.inpaint_queue {
        let done = queue.total.saturating_sub(queue.pages.len());
        job(ui, &format!("전체 인페인트 {done}/{}", queue.total), ratio(done, queue.total));
    } else if editor.inpaint_busy() {
        ui.label(RichText::new("인페인트 중").small());
        ui.spinner();
    }
    if let Some(report) = &editor.report {
        job(ui, &format!("{} {}/{}", report.label, report.current, report.total), ratio(report.current, report.total));
    }
}

fn ratio(done: usize, total: usize) -> f32 {
    if total == 0 { 0.0 } else { (done as f32 / total as f32).clamp(0.0, 1.0) }
}

fn job(ui: &mut egui::Ui, label: &str, progress: f32) {
    ui.add(egui::ProgressBar::new(progress).desired_width(90.0).desired_height(8.0));
    ui.label(RichText::new(label).small());
}

// ---- page strip ---------------------------------------------------------------------------

/// Page drag payload (distinct from block ids dragged in the inspector).
#[derive(Clone)]
struct PageDrag(String);

const ROW_HEIGHT: f32 = 74.0;
const THUMB_SIZE: egui::Vec2 = egui::vec2(46.0, 62.0);

pub fn page_strip(editor: &mut Editor, ui: &mut egui::Ui, services: &mut Services, worker: &Worker<AppEvent>) {
    egui::Panel::left("editor-pages")
        .resizable(true)
        .default_size(196.0)
        .size_range(180.0..=300.0)
        .frame(theme::side_frame())
        .show(ui, |ui| {
            let completed = editor.chapter.pages.iter().filter(|page| page.is_completed()).count();
            let total = editor.page_count();
            ui.horizontal(|ui| {
                ui.label(RichText::new("페이지").strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    let has_next = editor.current + 1 < total;
                    let next = egui::Button::new((theme::icon(icon::CARET_RIGHT), theme::key_hint("F", false)));
                    if ui.add_enabled(has_next, next).on_hover_text("다음 페이지 (→ 도 됨)").clicked() {
                        editor.next_page();
                    }
                    let previous = egui::Button::new((theme::icon(icon::CARET_LEFT), theme::key_hint("D", false)));
                    if ui.add_enabled(editor.current > 0, previous).on_hover_text("이전 페이지 (← 도 됨)").clicked()
                    {
                        editor.previous_page();
                    }
                });
            });
            ui.horizontal(|ui| {
                let label = RichText::new(format!("완료 {completed}/{total}")).small().color(theme::TEXT_WEAK);
                let width = (ui.available_width() - 64.0).max(40.0);
                ui.add(egui::ProgressBar::new(ratio(completed, total)).desired_width(width).desired_height(4.0));
                ui.label(label);
            });
            ui.add_space(4.0);
            if total == 0 {
                theme::hint(ui, "불러온 페이지가 없습니다.");
                return;
            }
            page_rows(editor, ui, services, worker);
        });
}

fn page_rows(editor: &mut Editor, ui: &mut egui::Ui, services: &mut Services, worker: &Worker<AppEvent>) {
    let job_active = editor.translation_running();
    // Thumbnails wait until the page being edited has decoded.
    let decode_thumbs = editor.is_page_decoded();
    let mut go_to = None;
    let mut toggle = None;
    let mut retranslate = false;
    let mut remove = None;
    let mut reorder: Option<(String, String, DropPosition)> = None;
    egui::ScrollArea::vertical().auto_shrink(false).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 2.0;
        for index in 0..editor.page_count() {
            let (page_id, page_name, completed) = {
                let page = &editor.chapter.pages[index];
                (page.id.clone(), page.name.clone(), page.is_completed())
            };
            let selected = index == editor.current;
            let (rect, response) =
                ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW_HEIGHT), Sense::click_and_drag());
            let response = if job_active {
                response
            } else {
                crate::dnd::drag_source(ui, response, PageDrag(page_id.clone()), &page_name)
            };
            if ui.is_rect_visible(rect) {
                if decode_thumbs {
                    editor.thumbnails.request(&services.library, &editor.chapter, &editor.chapter.pages[index], worker);
                }
                paint_page_row(editor, ui, rect, &response, &editor.chapter.pages[index], index, selected);
            }
            // Completion check on the right edge of the row.
            let check =
                Rect::from_center_size(egui::pos2(rect.right() - 16.0, rect.center().y), egui::vec2(22.0, 22.0));
            let check_response = ui
                .interact(check, response.id.with("done"), Sense::click())
                .on_hover_text(if completed { "완료 표시 해제 (₩)" } else { "완료로 표시 (₩)" });
            paint_check(ui, check, completed, check_response.hovered());
            if check_response.clicked() {
                toggle = Some(index);
            } else if response.clicked() {
                go_to = Some(index);
            }
            if selected && editor.view.fit_requested {
                response.scroll_to_me(None);
            }
            response.context_menu(|ui| {
                if ui.button(if completed { "완료 표시 해제" } else { "완료로 표시" }).clicked() {
                    toggle = Some(index);
                    ui.close();
                }
                if ui.add_enabled(!job_active, egui::Button::new("이 페이지 다시 번역…")).clicked() {
                    go_to = Some(index);
                    retranslate = true;
                    ui.close();
                }
                ui.separator();
                if ui.add_enabled(!job_active, theme::danger("페이지 삭제…")).clicked() {
                    remove = Some(page_id.clone());
                    ui.close();
                }
            });
            if !job_active
                && let Some(pointer) = ui.input(|i| i.pointer.hover_pos())
                && response.dnd_hover_payload::<PageDrag>().is_some()
            {
                let after = pointer.y > rect.center().y;
                let y = if after { rect.bottom() + 1.0 } else { rect.top() - 1.0 };
                ui.painter().hline(rect.x_range(), y, Stroke::new(2.0, theme::ACCENT));
                if let Some(source) = response.dnd_release_payload::<PageDrag>()
                    && source.0 != page_id
                {
                    let position = if after { DropPosition::After } else { DropPosition::Before };
                    reorder = Some((source.0.clone(), page_id.clone(), position));
                }
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
    if retranslate
        && theme::confirm(
            "페이지 재번역",
            "정말 재번역 하시겠습니까?",
            "기존 번역 결과와 수정 내용이 이 페이지에서 덮어써집니다.",
        )
    {
        editor.start_translation(RunMode::CurrentPage, services, worker);
    }
    if let Some(page_id) = remove
        && theme::confirm(
            "페이지 삭제",
            "정말 삭제하시겠습니까?",
            "이 페이지와 해당 번역 결과가 보관함에서 삭제됩니다.",
        )
    {
        editor.delete_page(&page_id, services);
    }
}

fn paint_page_row(
    editor: &Editor,
    ui: &egui::Ui,
    rect: Rect,
    response: &egui::Response,
    page: &PageRecord,
    index: usize,
    selected: bool,
) {
    let painter = ui.painter();
    let fill = if selected {
        theme::ACCENT_DIM
    } else if response.hovered() {
        Color32::from_rgb(40, 44, 50)
    } else {
        Color32::TRANSPARENT
    };
    painter.rect_filled(rect, 6.0, fill);
    if response.dragged() {
        painter.rect_stroke(rect, 6.0, Stroke::new(1.0, theme::ACCENT), egui::StrokeKind::Inside);
    }

    // Thumbnail fitted into its box.
    let slot = Rect::from_min_size(rect.left_top() + egui::vec2(6.0, (rect.height() - THUMB_SIZE.y) / 2.0), THUMB_SIZE);
    match editor.thumbnails.texture(&page.id) {
        Some(texture) => {
            let size = texture.size_vec2();
            let scale = (slot.width() / size.x).min(slot.height() / size.y);
            let image = Rect::from_center_size(slot.center(), size * scale);
            painter.image(texture.id(), image, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            if page.is_completed() {
                painter.rect_filled(image, 2.0, Color32::from_black_alpha(90));
            }
        }
        None => {
            painter.rect_filled(slot, 3.0, Color32::from_rgb(46, 50, 57));
        }
    }

    // Number, name and the two status chips.
    let text_left = slot.right() + 10.0;
    let text_width = (rect.right() - 34.0 - text_left).max(10.0);
    let number = painter.layout_no_wrap(
        format!("{}", index + 1),
        egui::FontId::proportional(15.0),
        if selected { Color32::WHITE } else { theme::TEXT },
    );
    painter.galley(egui::pos2(text_left, rect.top() + 10.0), number, theme::TEXT);
    let mut job =
        egui::text::LayoutJob::simple_singleline(page.name.clone(), egui::FontId::proportional(11.5), theme::TEXT_WEAK);
    job.wrap = egui::text::TextWrapping::truncate_at_width(text_width);
    let name = painter.layout_job(job);
    painter.galley(egui::pos2(text_left, rect.top() + 30.0), name, theme::TEXT_WEAK);

    let (translate_color, translate_hint) = match page.analysis_status {
        PageAnalysisStatus::Completed => (theme::SUCCESS, "번역됨"),
        PageAnalysisStatus::Running => (theme::ACCENT, "번역 중"),
        PageAnalysisStatus::Failed => (theme::DANGER, "번역 실패"),
        PageAnalysisStatus::Idle => (theme::IDLE, "번역 전"),
    };
    let (inpaint_color, inpaint_hint) = match page.inpaint_status {
        Some(InpaintStatus::Completed) => (theme::SUCCESS, "인페인트됨"),
        Some(InpaintStatus::Running) => (theme::ACCENT, "인페인트 중"),
        Some(InpaintStatus::Failed) => (theme::DANGER, "인페인트 실패"),
        _ => (theme::IDLE, "인페인트 전"),
    };
    let mut x = text_left;
    let y = rect.top() + 52.0;
    for (label, color) in [("번역", translate_color), ("지움", inpaint_color)] {
        let galley = painter.layout_no_wrap(label.into(), egui::FontId::proportional(10.5), color);
        let chip = Rect::from_min_size(egui::pos2(x, y - 1.0), galley.size() + egui::vec2(10.0, 2.0));
        painter.rect_filled(chip, 6.0, color.gamma_multiply(0.18));
        painter.galley(chip.min + egui::vec2(5.0, 1.0), galley, color);
        x = chip.right() + 4.0;
    }
    if response.hovered() && !response.dragged() {
        response.clone().on_hover_text(format!("{} · {translate_hint} · {inpaint_hint}", page.name));
    }
}

fn paint_check(ui: &egui::Ui, rect: Rect, done: bool, hovered: bool) {
    let painter = ui.painter();
    let center = rect.center();
    if done {
        painter.circle_filled(center, 9.0, theme::SUCCESS);
        let points = [center + egui::vec2(-4.0, 0.0), center + egui::vec2(-1.0, 3.0), center + egui::vec2(4.5, -3.5)];
        painter.line(points.to_vec(), Stroke::new(2.0, Color32::WHITE));
    } else {
        let color = if hovered { theme::TEXT } else { Color32::from_rgb(85, 92, 102) };
        painter.circle_stroke(center, 8.0, Stroke::new(1.5, color));
    }
}

// ---- canvas toolbar ---------------------------------------------------------------------

/// Tools of the mode on the left (wrapping onto a second row when the canvas is narrow),
/// view and zoom on the right.
pub fn canvas_toolbar(editor: &mut Editor, ui: &mut egui::Ui) {
    egui::Panel::top("canvas-toolbar").frame(theme::bar_frame(theme::CARD_BG, 5)).show(ui, |ui| {
        // One row high: separators and wrapping are measured against this, not the
        // panel's whole available height.
        let row_height = ui.spacing().interact_size.y + 6.0;
        let full = Rect::from_min_size(ui.max_rect().min, egui::vec2(ui.max_rect().width(), row_height));
        let right = ui.scope_builder(
            egui::UiBuilder::new().max_rect(full).layout(egui::Layout::right_to_left(egui::Align::Center)),
            |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.spacing_mut().button_padding.x = 7.0;
                zoom_controls(editor, ui);
                ui.separator();
                // On a narrow canvas these two drop their labels so the tools fit.
                let compact = full.width() < 720.0;
                view_menu(editor, ui, compact);
                let source = if compact {
                    theme::icon_tool(ui, true, editor.source_view(), icon::IMAGE, "5", "원본 이미지만 보기")
                } else {
                    theme::icon_text_toggle(
                        ui,
                        true,
                        editor.source_view(),
                        icon::IMAGE,
                        "원본",
                        "5",
                        "원본 이미지만 보기",
                    )
                };
                if source.clicked() {
                    editor.toggle_source_view();
                }
                ui.min_rect()
            },
        );
        let tools_rect =
            Rect::from_min_max(full.min, egui::pos2((right.inner.left() - 12.0).max(full.left()), full.bottom()));
        let tools = ui.scope_builder(
            egui::UiBuilder::new()
                .max_rect(tools_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center).with_main_wrap(true)),
            |ui| {
                ui.spacing_mut().item_spacing = egui::vec2(3.0, 4.0);
                // Tight buttons: every tool carries its shortcut beside the icon.
                ui.spacing_mut().button_padding.x = 7.0;
                let locked = editor.page_edit_locked();
                match editor.mode() {
                    Mode::Translate => translate_tools(editor, ui, locked),
                    Mode::Inpaint => inpaint_tools(editor, ui, locked),
                    Mode::Review => review_tools(editor, ui),
                }
            },
        );
        // Reserve the taller of the two groups.
        let height = tools.response.rect.height().max(right.response.rect.height());
        ui.allocate_rect(Rect::from_min_size(full.min, egui::vec2(full.width(), height)), Sense::hover());
    });
}

/// Select / range / zoom: the three canvas tools every mode shares.
fn pointer_tools(editor: &mut Editor, ui: &mut egui::Ui, locked: bool, pointer_hint: &str, range_hint: &str) {
    if theme::icon_tool(ui, true, editor.tool == Tool::Select, icon::CURSOR, "A", pointer_hint).clicked() {
        editor.select_pointer_tool();
    }
    if theme::icon_tool(ui, !locked, editor.tool == Tool::Range, icon::SELECTION, "T", range_hint).clicked() {
        editor.select_range_tool();
    }
    let zoom_hint = format!("확대 도구\n클릭 확대, {} 클릭 축소", shortcuts::keys("⌥"));
    if theme::icon_tool(ui, true, editor.tool == Tool::Zoom, icon::MAGNIFYING_GLASS, "Z", &zoom_hint).clicked() {
        editor.select_zoom_tool();
    }
}

fn translate_tools(editor: &mut Editor, ui: &mut egui::Ui, locked: bool) {
    let hint = format!(
        "선택\n블록을 눌러 고르고 끌어서 옮깁니다\n{} 드래그: 선택한 블록 영역 다시 그리기\n{} 클릭: 블록 복제",
        shortcuts::keys("⌥"),
        shortcuts::keys("⌘"),
    );
    pointer_tools(editor, ui, locked, &hint, "범위 선택\n끈 사각형에 걸친 블록을 모두 고릅니다");
    ui.separator();
    let page_ready = editor.page().is_some() && !locked && !editor.inpaint_busy();
    if ui
        .add_enabled(page_ready, egui::Button::new(theme::icon_text(icon::PLUS_SQUARE, "블록 추가")))
        .on_hover_text("빈 텍스트 블록 만들기")
        .clicked()
    {
        editor.create_empty_block();
    }
    let single = editor.selection.len() == 1;
    if ui
        .add_enabled(single && !locked, theme::icon_button_key(icon::CURSOR_TEXT, "직접 입력", "E"))
        .on_hover_text("캔버스에서 문장 바로 고치기 (더블클릭도 됨)")
        .on_disabled_hover_text("블록 하나를 고르면 캔버스에서 바로 고칠 수 있습니다")
        .clicked()
    {
        let id = editor.selection[0].clone();
        editor.start_inline_edit(&id);
    }
    if ui.add(theme::icon_button_key(icon::BINOCULARS, "찾기", "⌘F")).on_hover_text("화 전체에서 찾아 바꾸기").clicked()
    {
        editor.find.open = true;
    }
}

fn inpaint_tools(editor: &mut Editor, ui: &mut egui::Ui, locked: bool) {
    if let Some(layer) = theme::segmented_icons(
        ui,
        editor.inpaint_layer,
        &[
            (Layer::InpaintResult, "", "결과", "3", "지운 결과를 보면서 직접 칠하기"),
            (Layer::InpaintMask, "", "마스크", "4", "지울 영역을 칠하기"),
        ],
    ) {
        editor.set_layer(layer);
    }
    ui.separator();
    // Painting waits for a running inpaint (its result replaces the layers).
    let paint = !locked && !editor.inpaint_busy() && !editor.source_view();
    pointer_tools(editor, ui, !paint, "선택 · 화면 이동", "범위 선택\n범위 안만 채우기·비우기·다시 지우기");
    ui.separator();
    if editor.inpaint_layer == Layer::InpaintMask {
        for (tool, glyph, key, hint) in [
            (Tool::MaskBrush, icon::PAINT_BRUSH, "B", "마스크 브러시\n지울 영역을 칠합니다"),
            (Tool::MaskEraser, icon::ERASER, "E", "마스크 지우개"),
            (Tool::MaskAutoEraser, icon::BROOM, "⌥E", "덩어리 지우개\n끌어서 닿은 마스크 덩어리를 통째로 지웁니다"),
        ] {
            if theme::icon_tool(ui, paint, editor.tool == tool, glyph, key, hint).clicked() {
                editor.select_shared_inpaint_tool(tool);
            }
        }
        return;
    }
    let mut pick = None;
    let active = |tool| editor.tool == Tool::Result && editor.result_tool == tool;
    for (tool, glyph, key, hint) in [
        (ResultTool::Brush, icon::PAINT_BRUSH, "B", "브러시\n브러시 색으로 칠합니다"),
        (ResultTool::SmartBrush, icon::MAGIC_WAND, "⌥B", "스마트 브러시\n주변 질감으로 메웁니다"),
        (ResultTool::Eraser, icon::ERASER, "E", "지우개\n결과를 지워 원본이 보이게 합니다"),
    ] {
        if theme::icon_tool(ui, paint, active(tool), glyph, key, hint).clicked() {
            pick = Some(tool);
        }
    }
    // Blur / sharpen / smudge share one button with a menu.
    let adjust = [(ResultTool::Blur, "흐림"), (ResultTool::Sharpen, "선명"), (ResultTool::Smudge, "뭉개기")];
    let current = adjust.iter().find(|(tool, _)| active(*tool)).copied();
    ui.add_enabled_ui(paint, |ui| {
        let color = if current.is_some() { Color32::WHITE } else { theme::TEXT };
        let label = current.map_or("보정", |(_, label)| label);
        let button = egui::Button::new((
            theme::icon(icon::DROP).size(18.0).color(color),
            RichText::new(label).color(color),
            theme::icon(icon::CARET_DOWN).size(11.0).color(color),
        ));
        let button = if current.is_some() { button.fill(theme::ACCENT_DIM) } else { button.frame_when_inactive(false) };
        egui::containers::menu::MenuButton::from_button(button)
            .ui(ui, |ui| {
                for (tool, label) in adjust {
                    if ui.button(label).clicked() {
                        pick = Some(tool);
                        ui.close();
                    }
                }
            })
            .0
            .on_hover_text("보정 도구: 흐림 · 선명 · 뭉개기");
    });
    let picker = active(ResultTool::ColorPicker);
    if theme::icon_tool(ui, paint, picker, icon::EYEDROPPER, "I", "스포이드\n화면에서 브러시 색을 가져옵니다").clicked()
    {
        pick = Some(ResultTool::ColorPicker);
    }
    match pick {
        Some(ResultTool::Brush) => editor.select_shared_inpaint_tool(Tool::MaskBrush),
        Some(ResultTool::Eraser) => editor.select_shared_inpaint_tool(Tool::MaskEraser),
        Some(other) => editor.select_result_edit_tool(other),
        None => {}
    }
}

fn review_tools(editor: &mut Editor, ui: &mut egui::Ui) {
    if theme::icon_tool(ui, true, editor.tool != Tool::Zoom, icon::HAND, "A", "화면 이동\n끌어서 옮깁니다").clicked()
    {
        editor.select_pointer_tool();
    }
    let zoom_hint = format!("확대 도구\n클릭 확대, {} 클릭 축소", shortcuts::keys("⌥"));
    if theme::icon_tool(ui, true, editor.tool == Tool::Zoom, icon::MAGNIFYING_GLASS, "Z", &zoom_hint).clicked() {
        editor.select_zoom_tool();
    }
    ui.separator();
    let Some(done) = editor.page().map(|page| page.is_completed()) else { return };
    let (glyph, label) = if done { (icon::CHECK_CIRCLE, "완료됨") } else { (icon::CHECK, "완료로 표시") };
    if theme::icon_text_toggle(ui, true, done, glyph, label, "₩", "이 페이지 작업 완료 표시 (` 도 됨)").clicked()
    {
        editor.toggle_page_completed();
    }
}

/// `− [62% ▾] +` (right-to-left order); the menu has fit and 100%.
fn zoom_controls(editor: &mut Editor, ui: &mut egui::Ui) {
    let size = editor.page().map(|page| (page.width as f32, page.height as f32));
    let small =
        |glyph| egui::Button::new(theme::icon(glyph)).min_size(egui::vec2(28.0, 26.0)).frame_when_inactive(false);
    if ui.add(small(icon::PLUS)).on_hover_text(format!("확대 ({})", shortcuts::keys("⌘+"))).clicked() {
        editor.zoom_step(true);
    }
    let label =
        (RichText::new(format!("{}%", (editor.view.zoom * 100.0).round())).monospace(), theme::key_hint("⌘0", false));
    ui.menu_button(label, |ui| {
        let fit = egui::Button::new(theme::icon_text(icon::FRAME_CORNERS, "화면에 맞춤"))
            .shortcut_text(shortcuts::keys("⌘0"));
        if ui.add(fit).clicked() {
            editor.view.fit_requested = true;
            ui.close();
        }
        if ui.add_enabled(size.is_some(), egui::Button::new("원본 크기 (100%)")).clicked()
            && let Some((width, height)) = size
        {
            editor.view.original_size(width, height);
            ui.close();
        }
    })
    .response
    .on_hover_text("배율: 화면에 맞춤 / 100%");
    if ui.add(small(icon::MINUS)).on_hover_text(format!("축소 ({})", shortcuts::keys("⌘-"))).clicked() {
        editor.zoom_step(false);
    }
}

/// "보기": layer visibility and opacity, FOCUS MODE and the page block opacity.
fn view_menu(editor: &mut Editor, ui: &mut egui::Ui, compact: bool) {
    use egui::IntoAtoms as _;
    let caret = theme::icon(icon::CARET_DOWN).size(11.0);
    let atoms = if compact {
        (theme::icon(icon::STACK), caret).into_atoms()
    } else {
        (theme::icon(icon::STACK), RichText::new("보기"), caret).into_atoms()
    };
    let response = ui.menu_button(atoms, |ui| {
        ui.set_width(280.0);
        ui.checkbox(&mut editor.focus_mode, "모드를 바꾸면 레이어 투명도 자동 조정")
            .on_hover_text("FOCUS MODE: 작업 중인 레이어가 잘 보이게 나머지를 흐리게 합니다");
        ui.separator();
        egui::Grid::new("view-layers").num_columns(3).spacing([8.0, 6.0]).show(ui, |ui| {
            let mut visible = editor.visibility.overlay;
            let mut opacity =
                if editor.overlay_opacity_edit { editor.page_block_opacity() as f32 } else { editor.opacity.overlay };
            let before = opacity;
            layer_row(ui, "번역 블록", &mut visible, &mut opacity);
            editor.visibility.overlay = visible;
            if opacity != before {
                if !editor.overlay_opacity_edit {
                    editor.opacity.overlay = opacity;
                } else if !editor.page_edit_locked() {
                    editor.set_page_block_opacity(f64::from(opacity));
                }
            }

            let mut visible = editor.visibility.inpaint;
            layer_row(ui, "인페인트 (결과+마스크)", &mut visible, &mut editor.opacity.inpaint);
            if visible != editor.visibility.inpaint {
                editor.visibility.inpaint = visible;
                if !visible {
                    editor.visibility.inpaint_result = false;
                    editor.visibility.inpaint_mask = false;
                }
            }
            let mut visible = editor.visibility.inpaint && editor.visibility.inpaint_result;
            let shown = visible;
            layer_row(ui, "  └ 결과", &mut visible, &mut editor.opacity.inpaint_result);
            if visible != shown {
                editor.visibility.inpaint |= visible;
                editor.visibility.inpaint_result = visible;
            }
            let mut visible = editor.visibility.inpaint && editor.visibility.inpaint_mask;
            let shown = visible;
            layer_row(ui, "  └ 마스크", &mut visible, &mut editor.opacity.inpaint_mask);
            if visible != shown {
                editor.visibility.inpaint |= visible;
                editor.visibility.inpaint_mask = visible;
            }
            layer_row(ui, "원본 이미지", &mut editor.visibility.image, &mut editor.opacity.image);
        });
        ui.separator();
        let mut edit = editor.overlay_opacity_edit;
        if ui
            .checkbox(&mut edit, "번역 블록 슬라이더로 이 페이지 블록 배경 투명도 편집")
            .on_hover_text("켜면 위 '번역 블록' 슬라이더가 이 페이지 모든 블록의 배경 투명도를 바꿉니다")
            .changed()
        {
            editor.overlay_opacity_edit = edit;
            if edit {
                editor.opacity.overlay = 1.0;
            }
        }
        if ui.button("투명도 기본값").clicked() {
            editor.opacity = super::DEFAULT_LAYER_OPACITY;
            editor.visibility = mtt_render::page::LayerVisibility {
                image: true,
                inpaint: true,
                inpaint_result: true,
                inpaint_mask: true,
                overlay: true,
            };
        }
    });
    response.response.on_hover_text("보기: 레이어 표시와 투명도");
}

fn layer_row(ui: &mut egui::Ui, label: &str, visible: &mut bool, opacity: &mut f32) {
    ui.checkbox(visible, label);
    ui.add_enabled(*visible, egui::Slider::new(opacity, 0.0..=1.0).show_value(false));
    ui.label(RichText::new(format!("{}%", (*opacity * 100.0).round())).small().color(theme::TEXT_WEAK));
    ui.end_row();
}
