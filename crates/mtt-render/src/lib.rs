//! Translation overlay renderer shared by the editor view and PNG export.
//!
//! Ports `overlayLayout.ts`, `pageRender.ts`, `fontFilterEffects.ts` and
//! `hardTextRendering.ts` onto cosmic-text (shaping) and tiny-skia (rasterization).

pub mod block;
pub mod canvas;
pub mod effects;
pub mod fonts;
pub mod layout;
pub mod page;

pub use block::{BlockRaster, BlockRenderOptions, BlockText, block_text, render_block};
pub use fonts::FontContext;
pub use layout::Size;
pub use page::{PageImages, PageRenderOptions, render_page};
