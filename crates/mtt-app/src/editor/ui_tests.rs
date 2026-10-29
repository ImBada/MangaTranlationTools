//! Headless frames of the whole editor: which keys reach the editor shortcuts while text
//! fields and dialogs are open.

use super::*;

const SCREEN: egui::Vec2 = egui::vec2(1440.0, 900.0);

/// A field drawn over the canvas by the test, standing in for the inspector's number
/// fields (`DragValue`) and the colour picker's hex field (`TextEdit`), which don't set
/// `Editor::text_editing`.
enum Field {
    Number(f64),
    Text(String),
}

/// An editor on a temp data folder, driven one egui frame at a time.
struct Harness {
    ctx: egui::Context,
    editor: Editor,
    services: Services,
    worker: Worker<AppEvent>,
    time: f64,
    field: Option<Field>,
    field_rect: egui::Rect,
    _dir: tempfile::TempDir,
}

impl Harness {
    /// Two pages of `blocks` blocks each, in the translate mode.
    fn new(blocks: usize) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let ctx = egui::Context::default();
        crate::theme::install_icon_font(&ctx);
        crate::theme::apply(&ctx);
        let services = Services::new(&ctx, mtt_core::paths::AppPaths::from_data_root(dir.path()));
        let worker = Worker::new(&ctx, 1);
        let pages = (0..2)
            .map(|page| PageRecord {
                id: format!("p{page}"),
                width: 1000,
                height: 1400,
                blocks: (0..blocks)
                    .map(|index| TranslationBlock {
                        id: format!("b{index}"),
                        bbox: BBox::new(100.0, 100.0 + 200.0 * index as f64, 200.0, 100.0),
                        translated_text: format!("블록 {index}"),
                        ..TranslationBlock::default()
                    })
                    .collect(),
                ..PageRecord::default()
            })
            .collect::<Vec<_>>();
        let chapter = Chapter {
            id: "c".into(),
            work_id: "w".into(),
            page_order: pages.iter().map(|p| p.id.clone()).collect(),
            pages,
            ..Chapter::default()
        };
        // Like `Editor::open`.
        let mut editor = Editor::new(chapter);
        editor.set_layer(Layer::Overlay);
        let mut harness =
            Self { ctx, editor, services, worker, time: 0.0, field: None, field_rect: egui::Rect::NOTHING, _dir: dir };
        harness.frame(Vec::new());
        harness
    }

    /// Runs one frame with `events`.
    fn frame(&mut self, events: Vec<egui::Event>) {
        self.time += 0.25;
        let input = egui::RawInput {
            time: Some(self.time),
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, SCREEN)),
            events,
            ..Default::default()
        };
        let Self { ctx, editor, services, worker, field, field_rect, .. } = self;
        let mut output = ctx.run_ui(input, |ui| {
            editor.ui(ui, services, worker);
            if let Some(field) = field {
                let area = egui::Area::new(egui::Id::unique("test-field"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(egui::pos2(700.0, 400.0));
                *field_rect = area
                    .show(ui.ctx(), |ui| match field {
                        Field::Number(value) => ui.add(egui::DragValue::new(value)),
                        Field::Text(text) => ui.add(egui::TextEdit::singleline(text)),
                    })
                    .inner
                    .rect;
            }
        });
        output.textures_delta.clear();
    }

    fn idle(&mut self, frames: usize) {
        for _ in 0..frames {
            self.frame(Vec::new());
        }
    }

    /// Presses and releases the primary button at `pos`.
    fn click(&mut self, pos: egui::Pos2) {
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.frame(vec![egui::Event::PointerMoved(pos)]);
        self.frame(vec![button(true)]);
        self.frame(vec![button(false)]);
        self.idle(2);
    }

    fn press(&mut self, key: egui::Key, modifiers: egui::Modifiers) {
        self.frame(vec![key_event(key, true, modifiers)]);
        self.frame(vec![key_event(key, false, modifiers)]);
    }

    /// Types `text` as the platform does: a key press and its text.
    fn type_key(&mut self, key: egui::Key, text: &str) {
        self.frame(vec![key_event(key, true, egui::Modifiers::NONE), egui::Event::Text(text.into())]);
        self.frame(vec![key_event(key, false, egui::Modifiers::NONE)]);
    }

    fn block_count(&self) -> usize {
        self.editor.page().unwrap().blocks.len()
    }
}

fn key_event(key: egui::Key, pressed: bool, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers }
}

#[test]
fn number_and_colour_fields_keep_their_keys() {
    for field in [Field::Number(24.0), Field::Text("#112233".into())] {
        let mut harness = Harness::new(2);
        harness.editor.selection = vec!["b0".into()];
        harness.field = Some(field);
        harness.idle(2);
        harness.click(harness.field_rect.center());
        assert!(harness.ctx.text_edit_focused(), "the field is being typed in");

        harness.press(egui::Key::Backspace, egui::Modifiers::NONE);
        assert_eq!(harness.block_count(), 2, "Backspace deletes a character, not the block");
        harness.type_key(egui::Key::Num1, "1");
        assert_eq!(harness.editor.mode(), Mode::Translate, "1 is typed, not a mode switch");
        harness.type_key(egui::Key::F, "f");
        assert_eq!(harness.editor.current, 0, "f is typed, not the next page");
        harness.frame(vec![egui::Event::Copy]);
        harness.frame(vec![egui::Event::Paste("18".into())]);
        assert_eq!(harness.block_count(), 2, "⌘C/⌘V copy text, not the block");

        // Once the field is done, the keys are the editor's again.
        harness.press(egui::Key::Enter, egui::Modifiers::NONE);
        harness.idle(2);
        assert!(!harness.ctx.text_edit_focused());
        harness.press(egui::Key::Backspace, egui::Modifiers::NONE);
        assert_eq!(harness.block_count(), 1, "Backspace deletes the block again");
    }
}
