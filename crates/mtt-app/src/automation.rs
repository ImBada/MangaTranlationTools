//! Scripted UI automation for self-tests and screenshots.
//!
//! `MTT_AUTOMATION="open:first;ready;layer:2;select:first;ready;shot:/tmp/a.png;quit"`
//! Steps run in order; `ready` waits until the current page and its overlay are rendered.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, atomic::AtomicBool, atomic::Ordering};

use eframe::egui;

use crate::editor::Editor;

pub enum Step {
    OpenFirst,
    Page(usize),
    Layer(u8),
    SelectFirst,
    SelectAll,
    Text(String),
    Zoom(f32),
    Ready,
    Wait(u32),
    Screenshot(PathBuf),
    Export,
    Undo,
    Paint([f32; 4], bool),
    Range([f32; 4]),
    Inpaint(crate::editor::InpaintScope),
    ImportFolder(PathBuf),
    WaitEditor,
    Block([f32; 4], String),
    Group,
    GroupShadow,
    WaitInpaint,
    /// `settings:inpaint` opens it on the 인페인트 tab.
    Settings(bool),
    Report,
    WaitReport,
    PresetEdit(String),
    /// Opens the shortcut sheet.
    ShortcutSheet,
    /// Presses (`down`) or releases (`up`) Space as a real key event.
    Space(bool),
    /// `float:<widget>` pops a widget out as a window, `dock:<widget>` puts it back.
    Widget(crate::editor::widgets::Widget, bool),
    /// `card-drag:<card>><target>` drags an inspector card's grip onto the top of another.
    CardDrag(crate::editor::widgets::Card, crate::editor::widgets::Card),
    Sample([f32; 2]),
    /// Moves the pointer to a screen point (tooltips, hover states).
    Hover([f32; 2]),
    /// Clicks the primary button at a screen point.
    Click([f32; 2]),
    /// Translates the current page.
    Translate,
    WaitTranslation,
    Window([f32; 2]),
    InpaintAll,
    WaitSample,
    Quit,
}

pub struct Automation {
    steps: VecDeque<Step>,
    wait_frames: u32,
    screenshot_done: Option<Arc<AtomicBool>>,
    /// Input fed to egui by `MttApp::raw_input_hook`, one batch per frame.
    injected: VecDeque<Vec<egui::Event>>,
}

pub enum Request {
    OpenFirstChapter,
    ImportFolder(PathBuf),
    OpenSettings { inpaint: bool },
}

impl Automation {
    pub fn from_env() -> Option<Self> {
        let script = std::env::var("MTT_AUTOMATION").ok()?;
        let steps = script
            .split(';')
            .filter_map(|raw| {
                let raw = raw.trim();
                let (name, arg) = raw.split_once(':').unwrap_or((raw, ""));
                Some(match name {
                    "open" => Step::OpenFirst,
                    "page" => Step::Page(arg.parse::<usize>().ok()?.saturating_sub(1)),
                    "layer" => Step::Layer(arg.parse().ok()?),
                    "select" if arg == "all" => Step::SelectAll,
                    "select" => Step::SelectFirst,
                    "text" => Step::Text(arg.replace("\\n", "\n")),
                    "zoom" => Step::Zoom(arg.parse().ok()?),
                    "ready" => Step::Ready,
                    "wait" => Step::Wait(arg.parse().unwrap_or(10)),
                    "shot" => Step::Screenshot(PathBuf::from(arg)),
                    "export" => Step::Export,
                    "undo" => Step::Undo,
                    "range" => {
                        let v: Vec<f32> = arg.split(',').filter_map(|n| n.trim().parse().ok()).collect();
                        (v.len() == 4).then(|| Step::Range([v[0], v[1], v[2], v[3]]))?
                    }
                    "inpaint" => Step::Inpaint(match arg {
                        "range" => crate::editor::InpaintScope::Range,
                        "blocks" => crate::editor::InpaintScope::SelectedBlocks,
                        _ => crate::editor::InpaintScope::WholeMask,
                    }),
                    "inpaint-wait" => Step::WaitInpaint,
                    "import" => Step::ImportFolder(PathBuf::from(arg)),
                    "wait-editor" => Step::WaitEditor,
                    "group" => Step::Group,
                    "group-shadow" => Step::GroupShadow,
                    "block" => {
                        let mut parts = arg.splitn(5, ',');
                        let numbers: Vec<f32> = parts.by_ref().take(4).filter_map(|n| n.trim().parse().ok()).collect();
                        let text = parts.next().unwrap_or("").replace("\\n", "\n");
                        (numbers.len() == 4)
                            .then(|| Step::Block([numbers[0], numbers[1], numbers[2], numbers[3]], text))?
                    }
                    "paint" | "erase" => {
                        let v: Vec<f32> = arg.split(',').filter_map(|n| n.trim().parse().ok()).collect();
                        (v.len() == 4).then(|| Step::Paint([v[0], v[1], v[2], v[3]], name == "erase"))?
                    }
                    "settings" => Step::Settings(arg == "inpaint"),
                    "report" => Step::Report,
                    "report-wait" => Step::WaitReport,
                    "preset-edit" => Step::PresetEdit(arg.to_owned()),
                    "shortcuts" => Step::ShortcutSheet,
                    "space" => Step::Space(arg == "down"),
                    "float" | "dock" => Step::Widget(crate::editor::widgets::Widget::from_key(arg)?, name == "float"),
                    "card-drag" => {
                        let (card, target) = arg.split_once('>')?;
                        let parse = crate::editor::widgets::Card::from_key;
                        Step::CardDrag(parse(card)?, parse(target)?)
                    }
                    "sample" => {
                        let v: Vec<f32> = arg.split(',').filter_map(|n| n.trim().parse().ok()).collect();
                        (v.len() == 2).then(|| Step::Sample([v[0], v[1]]))?
                    }
                    "sample-wait" => Step::WaitSample,
                    "hover" => {
                        let v: Vec<f32> = arg.split(',').filter_map(|n| n.trim().parse().ok()).collect();
                        (v.len() == 2).then(|| Step::Hover([v[0], v[1]]))?
                    }
                    "click" => {
                        let v: Vec<f32> = arg.split(',').filter_map(|n| n.trim().parse().ok()).collect();
                        (v.len() == 2).then(|| Step::Click([v[0], v[1]]))?
                    }
                    "translate" => Step::Translate,
                    "translate-wait" => Step::WaitTranslation,
                    "window" => {
                        let v: Vec<f32> = arg.split(',').filter_map(|n| n.trim().parse().ok()).collect();
                        (v.len() == 2).then(|| Step::Window([v[0], v[1]]))?
                    }
                    "inpaint-all" => Step::InpaintAll,
                    "quit" => Step::Quit,
                    "" => return None,
                    other => {
                        log::warn!("unknown automation step {other}");
                        return None;
                    }
                })
            })
            .collect();
        Some(Self { steps, wait_frames: 2, screenshot_done: None, injected: VecDeque::new() })
    }

    pub fn take_injected_events(&mut self) -> Vec<egui::Event> {
        self.injected.pop_front().unwrap_or_default()
    }

    /// Runs at most one step per frame. Returns a request the app must perform.
    pub fn tick(
        &mut self,
        ctx: &egui::Context,
        editor: Option<&mut Editor>,
        services: &mut crate::services::Services,
        worker: Option<&crate::worker::Worker<crate::app::AppEvent>>,
    ) -> Option<Request> {
        ctx.request_repaint();
        if let Some(done) = &self.screenshot_done {
            if !done.load(Ordering::SeqCst) {
                return None;
            }
            self.screenshot_done = None;
        }
        if self.wait_frames > 0 {
            self.wait_frames -= 1;
            return None;
        }
        let step = self.steps.front()?;
        match (step, editor) {
            (Step::ImportFolder(path), None) => {
                let path = path.clone();
                self.steps.pop_front();
                return Some(Request::ImportFolder(path));
            }
            (Step::ImportFolder(_), Some(_)) => log::warn!("automation: import only runs on the library screen"),
            (Step::Settings(inpaint), _) => {
                let inpaint = *inpaint;
                self.steps.pop_front();
                return Some(Request::OpenSettings { inpaint });
            }
            (Step::WaitEditor, None) => return None,
            (Step::WaitEditor, Some(_)) => {}
            (Step::Group, Some(editor)) => editor.group_selected(),
            (Step::GroupShadow, Some(editor)) => {
                let settings = mtt_core::groups::DropShadowSettings {
                    distance_px: 8.0,
                    blur_px: 3.0,
                    color: "#3355ff".into(),
                    ..Default::default()
                };
                editor.set_group_drop_shadow(true, &settings);
            }
            (Step::Block(rect, text), Some(editor)) => {
                editor.create_block(egui::Rect::from_min_size(
                    egui::pos2(rect[0], rect[1]),
                    egui::vec2(rect[2], rect[3]),
                ));
                let text = text.clone();
                editor.edit_selected(|block| block.translated_text = text.clone());
            }
            (Step::OpenFirst, _) => {
                self.steps.pop_front();
                self.wait_frames = 2;
                return Some(Request::OpenFirstChapter);
            }
            (Step::Ready, Some(editor)) => {
                if !editor.is_ready() {
                    return None;
                }
                self.wait_frames = 3;
            }
            (Step::Ready, None) => return None,
            (Step::Page(index), Some(editor)) => editor.go_to_page(*index),
            (Step::Widget(widget, floating), Some(editor)) => editor.widgets.set_floating(*widget, *floating, None),
            (Step::Layer(layer), Some(editor)) => editor.select_layer_shortcut(*layer),
            (Step::SelectFirst, Some(editor)) => {
                editor.selection = editor
                    .page()
                    .and_then(|page| page.blocks.first())
                    .map(|block| vec![block.id.clone()])
                    .unwrap_or_default();
            }
            (Step::SelectAll, Some(editor)) => editor.select_all(),
            (Step::Text(text), Some(editor)) => {
                let text = text.clone();
                editor.edit_selected(|block| block.translated_text = text.clone());
            }
            (Step::Zoom(factor), Some(editor)) => editor.zoom_relative(*factor),
            (Step::Export, Some(editor)) => {
                editor.status = match editor.export_current(services) {
                    Ok(path) => format!("출력 완료: {}", path.display()),
                    Err(error) => format!("출력 실패: {error}"),
                };
                log::info!("automation export: {}", editor.status);
            }
            (Step::Undo, Some(editor)) => editor.undo(ctx),
            (Step::Range(v), Some(editor)) => {
                editor.range = Some(egui::Rect::from_min_size(egui::pos2(v[0], v[1]), egui::vec2(v[2], v[3])))
            }
            (Step::Inpaint(scope), Some(editor)) => {
                if let Some(worker) = worker {
                    editor.start_inpaint(*scope, worker);
                }
            }
            (Step::Report, Some(editor)) => {
                if let Some(worker) = worker {
                    editor.start_report(services, worker);
                }
            }
            (Step::WaitReport, Some(editor)) => {
                if editor.report_running() {
                    return None;
                }
                log::info!("automation: {} ({:?})", editor.status, editor.last_report);
            }
            (Step::Sample(pos), Some(editor)) => {
                editor.sample_screen(crate::editor::color::SampleTarget::ResultBrush, egui::pos2(pos[0], pos[1]));
            }
            (Step::Translate, Some(editor)) => match worker {
                Some(worker) => {
                    editor.start_translation(crate::editor::translation::RunMode::CurrentPage, services, worker)
                }
                None => log::warn!("automation: no worker to translate with"),
            },
            (Step::WaitTranslation, Some(editor)) => {
                if editor.translation_running() {
                    return None;
                }
                log::info!("automation: translation done: {}", editor.status);
            }
            (Step::InpaintAll, Some(editor)) => editor.start_inpaint_all_pages(),
            (Step::WaitSample, Some(editor)) => {
                if editor.eyedropper.is_some() {
                    return None;
                }
                log::info!("automation: sampled {}", crate::editor::color::to_hex(editor.result_brush.color));
            }
            (Step::ShortcutSheet, Some(editor)) => editor.shortcut_sheet = true,
            (Step::PresetEdit(id), Some(editor)) => {
                editor.selection.clear();
                editor.inspector_tab = crate::editor::InspectorTab::Presets;
                editor.editing_preset_id = Some(id.clone());
            }
            (Step::WaitInpaint, Some(editor)) => {
                if editor.inpaint_busy() {
                    return None;
                }
                log::info!("automation: {}", editor.status);
            }
            (Step::Paint(points, erase), Some(editor)) => {
                editor.paint_stroke(ctx, (points[0], points[1]), (points[2], points[3]), *erase)
            }
            (Step::Window(size), _) => {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(size[0], size[1])));
                self.wait_frames = 10;
            }
            (Step::Wait(frames), _) => self.wait_frames = *frames,
            (Step::Hover(pos), _) => {
                self.injected.push_back(vec![egui::Event::PointerMoved(egui::pos2(pos[0], pos[1]))]);
                self.wait_frames = 3;
            }
            (Step::Click(pos), _) => {
                let pos = egui::pos2(pos[0], pos[1]);
                let button = |pressed| egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                };
                // Move, press and release on separate frames, like a real click.
                self.injected.extend([vec![egui::Event::PointerMoved(pos)], vec![button(true)], vec![button(false)]]);
                self.wait_frames = 5;
            }
            (Step::Space(pressed), _) => {
                self.injected.push_back(vec![egui::Event::Key {
                    key: egui::Key::Space,
                    physical_key: Some(egui::Key::Space),
                    pressed: *pressed,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }]);
                self.wait_frames = 3;
            }
            (Step::CardDrag(card, target), Some(_)) => {
                let grip =
                    |card: crate::editor::widgets::Card| ctx.read_response(card.grip_id()).map(|r| r.rect.center());
                match (grip(*card), grip(*target)) {
                    (Some(from), Some(to)) => {
                        // Press on the grip, move past the drag threshold in steps, release
                        // in the top half of the target card.
                        let to = to - egui::vec2(0.0, 12.0);
                        let button = |pos, pressed| egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        };
                        self.injected.push_back(vec![egui::Event::PointerMoved(from)]);
                        self.injected.push_back(vec![button(from, true)]);
                        for t in [0.1, 0.4, 0.7, 1.0, 1.0] {
                            self.injected.push_back(vec![egui::Event::PointerMoved(from.lerp(to, t))]);
                        }
                        self.injected.push_back(vec![button(to, false)]);
                        self.wait_frames = self.injected.len() as u32 + 3;
                    }
                    _ => log::warn!("automation: card grip not on screen"),
                }
            }
            (Step::Screenshot(path), _) => {
                let path = path.clone();
                let done = Arc::new(AtomicBool::new(false));
                let flag = Arc::clone(&done);
                ctx.request_screenshot(move |image| {
                    let pixels: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
                    match image::RgbaImage::from_raw(image.size[0] as u32, image.size[1] as u32, pixels) {
                        Some(shot) => match shot.save(&path) {
                            Ok(()) => log::info!("screenshot saved to {}", path.display()),
                            Err(error) => log::error!("screenshot save failed: {error}"),
                        },
                        None => log::error!("screenshot buffer size mismatch"),
                    }
                    flag.store(true, Ordering::SeqCst);
                });
                self.screenshot_done = Some(done);
            }
            (Step::Quit, _) => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            (_, None) => return None,
        }
        self.steps.pop_front();
        None
    }
}
