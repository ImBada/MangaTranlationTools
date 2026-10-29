//! GPU textures for page-sized images, split into tiles that fit the GPU's texture limit.
//! Pixels come from a row filler so derived views (masked results, mask overlays) can be
//! uploaded without keeping a second CPU copy of the page.

use std::ops::Range;
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui;
use egui::{Color32, ColorImage, Pos2, Rect, TextureHandle, TextureOptions};

pub struct TiledTexture {
    pub width: u32,
    pub height: u32,
    smooth: bool,
    /// Filtering the tiles were uploaded with.
    options: TextureOptions,
    tiles: Vec<Tile>,
}

struct Tile {
    x: u32,
    y: u32,
    width: u32,
    height: u32,
    texture: TextureHandle,
}

/// `AppSettings::zoom_smoothing`, read by textures built on worker threads too.
static SMOOTH_MAGNIFICATION: AtomicBool = AtomicBool::new(false);

pub fn set_smooth_magnification(smooth: bool) {
    SMOOTH_MAGNIFICATION.store(smooth, Ordering::Relaxed);
}

/// Smooth images always filter when zoomed out; zoomed in they show sharp pixels unless
/// the zoom smoothing setting is on.
pub fn options(smooth: bool) -> TextureOptions {
    if smooth {
        let magnification = if SMOOTH_MAGNIFICATION.load(Ordering::Relaxed) {
            egui::TextureFilter::Linear
        } else {
            egui::TextureFilter::Nearest
        };
        TextureOptions { magnification, mipmap_mode: Some(egui::TextureFilter::Linear), ..TextureOptions::LINEAR }
    } else {
        TextureOptions::NEAREST
    }
}

/// Appends the pixels `xs` of row `y` to the output (one call per row of a tile).
pub trait RowFill: Fn(u32, Range<u32>, &mut Vec<Color32>) {}
impl<F: Fn(u32, Range<u32>, &mut Vec<Color32>)> RowFill for F {}

fn build_image(x: u32, y: u32, width: u32, height: u32, fill: &impl RowFill) -> ColorImage {
    let mut pixels = Vec::with_capacity((width * height) as usize);
    for py in y..y + height {
        fill(py, x..x + width, &mut pixels);
    }
    debug_assert_eq!(pixels.len(), (width * height) as usize);
    ColorImage::new([width as usize, height as usize], pixels)
}

/// The bytes of `xs` in row `y` of an image `width` pixels wide.
pub fn row<'a, T>(data: &'a [T], width: u32, channels: u32, y: u32, xs: &Range<u32>) -> &'a [T] {
    let start = ((y * width + xs.start) * channels) as usize;
    &data[start..start + (xs.len() as u32 * channels) as usize]
}

/// Straight-alpha RGBA bytes to egui colors.
pub fn extend_rgba(out: &mut Vec<Color32>, rgba: &[u8]) {
    out.extend(rgba.as_chunks::<4>().0.iter().map(|p| Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3])));
}

impl TiledTexture {
    pub fn new(ctx: &egui::Context, name: &str, width: u32, height: u32, smooth: bool, fill: impl RowFill) -> Self {
        let (width, height) = (width.max(1), height.max(1));
        let tile_size = (ctx.input(|i| i.max_texture_side) as u32).clamp(512, 4096);
        let options = options(smooth);
        let mut tiles = Vec::new();
        for y in (0..height).step_by(tile_size as usize) {
            for x in (0..width).step_by(tile_size as usize) {
                let (tile_width, tile_height) = (tile_size.min(width - x), tile_size.min(height - y));
                let image = build_image(x, y, tile_width, tile_height, &fill);
                let texture = ctx.load_texture(format!("{name}-{x}-{y}"), image, options);
                tiles.push(Tile { x, y, width: tile_width, height: tile_height, texture });
            }
        }
        Self { width, height, smooth, options, tiles }
    }

    pub fn from_rgba(ctx: &egui::Context, name: &str, image: &image::RgbaImage, smooth: bool) -> Self {
        let width = image.width();
        Self::new(ctx, name, width, image.height(), smooth, |y, xs, out| {
            extend_rgba(out, row(image.as_raw(), width, 4, y, &xs))
        })
    }

    /// Whether the tiles still use the filtering the settings ask for.
    pub fn filter_current(&self) -> bool {
        self.options == options(self.smooth)
    }

    /// Re-uploads the pixels inside a page-pixel rect (brush strokes).
    pub fn update_region(&mut self, x: u32, y: u32, width: u32, height: u32, fill: impl RowFill) {
        let right = (x + width).min(self.width);
        let bottom = (y + height).min(self.height);
        for tile in &mut self.tiles {
            let left = x.max(tile.x);
            let top = y.max(tile.y);
            let tile_right = right.min(tile.x + tile.width);
            let tile_bottom = bottom.min(tile.y + tile.height);
            if left >= tile_right || top >= tile_bottom {
                continue;
            }
            let patch = build_image(left, top, tile_right - left, tile_bottom - top, &fill);
            tile.texture.set_partial([(left - tile.x) as usize, (top - tile.y) as usize], patch, self.options);
        }
    }

    /// Paints the image so that page pixels (0,0)..(width,height) map onto `rect`.
    pub fn paint(&self, painter: &egui::Painter, rect: Rect, tint: Color32) {
        let scale_x = rect.width() / self.width as f32;
        let scale_y = rect.height() / self.height as f32;
        let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
        for tile in &self.tiles {
            let tile_rect = Rect::from_min_size(
                rect.min + egui::vec2(tile.x as f32 * scale_x, tile.y as f32 * scale_y),
                egui::vec2(tile.width as f32 * scale_x, tile.height as f32 * scale_y),
            );
            if painter.clip_rect().intersects(tile_rect) {
                painter.image(tile.texture.id(), tile_rect, uv, tint);
            }
        }
    }
}

/// Texture for a small premultiplied raster (a rendered block).
pub fn pixmap_texture(ctx: &egui::Context, name: &str, pixmap: &tiny_skia::Pixmap) -> TextureHandle {
    let size = [pixmap.width() as usize, pixmap.height() as usize];
    ctx.load_texture(name, ColorImage::from_rgba_premultiplied(size, pixmap.data()), options(true))
}

/// Opacity in 0..=1 as a white tint for `Painter::image`.
pub fn alpha_tint(opacity: f32) -> Color32 {
    Color32::from_white_alpha((opacity.clamp(0.0, 1.0) * 255.0).round() as u8)
}
