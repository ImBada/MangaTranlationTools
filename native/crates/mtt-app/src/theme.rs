//! Look of the app: one dark palette, spacing and text sizes, the Phosphor icon font, plus
//! the small widgets the screens share (cards, segmented controls, tool buttons, status
//! pills, confirm dialogs).

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Margin, RichText, Stroke, TextStyle};

pub const APP_BG: Color32 = Color32::from_rgb(27, 29, 33);
pub const CANVAS_BG: Color32 = Color32::from_rgb(18, 19, 22);
pub const CARD_BG: Color32 = Color32::from_rgb(34, 37, 42);
pub const INPUT_BG: Color32 = Color32::from_rgb(20, 22, 25);
pub const BORDER: Color32 = Color32::from_rgb(48, 52, 59);
pub const TEXT: Color32 = Color32::from_rgb(228, 231, 235);
pub const TEXT_WEAK: Color32 = Color32::from_rgb(150, 157, 167);
pub const ACCENT: Color32 = Color32::from_rgb(76, 141, 255);
pub const ACCENT_DIM: Color32 = Color32::from_rgb(40, 74, 134);
pub const SUCCESS: Color32 = Color32::from_rgb(63, 185, 80);
pub const WARNING: Color32 = Color32::from_rgb(214, 158, 46);
pub const DANGER: Color32 = Color32::from_rgb(248, 96, 88);
pub const IDLE: Color32 = Color32::from_rgb(110, 117, 128);

const BUTTON_BG: Color32 = Color32::from_rgb(44, 48, 55);
const BUTTON_HOVER: Color32 = Color32::from_rgb(54, 59, 67);
const BUTTON_ACTIVE: Color32 = Color32::from_rgb(64, 70, 80);

/// Font family holding only the icon font (its codepoints are in the Private Use Area,
/// where egui's emoji font has glyphs of its own, so icons never share a family).
const ICON_FAMILY: &str = "phosphor";

/// Phosphor icon codepoints (`assets/fonts/Phosphor.ttf`, regular weight).
pub mod icon {
    pub const ARROW_LEFT: &str = "\u{E058}";
    pub const CARET_DOWN: &str = "\u{E136}";
    pub const CARET_LEFT: &str = "\u{E138}";
    pub const CARET_RIGHT: &str = "\u{E13A}";
    pub const CHECK_CIRCLE: &str = "\u{E184}";
    pub const CLOCK_COUNTER_CLOCKWISE: &str = "\u{E1A0}";
    pub const COPY: &str = "\u{E1CA}";
    pub const CURSOR: &str = "\u{E1DC}";
    pub const CURSOR_TEXT: &str = "\u{E7D8}";
    pub const DOTS_THREE: &str = "\u{E1FE}";
    pub const DOWNLOAD_SIMPLE: &str = "\u{E20C}";
    pub const DROP: &str = "\u{E210}";
    pub const ERASER: &str = "\u{E21E}";
    pub const EXPORT: &str = "\u{EAF0}";
    pub const EYEDROPPER: &str = "\u{E568}";
    pub const FILE_TEXT: &str = "\u{E23A}";
    pub const FOLDER_OPEN: &str = "\u{E256}";
    pub const GEAR_SIX: &str = "\u{E272}";
    pub const HAND: &str = "\u{E298}";
    pub const IMAGE: &str = "\u{E2CA}";
    pub const MAGIC_WAND: &str = "\u{E6B6}";
    pub const MAGNIFYING_GLASS: &str = "\u{E30C}";
    pub const PAINT_BRUSH: &str = "\u{E6F0}";
    pub const BROOM: &str = "\u{EC54}";
    pub const PLUS_SQUARE: &str = "\u{ED4A}";
    pub const SELECTION: &str = "\u{E69A}";
    pub const SPARKLE: &str = "\u{E6A2}";
    pub const STACK: &str = "\u{E466}";
    pub const TRANSLATE: &str = "\u{E4A2}";
    pub const TRASH: &str = "\u{E4A6}";
    pub const BANDAIDS: &str = "\u{E0B2}";
    pub const BINOCULARS: &str = "\u{EA64}";
    pub const CHECK: &str = "\u{E182}";
    pub const FRAME_CORNERS: &str = "\u{E626}";
    pub const MINUS: &str = "\u{E32A}";
    pub const PLUS: &str = "\u{E3D4}";
    pub const KEYBOARD: &str = "\u{E2D8}";
    pub const X: &str = "\u{E4F6}";
    pub const FILE_ZIP: &str = "\u{E958}";
    pub const PAINT_ROLLER: &str = "\u{E6F4}";
    pub const TEXT_AA: &str = "\u{E6EE}";
    pub const TEXT_ALIGN_CENTER: &str = "\u{E480}";
    pub const TEXT_ALIGN_LEFT: &str = "\u{E484}";
    pub const TEXT_ALIGN_RIGHT: &str = "\u{E486}";
}

/// Adds the icon font as its own family.
pub fn install_icon_font(ctx: &egui::Context) {
    ctx.add_font(egui::epaint::text::FontInsert {
        name: "phosphor-icons".into(),
        data: egui::FontData::from_static(include_bytes!("../assets/fonts/Phosphor.ttf")),
        families: vec![egui::epaint::text::InsertFontFamily {
            family: FontFamily::Name(ICON_FAMILY.into()),
            priority: egui::epaint::text::FontPriority::Highest,
        }],
    });
}

/// An icon glyph as text (16 pt unless resized).
pub fn icon(glyph: &str) -> RichText {
    RichText::new(glyph).family(FontFamily::Name(ICON_FAMILY.into())).size(16.0)
}

/// Icon followed by a label, for buttons and menu buttons.
pub fn icon_text(glyph: &str, label: impl Into<String>) -> (RichText, RichText) {
    (icon(glyph), RichText::new(label.into()))
}

/// Installs the theme for both egui themes so the OS appearance does not switch it.
pub fn apply(ctx: &egui::Context) {
    install_icon_font(ctx);
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.set_visuals_of(egui::Theme::Dark, visuals());
    ctx.set_visuals_of(egui::Theme::Light, visuals());
    ctx.all_styles_mut(|style| {
        style.text_styles = [
            (TextStyle::Heading, FontId::new(18.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(14.0, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(12.0, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(13.0, FontFamily::Monospace)),
        ]
        .into();
        let spacing = &mut style.spacing;
        spacing.item_spacing = egui::vec2(8.0, 6.0);
        spacing.button_padding = egui::vec2(10.0, 4.0);
        spacing.interact_size.y = 26.0;
        spacing.window_margin = Margin::same(14);
        spacing.menu_margin = Margin::same(6);
        spacing.combo_height = 320.0;
        spacing.slider_width = 140.0;
        spacing.indent = 16.0;
    });
}

fn visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = APP_BG;
    visuals.window_fill = CARD_BG;
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.menu_corner_radius = CornerRadius::same(8);
    visuals.extreme_bg_color = INPUT_BG;
    visuals.faint_bg_color = Color32::from_rgb(38, 41, 47);
    visuals.code_bg_color = INPUT_BG;
    visuals.hyperlink_color = ACCENT;
    visuals.warn_fg_color = WARNING;
    visuals.error_fg_color = DANGER;
    visuals.override_text_color = None;
    visuals.selection.bg_fill = ACCENT_DIM;
    visuals.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    visuals.collapsing_header_frame = false;
    visuals.slider_trailing_fill = true;
    visuals.striped = false;

    let radius = CornerRadius::same(6);
    let widgets = &mut visuals.widgets;
    widgets.noninteractive.bg_fill = CARD_BG;
    widgets.noninteractive.weak_bg_fill = CARD_BG;
    widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);
    widgets.noninteractive.fg_stroke = Stroke::new(1.0, TEXT);
    widgets.noninteractive.corner_radius = radius;
    widgets.inactive.bg_fill = BUTTON_BG;
    widgets.inactive.weak_bg_fill = BUTTON_BG;
    widgets.inactive.bg_stroke = Stroke::NONE;
    widgets.inactive.fg_stroke = Stroke::new(1.0, TEXT);
    widgets.inactive.corner_radius = radius;
    widgets.hovered.bg_fill = BUTTON_HOVER;
    widgets.hovered.weak_bg_fill = BUTTON_HOVER;
    widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(78, 84, 94));
    widgets.hovered.fg_stroke = Stroke::new(1.0, TEXT);
    widgets.hovered.corner_radius = radius;
    widgets.hovered.expansion = 0.0;
    widgets.active.bg_fill = BUTTON_ACTIVE;
    widgets.active.weak_bg_fill = BUTTON_ACTIVE;
    widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);
    widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    widgets.active.corner_radius = radius;
    widgets.active.expansion = 0.0;
    widgets.open.bg_fill = BUTTON_ACTIVE;
    widgets.open.weak_bg_fill = BUTTON_ACTIVE;
    widgets.open.bg_stroke = Stroke::new(1.0, BORDER);
    widgets.open.fg_stroke = Stroke::new(1.0, TEXT);
    widgets.open.corner_radius = radius;
    visuals
}

/// Frame of a bar (top bar, toolbars, status bar).
pub fn bar_frame(fill: Color32, vertical_margin: i8) -> egui::Frame {
    egui::Frame::new().fill(fill).inner_margin(Margin::symmetric(10, vertical_margin))
}

/// Frame of a side panel.
pub fn side_frame() -> egui::Frame {
    egui::Frame::new().fill(APP_BG).inner_margin(Margin::symmetric(10, 10))
}

/// A titled card in the inspector.
pub fn card<R>(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    card_with(ui, title, |_| {}, add_contents)
}

/// A card with extra controls on the right of the title.
pub fn card_with<R>(
    ui: &mut egui::Ui,
    title: &str,
    header_right: impl FnOnce(&mut egui::Ui),
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let inner = egui::Frame::new()
        .fill(CARD_BG)
        .stroke(Stroke::new(1.0, BORDER))
        .corner_radius(8)
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            if !title.is_empty() {
                // Controls first from the right, then the title truncated into what is left,
                // so a long page or preset name never widens the panel.
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        header_right(ui);
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.add(egui::Label::new(RichText::new(title).strong().color(TEXT)).truncate());
                        });
                    });
                });
                ui.add_space(2.0);
            }
            add_contents(ui)
        })
        .inner;
    ui.add_space(8.0);
    inner
}

/// Segmented control: one selectable button per option; returns the clicked option.
pub fn segmented<T: Copy + PartialEq>(ui: &mut egui::Ui, current: T, options: &[(T, &str, &str)]) -> Option<T> {
    let options: Vec<Segment<T>> = options.iter().map(|(value, label, hint)| (*value, "", *label, "", *hint)).collect();
    segmented_icons(ui, current, &options)
}

/// `(value, icon, label, shortcut, hint)`; `""` leaves out the icon or shortcut.
pub type Segment<'a, T> = (T, &'a str, &'a str, &'a str, &'a str);

/// [`segmented`] with an icon before and a small shortcut after each label.
pub fn segmented_icons<T: Copy + PartialEq>(ui: &mut egui::Ui, current: T, options: &[Segment<'_, T>]) -> Option<T> {
    let mut picked = None;
    egui::Frame::new().fill(INPUT_BG).corner_radius(8).inner_margin(Margin::same(3)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.horizontal(|ui| {
            for (value, glyph, label, key, hint) in options {
                let selected = *value == current;
                let color = if selected { Color32::WHITE } else { TEXT };
                let text = RichText::new(*label).color(color);
                let mut atoms = egui::Atoms::default();
                if !glyph.is_empty() {
                    atoms.push_right(icon(glyph).color(color));
                }
                atoms.push_right(text);
                if !key.is_empty() {
                    atoms.push_right(key_hint(key, selected));
                }
                let button = egui::Button::new(atoms);
                let button = button.corner_radius(6);
                // Unselected segments show a frame only while hovered.
                let button = if selected { button.fill(ACCENT_DIM) } else { button.frame_when_inactive(false) };
                let response = ui.add(button);
                let response = if hint.is_empty() { response } else { response.on_hover_text(*hint) };
                if response.clicked() && !selected {
                    picked = Some(*value);
                }
            }
        });
    });
    picked
}

/// Small shortcut label next to a control (`mac` spelling, see [`crate::shortcuts::keys`]).
pub fn key_hint(mac: &str, on_accent: bool) -> RichText {
    let color = if on_accent { Color32::from_rgb(190, 208, 245) } else { KEY_HINT };
    RichText::new(crate::shortcuts::keys(mac)).size(10.5).color(color)
}

const KEY_HINT: Color32 = Color32::from_rgb(120, 128, 140);

/// Toolbar toggle: an icon with its shortcut in small type beside it (the tooltip
/// carries the name).
pub fn icon_tool(ui: &mut egui::Ui, enabled: bool, active: bool, glyph: &str, key: &str, hint: &str) -> egui::Response {
    let glyph = if active { icon(glyph).size(18.0).color(Color32::WHITE) } else { icon(glyph).size(18.0) };
    let button = if key.is_empty() {
        egui::Button::new(glyph)
    } else {
        egui::Button::new((glyph, key_hint(key, active))).gap(2.0)
    };
    toggle(ui, enabled, active, button.min_size(egui::vec2(30.0, 30.0)), hint)
}

/// Toolbar toggle with an icon, a label and its shortcut.
pub fn icon_text_toggle(
    ui: &mut egui::Ui,
    enabled: bool,
    active: bool,
    glyph: &str,
    label: &str,
    key: &str,
    hint: &str,
) -> egui::Response {
    let color = if active { Color32::WHITE } else { TEXT };
    let button =
        egui::Button::new((icon(glyph).color(color), RichText::new(label).color(color), key_hint(key, active)));
    toggle(ui, enabled, active, button, hint)
}

/// Plain button with an icon, a label and its shortcut.
pub fn icon_button_key(glyph: &str, label: impl Into<String>, key: &str) -> egui::Button<'static> {
    egui::Button::new((icon(glyph), RichText::new(label.into()), key_hint(key, false)))
}

/// Highlighted when active. The accent outline is painted over the button instead of
/// set as its stroke: an overridden stroke is not compensated in the layout, so the
/// button (and everything after it) would shift by a pixel on hover.
fn toggle(ui: &mut egui::Ui, enabled: bool, active: bool, button: egui::Button<'_>, hint: &str) -> egui::Response {
    // Inactive: no frame until hovered (egui keeps the frame's spacing either way).
    let button = if active { button.fill(ACCENT_DIM) } else { button.frame_when_inactive(false) };
    let response = ui.add_enabled(enabled, button).on_hover_text(hint).on_disabled_hover_text(hint);
    if active {
        ui.painter().rect_stroke(response.rect, 6.0, Stroke::new(1.0, ACCENT), egui::StrokeKind::Inside);
    }
    response
}

/// Accent-filled main action button.
pub fn primary(label: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(RichText::new(label.into()).strong().color(Color32::WHITE)).fill(ACCENT_DIM)
}

/// [`primary`] with an icon.
pub fn primary_icon(glyph: &str, label: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new((icon(glyph).color(Color32::WHITE), RichText::new(label.into()).strong().color(Color32::WHITE)))
        .fill(ACCENT_DIM)
}

/// Plain button with an icon.
pub fn icon_button(glyph: &str, label: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(icon_text(glyph, label))
}

/// Button for destructive actions.
pub fn danger(label: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(RichText::new(label.into()).color(DANGER))
}

/// Small coloured dot followed by a label.
pub fn status_dot(ui: &mut egui::Ui, color: Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(10.0, 16.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 4.0, color);
    response
}

/// Rounded label with a tinted background.
pub fn pill(ui: &mut egui::Ui, text: &str, color: Color32) -> egui::Response {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.18))
        .corner_radius(10)
        .inner_margin(Margin::symmetric(7, 1))
        .show(ui, |ui| ui.label(RichText::new(text).small().color(color)))
        .inner
}

/// Weak explanatory text.
pub fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).small().color(TEXT_WEAK));
}

/// Shows `window` as a dialog: everything behind it is dimmed and blocked.
pub fn modal<R>(
    ctx: &egui::Context,
    window: egui::Window<'_>,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<egui::InnerResponse<Option<R>>> {
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::unique("modal-backdrop")).order(egui::Order::Middle).fixed_pos(screen.min).show(
        ctx,
        |ui| {
            ui.painter().rect_filled(screen, 0.0, Color32::from_black_alpha(140));
            ui.allocate_rect(screen, egui::Sense::click_and_drag());
        },
    );
    let response = window.show(ctx, add_contents);
    // The backdrop shares the window's layer order; keep the window above it.
    if let Some(response) = &response {
        ctx.move_to_top(response.response.layer_id);
    }
    response
}

/// System confirm dialog (`window.mangaApi.confirm` in the TS app).
pub fn confirm(title: &str, message: &str, detail: &str) -> bool {
    rfd::MessageDialog::new()
        .set_title(title)
        .set_description(format!("{message}\n\n{detail}"))
        .set_buttons(rfd::MessageButtons::OkCancel)
        .show()
        == rfd::MessageDialogResult::Ok
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Width a card takes in a `width`-wide panel with a long title.
    fn card_width(width: f32, title: &str) -> f32 {
        let ctx = egui::Context::default();
        let mut measured = 0.0;
        for _ in 0..3 {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0))),
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                ui.scope_builder(
                    egui::UiBuilder::new()
                        .max_rect(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 600.0))),
                    |ui| {
                        let response = ui.scope(|ui| {
                            card_with(
                                ui,
                                title,
                                |ui| {
                                    let _ = ui.button("›");
                                },
                                |ui| {
                                    ui.label("body");
                                },
                            )
                        });
                        measured = response.response.rect.width();
                    },
                );
            });
            output.textures_delta.clear();
        }
        measured
    }

    /// Rects of a toolbar row (inactive tool, active tool, icon+text toggle, segmented
    /// control, plain button) with the pointer at `pointer`.
    fn toolbar_rects(ctx: &egui::Context, pointer: egui::Pos2) -> Vec<egui::Rect> {
        install_icon_font(ctx);
        apply(ctx);
        let mut rects = Vec::new();
        for _ in 0..3 {
            rects.clear();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(900.0, 200.0))),
                events: vec![egui::Event::PointerMoved(pointer)],
                ..Default::default()
            };
            let mut output = ctx.run_ui(input, |ui| {
                ui.horizontal(|ui| {
                    rects.push(icon_tool(ui, true, false, icon::CURSOR, "A", "").rect);
                    rects.push(icon_tool(ui, true, true, icon::SELECTION, "T", "").rect);
                    rects.push(icon_text_toggle(ui, true, false, icon::IMAGE, "원본", "5", "").rect);
                    let segment = ui.scope(|ui| segmented(ui, 1, &[(1, "번역", ""), (2, "인페인트", "")]));
                    rects.push(segment.response.rect);
                    rects.push(ui.button("보기").rect);
                    rects.push(ui.label("끝").rect);
                });
            });
            output.textures_delta.clear();
        }
        rects
    }

    #[test]
    fn hovering_buttons_does_not_move_anything() {
        let ctx = egui::Context::default();
        let resting = toolbar_rects(&ctx, egui::pos2(880.0, 190.0));
        for (index, rect) in resting.clone().into_iter().enumerate().take(5) {
            let hovered = toolbar_rects(&ctx, rect.center());
            assert_eq!(hovered, resting, "hovering item {index} moved the row");
        }
    }

    #[test]
    fn long_card_titles_do_not_widen_the_panel() {
        let long = "이 페이지 · very_long_page_file_name_from_scanner_0001_final_v2.png";
        assert!(card_width(300.0, long) <= 300.0, "card is {} wide", card_width(300.0, long));
    }
}
