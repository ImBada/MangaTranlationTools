//! Core data model and storage for MangaTranslationTools (native).

pub mod cbz;
pub mod error;
pub mod font_presets;
pub mod geometry;
pub mod groups;
pub mod image_io;
pub mod import;
pub mod json_io;
pub mod library;
pub mod library_filter;
pub mod model;
pub mod paths;
pub mod preset_backups;
pub mod settings;
pub mod text;
pub mod time;

pub use error::{CoreError, CoreResult};
