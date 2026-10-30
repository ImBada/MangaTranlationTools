//! Library covers: each chapter's first page, decoded small on the worker pool, a few at a
//! time and only for what is on screen.

use std::collections::HashMap;

use eframe::egui;
use mtt_core::library::{ChapterSummary, Library};

use crate::app::AppEvent;
use crate::theme;
use crate::worker::Worker;

const MAX_IN_FLIGHT: usize = 3;

/// A chapter's cover at one size. The stored image path is part of it, so a chapter whose
/// first page changed asks for a new cover.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct CoverKey {
    chapter_id: String,
    image: String,
    pixels: u32,
}

pub struct CoverLoad {
    key: CoverKey,
    image: Result<egui::ColorImage, String>,
}

enum Cover {
    Loading,
    Ready(egui::TextureHandle),
    Failed,
}

#[derive(Default)]
pub struct Covers {
    covers: HashMap<CoverKey, Cover>,
    in_flight: usize,
}

impl Covers {
    /// The cover of `chapter` sharp at `pixels` on its longest side, once decoded; asks for it
    /// otherwise.
    pub fn get(
        &mut self,
        library: &Library,
        chapter: &ChapterSummary,
        pixels: u32,
        worker: &Worker<AppEvent>,
    ) -> Option<&egui::TextureHandle> {
        let key = CoverKey { chapter_id: chapter.id.clone(), image: chapter.cover_image.clone(), pixels };
        if !self.covers.contains_key(&key) {
            if chapter.cover_image.is_empty() || self.in_flight >= MAX_IN_FLIGHT {
                return None;
            }
            self.covers.insert(key.clone(), Cover::Loading);
            self.in_flight += 1;
            let library = library.clone();
            let chapter = chapter.clone();
            let job_key = key.clone();
            worker.spawn(move || {
                let image =
                    library.cover_path(&chapter).ok_or_else(|| "원본 이미지 없음".to_owned()).and_then(|path| {
                        crate::worker::catch_panic(|| crate::editor::thumbnails::decode(&path, pixels))
                            .and_then(|image| image)
                    });
                AppEvent::Cover(Box::new(CoverLoad { key: job_key, image }))
            });
        }
        match self.covers.get(&key) {
            Some(Cover::Ready(texture)) => Some(texture),
            _ => None,
        }
    }

    pub fn on_loaded(&mut self, ctx: &egui::Context, load: CoverLoad) {
        self.in_flight = self.in_flight.saturating_sub(1);
        let cover = match load.image {
            Ok(image) => Cover::Ready(ctx.load_texture(
                format!("cover-{}-{}", load.key.chapter_id, load.key.pixels),
                image,
                Default::default(),
            )),
            Err(error) => {
                log::warn!("cover {}: {error}", load.key.chapter_id);
                Cover::Failed
            }
        };
        self.covers.insert(load.key, cover);
    }
}

/// Paints `texture` filling `rect` (cropped to its shape, top kept), or a placeholder.
pub fn paint(painter: &egui::Painter, rect: egui::Rect, radius: f32, texture: Option<&egui::TextureHandle>) {
    let Some(texture) = texture else {
        painter.rect_filled(rect, radius, theme::CARD_BG);
        let size = (rect.width().min(rect.height()) * 0.4).clamp(10.0, 28.0);
        painter.text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            theme::icon::IMAGE,
            egui::FontId::new(size, egui::FontFamily::Name(theme::ICON_FAMILY.into())),
            theme::TEXT_FAINT,
        );
        return;
    };
    let [width, height] = texture.size().map(|side| side as f32);
    let (image_ratio, rect_ratio) = (width / height.max(1.0), rect.width() / rect.height().max(1.0));
    let uv = if image_ratio > rect_ratio {
        // Wider than the frame: keep the middle.
        let visible = rect_ratio / image_ratio;
        egui::Rect::from_x_y_ranges((0.5 - visible / 2.0)..=(0.5 + visible / 2.0), 0.0..=1.0)
    } else {
        egui::Rect::from_x_y_ranges(0.0..=1.0, 0.0..=(image_ratio / rect_ratio))
    };
    painter.add(egui::epaint::RectShape::filled(rect, radius, egui::Color32::WHITE).with_texture(texture.id(), uv));
    painter.rect_stroke(rect, radius, egui::Stroke::new(1.0, theme::BORDER), egui::StrokeKind::Inside);
}
