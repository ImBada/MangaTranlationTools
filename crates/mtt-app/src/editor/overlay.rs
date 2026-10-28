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
/// Pages whose rasters are kept, so going back to a page doesn't render it again.
const CACHED_PAGES: usize = 3;

#[derive(Clone)]
pub struct CachedRaster {
    pub texture: TextureHandle,
    /// Covered area in page pixels.
    pub rect: Rect,
}

struct Entry {
    shape_key: u64,
    /// Raster scale it was rendered at; one at another scale is shown until replaced.
    scale: f32,
    /// Editable bbox origin (normalized) the raster was rendered at.
    origin: (f64, f64),
    raster: Option<CachedRaster>,
    overflow: bool,
}

struct PageRasters {
    page_id: String,
    scale: f32,
    revision: u64,
    complete: bool,
    entries: HashMap<String, Entry>,
    shadows_key: u64,
    shadows: Vec<CachedRaster>,
}

impl PageRasters {
    fn new(page_id: &str) -> Self {
        Self {
            page_id: page_id.to_owned(),
            scale: 0.0,
            revision: u64::MAX,
            complete: false,
            entries: HashMap::new(),
            shadows_key: 0,
            shadows: Vec::new(),
        }
    }
}

/// Rasters of the last few pages, the current one first.
#[derive(Default)]
pub struct OverlayCache {
    pages: Vec<PageRasters>,
}

/// Feeds serialized bytes straight into a hasher (no intermediate JSON buffer).
struct HashWriter<'a>(&'a mut DefaultHasher);

impl std::io::Write for HashWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.write(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn hash_into(hasher: &mut DefaultHasher, value: &impl serde::Serialize) {
    let _ = serde_json::to_writer(HashWriter(hasher), value);
}

/// Hash of a value's serialized form, for cache keys.
pub fn content_hash(value: &impl serde::Serialize) -> u64 {
    let mut hasher = DefaultHasher::new();
    hash_into(&mut hasher, value);
    hasher.finish()
}

/// Content hash ignoring the block position, so a moved block can reuse its raster.
fn shape_key(block: &TranslationBlock, page: &PageRecord) -> u64 {
    let mut shape = block.clone();
    shape.bbox.x = 0.0;
    shape.bbox.y = 0.0;
    if let Some(render) = &mut shape.render_bbox {
        render.x = 0.0;
        render.y = 0.0;
    }
    let mut hasher = DefaultHasher::new();
    hash_into(&mut hasher, &shape);
    (page.width, page.height).hash(&mut hasher);
    hasher.finish()
}

/// What group shadows depend on: the groups and their member blocks (not other blocks).
fn groups_key(page: &PageRecord) -> u64 {
    let Some(groups) = &page.block_groups else { return 0 };
    let mut hasher = DefaultHasher::new();
    hash_into(&mut hasher, groups);
    let members: HashSet<&str> = groups.iter().flat_map(|group| group.block_ids.iter().map(String::as_str)).collect();
    for block in page.blocks.iter().filter(|block| members.contains(block.id.as_str())) {
        hash_into(&mut hasher, block);
    }
    (page.width, page.height).hash(&mut hasher);
    hasher.finish() | 1
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
    fn current(&self) -> Option<&PageRasters> {
        self.pages.first()
    }

    pub fn is_complete(&self, revision: u64) -> bool {
        self.current().is_some_and(|page| page.complete && page.revision == revision)
    }

    /// Whether `sync` would have nothing to do, so the caller can skip taking the fonts.
    pub fn up_to_date(&self, page: &PageRecord, revision: u64, scale: f32, moving: &HashSet<String>) -> bool {
        moving.is_empty()
            && self.current().is_some_and(|cached| {
                cached.page_id == page.id && cached.complete && cached.revision == revision && cached.scale == scale
            })
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
        match self.pages.iter().position(|cached| cached.page_id == page.id) {
            Some(0) => {}
            Some(index) => {
                let cached = self.pages.remove(index);
                self.pages.insert(0, cached);
            }
            None => {
                self.pages.insert(0, PageRasters::new(&page.id));
                self.pages.truncate(CACHED_PAGES);
            }
        }
        let cached = &mut self.pages[0];
        if cached.scale != scale {
            cached.scale = scale;
            cached.complete = false;
        }
        if cached.complete && cached.revision == revision && moving.is_empty() {
            return false;
        }
        let started = Instant::now();
        let size = page_size(page);
        let live: HashSet<&str> = page.blocks.iter().map(|block| block.id.as_str()).collect();
        cached.entries.retain(|id, _| live.contains(id.as_str()));

        let mut pending = false;
        for block in &page.blocks {
            let key = shape_key(block, page);
            let block_origin = origin(block);
            if let Some(entry) = cached.entries.get(&block.id)
                && entry.shape_key == key
                && entry.scale == scale
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
            cached
                .entries
                .insert(block.id.clone(), Entry { shape_key: key, scale, origin: block_origin, raster, overflow });
        }

        let groups_key = groups_key(page);
        if !pending && groups_key != cached.shadows_key && moving.is_empty() {
            cached.shadows = render_group_shadows(fonts, page)
                .iter()
                .enumerate()
                .map(|(index, raster)| to_cached(ctx, &format!("group-shadow-{index}"), raster))
                .collect();
            cached.shadows_key = groups_key;
        }

        cached.revision = revision;
        cached.complete = !pending;
        pending
    }

    /// Raster for `block` and the page-pixel offset to draw it at (non-zero while dragging).
    pub fn raster(&self, block: &TranslationBlock, page: &PageRecord) -> Option<(CachedRaster, egui::Vec2)> {
        let entry = self.current().filter(|cached| cached.page_id == page.id)?.entries.get(&block.id)?;
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
        self.current().and_then(|cached| cached.entries.get(block_id)).is_some_and(|entry| entry.overflow)
    }

    pub fn shadows(&self, page: &PageRecord) -> &[CachedRaster] {
        self.current().filter(|cached| cached.page_id == page.id).map_or(&[], |cached| &cached.shadows)
    }
}
