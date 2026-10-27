//! Per-block raster cache. Only blocks whose content changed are re-rendered, within a
//! per-frame time budget so typing or dragging never stalls the UI.

use std::collections::{HashMap, HashSet};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::time::{Duration, Instant};

use eframe::egui::{self, Rect, TextureHandle};
use mtt_core::geometry::editable_block_bbox;
use mtt_core::model::{PageRecord, TranslationBlock};
use mtt_render::block::{BlockRenderOptions, render_block};
use mtt_render::page::{page_size, render_group_shadows};
use mtt_render::{BlockRaster, FontContext};

use crate::textures::pixmap_texture;

const FRAME_BUDGET: Duration = Duration::from_millis(12);

#[derive(Clone)]
pub struct CachedRaster {
    pub texture: TextureHandle,
    /// Covered area in page pixels.
    pub rect: Rect,
}

struct Entry {
    shape_key: u64,
    /// Editable bbox origin (normalized) the raster was rendered at.
    origin: (f64, f64),
    raster: Option<CachedRaster>,
    overflow: bool,
}

pub struct OverlayCache {
    page_id: String,
    scale: f32,
    revision: u64,
    complete: bool,
    entries: HashMap<String, Entry>,
    shadows_key: u64,
    shadows: Vec<CachedRaster>,
}

impl Default for OverlayCache {
    fn default() -> Self {
        Self {
            page_id: String::new(),
            scale: 1.0,
            revision: u64::MAX,
            complete: false,
            entries: HashMap::new(),
            shadows_key: 0,
            shadows: Vec::new(),
        }
    }
}

fn hash_value(value: &impl serde::Serialize) -> u64 {
    let mut hasher = DefaultHasher::new();
    serde_json::to_vec(value).unwrap_or_default().hash(&mut hasher);
    hasher.finish()
}

/// Content hash ignoring the block position, so a moved block can reuse its raster.
fn shape_key(block: &TranslationBlock, page: &PageRecord, scale: f32) -> u64 {
    let mut shape = block.clone();
    shape.bbox.x = 0.0;
    shape.bbox.y = 0.0;
    if let Some(render) = &mut shape.render_bbox {
        render.x = 0.0;
        render.y = 0.0;
    }
    let mut hasher = DefaultHasher::new();
    hash_value(&shape).hash(&mut hasher);
    (page.width, page.height, scale.to_bits()).hash(&mut hasher);
    hasher.finish()
}

fn origin(block: &TranslationBlock) -> (f64, f64) {
    let bbox = editable_block_bbox(block);
    (bbox.x, bbox.y)
}

fn to_cached(ctx: &egui::Context, name: &str, raster: &BlockRaster) -> CachedRaster {
    let texture = pixmap_texture(ctx, name, &raster.pixmap);
    let size = egui::vec2(raster.pixmap.width() as f32 / raster.scale, raster.pixmap.height() as f32 / raster.scale);
    CachedRaster { texture, rect: Rect::from_min_size(egui::pos2(raster.x as f32, raster.y as f32), size) }
}

impl OverlayCache {
    pub fn is_complete(&self, revision: u64) -> bool {
        self.complete && self.revision == revision
    }

    /// Brings rasters up to date with `page`. `moving` blocks keep their old raster (drawn
    /// with an offset) until the drag ends. Returns `true` while work remains.
    pub fn sync(
        &mut self,
        ctx: &egui::Context,
        fonts: &mut FontContext,
        page: &PageRecord,
        revision: u64,
        scale: f32,
        moving: &HashSet<String>,
    ) -> bool {
        if self.page_id != page.id || self.scale != scale {
            self.entries.clear();
            self.shadows.clear();
            self.shadows_key = 0;
            self.page_id = page.id.clone();
            self.scale = scale;
            self.complete = false;
        }
        if self.complete && self.revision == revision && moving.is_empty() {
            return false;
        }
        let started = Instant::now();
        let size = page_size(page);
        let live: HashSet<&str> = page.blocks.iter().map(|block| block.id.as_str()).collect();
        self.entries.retain(|id, _| live.contains(id.as_str()));

        let mut pending = false;
        for block in &page.blocks {
            let key = shape_key(block, page, scale);
            let block_origin = origin(block);
            if let Some(entry) = self.entries.get(&block.id)
                && entry.shape_key == key
                && (entry.origin == block_origin || moving.contains(&block.id))
            {
                continue;
            }
            if started.elapsed() > FRAME_BUDGET {
                pending = true;
                break;
            }
            let rendered = render_block(fonts, size, block, BlockRenderOptions { editing_enabled: false, scale });
            let overflow = rendered.as_ref().is_some_and(|raster| raster.overflow);
            let raster = rendered.map(|raster| to_cached(ctx, &format!("block-{}", block.id), &raster));
            self.entries.insert(block.id.clone(), Entry { shape_key: key, origin: block_origin, raster, overflow });
        }

        let groups_key = hash_value(&(&page.block_groups, page.block_groups.as_ref().map(|_| &page.blocks)));
        if !pending && groups_key != self.shadows_key && moving.is_empty() {
            self.shadows = render_group_shadows(fonts, page)
                .iter()
                .enumerate()
                .map(|(index, raster)| to_cached(ctx, &format!("group-shadow-{index}"), raster))
                .collect();
            self.shadows_key = groups_key;
        }

        self.revision = revision;
        self.complete = !pending;
        pending
    }

    /// Raster for `block` and the page-pixel offset to draw it at (non-zero while dragging).
    pub fn raster(&self, block: &TranslationBlock, page: &PageRecord) -> Option<(CachedRaster, egui::Vec2)> {
        let entry = self.entries.get(&block.id)?;
        let raster = entry.raster.clone()?;
        let (x, y) = origin(block);
        let offset = egui::vec2(
            ((x - entry.origin.0) / 1000.0 * f64::from(page.width)) as f32,
            ((y - entry.origin.1) / 1000.0 * f64::from(page.height)) as f32,
        );
        Some((raster, offset))
    }

    /// Whether the block's text overflows its render box (`overflowing`).
    pub fn overflowing(&self, block_id: &str) -> bool {
        self.entries.get(block_id).is_some_and(|entry| entry.overflow)
    }

    pub fn shadows(&self) -> &[CachedRaster] {
        &self.shadows
    }
}
