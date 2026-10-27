//! Colour input like the TS `FastColorPicker`: a swatch opening a popover with the colour
//! wheel, a hex field, eight quick colours and "스포이드". The eyedropper samples the app
//! window (a viewport screenshot) at the next click, which is also how the result layer's
//! colour picker reads the final output.

use std::sync::{Arc, Mutex};

use eframe::egui::{self, Color32, ColorImage, Pos2, RichText};

use super::Editor;

/// `QUICK_COLORS`.
const QUICK_COLORS: [&str; 8] =
    ["#111111", "#ffffff", "#ef4444", "#f59e0b", "#22c55e", "#3b82f6", "#8b5cf6", "#ec4899"];

/// Where a screenshot sample goes.
#[derive(Debug, Clone, PartialEq)]
pub enum SampleTarget {
    /// A colour field (by key); its widget reads the result.
    Field(String),
    /// The result layer brush colour (`I` colour picker).
    ResultBrush,
}

/// Filled by the screenshot callback.
#[derive(Debug, Clone, Default)]
pub struct ScreenshotSlot(Arc<Mutex<Option<Arc<ColorImage>>>>);

impl PartialEq for ScreenshotSlot {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// Eyedropper progress.
#[derive(Debug, Clone, PartialEq)]
pub enum Eyedropper {
    /// Waiting for the click that picks the colour.
    Armed(SampleTarget),
    /// Screenshot of the next frame requested at this screen point.
    Waiting {
        target: SampleTarget,
        pos: Pos2,
        frames: u8,
    },
    Requested {
        target: SampleTarget,
        pos: Pos2,
        image: ScreenshotSlot,
    },
    /// Picked colour for a field, consumed by its widget.
    Done {
        key: String,
        hex: String,
        /// Frames left for the field to pick the colour up before it is dropped.
        frames: u8,
    },
}

pub fn parse_hex(hex: &str) -> Option<[u8; 3]> {
    mtt_core::geometry::parse_hex_color(hex).map(|(r, g, b)| [r, g, b])
}

pub fn to_hex(rgb: [u8; 3]) -> String {
    format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2])
}

impl Editor {
    /// Arms the eyedropper; Esc cancels.
    pub fn arm_eyedropper(&mut self, target: SampleTarget) {
        self.eyedropper = Some(Eyedropper::Armed(target));
    }

    /// Requests a sample of the screen point on the next frame (after it is drawn).
    pub fn sample_screen(&mut self, target: SampleTarget, pos: Pos2) {
        self.eyedropper = Some(Eyedropper::Waiting { target, pos, frames: 1 });
    }

    /// Drives the eyedropper each frame: arms clicks, requests screenshots and reads them.
    pub fn eyedropper_frame(&mut self, ctx: &egui::Context) {
        match self.eyedropper.clone() {
            Some(Eyedropper::Armed(target)) => {
                ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
                if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.eyedropper = None;
                } else if let Some(pos) =
                    ctx.input(|i| i.pointer.press_origin().filter(|_| i.pointer.primary_pressed()))
                {
                    self.sample_screen(target, pos);
                }
            }
            Some(Eyedropper::Waiting { target, pos, frames }) => {
                if frames > 0 {
                    self.eyedropper = Some(Eyedropper::Waiting { target, pos, frames: frames - 1 });
                } else {
                    let slot = ScreenshotSlot::default();
                    let filled = Arc::clone(&slot.0);
                    let repaint = ctx.clone();
                    ctx.request_screenshot(move |image| {
                        if let Ok(mut target) = filled.lock() {
                            *target = Some(image);
                        }
                        repaint.request_repaint();
                    });
                    self.eyedropper = Some(Eyedropper::Requested { target, pos, image: slot });
                }
                ctx.request_repaint();
            }
            Some(Eyedropper::Requested { target, pos, image }) => {
                let image = image.0.lock().ok().and_then(|mut slot| slot.take());
                let Some(image) = image else {
                    ctx.request_repaint();
                    return;
                };
                let ppp = ctx.pixels_per_point();
                let (x, y) = ((pos.x * ppp) as usize, (pos.y * ppp) as usize);
                let color = (x < image.size[0] && y < image.size[1]).then(|| image.pixels[y * image.size[0] + x]);
                self.eyedropper = None;
                let Some(color) = color else { return };
                let hex = to_hex([color.r(), color.g(), color.b()]);
                match target {
                    SampleTarget::Field(key) => self.eyedropper = Some(Eyedropper::Done { key, hex, frames: 2 }),
                    SampleTarget::ResultBrush => {
                        self.result_brush.color = [color.r(), color.g(), color.b()];
                        self.picked_color = Some((pos, [color.r(), color.g(), color.b()]));
                    }
                }
            }
            // A field that is no longer shown (selection changed) must not get it later.
            Some(Eyedropper::Done { key, hex, frames }) => {
                self.eyedropper = (frames > 0).then_some(Eyedropper::Done { key, hex, frames: frames - 1 });
                ctx.request_repaint();
            }
            None => {}
        }
    }
}

/// The colour field. `key` identifies it for the eyedropper. Returns a new hex colour.
pub fn color_field(editor: &mut Editor, ui: &mut egui::Ui, key: &str, hex: &str) -> Option<String> {
    if let Some(Eyedropper::Done { key: done_key, hex, .. }) = &editor.eyedropper
        && done_key == key
    {
        let hex = hex.clone();
        editor.eyedropper = None;
        return Some(hex);
    }
    let rgb = parse_hex(hex).unwrap_or([0, 0, 0]);
    let swatch = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
    let id = ui.make_persistent_id(("color-field", key));
    let mut changed = None;
    let button = ui.add(egui::Button::new("").fill(swatch).min_size(egui::vec2(36.0, 18.0)));
    egui::Popup::from_toggle_button_response(&button)
        .id(id)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            ui.set_width(220.0);
            let mut color = swatch;
            if egui::color_picker::color_picker_color32(ui, &mut color, egui::color_picker::Alpha::Opaque) {
                changed = Some(to_hex([color.r(), color.g(), color.b()]));
            }
            let draft_id = id.with("hex");
            let mut draft: String = ui.data_mut(|data| data.get_temp(draft_id)).unwrap_or_else(|| hex.to_owned());
            let response = ui.add(egui::TextEdit::singleline(&mut draft).desired_width(90.0));
            if response.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                // `HEX_COLOR_PATTERN`: only complete #rrggbb values apply.
                let text = draft.trim().to_lowercase();
                let text = if text.starts_with('#') { text } else { format!("#{text}") };
                if text.len() == 7 && parse_hex(&text).is_some() {
                    changed = Some(text);
                }
                ui.data_mut(|data| data.remove::<String>(draft_id));
            } else {
                ui.data_mut(|data| data.insert_temp(draft_id, draft));
            }
            ui.horizontal(|ui| {
                for quick in QUICK_COLORS {
                    let rgb = parse_hex(quick).unwrap_or_default();
                    let fill = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                    if ui
                        .add(egui::Button::new("").fill(fill).min_size(egui::vec2(18.0, 18.0)))
                        .on_hover_text(quick)
                        .clicked()
                    {
                        changed = Some(quick.to_owned());
                    }
                }
            });
            if ui.button(RichText::new("스포이드").small()).on_hover_text("화면에서 색상 가져오기").clicked()
            {
                editor.arm_eyedropper(SampleTarget::Field(key.to_owned()));
                egui::Popup::close_id(ui.ctx(), id);
            }
        });
    changed.filter(|next| next != hex)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        assert_eq!(parse_hex("#3b82f6"), Some([0x3b, 0x82, 0xf6]));
        assert_eq!(to_hex([17, 17, 17]), "#111111");
    }
}
