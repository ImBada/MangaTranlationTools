//! Colour input like the TS `FastColorPicker`: a swatch opening a popover with a
//! saturation/brightness plane and hue bar, a hex field, eight quick colours and "스포이드". The eyedropper samples the app
//! window (a viewport screenshot) at the next click, which is also how the result layer's
//! colour picker reads the final output.

use std::sync::{Arc, Mutex};

use eframe::egui::{self, Color32, ColorImage, Pos2, ecolor::Hsva};

use super::Editor;
use crate::theme;

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

const PICKER_WIDTH: f32 = 232.0;
const PLANE_HEIGHT: f32 = 156.0;
const HUE_HEIGHT: f32 = 12.0;
const THUMB_RADIUS: f32 = 7.0;

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
    let (rect, button) = ui.allocate_exact_size(egui::vec2(36.0, 20.0), egui::Sense::click());
    let border = if button.hovered() { theme::TEXT_FAINT } else { theme::BORDER };
    ui.painter().rect(rect, 4.0, swatch, egui::Stroke::new(1.0, border), egui::StrokeKind::Inside);
    egui::Popup::from_toggle_button_response(&button)
        .id(id)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .gap(6.0)
        .frame(egui::Frame::popup(ui.style()).inner_margin(12.0).corner_radius(10.0))
        .show(|ui| {
            ui.set_width(PICKER_WIDTH);
            ui.spacing_mut().item_spacing = egui::vec2(8.0, 10.0);
            // HSV survives round trips through hex, so gray and black keep their hue.
            let hsv_id = id.with("hsv");
            let mut hsv = ui
                .data(|data| data.get_temp::<(String, Hsva)>(hsv_id))
                .filter(|(stored, _)| stored == hex)
                .map_or_else(|| Hsva::from_srgb(rgb), |(_, hsv)| hsv);
            let before = hsv;
            saturation_value_plane(ui, &mut hsv);
            hue_bar(ui, &mut hsv);
            if hsv != before {
                let next = to_hex(hsv.to_srgb());
                ui.data_mut(|data| data.insert_temp(hsv_id, (next.clone(), hsv)));
                changed = Some(next);
            }

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                let shown = changed.as_deref().and_then(parse_hex).unwrap_or(rgb);
                let (chip, _) = ui.allocate_exact_size(egui::vec2(28.0, 28.0), egui::Sense::hover());
                ui.painter().rect(
                    chip,
                    6.0,
                    Color32::from_rgb(shown[0], shown[1], shown[2]),
                    egui::Stroke::new(1.0, theme::BORDER),
                    egui::StrokeKind::Inside,
                );
                let draft_id = id.with("hex");
                let mut draft: String = ui.data_mut(|data| data.get_temp(draft_id)).unwrap_or_else(|| hex.to_owned());
                let field = egui::TextEdit::singleline(&mut draft)
                    .font(egui::TextStyle::Monospace)
                    .align(egui::Align2::LEFT_CENTER)
                    .min_size(egui::vec2(0.0, 28.0))
                    .desired_width(PICKER_WIDTH - 28.0 - 28.0 - 12.0 - 8.0);
                let response = ui.add(field);
                if response.lost_focus() || (response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    // `HEX_COLOR_PATTERN`: only complete #rrggbb values apply.
                    let text = draft.trim().to_lowercase();
                    let text = if text.starts_with('#') { text } else { format!("#{text}") };
                    if text.len() == 7 && parse_hex(&text).is_some() {
                        changed = Some(text);
                    }
                    ui.data_mut(|data| data.remove::<String>(draft_id));
                } else if response.has_focus() {
                    ui.data_mut(|data| data.insert_temp(draft_id, draft));
                } else {
                    ui.data_mut(|data| data.remove::<String>(draft_id));
                }
                let dropper =
                    ui.add(egui::Button::new(theme::icon(theme::icon::EYEDROPPER)).min_size(egui::vec2(28.0, 28.0)));
                if theme::tip(dropper, "스포이드\n화면에서 색상 가져오기", "").clicked() {
                    editor.arm_eyedropper(SampleTarget::Field(key.to_owned()));
                    egui::Popup::close_id(ui.ctx(), id);
                }
            });

            let current = changed.clone().unwrap_or_else(|| hex.to_lowercase());
            let size = 22.0;
            let gap = (PICKER_WIDTH - size * QUICK_COLORS.len() as f32) / (QUICK_COLORS.len() - 1) as f32;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for quick in QUICK_COLORS {
                    let rgb = parse_hex(quick).unwrap_or_default();
                    let (rect, response) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::click());
                    let painter = ui.painter();
                    painter.rect(
                        rect,
                        size / 2.0,
                        Color32::from_rgb(rgb[0], rgb[1], rgb[2]),
                        egui::Stroke::new(1.0, theme::BORDER),
                        egui::StrokeKind::Inside,
                    );
                    if current == quick {
                        painter.circle_stroke(rect.center(), size / 2.0 + 2.5, egui::Stroke::new(1.5, theme::ACCENT));
                    } else if response.hovered() {
                        painter.circle_stroke(
                            rect.center(),
                            size / 2.0 + 2.5,
                            egui::Stroke::new(1.0, theme::TEXT_FAINT),
                        );
                    }
                    if response.on_hover_text(quick).clicked() {
                        changed = Some(quick.to_owned());
                    }
                }
            });
        });
    changed.filter(|next| next != hex)
}

/// A round handle filled with `fill`, readable on any colour.
fn thumb(painter: &egui::Painter, center: Pos2, fill: Color32) {
    painter.circle_filled(center, THUMB_RADIUS + 1.0, Color32::from_black_alpha(90));
    painter.circle(center, THUMB_RADIUS, fill, egui::Stroke::new(2.0, Color32::WHITE));
}

/// Cached gradient texture under `key`, rebuilt when `version` changes.
fn gradient(
    ui: &egui::Ui,
    key: &str,
    version: u32,
    size: [usize; 2],
    pixel: impl Fn(f32, f32) -> Color32,
) -> egui::TextureHandle {
    let id = egui::Id::unique(("color-gradient", key));
    if let Some((cached, texture)) = ui.data(|data| data.get_temp::<(u32, egui::TextureHandle)>(id))
        && cached == version
    {
        return texture;
    }
    let mut image = ColorImage::filled(size, Color32::BLACK);
    for y in 0..size[1] {
        for x in 0..size[0] {
            let (u, v) = (x as f32 / (size[0] - 1).max(1) as f32, y as f32 / (size[1] - 1).max(1) as f32);
            image.pixels[y * size[0] + x] = pixel(u, v);
        }
    }
    let texture = ui.ctx().load_texture(key, image, egui::TextureOptions::LINEAR);
    ui.data_mut(|data| data.insert_temp(id, (version, texture.clone())));
    texture
}

fn hsv_color(h: f32, s: f32, v: f32) -> Color32 {
    let [r, g, b] = Hsva::new(h, s, v, 1.0).to_srgb();
    Color32::from_rgb(r, g, b)
}

/// Saturation left to right, brightness bottom to top, for the current hue.
fn saturation_value_plane(ui: &mut egui::Ui, hsv: &mut Hsva) {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(PICKER_WIDTH, PLANE_HEIGHT), egui::Sense::click_and_drag());
    let hue = hsv.h;
    let texture = gradient(ui, "sv-plane", (hue * 1000.0) as u32, [64, 64], |u, v| hsv_color(hue, u, 1.0 - v));
    egui::Image::new((texture.id(), rect.size())).corner_radius(8.0).paint_at(ui, rect);
    ui.painter().rect_stroke(
        rect,
        8.0,
        egui::Stroke::new(1.0, Color32::from_white_alpha(18)),
        egui::StrokeKind::Inside,
    );
    if let Some(pointer) = response.interact_pointer_pos() {
        hsv.s = ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
        hsv.v = 1.0 - ((pointer.y - rect.top()) / rect.height()).clamp(0.0, 1.0);
    }
    let inner = rect.shrink(THUMB_RADIUS * 0.5);
    let center = Pos2::new(inner.left() + hsv.s * inner.width(), inner.top() + (1.0 - hsv.v) * inner.height());
    thumb(ui.painter(), center, hsv_color(hsv.h, hsv.s, hsv.v));
}

fn hue_bar(ui: &mut egui::Ui, hsv: &mut Hsva) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(PICKER_WIDTH, HUE_HEIGHT), egui::Sense::click_and_drag());
    let texture = gradient(ui, "hue-bar", 0, [256, 1], |u, _| hsv_color(u, 1.0, 1.0));
    egui::Image::new((texture.id(), rect.size())).corner_radius(HUE_HEIGHT / 2.0).paint_at(ui, rect);
    if let Some(pointer) = response.interact_pointer_pos() {
        hsv.h = ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 0.9999);
    }
    let inner = rect.shrink2(egui::vec2(HUE_HEIGHT / 2.0, 0.0));
    thumb(ui.painter(), Pos2::new(inner.left() + hsv.h * inner.width(), rect.center().y), hsv_color(hsv.h, 1.0, 1.0));
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
