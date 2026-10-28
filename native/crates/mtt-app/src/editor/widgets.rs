//! Widgets: parts of the editor that sit in a side panel or float as their own window over
//! the canvas — the page list, the text block list, the selected block, the preset palette
//! and "이 페이지".
//! The button in a widget's header moves it between the two; where each one is and where
//! its window was are kept in `native-ui-state.json`.

use std::collections::{BTreeMap, HashMap, HashSet};

use eframe::egui::{self, Pos2, Rect, RichText, Vec2};

use super::{Editor, Mode};
use crate::app::AppEvent;
use crate::services::{SavedWidget, Services};
use crate::theme::{self, icon};
use crate::worker::Worker;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Widget {
    Pages,
    Blocks,
    /// The selected block's text and actions.
    Block,
    Presets,
    Page,
}

impl Widget {
    pub const ALL: [Widget; 5] = [Widget::Pages, Widget::Blocks, Widget::Block, Widget::Presets, Widget::Page];

    pub fn label(self) -> &'static str {
        match self {
            Widget::Pages => "페이지",
            Widget::Blocks => "텍스트 블록",
            Widget::Block => "블록",
            Widget::Presets => "프리셋",
            Widget::Page => "이 페이지",
        }
    }

    pub fn from_key(key: &str) -> Option<Widget> {
        Widget::ALL.into_iter().find(|widget| widget.key() == key)
    }

    fn key(self) -> &'static str {
        match self {
            Widget::Pages => "pages",
            Widget::Blocks => "blocks",
            Widget::Block => "block",
            Widget::Presets => "presets",
            Widget::Page => "page",
        }
    }

    /// The block widgets (list, selected block, presets) start out as their own windows.
    fn floats_by_default(self) -> bool {
        matches!(self, Widget::Blocks | Widget::Block | Widget::Presets)
    }

    /// Modes a floating widget shows in; the block widgets only work on blocks.
    pub fn shown_in(self, mode: Mode) -> bool {
        match self {
            Widget::Blocks | Widget::Block | Widget::Presets => mode == Mode::Translate,
            Widget::Pages | Widget::Page => true,
        }
    }

    fn default_size(self) -> Vec2 {
        match self {
            Widget::Pages => egui::vec2(250.0, 520.0),
            Widget::Blocks => egui::vec2(300.0, 220.0),
            Widget::Block => egui::vec2(300.0, 240.0),
            Widget::Presets => egui::vec2(300.0, 150.0),
            Widget::Page => egui::vec2(320.0, 110.0),
        }
    }

    /// Lists can be made taller; the others are as tall as their contents.
    fn resizable(self) -> [bool; 2] {
        match self {
            Widget::Pages | Widget::Blocks => [true, true],
            Widget::Block | Widget::Presets | Widget::Page => [true, false],
        }
    }

    /// First window place when it has none yet: the page list on the canvas's left, the
    /// block widgets stacked on its right (list, block, presets).
    fn default_rect(self, stage: Rect) -> Rect {
        let size = self.default_size();
        let stacked = |above: &[Widget]| {
            let top = stage.top() + 48.0 + above.iter().map(|widget| widget.default_size().y + 12.0).sum::<f32>();
            egui::pos2(stage.right() - size.x - 12.0, top)
        };
        let min = match self {
            Widget::Pages => stage.left_top() + egui::vec2(12.0, 12.0),
            Widget::Blocks => stacked(&[]),
            Widget::Block => stacked(&[Widget::Blocks]),
            Widget::Presets => stacked(&[Widget::Blocks, Widget::Block]),
            Widget::Page => egui::pos2(stage.left() + 12.0, stage.bottom() - size.y - 60.0),
        };
        Rect::from_min_size(min, size)
    }
}

/// The selection's cards in the 번역 inspector, in an order the user can drag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Card {
    Block,
    Presets,
    Placement,
    Style,
}

impl Card {
    pub const ALL: [Card; 4] = [Card::Block, Card::Presets, Card::Placement, Card::Style];

    pub fn label(self) -> &'static str {
        match self {
            Card::Block => "블록",
            Card::Presets => "프리셋",
            Card::Placement => "배치",
            Card::Style => "스타일",
        }
    }

    pub fn from_key(key: &str) -> Option<Card> {
        Card::ALL.into_iter().find(|card| card.key() == key)
    }

    fn key(self) -> &'static str {
        match self {
            Card::Block => "block",
            Card::Presets => "presets",
            Card::Placement => "placement",
            Card::Style => "style",
        }
    }

    /// Id of the card's `⠿` grip (`theme::card_full`).
    pub fn grip_id(self) -> egui::Id {
        egui::Id::unique(("inspector-card-grip", self.key()))
    }
}

/// Drag payload of a card grip.
#[derive(Debug, Clone, Copy)]
pub struct CardDrag(pub Card);

/// Where each widget is. Loaded from the UI state on the editor's first frame.
pub struct Layout {
    loaded: bool,
    cards: Vec<Card>,
    floating: HashSet<Widget>,
    rects: HashMap<Widget, Rect>,
    /// Windows to place at their rect on the next frame (just popped out).
    place: HashSet<Widget>,
    dirty: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            loaded: false,
            cards: Card::ALL.to_vec(),
            floating: HashSet::new(),
            rects: HashMap::new(),
            place: HashSet::new(),
            dirty: false,
        }
    }
}

impl Layout {
    pub fn load(&mut self, saved: &BTreeMap<String, SavedWidget>, order: &[String]) {
        if self.loaded {
            return;
        }
        self.loaded = true;
        // Saved order first, then any card it doesn't know yet.
        let mut cards: Vec<Card> =
            order.iter().filter_map(|key| Card::ALL.into_iter().find(|card| card.key() == key)).collect();
        cards.dedup();
        for card in Card::ALL {
            if !cards.contains(&card) {
                cards.push(card);
            }
        }
        self.cards = cards;
        for widget in Widget::ALL {
            let entry = saved.get(widget.key());
            if entry.map_or(widget.floats_by_default(), |entry| entry.floating) {
                self.floating.insert(widget);
            }
            if let Some([x, y, w, h]) = entry.and_then(|entry| entry.rect) {
                self.rects.insert(widget, Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, h)));
            }
        }
    }

    pub fn floating(&self, widget: Widget) -> bool {
        self.floating.contains(&widget)
    }

    pub fn cards(&self) -> &[Card] {
        &self.cards
    }

    /// Puts `card` just before or after `target`.
    pub fn move_card(&mut self, card: Card, target: Card, after: bool) {
        if card == target {
            return;
        }
        self.cards.retain(|other| *other != card);
        let index = self.cards.iter().position(|other| *other == target).unwrap_or(self.cards.len());
        self.cards.insert(if after { index + 1 } else { index }, card);
        self.dirty = true;
    }

    /// Pops `widget` out (opening where its card was, when known) or docks it again.
    pub fn set_floating(&mut self, widget: Widget, floating: bool, from: Option<Pos2>) {
        if floating {
            if let Some(from) = from {
                let size = self.rects.get(&widget).map_or(widget.default_size(), |rect| rect.size());
                self.rects.insert(widget, Rect::from_min_size(from, size));
            }
            self.floating.insert(widget);
            self.place.insert(widget);
        } else {
            self.floating.remove(&widget);
        }
        self.dirty = true;
    }

    /// Writes the layout into the UI state when it changed; returns whether it did.
    pub fn store(&mut self, saved: &mut BTreeMap<String, SavedWidget>, order: &mut Vec<String>) -> bool {
        if !std::mem::take(&mut self.dirty) {
            return false;
        }
        *order = self.cards.iter().map(|card| card.key().to_owned()).collect();
        for widget in Widget::ALL {
            let rect = self.rects.get(&widget).map(|rect| [rect.min.x, rect.min.y, rect.width(), rect.height()]);
            saved.insert(widget.key().to_owned(), SavedWidget { floating: self.floating(widget), rect });
        }
        true
    }

    /// Remembers the window's place once the user lets go of it.
    fn track(&mut self, widget: Widget, rect: Rect, pointer_down: bool) {
        let known = self.rects.get(&widget).copied();
        let moved = known
            .is_none_or(|known| (known.min - rect.min).length() > 0.5 || (known.size() - rect.size()).length() > 0.5);
        if moved && !pointer_down {
            self.rects.insert(widget, rect);
            self.dirty = true;
        }
    }
}

/// Where a widget is being drawn.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Host {
    Panel,
    Window,
}

/// A widget's header and body: a card with a pop-out button in its panel, or the body of
/// its window with a button that puts it back. The header shows `title` with
/// `header_right` next to the button; returns the contents' result and whether the
/// button was pressed, with where the widget was.
pub fn frame<R>(
    ui: &mut egui::Ui,
    host: Host,
    widget: Widget,
    title: &str,
    header_right: impl FnOnce(&mut egui::Ui),
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> (R, Option<Pos2>) {
    let top_left = ui.cursor().min;
    let mut toggle = false;
    // The page list fills its side panel; the rest are cards, and in the inspector the
    // block and preset cards can be dragged into another order.
    let inner = if host == Host::Panel && widget != Widget::Pages {
        let grip = match widget {
            Widget::Block => Some(Card::Block.grip_id()),
            Widget::Presets => Some(Card::Presets.grip_id()),
            _ => None,
        };
        theme::card_full(
            ui,
            title,
            grip,
            |ui| {
                toggle = toggle_button(ui, host);
                header_right(ui);
            },
            contents,
        )
    } else {
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                toggle = toggle_button(ui, host);
                header_right(ui);
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    if host == Host::Window {
                        // Drag anywhere that isn't a control to move the window.
                        ui.add(
                            egui::Label::new(theme::icon(icon::DOTS_SIX_VERTICAL).color(theme::TEXT_WEAK))
                                .selectable(false),
                        );
                    }
                    ui.add(
                        egui::Label::new(RichText::new(title).strong().color(theme::TEXT)).truncate().selectable(false),
                    );
                });
            });
        });
        ui.add_space(2.0);
        contents(ui)
    };
    (inner, toggle.then_some(top_left))
}

fn toggle_button(ui: &mut egui::Ui, host: Host) -> bool {
    let (glyph, tip) = match host {
        Host::Panel => (icon::ARROW_SQUARE_OUT, "위젯으로 빼기: 따로 창으로 띄워 원하는 곳에 둡니다"),
        Host::Window => (icon::ARROW_SQUARE_IN, "패널에 다시 붙이기"),
    };
    ui.add(egui::Button::new(theme::icon(glyph).size(15.0).color(theme::TEXT_WEAK)).frame_when_inactive(false))
        .on_hover_text(tip)
        .clicked()
}

/// Applies a header button press from `frame`.
pub fn toggled(editor: &mut Editor, widget: Widget, host: Host, pressed: Option<Pos2>) {
    if let Some(from) = pressed {
        editor.widgets.set_floating(widget, host == Host::Panel, Some(from));
    }
}

/// Every floating widget of the current mode as its own window.
pub fn floating_windows(editor: &mut Editor, ctx: &egui::Context, services: &mut Services, worker: &Worker<AppEvent>) {
    let stage = editor.view.stage_rect();
    if !stage.is_positive() {
        return;
    }
    let mode = editor.mode();
    let pointer_down = ctx.input(|input| input.pointer.any_down());
    for widget in Widget::ALL {
        if !editor.widgets.floating(widget) || !widget.shown_in(mode) {
            continue;
        }
        let rect = editor.widgets.rects.get(&widget).copied().unwrap_or_else(|| widget.default_rect(stage));
        let mut window = egui::Window::new(widget.label())
            .id(egui::Id::unique(("mtt-widget", widget.key())))
            .title_bar(false)
            .resizable(widget.resizable())
            .default_rect(rect)
            .min_width(220.0)
            .min_height(80.0)
            .constrain_to(ctx.content_rect())
            .frame(window_frame(ctx));
        if editor.widgets.place.remove(&widget) {
            window = window.current_pos(rect.min);
        }
        let shown = window.show(ctx, |ui| match widget {
            Widget::Pages => super::chrome::pages_widget(editor, ui, services, worker, Host::Window),
            Widget::Blocks => super::block_list::blocks_widget(editor, ui, Host::Window),
            Widget::Block => super::inspector::block_widget(editor, ui, worker, Host::Window),
            Widget::Presets => super::inspector::presets_widget(editor, ui, services, Host::Window),
            Widget::Page => super::inspector::page_widget(editor, ui, services, worker, Host::Window),
        });
        if let Some(shown) = shown {
            editor.widgets.track(widget, shown.response.rect, pointer_down);
        }
    }
}

fn window_frame(ctx: &egui::Context) -> egui::Frame {
    egui::Frame::window(&ctx.global_style())
        .fill(theme::CARD_BG)
        .stroke(egui::Stroke::new(1.0, theme::BORDER))
        .corner_radius(8)
        .inner_margin(egui::Margin::same(10))
}

/// "보기" menu rows: each widget's window on or off.
pub fn menu(editor: &mut Editor, ui: &mut egui::Ui) {
    ui.label(RichText::new("위젯").small().color(theme::TEXT_WEAK));
    for widget in Widget::ALL {
        let mut floating = editor.widgets.floating(widget);
        if ui.checkbox(&mut floating, format!("{} 따로 띄우기", widget.label())).changed() {
            editor.widgets.set_floating(widget, floating, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_defaults_and_round_trips() {
        let mut layout = Layout::default();
        layout.load(&BTreeMap::new(), &[]);
        assert!(layout.floating(Widget::Blocks) && layout.floating(Widget::Block) && layout.floating(Widget::Presets));
        assert!(!layout.floating(Widget::Pages) && !layout.floating(Widget::Page));

        layout.set_floating(Widget::Blocks, false, None);
        layout.set_floating(Widget::Pages, true, Some(egui::pos2(10.0, 20.0)));
        layout.move_card(Card::Presets, Card::Block, false);
        layout.move_card(Card::Style, Card::Presets, true);
        assert_eq!(layout.cards(), [Card::Presets, Card::Style, Card::Block, Card::Placement]);
        let mut saved = BTreeMap::new();
        let mut order = Vec::new();
        assert!(layout.store(&mut saved, &mut order));
        assert!(!layout.store(&mut saved, &mut order), "nothing new to write");

        let mut again = Layout::default();
        again.load(&saved, &order[..2]);
        assert_eq!(again.cards(), [Card::Presets, Card::Style, Card::Block, Card::Placement], "unknown cards follow");
        assert!(!again.floating(Widget::Blocks), "docking is remembered over the default");
        assert!(again.floating(Widget::Pages));
        assert_eq!(again.rects[&Widget::Pages].min, egui::pos2(10.0, 20.0));
    }
}
