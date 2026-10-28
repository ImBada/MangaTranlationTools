//! Page images: decoded in the background, kept on the CPU for editing (mask/result) and
//! mirrored to GPU textures for display.

use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{self, Color32};
use image::{GrayImage, RgbaImage};
use mtt_core::library::{Library, PageLayer};
use mtt_core::model::{Chapter, PageRecord};

use crate::textures::TiledTexture;

/// Mask overlay color in the editor (covered pixels).
pub const MASK_DISPLAY_COLOR: Color32 = Color32::from_rgb(255, 64, 140);
/// Mask islands picked by the auto eraser (`tintSelectedMaskPixel`).
pub const MASK_SELECTED_COLOR: Color32 = Color32::from_rgb(255, 120, 88);

/// Result of decoding one page's images on a worker thread.
pub struct PageLoad {
    pub chapter_id: String,
    pub page_id: String,
    pub source: Result<RgbaImage, String>,
    pub mask: Option<GrayImage>,
    pub result: Option<RgbaImage>,
}

/// Decodes the source image and existing inpaint layers of `page`.
pub fn load_page(library: &Library, chapter: &Chapter, page: &PageRecord) -> PageLoad {
    let decode = |path: Option<PathBuf>| -> Option<Result<RgbaImage, String>> {
        path.map(|path| {
            image::open(&path).map(|image| image.to_rgba8()).map_err(|error| format!("{}: {error}", path.display()))
        })
    };
    let source = decode(library.resolve_page_layer_path(chapter, page, PageLayer::Source))
        .unwrap_or_else(|| Err(format!("원본 이미지를 찾지 못했습니다: {}", page.image_path)));
    let (width, height) = source.as_ref().map(|image| image.dimensions()).unwrap_or((page.width, page.height));
    let mask = decode(library.resolve_page_layer_path(chapter, page, PageLayer::InpaintMask))
        .and_then(|result| result.map_err(|error| log::warn!("mask decode failed: {error}")).ok())
        .map(|mask| mask_from_rgba(&mask, width, height));
    let result = decode(library.resolve_page_layer_path(chapter, page, PageLayer::InpaintResult))
        .and_then(|result| result.map_err(|error| log::warn!("result decode failed: {error}")).ok())
        .map(|image| resize_nearest(image, width, height));
    PageLoad { chapter_id: chapter.id.clone(), page_id: page.id.clone(), source, mask, result }
}

/// Binary coverage mask (0/255) from a stored mask PNG (`normalizeOpaqueMaskPixels`).
pub fn mask_from_rgba(image: &RgbaImage, width: u32, height: u32) -> GrayImage {
    let image = resize_nearest(image.clone(), width, height);
    GrayImage::from_fn(width, height, |x, y| {
        let covered = mtt_render::page::mask_coverage(image.get_pixel(x, y)) > 0.0;
        image::Luma([if covered { 255 } else { 0 }])
    })
}

/// Opaque black/white RGBA, the format both apps store masks in.
pub fn mask_to_rgba(mask: &GrayImage) -> RgbaImage {
    RgbaImage::from_fn(mask.width(), mask.height(), |x, y| {
        let v = mask.get_pixel(x, y)[0];
        image::Rgba([v, v, v, 255])
    })
}

fn resize_nearest(image: RgbaImage, width: u32, height: u32) -> RgbaImage {
    if image.dimensions() == (width, height) {
        return image;
    }
    image::imageops::resize(&image, width, height, image::imageops::FilterType::Nearest)
}

pub struct LoadedPage {
    pub source: Arc<RgbaImage>,
    pub mask: Option<GrayImage>,
    pub result: Option<RgbaImage>,
    pub source_texture: TiledTexture,
    /// Result with alpha multiplied by mask coverage (what the output shows).
    pub result_masked_texture: Option<TiledTexture>,
    /// Unmasked result (shown while the result layer is active).
    pub result_texture: Option<TiledTexture>,
    pub mask_texture: Option<TiledTexture>,
    /// Auto-eraser selection (1 = picked) shown in a different color until released.
    pub mask_selection: Option<Vec<u8>>,
}

pub enum PageAssets {
    Loading,
    Ready(Box<LoadedPage>),
    Failed(String),
}

impl LoadedPage {
    pub fn new(
        ctx: &egui::Context,
        page_id: &str,
        source: RgbaImage,
        mask: Option<GrayImage>,
        result: Option<RgbaImage>,
    ) -> Self {
        let source_texture = TiledTexture::from_rgba(ctx, &format!("{page_id}-source"), &source, true);
        let mut page = Self {
            source: Arc::new(source),
            mask,
            result,
            source_texture,
            result_masked_texture: None,
            result_texture: None,
            mask_texture: None,
            mask_selection: None,
        };
        page.rebuild_layer_textures(ctx, page_id);
        page
    }

    pub fn width(&self) -> u32 {
        self.source.width()
    }

    pub fn height(&self) -> u32 {
        self.source.height()
    }

    pub fn rebuild_layer_textures(&mut self, ctx: &egui::Context, page_id: &str) {
        let (width, height) = (self.width(), self.height());
        self.result_texture =
            self.result.as_ref().map(|result| TiledTexture::from_rgba(ctx, &format!("{page_id}-result"), result, true));
        self.result_masked_texture = self.result.as_ref().map(|_| {
            TiledTexture::new(ctx, &format!("{page_id}-result-masked"), width, height, true, |x, y| {
                self.masked_result_pixel(x, y)
            })
        });
        self.mask_texture = self.mask.as_ref().map(|_| {
            TiledTexture::new(ctx, &format!("{page_id}-mask"), width, height, false, |x, y| self.mask_pixel(x, y))
        });
    }

    fn masked_result_pixel(&self, x: u32, y: u32) -> Color32 {
        let Some(result) = &self.result else {
            return Color32::TRANSPARENT;
        };
        let p = result.get_pixel(x, y).0;
        let alpha = match &self.mask {
            Some(mask) if mask.get_pixel(x, y)[0] == 0 => 0,
            _ => p[3],
        };
        Color32::from_rgba_unmultiplied(p[0], p[1], p[2], alpha)
    }

    fn result_pixel(&self, x: u32, y: u32) -> Color32 {
        self.result.as_ref().map_or(Color32::TRANSPARENT, |result| {
            let p = result.get_pixel(x, y).0;
            Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3])
        })
    }

    fn mask_pixel(&self, x: u32, y: u32) -> Color32 {
        match &self.mask {
            Some(mask) if mask.get_pixel(x, y)[0] > 0 => {
                let picked = self.mask_selection.as_ref().is_some_and(|sel| sel[(y * mask.width() + x) as usize] != 0);
                if picked { MASK_SELECTED_COLOR } else { MASK_DISPLAY_COLOR }
            }
            _ => Color32::TRANSPARENT,
        }
    }

    /// Pushes CPU edits inside a rect to every texture that depends on the layers.
    pub fn refresh_layer_region(
        &mut self,
        ctx: &egui::Context,
        page_id: &str,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    ) {
        let missing_texture = (self.mask.is_some() && self.mask_texture.is_none())
            || (self.result.is_some() && (self.result_texture.is_none() || self.result_masked_texture.is_none()));
        if missing_texture {
            self.rebuild_layer_textures(ctx, page_id);
            return;
        }
        let mut mask_texture = self.mask_texture.take();
        let mut result_texture = self.result_texture.take();
        let mut masked_texture = self.result_masked_texture.take();
        let this = &*self;
        if let Some(texture) = &mut mask_texture {
            texture.update_region(x, y, width, height, |px, py| this.mask_pixel(px, py));
        }
        if let Some(texture) = &mut result_texture {
            texture.update_region(x, y, width, height, |px, py| this.result_pixel(px, py));
        }
        if let Some(texture) = &mut masked_texture {
            texture.update_region(x, y, width, height, |px, py| this.masked_result_pixel(px, py));
        }
        self.mask_texture = mask_texture;
        self.result_texture = result_texture;
        self.result_masked_texture = masked_texture;
    }

    pub fn ensure_result(&mut self) -> &mut RgbaImage {
        let (width, height) = (self.width(), self.height());
        self.result.get_or_insert_with(|| RgbaImage::new(width, height))
    }

    pub fn ensure_mask(&mut self) -> &mut GrayImage {
        let (width, height) = (self.width(), self.height());
        self.mask.get_or_insert_with(|| GrayImage::new(width, height))
    }
}

/// Paints a round-capped stroke segment into a binary mask. Returns the touched rect.
pub fn stroke_mask(
    mask: &mut GrayImage,
    from: (f32, f32),
    to: (f32, f32),
    diameter: f32,
    value: u8,
) -> Option<(u32, u32, u32, u32)> {
    let radius = diameter / 2.0 + 0.5;
    let (width, height) = (mask.width() as f32, mask.height() as f32);
    let min_x = (from.0.min(to.0) - radius).floor().max(0.0);
    let min_y = (from.1.min(to.1) - radius).floor().max(0.0);
    let max_x = (from.0.max(to.0) + radius).ceil().min(width);
    let max_y = (from.1.max(to.1) + radius).ceil().min(height);
    if min_x >= max_x || min_y >= max_y {
        return None;
    }
    let (dx, dy) = (to.0 - from.0, to.1 - from.1);
    let length_squared = dx * dx + dy * dy;
    for y in min_y as u32..max_y as u32 {
        for x in min_x as u32..max_x as u32 {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let t = if length_squared > 0.0 {
                (((px - from.0) * dx + (py - from.1) * dy) / length_squared).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let (cx, cy) = (from.0 + dx * t, from.1 + dy * t);
            if (px - cx).powi(2) + (py - cy).powi(2) <= radius * radius {
                mask.put_pixel(x, y, image::Luma([value]));
            }
        }
    }
    Some((min_x as u32, min_y as u32, (max_x - min_x) as u32, (max_y - min_y) as u32))
}

/// `selectTouchedMaskIslands`: flood-fills (4-connected) every covered island touched by
/// the brush at `point`. Returns the bounding rect of newly picked pixels.
pub fn select_mask_islands(
    mask: &GrayImage,
    selected: &mut [u8],
    point: (f32, f32),
    diameter: f32,
) -> Option<(u32, u32, u32, u32)> {
    let (width, height) = (mask.width() as i64, mask.height() as i64);
    if point.0 < 0.0 || point.1 < 0.0 || point.0 >= width as f32 || point.1 >= height as f32 {
        return None;
    }
    let radius = (diameter / 2.0).max(0.5);
    let selectable = |index: usize, selected: &[u8]| selected[index] == 0 && mask.as_raw()[index] > 0;
    let mut queue = Vec::new();
    let (min_x, max_x) =
        (((point.0 - radius).floor() as i64).max(0), ((point.0 + radius).ceil() as i64).min(width - 1));
    let (min_y, max_y) =
        (((point.1 - radius).floor() as i64).max(0), ((point.1 + radius).ceil() as i64).min(height - 1));
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = ((x as f32 + 0.5 - point.0).abs() - 0.5).max(0.0);
            let dy = ((y as f32 + 0.5 - point.1).abs() - 0.5).max(0.0);
            let index = (y * width + x) as usize;
            if dx * dx + dy * dy <= radius * radius && selectable(index, selected) {
                selected[index] = 1;
                queue.push(index);
            }
        }
    }
    if queue.is_empty() {
        return None;
    }
    let (mut left, mut top, mut right, mut bottom) = (u32::MAX, u32::MAX, 0, 0);
    let mut head = 0;
    while head < queue.len() {
        let index = queue[head];
        head += 1;
        let (x, y) = ((index as i64 % width) as u32, (index as i64 / width) as u32);
        (left, top, right, bottom) = (left.min(x), top.min(y), right.max(x), bottom.max(y));
        let mut visit = |neighbor: usize| {
            if selectable(neighbor, selected) {
                selected[neighbor] = 1;
                queue.push(neighbor);
            }
        };
        if x > 0 {
            visit(index - 1);
        }
        if (x as i64) + 1 < width {
            visit(index + 1);
        }
        if y > 0 {
            visit(index - width as usize);
        }
        if (y as i64) + 1 < height {
            visit(index + width as usize);
        }
    }
    Some((left, top, right - left + 1, bottom - top + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_eraser_picks_whole_touched_islands_only() {
        let mut mask = GrayImage::new(10, 3);
        for x in 0..4 {
            mask.put_pixel(x, 1, image::Luma([255]));
        }
        for x in 6..10 {
            mask.put_pixel(x, 1, image::Luma([255]));
        }
        let mut selected = vec![0u8; 30];
        let rect = select_mask_islands(&mask, &mut selected, (0.5, 1.5), 1.0).unwrap();
        assert_eq!(rect, (0, 1, 4, 1));
        assert_eq!(selected.iter().filter(|v| **v != 0).count(), 4, "right island untouched");
        assert!(select_mask_islands(&mask, &mut selected, (5.0, 0.5), 1.0).is_none());
    }

    #[test]
    fn stroke_covers_capsule_and_reports_rect() {
        let mut mask = GrayImage::new(20, 20);
        let rect = stroke_mask(&mut mask, (5.0, 10.0), (15.0, 10.0), 4.0, 255).unwrap();
        assert_eq!(mask.get_pixel(10, 10)[0], 255);
        assert_eq!(mask.get_pixel(10, 15)[0], 0);
        assert_eq!(mask.get_pixel(4, 10)[0], 255, "round cap");
        assert!(rect.0 <= 3 && rect.0 + rect.2 >= 17);
        assert!(stroke_mask(&mut mask, (-50.0, -50.0), (-40.0, -40.0), 4.0, 255).is_none());
    }

    #[test]
    fn mask_round_trip_is_binary() {
        let mut rgba = RgbaImage::new(2, 1);
        rgba.put_pixel(0, 0, image::Rgba([255, 255, 255, 10]));
        rgba.put_pixel(1, 0, image::Rgba([0, 0, 0, 255]));
        let mask = mask_from_rgba(&rgba, 2, 1);
        assert_eq!(mask.as_raw(), &[255, 0]);
        assert_eq!(mask_to_rgba(&mask).get_pixel(0, 0).0, [255, 255, 255, 255]);
    }
}
