//! Page strip thumbnails: decoded from the source image on the worker pool, a few at a
//! time and only for rows on screen, after the current page itself has loaded.

use std::collections::HashMap;

use eframe::egui;
use mtt_core::library::{Library, PageLayer};
use mtt_core::model::{Chapter, PageRecord};

use crate::app::AppEvent;
use crate::worker::Worker;

/// Longest side of a decoded thumbnail in pixels (sharp at 2x for the strip's size).
const THUMB_PIXELS: u32 = 160;
const MAX_IN_FLIGHT: usize = 2;

pub struct ThumbLoad {
    pub chapter_id: String,
    pub page_id: String,
    pub image: Result<egui::ColorImage, String>,
}

enum Thumb {
    Loading,
    Ready(egui::TextureHandle),
    Failed,
}

#[derive(Default)]
pub struct Thumbnails {
    thumbs: HashMap<String, Thumb>,
    in_flight: usize,
}

impl Thumbnails {
    pub fn texture(&self, page_id: &str) -> Option<&egui::TextureHandle> {
        match self.thumbs.get(page_id) {
            Some(Thumb::Ready(texture)) => Some(texture),
            _ => None,
        }
    }

    /// Starts decoding `page` unless it is known or too many are running.
    pub fn request(&mut self, library: &Library, chapter: &Chapter, page: &PageRecord, worker: &Worker<AppEvent>) {
        if self.thumbs.contains_key(&page.id) || self.in_flight >= MAX_IN_FLIGHT {
            return;
        }
        self.thumbs.insert(page.id.clone(), Thumb::Loading);
        self.in_flight += 1;
        let path = library.resolve_page_layer_path(chapter, page, PageLayer::Source);
        let chapter_id = chapter.id.clone();
        let page_id = page.id.clone();
        worker.spawn(move || {
            let image = path
                .ok_or_else(|| "원본 이미지 없음".to_owned())
                .and_then(|path| crate::worker::catch_panic(|| decode(&path)).and_then(|image| image));
            AppEvent::Thumbnail(Box::new(ThumbLoad { chapter_id, page_id, image }))
        });
    }

    pub fn on_loaded(&mut self, ctx: &egui::Context, chapter_id: &str, load: ThumbLoad) {
        if load.chapter_id != chapter_id {
            return;
        }
        self.in_flight = self.in_flight.saturating_sub(1);
        let thumb = match load.image {
            Ok(image) => Thumb::Ready(ctx.load_texture(format!("thumb-{}", load.page_id), image, Default::default())),
            Err(error) => {
                log::warn!("thumbnail {}: {error}", load.page_id);
                Thumb::Failed
            }
        };
        self.thumbs.insert(load.page_id, thumb);
    }
}

fn decode(path: &std::path::Path) -> Result<egui::ColorImage, String> {
    let mut image = mtt_core::image_io::open_image(path).map_err(|error| error.to_string())?;
    // Long strips (webtoons) show their top instead of a sliver of the whole page.
    let tallest = image.width().saturating_mul(8) / 5;
    if image.height() > tallest && tallest > 0 {
        image = image.crop_imm(0, 0, image.width(), tallest);
    }
    let small = image.thumbnail(THUMB_PIXELS, THUMB_PIXELS).to_rgba8();
    let size = [small.width() as usize, small.height() as usize];
    Ok(egui::ColorImage::from_rgba_unmultiplied(size, small.as_raw()))
}
