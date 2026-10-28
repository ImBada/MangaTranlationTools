//! Look of the app: one dark palette, spacing and text sizes, the Phosphor icon font, plus
//! the small widgets the screens share (sections, segmented controls, tool buttons, status
//! labels, progress bars, tooltips, confirm dialogs).
//!
//! The palette is mostly neutral grays. Accent blue is kept for the one main action of a
//! screen, focus, selection and drop indicators; selected list rows get a muted tint.
//! Controls with a shortcut show it in small muted type beside their label, and again in
//! the tooltip with what the control does.

use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{self, Color32, CornerRadius, FontFamily, FontId, Margin, RichText, Stroke, TextStyle};

/// Panels and bars.
pub const APP_BG: Color32 = Color32::from_rgb(30, 31, 34);
/// Behind the page (canvas) and the library's content area.
pub const CANVAS_BG: Color32 = Color32::from_rgb(20, 21, 23);
/// Raised surfaces: windows, menus, tooltips, notices.
pub const CARD_BG: Color32 = Color32::from_rgb(38, 40, 44);
pub const INPUT_BG: Color32 = Color32::from_rgb(22, 23, 26);
/// Hairlines between sections and around raised surfaces.
pub const BORDER: Color32 = Color32::from_rgb(48, 50, 55);
pub const TEXT: Color32 = Color32::from_rgb(230, 231, 234);
pub const TEXT_WEAK: Color32 = Color32::from_rgb(150, 154, 162);
/// Tertiary text and idle icons (grips, empty checks).
pub const TEXT_FAINT: Color32 = Color32::from_rgb(102, 106, 114);
pub const ACCENT: Color32 = Color32::from_rgb(76, 141, 255);
/// Fill of the main action button.
pub const PRIMARY: Color32 = Color32::from_rgb(46, 108, 224);
/// Muted accent behind selected rows, active tools and text selection.
pub const SELECTED: Color32 = Color32::from_rgb(41, 59, 92);
/// Hover fill of rows drawn by hand (page list, library lists).
pub const ROW_HOVER: Color32 = Color32::from_rgb(38, 40, 45);
pub const SUCCESS: Color32 = Color32::from_rgb(64, 182, 107);
pub const WARNING: Color32 = Color32::from_rgb(220, 164, 56);
pub const DANGER: Color32 = Color32::from_rgb(240, 98, 90);
pub const IDLE: Color32 = Color32::from_rgb(96, 100, 108);

const BUTTON_BG: Color32 = Color32::from_rgb(46, 48, 53);
const BUTTON_HOVER: Color32 = Color32::from_rgb(55, 58, 64);
const BUTTON_ACTIVE: Color32 = Color32::from_rgb(63, 66, 73);
/// The chosen segment of a segmented control (a raised neutral chip).
const SEGMENT_ON: Color32 = Color32::from_rgb(64, 67, 75);
/// Text of selected rows and the focus ring of text fields.
const ACCENT_TEXT: Color32 = Color32::from_rgb(150, 188, 255);
const TRACK: Color32 = Color32::from_rgb(52, 55, 61);
/// Shortcut letters beside a control, and on a filled (selected or primary) one.
const KEY_HINT: Color32 = Color32::from_rgb(124, 129, 138);
const KEY_HINT_ON_FILL: Color32 = Color32::from_rgb(184, 198, 224);

/// Font family holding only the icon font (its codepoints are in the Private Use Area,
/// where egui's emoji font has glyphs of its own, so icons never share a family).
const ICON_FAMILY: &str = "phosphor";
/// Semibold UI face for titles, installed by `main.rs` when the system has one.
pub const SEMIBOLD_FAMILY: &str = "ui-semibold";
static SEMIBOLD_READY: AtomicBool = AtomicBool::new(false);

/// Phosphor icon codepoints (`assets/fonts/Phosphor.ttf`, regular weight).
pub mod icon {
    pub const ARROW_LEFT: &str = "\u{E058}";
    pub const ARROW_SQUARE_IN: &str = "\u{E5DC}";
    pub const ARROW_SQUARE_OUT: &str = "\u{E5DE}";
    pub const DOTS_SIX_VERTICAL: &str = "\u{EAE2}";
    pub const CARET_DOWN: &str = "\u{E136}";
    pub const CARET_LEFT: &str = "\u{E138}";
    pub const CARET_RIGHT: &str = "\u{E13A}";
    pub const CARET_UP: &str = "\u{E13C}";
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
    pub const INFO: &str = "\u{E2CE}";
    pub const LINK_SIMPLE: &str = "\u{E2E6}";
    pub const LINK_SIMPLE_BREAK: &str = "\u{E2E8}";
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

/// Records that [`SEMIBOLD_FAMILY`] has a font, so [`semibold`] can use it.
pub fn set_semibold_ready() {
    SEMIBOLD_READY.store(true, Ordering::Relaxed);
}

/// Text in the semibold UI face (titles); just brighter text when there is none.
pub fn semibold(text: impl Into<String>) -> RichText {
    let text = RichText::new(text).color(TEXT);
    if SEMIBOLD_READY.load(Ordering::Relaxed) { text.family(FontFamily::Name(SEMIBOLD_FAMILY.into())) } else { text }
}

/// The semibold face as a `FontId` for painter text.
pub fn semibold_font(size: f32) -> FontId {
    if SEMIBOLD_READY.load(Ordering::Relaxed) {
        FontId::new(size, FontFamily::Name(SEMIBOLD_FAMILY.into()))
    } else {
        FontId::proportional(size)
    }
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
            (TextStyle::Heading, FontId::new(17.0, FontFamily::Proportional)),
            (TextStyle::Body, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Button, FontId::new(13.0, FontFamily::Proportional)),
            (TextStyle::Small, FontId::new(11.5, FontFamily::Proportional)),
            (TextStyle::Monospace, FontId::new(12.5, FontFamily::Monospace)),
        ]
        .into();
        let spacing = &mut style.spacing;
        spacing.item_spacing = egui::vec2(8.0, 6.0);
        spacing.button_padding = egui::vec2(9.0, 3.0);
        spacing.interact_size.y = 24.0;
        spacing.window_margin = Margin::same(16);
        spacing.menu_margin = Margin::same(5);
        spacing.combo_height = 320.0;
        spacing.slider_width = 140.0;
        spacing.indent = 14.0;
        spacing.tooltip_width = 320.0;
    });
}

fn visuals() -> egui::Visuals {
    let mut visuals = egui::Visuals::dark();
    visuals.panel_fill = APP_BG;
    visuals.window_fill = CARD_BG;
    visuals.window_stroke = Stroke::new(1.0, BORDER);
    visuals.window_corner_radius = CornerRadius::same(10);
    visuals.window_shadow = egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(110) };
    visuals.popup_shadow = egui::Shadow { offset: [0, 4], blur: 12, spread: 0, color: Color32::from_black_alpha(90) };
    visuals.menu_corner_radius = CornerRadius::same(7);
    visuals.extreme_bg_color = INPUT_BG;
    visuals.faint_bg_color = ROW_HOVER;
    visuals.code_bg_color = INPUT_BG;
    visuals.hyperlink_color = ACCENT;
    visuals.warn_fg_color = WARNING;
    visuals.error_fg_color = DANGER;
    visuals.override_text_color = None;
    visuals.selection.bg_fill = SELECTED;
    visuals.selection.stroke = Stroke::new(1.0, ACCENT_TEXT);
    visuals.collapsing_header_frame = false;
    visuals.slider_trailing_fill = true;
    visuals.striped = false;

    let radius = CornerRadius::same(5);
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
    widgets.hovered.bg_stroke = Stroke::NONE;
    widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    widgets.hovered.corner_radius = radius;
    widgets.hovered.expansion = 0.0;
    widgets.active.bg_fill = BUTTON_ACTIVE;
    widgets.active.weak_bg_fill = BUTTON_ACTIVE;
    widgets.active.bg_stroke = Stroke::NONE;
    widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);
    widgets.active.corner_radius = radius;
    widgets.active.expansion = 0.0;
    // `open` also fills the title bar of the focused window: keep it the window's colour
    // so dialogs don't get a separate header strip.
    widgets.open.bg_fill = BUTTON_ACTIVE;
    widgets.open.weak_bg_fill = CARD_BG;
    widgets.open.bg_stroke = Stroke::NONE;
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
    egui::Frame::new().fill(APP_BG).inner_margin(Margin::symmetric(12, 10))
}

/// A titled section of a side panel: a hairline above, a semibold title, the contents.
pub fn card<R>(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui) -> R) -> R {
    card_with(ui, title, |_| {}, add_contents)
}

/// A section with extra controls on the right of the title.
pub fn card_with<R>(
    ui: &mut egui::Ui,
    title: &str,
    header_right: impl FnOnce(&mut egui::Ui),
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    card_full(ui, title, None, header_right, add_contents)
}

/// A section that can also carry a `⠿` grip left of its title; the grip senses drags under
/// `grip` (read it back with `Context::read_response`). The section starts at its hairline,
/// so a drop line for reordering belongs at its top or bottom edge.
pub fn card_full<R>(
    ui: &mut egui::Ui,
    title: &str,
    grip: Option<egui::Id>,
    header_right: impl FnOnce(&mut egui::Ui),
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let top = ui.cursor().top();
    ui.painter().hline(ui.max_rect().x_range(), top, Stroke::new(1.0, BORDER));
    ui.add_space(10.0);
    if !title.is_empty() {
        // Controls first from the right, then the title truncated into what is left,
        // so a long page or preset name never widens the panel.
        ui.horizontal(|ui| {
            ui.set_min_height(22.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                header_right(ui);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    if let Some(id) = grip {
                        grip_handle(ui, id);
                    }
                    ui.add(egui::Label::new(semibold(title)).truncate().selectable(false));
                });
            });
        });
        ui.add_space(4.0);
    }
    let inner = add_contents(ui);
    ui.add_space(12.0);
    inner
}

/// The `⠿` drag grip of a reorderable section.
fn grip_handle(ui: &mut egui::Ui, id: egui::Id) {
    ui.spacing_mut().item_spacing.x = 2.0;
    let (_, rect) = ui.allocate_space(egui::vec2(12.0, 18.0));
    let response = ui.interact(rect, id, egui::Sense::drag()).on_hover_text("끌어서 순서 바꾸기");
    let color = if response.hovered() || response.dragged() { TEXT } else { TEXT_FAINT };
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        icon::DOTS_SIX_VERTICAL,
        FontId::new(13.0, FontFamily::Name(ICON_FAMILY.into())),
        color,
    );
    if response.hovered() && !response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
    }
}

/// Small frameless icon button for section headers and bars.
pub fn ghost_icon(glyph: &str) -> egui::Button<'static> {
    egui::Button::new(icon(glyph).size(15.0).color(TEXT_WEAK)).frame_when_inactive(false)
}

/// [`ghost_icon`] with its shortcut beside the icon.
pub fn ghost_icon_key(glyph: &str, key: &str) -> egui::Button<'static> {
    egui::Button::new((icon(glyph).size(15.0).color(TEXT_WEAK), key_hint(key, false)))
        .gap(2.0)
        .frame_when_inactive(false)
}

/// Small shortcut label next to a control (`mac` spelling, see [`crate::shortcuts::keys`]);
/// `on_fill` for a selected or accent-filled control.
pub fn key_hint(mac: &str, on_fill: bool) -> RichText {
    RichText::new(crate::shortcuts::keys(mac)).size(11.0).color(if on_fill { KEY_HINT_ON_FILL } else { KEY_HINT })
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
    egui::Frame::new().fill(INPUT_BG).corner_radius(7).inner_margin(Margin::same(2)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        ui.horizontal(|ui| {
            for (value, glyph, label, key, hint) in options {
                let selected = *value == current;
                let color = if selected { Color32::WHITE } else { TEXT_WEAK };
                let mut atoms = egui::Atoms::default();
                if !glyph.is_empty() {
                    atoms.push_right(icon(glyph).size(15.0).color(color));
                }
                atoms.push_right(RichText::new(*label).color(color));
                if !key.is_empty() {
                    atoms.push_right(key_hint(key, selected));
                }
                let button = egui::Button::new(atoms).corner_radius(5);
                // Unselected segments show a frame only while hovered.
                let button = if selected { button.fill(SEGMENT_ON) } else { button.frame_when_inactive(false) };
                let response = tip(ui.add(button), hint, key);
                if response.clicked() && !selected {
                    picked = Some(*value);
                }
            }
        });
    });
    picked
}

/// Tooltip with the control's name and its shortcut (`mac` spelling, see
/// [`crate::shortcuts::keys`]) on the first line and the rest of `hint` below it in
/// weaker type. Shown on disabled controls too.
pub fn tip(response: egui::Response, hint: &str, key: &str) -> egui::Response {
    if hint.is_empty() && key.is_empty() {
        return response;
    }
    response.on_hover_ui(|ui| tooltip_body(ui, hint, key)).on_disabled_hover_ui(|ui| tooltip_body(ui, hint, key))
}

fn tooltip_body(ui: &mut egui::Ui, hint: &str, key: &str) {
    let mut lines = hint.lines();
    let title = lines.next().unwrap_or("");
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        if !title.is_empty() {
            ui.label(RichText::new(title).color(TEXT));
        }
        if !key.is_empty() {
            ui.label(RichText::new(crate::shortcuts::keys(key)).color(TEXT_WEAK));
        }
    });
    for line in lines {
        ui.label(RichText::new(line).small().color(TEXT_WEAK));
    }
}

/// Toolbar toggle: an icon with its shortcut in small type beside it (the tooltip
/// carries the name).
pub fn icon_tool(ui: &mut egui::Ui, enabled: bool, active: bool, glyph: &str, key: &str, hint: &str) -> egui::Response {
    let glyph = icon(glyph).size(17.0).color(if active { Color32::WHITE } else { TEXT });
    let button = if key.is_empty() {
        egui::Button::new(glyph)
    } else {
        egui::Button::new((glyph, key_hint(key, active))).gap(2.0)
    };
    toggle(ui, enabled, active, button.min_size(egui::vec2(28.0, 28.0)), hint, key)
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
    toggle(ui, enabled, active, button, hint, key)
}

/// Tinted when active; inactive shows no frame until hovered (egui keeps the frame's
/// spacing either way, so nothing moves).
fn toggle(
    ui: &mut egui::Ui,
    enabled: bool,
    active: bool,
    button: egui::Button<'_>,
    hint: &str,
    key: &str,
) -> egui::Response {
    let button = if active { button.fill(SELECTED) } else { button.frame_when_inactive(false) };
    tip(ui.add_enabled(enabled, button), hint, key)
}

/// Accent-filled main action button.
pub fn primary(label: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(RichText::new(label.into()).color(Color32::WHITE)).fill(PRIMARY)
}

/// [`primary`] with an icon.
pub fn primary_icon(glyph: &str, label: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new((icon(glyph).color(Color32::WHITE), RichText::new(label.into()).color(Color32::WHITE)))
        .fill(PRIMARY)
}

/// Plain button with an icon, a label and its shortcut.
pub fn icon_button_key(glyph: &str, label: impl Into<String>, key: &str) -> egui::Button<'static> {
    egui::Button::new((icon(glyph), RichText::new(label.into()), key_hint(key, false)))
}

/// Plain button with an icon.
pub fn icon_button(glyph: &str, label: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(icon_text(glyph, label))
}

/// Button for destructive actions.
pub fn danger(label: impl Into<String>) -> egui::Button<'static> {
    egui::Button::new(RichText::new(label.into()).color(DANGER))
}

/// Adds `button` stretched over the available width, its label centred.
pub fn wide(ui: &mut egui::Ui, enabled: bool, button: egui::Button<'_>) -> egui::Response {
    let height = ui.spacing().interact_size.y + 2.0;
    ui.with_layout(egui::Layout::top_down_justified(egui::Align::Center), |ui| {
        ui.add_enabled(enabled, button.min_size(egui::vec2(0.0, height)))
    })
    .inner
}

/// Buttons sharing a row in equal widths.
pub fn button_row(ui: &mut egui::Ui, count: usize, add: impl FnOnce(&mut egui::Ui, egui::Vec2)) {
    let spacing = 6.0;
    let width = ((ui.available_width() - spacing * (count.max(1) - 1) as f32) / count.max(1) as f32).floor();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = spacing;
        add(ui, egui::vec2(width, ui.spacing().interact_size.y + 2.0));
    });
}

/// Small coloured dot.
pub fn status_dot(ui: &mut egui::Ui, color: Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(8.0, 16.0), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), 3.5, color);
    response
}

/// A coloured dot and a label: a state such as "번역됨" or "인페인트 전".
pub fn status(ui: &mut egui::Ui, label: &str, color: Color32) -> egui::Response {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 5.0;
        status_dot(ui, color);
        let text = if color == IDLE { TEXT_WEAK } else { TEXT };
        ui.label(RichText::new(label).small().color(text));
    })
    .response
}

/// Thin progress bar (nothing filled at 0).
pub fn progress(ui: &mut egui::Ui, ratio: f32, width: f32, color: Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 12.0), egui::Sense::hover());
    paint_progress(ui.painter(), egui::Rect::from_center_size(rect.center(), egui::vec2(width, 4.0)), ratio, color);
    response
}

/// Paints a thin progress bar into `rect`.
pub fn paint_progress(painter: &egui::Painter, rect: egui::Rect, ratio: f32, color: Color32) {
    let radius = rect.height() / 2.0;
    painter.rect_filled(rect, radius, TRACK);
    let ratio = ratio.clamp(0.0, 1.0);
    if ratio > 0.0 {
        let width = (rect.width() * ratio).max(rect.height());
        painter.rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(width, rect.height())), radius, color);
    }
}

/// Weak explanatory text.
pub fn hint(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).small().color(TEXT_WEAK));
}

/// Small weak caption above a field.
pub fn caption(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).small().color(TEXT_WEAK));
}

/// Shows `window` as a dialog: everything behind it is dimmed and blocked.
pub fn modal<R>(
    ctx: &egui::Context,
    window: egui::Window<'_>,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> Option<egui::InnerResponse<Option<R>>> {
    let screen = ctx.content_rect();
    let backdrop = egui::Area::new(egui::Id::unique("modal-backdrop"))
        .order(egui::Order::Middle)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            ui.painter().rect_filled(screen, 0.0, Color32::from_black_alpha(130));
            ui.allocate_rect(screen, egui::Sense::click_and_drag());
        });
    let response = window.show(ctx, add_contents);
    // The backdrop shares the layer order of other windows (the editor's floating
    // widgets too): put it over them, and the dialog over it.
    ctx.move_to_top(backdrop.response.layer_id);
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

    /// Width a section takes in a `width`-wide panel with a long title.
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
