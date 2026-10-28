//! Page images: decoded in the background, kept on the CPU for editing (mask/result) and
//! mirrored to GPU textures for display.

use std::cell::Cell;
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;

use eframe::egui::{self, Color32};
use image::{DynamicImage, GrayImage, RgbaImage};
use mtt_core::library::{Library, PageLayer};
use mtt_core::model::{Chapter, PageRecord};

use super::history::PixelRect;
use crate::textures::{TiledTexture, extend_rgba, row};

/// Mask overlay color in the editor (covered pixels).
pub const MASK_DISPLAY_COLOR: Color32 = Color32::from_rgb(255, 64, 140);
/// Mask islands picked by the auto eraser (`tintSelectedMaskPixel`).
pub const MASK_SELECTED_COLOR: Color32 = Color32::from_rgb(255, 120, 88);

/// A page's images as decoded from disk.
pub struct DecodedPage {
    pub source: Result<RgbaImage, String>,
    pub mask: Option<GrayImage>,
    pub result: Option<RgbaImage>,
}

/// A page made ready for the editor on a worker thread (decoded and uploaded).
pub struct PageLoad {
    pub chapter_id: String,
    pub page_id: String,
    pub page: Result<Box<LoadedPage>, String>,
}

/// Decodes the source image and existing inpaint layers of `page`.
pub fn decode_page(library: &Library, chapter: &Chapter, page: &PageRecord) -> DecodedPage {
    let decode = |path: Option<PathBuf>| -> Option<Result<DynamicImage, String>> {
        path.map(|path| image::open(&path).map_err(|error| format!("{}: {error}", path.display())))
    };
    let source = decode(library.resolve_page_layer_path(chapter, page, PageLayer::Source))
        .unwrap_or_else(|| Err(format!("원본 이미지를 찾지 못했습니다: {}", page.image_path)))
        .map(DynamicImage::into_rgba8);
    let (width, height) = source.as_ref().map(|image| image.dimensions()).unwrap_or((page.width, page.height));
    let mask = decode(library.resolve_page_layer_path(chapter, page, PageLayer::InpaintMask))
        .and_then(|result| result.map_err(|error| log::warn!("mask decode failed: {error}")).ok())
        .map(|mask| binary_mask(mask, width, height));
    let result = decode(library.resolve_page_layer_path(chapter, page, PageLayer::InpaintResult))
        .and_then(|result| result.map_err(|error| log::warn!("result decode failed: {error}")).ok())
        .map(|image| resize_nearest(image.into_rgba8(), width, height));
    DecodedPage { source, mask, result }
}

/// Decodes `page` and builds its textures; runs on a worker (egui textures can be created
/// from any thread), so a page switch doesn't convert whole pages on the UI thread.
pub fn load_page(ctx: &egui::Context, library: &Library, chapter: &Chapter, page: &PageRecord) -> PageLoad {
    let decoded = decode_page(library, chapter, page);
    let page_id = page.id.clone();
    let page =
        decoded.source.map(|source| Box::new(LoadedPage::new(ctx, &page_id, source, decoded.mask, decoded.result)));
    PageLoad { chapter_id: chapter.id.clone(), page_id, page }
}

/// Binary coverage mask (0/255) from a stored mask (`normalizeOpaqueMaskPixels`): masks are
/// saved as grayscale; older ones (and the TS app's) as opaque black/white RGBA.
fn binary_mask(image: DynamicImage, width: u32, height: u32) -> GrayImage {
    let mut mask = match image {
        DynamicImage::ImageLuma8(mut gray) => {
            for value in gray.iter_mut() {
                *value = if *value > 0 { 255 } else { 0 };
            }
            gray
        }
        other => mask_from_rgba(&other.into_rgba8()),
    };
    if mask.dimensions() != (width, height) {
        mask = image::imageops::resize(&mask, width, height, image::imageops::FilterType::Nearest);
    }
    mask
}

/// Covered where `mask_coverage` > 0: some alpha and some brightness.
pub fn mask_from_rgba(image: &RgbaImage) -> GrayImage {
    let pixels = image.as_raw().chunks_exact(4).map(|p| if p[3] > 0 && (p[0] | p[1] | p[2]) > 0 { 255 } else { 0 });
    GrayImage::from_raw(image.width(), image.height(), pixels.collect()).expect("one byte per pixel")
}

fn resize_nearest(image: RgbaImage, width: u32, height: u32) -> RgbaImage {
    if image.dimensions() == (width, height) {
        return image;
    }
    image::imageops::resize(&image, width, height, image::imageops::FilterType::Nearest)
}

/// The editable layers are shared (`Arc`) with background saves, exports and inpaint jobs,
/// and copied only when edited while one of those still holds them.
pub struct LoadedPage {
    pub source: Arc<RgbaImage>,
    pub mask: Option<Arc<GrayImage>>,
    pub result: Option<Arc<RgbaImage>>,
    pub source_texture: TiledTexture,
    /// Result with alpha multiplied by mask coverage (what the output shows).
    pub result_masked_texture: Option<TiledTexture>,
    /// Unmasked result (shown while the result layer is active).
    pub result_texture: Option<TiledTexture>,
    pub mask_texture: Option<TiledTexture>,
    /// Auto-eraser selection (1 = picked) shown in a different color until released.
    pub mask_selection: Option<Vec<u8>>,
    /// Whether the layers have any pixel, known since the last change (`None` = scan again).
    /// Asked several times a frame; a full scan is up to 48 MB.
    mask_has_pixels: Cell<Option<bool>>,
    result_has_pixels: Cell<Option<bool>>,
}

pub enum PageAssets {
    Loading,
    Ready(Box<LoadedPage>),
    Failed(String),
}

fn any_mask_pixel(mask: &[u8]) -> bool {
    mask.iter().any(|value| *value > 0)
}

fn any_result_pixel(rgba: &[u8]) -> bool {
    rgba.chunks_exact(4).any(|p| p[3] > 0)
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
            mask: mask.map(Arc::new),
            result: result.map(Arc::new),
            source_texture,
            result_masked_texture: None,
            result_texture: None,
            mask_texture: None,
            mask_selection: None,
            mask_has_pixels: Cell::new(None),
            result_has_pixels: Cell::new(None),
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

    pub fn mask_has_pixels(&self) -> bool {
        let Some(mask) = &self.mask else { return false };
        let known = self.mask_has_pixels.get().unwrap_or_else(|| any_mask_pixel(mask.as_raw()));
        self.mask_has_pixels.set(Some(known));
        known
    }

    pub fn result_has_pixels(&self) -> bool {
        let Some(result) = &self.result else { return false };
        let known = self.result_has_pixels.get().unwrap_or_else(|| any_result_pixel(result.as_raw()));
        self.result_has_pixels.set(Some(known));
        known
    }

    /// Puts in layers built off the UI thread (an inpaint or PSD import).
    pub fn install_layers(&mut self, layers: LayerSet) {
        self.mask = Some(layers.mask);
        self.result = Some(layers.result);
        self.mask_selection = None;
        self.mask_has_pixels.set(None);
        self.result_has_pixels.set(None);
        self.set_textures(layers.textures);
    }

    fn set_textures(&mut self, textures: LayerTextures) {
        self.result_masked_texture = textures.result_masked;
        self.result_texture = textures.result;
        self.mask_texture = textures.mask;
    }

    pub fn rebuild_layer_textures(&mut self, ctx: &egui::Context, page_id: &str) {
        self.mask_has_pixels.set(None);
        self.result_has_pixels.set(None);
        let textures = LayerTextures::build(
            ctx,
            page_id,
            (self.width(), self.height()),
            self.mask.as_deref(),
            self.result.as_deref(),
            self.mask_selection.as_deref(),
        );
        self.set_textures(textures);
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
        self.track_pixels_in(x, y, width, height);
        let page_width = self.width();
        let (mask, result, selection) = (self.mask.as_deref(), self.result.as_deref(), self.mask_selection.as_deref());
        if let Some(texture) = &mut self.mask_texture {
            texture
                .update_region(x, y, width, height, |py, xs, out| mask_row(mask, selection, page_width, py, xs, out));
        }
        if let Some(texture) = &mut self.result_texture {
            texture.update_region(x, y, width, height, |py, xs, out| result_row(result, page_width, py, xs, out));
        }
        if let Some(texture) = &mut self.result_masked_texture {
            texture.update_region(x, y, width, height, |py, xs, out| {
                masked_result_row(result, mask, page_width, py, xs, out)
            });
        }
    }

    /// Updates the "has pixels" flags after a change inside a rect: pixels in the rect
    /// settle it; none there only matters when the layer had some (it may now be empty).
    fn track_pixels_in(&self, x: u32, y: u32, width: u32, height: u32) {
        let page_width = self.width();
        let right = (x + width).min(page_width);
        let bottom = (y + height).min(self.height());
        if x >= right || y >= bottom {
            return;
        }
        let xs = x..right;
        let track = |flag: &Cell<Option<bool>>, found: bool| {
            if found {
                flag.set(Some(true));
            } else if flag.get() == Some(true) {
                flag.set(None);
            }
        };
        if let Some(mask) = &self.mask {
            track(
                &self.mask_has_pixels,
                (y..bottom).any(|py| any_mask_pixel(row(mask.as_raw(), page_width, 1, py, &xs))),
            );
        }
        if let Some(result) = &self.result {
            let found = (y..bottom).any(|py| any_result_pixel(row(result.as_raw(), page_width, 4, py, &xs)));
            track(&self.result_has_pixels, found);
        }
    }

    pub fn ensure_result(&mut self) -> &mut RgbaImage {
        let (width, height) = (self.width(), self.height());
        Arc::make_mut(self.result.get_or_insert_with(|| Arc::new(RgbaImage::new(width, height))))
    }

    pub fn ensure_mask(&mut self) -> &mut GrayImage {
        let (width, height) = (self.width(), self.height());
        Arc::make_mut(self.mask.get_or_insert_with(|| Arc::new(GrayImage::new(width, height))))
    }

    /// Both layers for an edit that changes them together (result tools).
    pub fn ensure_layers(&mut self) -> (&mut RgbaImage, &mut GrayImage) {
        let (width, height) = (self.width(), self.height());
        let result = Arc::make_mut(self.result.get_or_insert_with(|| Arc::new(RgbaImage::new(width, height))));
        let mask = Arc::make_mut(self.mask.get_or_insert_with(|| Arc::new(GrayImage::new(width, height))));
        (result, mask)
    }
}

/// New layers and their textures, built on a worker.
pub struct LayerSet {
    mask: Arc<GrayImage>,
    result: Arc<RgbaImage>,
    textures: LayerTextures,
}

impl LayerSet {
    pub fn build(ctx: &egui::Context, page_id: &str, mask: GrayImage, result: RgbaImage) -> Self {
        let textures = LayerTextures::build(ctx, page_id, mask.dimensions(), Some(&mask), Some(&result), None);
        Self { mask: Arc::new(mask), result: Arc::new(result), textures }
    }
}

struct LayerTextures {
    result_masked: Option<TiledTexture>,
    result: Option<TiledTexture>,
    mask: Option<TiledTexture>,
}

impl LayerTextures {
    fn build(
        ctx: &egui::Context,
        page_id: &str,
        (width, height): (u32, u32),
        mask: Option<&GrayImage>,
        result: Option<&RgbaImage>,
        selection: Option<&[u8]>,
    ) -> Self {
        Self {
            result: result.map(|result| TiledTexture::from_rgba(ctx, &format!("{page_id}-result"), result, true)),
            result_masked: result.map(|_| {
                TiledTexture::new(ctx, &format!("{page_id}-result-masked"), width, height, true, |y, xs, out| {
                    masked_result_row(result, mask, width, y, xs, out)
                })
            }),
            mask: mask.map(|_| {
                TiledTexture::new(ctx, &format!("{page_id}-mask"), width, height, false, |y, xs, out| {
                    mask_row(mask, selection, width, y, xs, out)
                })
            }),
        }
    }
}

fn transparent(out: &mut Vec<Color32>, count: usize) {
    out.extend(std::iter::repeat_n(Color32::TRANSPARENT, count));
}

/// The result as the output shows it: hidden where the mask doesn't cover.
fn masked_result_row(
    result: Option<&RgbaImage>,
    mask: Option<&GrayImage>,
    width: u32,
    y: u32,
    xs: Range<u32>,
    out: &mut Vec<Color32>,
) {
    let Some(result) = result else { return transparent(out, xs.len()) };
    let rgba = row(result.as_raw(), width, 4, y, &xs);
    match mask {
        Some(mask) => out.extend(rgba.chunks_exact(4).zip(row(mask.as_raw(), width, 1, y, &xs)).map(|(p, covered)| {
            if *covered == 0 { Color32::TRANSPARENT } else { Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]) }
        })),
        None => extend_rgba(out, rgba),
    }
}

fn result_row(result: Option<&RgbaImage>, width: u32, y: u32, xs: Range<u32>, out: &mut Vec<Color32>) {
    match result {
        Some(result) => extend_rgba(out, row(result.as_raw(), width, 4, y, &xs)),
        None => transparent(out, xs.len()),
    }
}

fn mask_row(
    mask: Option<&GrayImage>,
    selection: Option<&[u8]>,
    width: u32,
    y: u32,
    xs: Range<u32>,
    out: &mut Vec<Color32>,
) {
    let Some(mask) = mask else { return transparent(out, xs.len()) };
    let covered = row(mask.as_raw(), width, 1, y, &xs);
    let color = |value: u8, picked: bool| match (value > 0, picked) {
        (false, _) => Color32::TRANSPARENT,
        (true, true) => MASK_SELECTED_COLOR,
        (true, false) => MASK_DISPLAY_COLOR,
    };
    match selection {
        Some(selection) => out.extend(
            covered.iter().zip(row(selection, width, 1, y, &xs)).map(|(value, picked)| color(*value, *picked != 0)),
        ),
        None => out.extend(covered.iter().map(|value| color(*value, false))),
    }
}

/// The rect a `stroke_mask` segment may paint.
pub fn stroke_bounds(width: u32, height: u32, from: (f32, f32), to: (f32, f32), diameter: f32) -> Option<PixelRect> {
    let radius = diameter / 2.0 + 0.5;
    let min_x = (from.0.min(to.0) - radius).floor().max(0.0);
    let min_y = (from.1.min(to.1) - radius).floor().max(0.0);
    let max_x = (from.0.max(to.0) + radius).ceil().min(width as f32);
    let max_y = (from.1.max(to.1) + radius).ceil().min(height as f32);
    (min_x < max_x && min_y < max_y).then_some((
        min_x as u32,
        min_y as u32,
        (max_x - min_x) as u32,
        (max_y - min_y) as u32,
    ))
}

/// Paints a round-capped stroke segment into a binary mask. Returns the touched rect.
pub fn stroke_mask(
    mask: &mut GrayImage,
    from: (f32, f32),
    to: (f32, f32),
    diameter: f32,
    value: u8,
) -> Option<PixelRect> {
    let rect = stroke_bounds(mask.width(), mask.height(), from, to, diameter)?;
    let radius = diameter / 2.0 + 0.5;
    let (min_x, min_y) = (rect.0 as f32, rect.1 as f32);
    let (max_x, max_y) = ((rect.0 + rect.2) as f32, (rect.1 + rect.3) as f32);
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
    Some(rect)
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
        assert_eq!(binary_mask(DynamicImage::ImageRgba8(rgba), 2, 1).as_raw(), &[255, 0]);
        let gray = GrayImage::from_raw(2, 1, vec![9, 0]).unwrap();
        assert_eq!(
            binary_mask(DynamicImage::ImageLuma8(gray), 4, 1).as_raw(),
            &[255, 255, 0, 0],
            "saved masks are gray"
        );
    }

    #[test]
    fn pixel_flags_follow_edits() {
        let ctx = egui::Context::default();
        let mut page = LoadedPage::new(&ctx, "p", RgbaImage::new(8, 8), Some(GrayImage::new(8, 8)), None);
        assert!(!page.mask_has_pixels() && !page.result_has_pixels());
        page.ensure_mask().put_pixel(6, 6, image::Luma([255]));
        page.refresh_layer_region(&ctx, "p", 6, 6, 1, 1);
        assert!(page.mask_has_pixels());
        page.ensure_mask().put_pixel(6, 6, image::Luma([0]));
        page.refresh_layer_region(&ctx, "p", 6, 6, 1, 1);
        assert!(!page.mask_has_pixels(), "erasing the last pixel empties the layer");
        page.ensure_result().put_pixel(1, 1, image::Rgba([1, 2, 3, 4]));
        page.refresh_layer_region(&ctx, "p", 0, 0, 2, 2);
        assert!(page.result_has_pixels());
    }
}
