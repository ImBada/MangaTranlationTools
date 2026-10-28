//! Rows that both click and drag (library chapters, pages, text blocks).
//!
//! egui's `Ui::dnd_drag_source` lays a drag-only sensor over its contents, which takes the
//! press away from a button inside it, so the row could be dragged but never clicked.
//! Here the button itself senses click *and* drag: a plain press-release is a click, and
//! moving past the drag threshold starts drag-and-drop with `payload`.

use std::any::Any;

use eframe::egui;

pub fn drag_button<Payload: Any + Send + Sync>(
    ui: &mut egui::Ui,
    button: egui::Button<'_>,
    payload: Payload,
    ghost: &str,
) -> egui::Response {
    let response = ui.add(button.sense(egui::Sense::click_and_drag()));
    drag_source(ui, response, payload, ghost)
}

/// The same for a custom-drawn row whose `response` senses click and drag.
pub fn drag_source<Payload: Any + Send + Sync>(
    ui: &egui::Ui,
    response: egui::Response,
    payload: Payload,
    ghost: &str,
) -> egui::Response {
    response.dnd_set_drag_payload(payload);
    if response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        if let Some(pointer) = ui.ctx().pointer_interact_pos() {
            egui::Area::new(response.id.with("ghost"))
                .fixed_pos(pointer + egui::vec2(12.0, 4.0))
                .order(egui::Order::Tooltip)
                .interactable(false)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        ui.label(ghost);
                    });
                });
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs one frame with `events` and returns whether the button was clicked and whether
    /// a drag payload is set.
    fn frame(ctx: &egui::Context, events: Vec<egui::Event>) -> (bool, bool) {
        let mut clicked = false;
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0))),
            events,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let response = drag_button(ui, egui::Button::new("1화"), "chapter".to_owned(), "1화");
                clicked |= response.clicked();
            });
        });
        output.textures_delta.clear();
        (clicked, egui::DragAndDrop::has_payload_of_type::<String>(ctx))
    }

    fn pointer(pos: egui::Pos2, pressed: Option<bool>) -> Vec<egui::Event> {
        let mut events = vec![egui::Event::PointerMoved(pos)];
        if let Some(pressed) = pressed {
            events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            });
        }
        events
    }

    #[test]
    fn plain_click_opens_and_drag_carries_the_payload() {
        let ctx = egui::Context::default();
        let at = egui::pos2(20.0, 15.0);
        frame(&ctx, pointer(at, None));
        frame(&ctx, pointer(at, Some(true)));
        let (clicked, dragging) = frame(&ctx, pointer(at, Some(false)));
        assert!(clicked, "press and release on the row is a click");
        assert!(!dragging);

        frame(&ctx, pointer(at, Some(true)));
        frame(&ctx, pointer(at + egui::vec2(0.0, 40.0), None));
        let (clicked, dragging) = frame(&ctx, pointer(at + egui::vec2(0.0, 80.0), None));
        assert!(!clicked);
        assert!(dragging, "moving past the threshold starts drag-and-drop");
    }
}
