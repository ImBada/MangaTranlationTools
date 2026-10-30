//! System fonts, CSS `font-family` resolution, shaping and glyph outlines.
//!
//! Text is shaped with cosmic-text (harfrust + font fallback) at em scale. Advances are
//! linear in font size because hinting is off, so a paragraph is shaped once and reused
//! for every candidate size during auto-fit.

use std::collections::HashMap;
use std::sync::Arc;

use cosmic_text::fontdb;
use cosmic_text::{Attrs, AttrsList, Family, FontSystem, ShapeLine, Shaping, Weight};
use swash::scale::image::Content;
use swash::scale::{Render, ScaleContext, Source, StrikeWith};
use swash::zeno::{Command, PathData};
use tiny_skia::{ColorU8, Path, PathBuilder, Pixmap, PremultipliedColorU8};
use unicode_normalization::UnicodeNormalization;

/// Weight at which a face counts as bold (`SYNTHETIC_BOLD_MIN_WEIGHT`).
pub const BOLD_WEIGHT_THRESHOLD: u16 = 600;
const MEASURE_CACHE_LIMIT: usize = 4096;
const COLOR_GLYPH_CACHE_LIMIT: usize = 512;
/// Colour glyphs are rasterized at most this many pixels per em and scaled up beyond.
pub const MAX_COLOR_GLYPH_PPEM: u32 = 1024;

/// A system font family as listed in the font picker (`SystemFont`).
#[derive(Debug, Clone, PartialEq)]
pub struct SystemFontFamily {
    /// Display name (Korean name preferred when available).
    pub family: String,
    /// All names this family is known by (localized family names).
    pub aliases: Vec<String>,
    pub weights: Vec<u16>,
    /// `font-family` value stored on blocks/presets.
    pub css_family: String,
}

/// One entry of a CSS `font-family` list.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FamilyName {
    Named(String),
    SansSerif,
    Serif,
    Monospace,
    Cursive,
    Fantasy,
}

/// Font chosen for a CSS family list and weight.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FontSpec {
    pub family: FamilyName,
    pub weight: u16,
}

#[derive(Debug, Clone)]
struct FamilyInfo {
    /// fontdb's canonical (first) family name used for matching.
    match_name: String,
    weights: Vec<u16>,
    /// `wght` axis range when the family has a variable face.
    variable_range: Option<(f32, f32)>,
}

impl FamilyInfo {
    fn variable_bold(&self) -> bool {
        self.variable_range.is_some_and(|(_, max)| max >= f32::from(BOLD_WEIGHT_THRESHOLD))
    }

    /// CSS font matching for weight: the installed weight the browser would pick.
    fn matched_weight(&self, desired: u16) -> u16 {
        if self.variable_range.is_some_and(|(min, max)| (min..=max).contains(&f32::from(desired))) {
            return desired;
        }
        css_weight_match(&self.weights, desired).unwrap_or(desired)
    }
}

/// CSS Fonts 4 §5.2 weight matching over the installed weights.
pub fn css_weight_match(available: &[u16], desired: u16) -> Option<u16> {
    if available.contains(&desired) {
        return Some(desired);
    }
    let below = || available.iter().copied().filter(|w| *w < desired).max();
    let above = || available.iter().copied().filter(|w| *w > desired).min();
    if (400..=500).contains(&desired) {
        available
            .iter()
            .copied()
            .filter(|w| *w > desired && *w <= 500)
            .min()
            .or_else(below)
            .or_else(|| available.iter().copied().filter(|w| *w > 500).min())
    } else if desired < 400 {
        below().or_else(above)
    } else {
        above().or_else(below)
    }
}

/// A glyph positioned in em units relative to the run origin (alphabetic baseline).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapedGlyph {
    pub font_id: fontdb::ID,
    pub weight: u16,
    pub glyph_id: u16,
    pub x_em: f32,
    pub x_offset_em: f32,
    pub y_offset_em: f32,
    /// Byte offset of the glyph's cluster in the shaped text.
    pub cluster: usize,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShapedRun {
    pub glyphs: Vec<ShapedGlyph>,
    pub advance_em: f32,
    /// Advance per `char` (em), cluster advances assigned to the cluster's first char.
    pub char_advances_em: Vec<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VerticalMetrics {
    /// Distance from the em-box top (`textBaseline = "top"`) to the alphabetic baseline, in em.
    pub top_to_baseline_em: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InkBounds {
    pub ascent_em: f32,
    pub descent_em: f32,
}

#[derive(Hash, PartialEq, Eq)]
struct MeasureKey {
    text: String,
    spans: Vec<(usize, usize, FontSpec)>,
    base: FontSpec,
}

#[derive(Hash, PartialEq, Eq, Clone, Copy)]
struct OutlineKey {
    font_id: fontdb::ID,
    weight: u16,
    glyph_id: u16,
}

/// A colour glyph (emoji) rasterized at some pixels per em: premultiplied pixels and the
/// image's top-left corner relative to the glyph origin, in pixels with y down.
#[derive(Debug)]
pub struct ColorGlyph {
    pub pixmap: Pixmap,
    pub left: i32,
    pub top: i32,
}

pub struct FontContext {
    font_system: FontSystem,
    scale_context: ScaleContext,
    families: HashMap<String, FamilyInfo>,
    family_list: Vec<SystemFontFamily>,
    measure_cache: HashMap<MeasureKey, Arc<ShapedRun>>,
    outline_cache: HashMap<OutlineKey, Option<Arc<Path>>>,
    metrics_cache: HashMap<FontSpec, VerticalMetrics>,
    /// Whether a face has colour glyphs (`COLR`, `CBDT` or `sbix`).
    color_faces: HashMap<fontdb::ID, bool>,
    color_cache: HashMap<(OutlineKey, u32), Option<Arc<ColorGlyph>>>,
}

impl FontContext {
    /// Loads every system font (same directories as the Node server).
    pub fn new() -> Self {
        Self::from_font_system(FontSystem::new())
    }

    /// Context with only the given font files; used by tests for deterministic output.
    pub fn with_fonts(fonts: impl IntoIterator<Item = fontdb::Source>) -> Self {
        Self::from_font_system(FontSystem::new_with_fonts(fonts))
    }

    /// A context of its own over the same font database (cheap: faces are shared), so a
    /// background job can render without holding the editor's context.
    pub fn fork(&self) -> Self {
        let locale = self.font_system.locale().to_owned();
        Self::from_font_system(FontSystem::new_with_locale_and_db(locale, self.font_system.db().clone()))
    }

    fn from_font_system(mut font_system: FontSystem) -> Self {
        set_browser_generic_families(font_system.db_mut());
        let (families, family_list) = index_families(font_system.db());
        Self {
            font_system,
            scale_context: ScaleContext::new(),
            families,
            family_list,
            measure_cache: HashMap::new(),
            outline_cache: HashMap::new(),
            metrics_cache: HashMap::new(),
            color_faces: HashMap::new(),
            color_cache: HashMap::new(),
        }
    }

    pub fn font_system_mut(&mut self) -> &mut FontSystem {
        &mut self.font_system
    }

    pub fn system_families(&self) -> &[SystemFontFamily] {
        &self.family_list
    }

    /// Raw font file bytes + face index for a family (used to register UI fonts in egui).
    pub fn family_font_data(&self, family: &str) -> Option<(Vec<u8>, u32)> {
        let info = self.families.get(&normalize_family_key(family))?;
        let id = self.font_system.db().query(&fontdb::Query {
            families: &[fontdb::Family::Name(&info.match_name)],
            weight: fontdb::Weight::NORMAL,
            ..fontdb::Query::default()
        })?;
        self.font_system.db().with_face_data(id, |data, index| (data.to_vec(), index))
    }

    /// Picks the first installed family of a CSS list and the installed weight closest to
    /// `weight` by CSS rules (cosmic-text itself only accepts exact weight matches).
    pub fn resolve_spec(&self, css_family: &str, weight: u16) -> FontSpec {
        let (family, info) = self.resolve_family(css_family);
        let weight = info.map(|info| info.matched_weight(weight)).unwrap_or(weight);
        match (family, info) {
            (FamilyName::Named(_), Some(info)) => {
                FontSpec { family: FamilyName::Named(info.match_name.clone()), weight }
            }
            (family, _) => FontSpec { family, weight },
        }
    }

    /// The family [`FontContext::resolve_spec`] picks from a CSS list: the first installed
    /// named family, else the first generic one (sans-serif when there is none), with the
    /// installed family it stands for when known.
    fn resolve_family(&self, css_family: &str) -> (FamilyName, Option<&FamilyInfo>) {
        for family in parse_css_font_family(css_family) {
            match &family {
                FamilyName::Named(name) => {
                    if let Some(info) = self.families.get(&normalize_family_key(name)) {
                        return (family, Some(info));
                    }
                }
                generic => return (generic.clone(), self.generic_family(generic)),
            }
        }
        (FamilyName::SansSerif, self.generic_family(&FamilyName::SansSerif))
    }

    /// The installed family a generic family maps to.
    fn generic_family(&self, family: &FamilyName) -> Option<&FamilyInfo> {
        let family = db_family(family);
        self.families.get(&normalize_family_key(self.font_system.db().family_name(&family)))
    }

    /// Installed weights of the primary family in a CSS list (`FontWeightAvailability`).
    pub fn family_weights(&self, css_family: &str) -> Option<(Vec<u16>, bool)> {
        let primary = parse_css_font_family(css_family).into_iter().next()?;
        let FamilyName::Named(name) = primary else {
            return None;
        };
        self.families.get(&normalize_family_key(&name)).map(|info| (info.weights.clone(), info.variable_bold()))
    }

    /// Installed weights of the family [`FontContext::resolve_spec`] picks from a CSS list,
    /// and whether it has a variable bold.
    pub fn resolved_family_weights(&self, css_family: &str) -> Option<(Vec<u16>, bool)> {
        self.resolve_family(css_family).1.map(|info| (info.weights.clone(), info.variable_bold()))
    }

    /// Shapes `text` with `base`, overriding the font for `spans` (byte ranges).
    pub fn shape(&mut self, text: &str, base: &FontSpec, spans: &[(usize, usize, FontSpec)]) -> Arc<ShapedRun> {
        let key = MeasureKey { text: text.to_owned(), spans: spans.to_vec(), base: base.clone() };
        if let Some(run) = self.measure_cache.get(&key) {
            return Arc::clone(run);
        }
        if self.measure_cache.len() >= MEASURE_CACHE_LIMIT {
            self.measure_cache.clear();
        }
        let run = Arc::new(shape_text(&mut self.font_system, text, base, spans));
        self.measure_cache.insert(key, Arc::clone(&run));
        run
    }

    /// Glyph outline in em units with y pointing down (canvas orientation).
    pub fn glyph_outline(&mut self, font_id: fontdb::ID, weight: u16, glyph_id: u16) -> Option<Arc<Path>> {
        let key = OutlineKey { font_id, weight, glyph_id };
        if let Some(path) = self.outline_cache.get(&key) {
            return path.clone();
        }
        let path = self.build_outline(key).map(Arc::new);
        self.outline_cache.insert(key, path.clone());
        path
    }

    fn build_outline(&mut self, key: OutlineKey) -> Option<Path> {
        let font = self.font_system.get_font(key.font_id, Weight(key.weight))?;
        let swash_font = font.as_swash();
        let units_per_em = f32::from(swash_font.metrics(&[]).units_per_em.max(1));
        let mut builder = self.scale_context.builder(swash_font).size(units_per_em).hint(false);
        if let Some(axis) = swash_font.variations().find_by_tag(swash::Tag::from_be_bytes(*b"wght")) {
            let value = f32::from(key.weight).clamp(axis.min_value(), axis.max_value());
            builder = builder.normalized_coords(swash_font.variations().normalized_coords([(axis.tag(), value)]));
        }
        let mut scaler = builder.build();
        let outline = scaler.scale_outline(key.glyph_id)?;
        let scale = 1.0 / units_per_em;
        let mut path = PathBuilder::new();
        for command in outline.path().commands() {
            match command {
                Command::MoveTo(p) => path.move_to(p.x * scale, -p.y * scale),
                Command::LineTo(p) => path.line_to(p.x * scale, -p.y * scale),
                Command::QuadTo(c, p) => path.quad_to(c.x * scale, -c.y * scale, p.x * scale, -p.y * scale),
                Command::CurveTo(c1, c2, p) => {
                    path.cubic_to(c1.x * scale, -c1.y * scale, c2.x * scale, -c2.y * scale, p.x * scale, -p.y * scale)
                }
                Command::Close => path.close(),
            }
        }
        path.finish()
    }

    /// The colour image of a glyph (layered `COLR` outlines or an embedded colour bitmap,
    /// as emoji fonts draw them) at `ppem` pixels per em (at most 1024), `None` for glyphs
    /// without one.
    pub fn color_glyph(
        &mut self,
        font_id: fontdb::ID,
        weight: u16,
        glyph_id: u16,
        ppem: u32,
    ) -> Option<Arc<ColorGlyph>> {
        if !self.has_color_glyphs(font_id, weight) {
            return None;
        }
        let key = (OutlineKey { font_id, weight, glyph_id }, ppem.clamp(1, MAX_COLOR_GLYPH_PPEM));
        if let Some(glyph) = self.color_cache.get(&key) {
            return glyph.clone();
        }
        if self.color_cache.len() >= COLOR_GLYPH_CACHE_LIMIT {
            self.color_cache.clear();
        }
        let glyph = self.render_color_glyph(key.0, key.1).map(Arc::new);
        self.color_cache.insert(key, glyph.clone());
        glyph
    }

    fn has_color_glyphs(&mut self, font_id: fontdb::ID, weight: u16) -> bool {
        if let Some(color) = self.color_faces.get(&font_id) {
            return *color;
        }
        let color = self.font_system.get_font(font_id, Weight(weight)).is_some_and(|font| {
            let font = font.as_swash();
            [*b"COLR", *b"CBDT", *b"sbix"].into_iter().any(|tag| font.table(swash::Tag::from_be_bytes(tag)).is_some())
        });
        self.color_faces.insert(font_id, color);
        color
    }

    fn render_color_glyph(&mut self, key: OutlineKey, ppem: u32) -> Option<ColorGlyph> {
        let font = self.font_system.get_font(key.font_id, Weight(key.weight))?;
        let mut scaler = self.scale_context.builder(font.as_swash()).size(ppem as f32).hint(false).build();
        let image = Render::new(&[Source::ColorOutline(0), Source::ColorBitmap(StrikeWith::BestFit)])
            .render(&mut scaler, key.glyph_id)
            .filter(|image| image.content == Content::Color)?;
        let (width, height) = (image.placement.width, image.placement.height);
        let mut pixmap = Pixmap::new(width, height)?;
        // Layers are blended over transparent black, so they come out premultiplied;
        // embedded bitmaps are decoded PNGs with straight alpha.
        let premultiplied = matches!(image.source, Source::ColorOutline(_));
        for (pixel, &[r, g, b, a]) in pixmap.pixels_mut().iter_mut().zip(image.data.as_chunks::<4>().0) {
            *pixel = if premultiplied {
                PremultipliedColorU8::from_rgba(r.min(a), g.min(a), b.min(a), a)
                    .unwrap_or(PremultipliedColorU8::TRANSPARENT)
            } else {
                ColorU8::from_rgba(r, g, b, a).premultiply()
            };
        }
        Some(ColorGlyph { pixmap, left: image.placement.left, top: -image.placement.top })
    }

    /// Em-box top → baseline distance for `spec`, following Chromium's normalized typo
    /// ascent (`textBaseline = "top"`).
    pub fn vertical_metrics(&mut self, spec: &FontSpec) -> VerticalMetrics {
        if let Some(metrics) = self.metrics_cache.get(spec) {
            return *metrics;
        }
        let metrics = self
            .primary_font_id(spec)
            .and_then(|id| self.font_system.get_font(id, Weight(spec.weight)))
            .map(|font| {
                let swash_font = font.as_swash();
                let (ascent, descent) = typo_ascent_descent(&swash_font).unwrap_or_else(|| {
                    let metrics = swash_font.metrics(&[]);
                    let units = f32::from(metrics.units_per_em.max(1));
                    (metrics.ascent / units, metrics.descent.abs() / units)
                });
                let height = ascent + descent;
                VerticalMetrics { top_to_baseline_em: if height > 0.0 { ascent / height } else { 0.8 } }
            })
            .unwrap_or(VerticalMetrics { top_to_baseline_em: 0.8 });
        self.metrics_cache.insert(spec.clone(), metrics);
        metrics
    }

    /// Ink extent of `text` (`actualBoundingBoxAscent/Descent`), in em.
    pub fn ink_bounds(&mut self, text: &str, spec: &FontSpec) -> Option<InkBounds> {
        let run = self.shape(text, spec, &[]);
        let mut top = f32::INFINITY;
        let mut bottom = f32::NEG_INFINITY;
        for glyph in &run.glyphs {
            if let Some(path) = self.glyph_outline(glyph.font_id, glyph.weight, glyph.glyph_id) {
                let bounds = path.bounds();
                top = top.min(bounds.top() - glyph.y_offset_em);
                bottom = bottom.max(bounds.bottom() - glyph.y_offset_em);
            }
        }
        (top.is_finite() && bottom.is_finite()).then_some(InkBounds { ascent_em: -top, descent_em: bottom })
    }

    /// PostScript name of the face that draws most of `text` in `spec` (fallback faces
    /// included), else of the face `spec` matches, else the family name.
    pub fn post_script_name(&mut self, text: &str, spec: &FontSpec) -> String {
        let run = self.shape(text, spec, &[]);
        let mut uses: Vec<(fontdb::ID, usize)> = Vec::new();
        for glyph in &run.glyphs {
            match uses.iter_mut().find(|(id, _)| *id == glyph.font_id) {
                Some((_, count)) => *count += 1,
                None => uses.push((glyph.font_id, 1)),
            }
        }
        // `max_by_key` keeps the last of equals: reversed, the first face used.
        let used = uses.into_iter().rev().max_by_key(|(_, count)| *count).map(|(id, _)| id);
        let db = self.font_system.db();
        used.or_else(|| self.primary_font_id(spec))
            .and_then(|id| db.face(id))
            .map(|face| face.post_script_name.clone())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| match &spec.family {
                FamilyName::Named(name) => name.clone(),
                generic => db.family_name(&db_family(generic)).to_owned(),
            })
    }

    fn primary_font_id(&self, spec: &FontSpec) -> Option<fontdb::ID> {
        self.font_system.db().query(&fontdb::Query {
            families: &[db_family(&spec.family)],
            weight: fontdb::Weight(spec.weight),
            ..fontdb::Query::default()
        })
    }
}

fn db_family(family: &FamilyName) -> fontdb::Family<'_> {
    match family {
        FamilyName::Named(name) => fontdb::Family::Name(name),
        FamilyName::SansSerif => fontdb::Family::SansSerif,
        FamilyName::Serif => fontdb::Family::Serif,
        FamilyName::Monospace => fontdb::Family::Monospace,
        FamilyName::Cursive => fontdb::Family::Cursive,
        FamilyName::Fantasy => fontdb::Family::Fantasy,
    }
}

impl Default for FontContext {
    fn default() -> Self {
        Self::new()
    }
}

/// Chrome's default generic families for a Korean UI locale (what the Electron app used),
/// so unstyled text falls back the same way.
fn set_browser_generic_families(db: &mut fontdb::Database) {
    let candidates: &[(fontdb::Family<'_>, &[&str])] = if cfg!(target_os = "macos") {
        &[
            (fontdb::Family::SansSerif, &["Apple SD Gothic Neo", "Helvetica"]),
            (fontdb::Family::Serif, &["AppleMyungjo", "Times"]),
            (fontdb::Family::Monospace, &["Courier"]),
            (fontdb::Family::Cursive, &["Apple Chancery"]),
            (fontdb::Family::Fantasy, &["Papyrus"]),
        ]
    } else if cfg!(target_os = "windows") {
        &[
            (fontdb::Family::SansSerif, &["Malgun Gothic", "Arial"]),
            (fontdb::Family::Serif, &["Batang", "Times New Roman"]),
            (fontdb::Family::Monospace, &["Consolas", "Courier New"]),
            (fontdb::Family::Cursive, &["Comic Sans MS"]),
            (fontdb::Family::Fantasy, &["Impact"]),
        ]
    } else {
        &[
            (fontdb::Family::SansSerif, &["Noto Sans CJK KR", "Noto Sans KR", "DejaVu Sans", "Noto Sans"]),
            (fontdb::Family::Serif, &["Noto Serif CJK KR", "Noto Serif KR", "DejaVu Serif", "Noto Serif"]),
            (fontdb::Family::Monospace, &["DejaVu Sans Mono", "Noto Sans Mono", "Liberation Mono"]),
        ]
    };
    for (generic, names) in candidates {
        let installed = names.iter().find(|name| {
            db.faces().any(|face| face.families.iter().any(|(family, _)| family.eq_ignore_ascii_case(name)))
        });
        let Some(name) = installed else {
            continue;
        };
        match generic {
            fontdb::Family::SansSerif => db.set_sans_serif_family(*name),
            fontdb::Family::Serif => db.set_serif_family(*name),
            fontdb::Family::Monospace => db.set_monospace_family(*name),
            fontdb::Family::Cursive => db.set_cursive_family(*name),
            fontdb::Family::Fantasy => db.set_fantasy_family(*name),
            fontdb::Family::Name(_) => {}
        }
    }
}

fn attrs_for(spec: &FontSpec) -> Attrs<'_> {
    let family = match &spec.family {
        FamilyName::Named(name) => Family::Name(name),
        FamilyName::SansSerif => Family::SansSerif,
        FamilyName::Serif => Family::Serif,
        FamilyName::Monospace => Family::Monospace,
        FamilyName::Cursive => Family::Cursive,
        FamilyName::Fantasy => Family::Fantasy,
    };
    Attrs::new().family(family).weight(Weight(spec.weight))
}

fn shape_text(
    font_system: &mut FontSystem,
    text: &str,
    base: &FontSpec,
    spans: &[(usize, usize, FontSpec)],
) -> ShapedRun {
    let char_starts: Vec<usize> = text.char_indices().map(|(index, _)| index).collect();
    let mut run = ShapedRun { char_advances_em: vec![0.0; char_starts.len()], ..ShapedRun::default() };
    if text.is_empty() {
        return run;
    }
    let mut attrs_list = AttrsList::new(&attrs_for(base));
    for (start, end, spec) in spans {
        attrs_list.add_span(*start..*end, &attrs_for(spec));
    }
    let line = ShapeLine::new(font_system, text, &attrs_list, Shaping::Advanced, 4);
    let mut x = 0.0f32;
    for span in &line.spans {
        for word in &span.words {
            for glyph in &word.glyphs {
                run.glyphs.push(ShapedGlyph {
                    font_id: glyph.font_id,
                    weight: glyph.font_weight.0,
                    glyph_id: glyph.glyph_id,
                    x_em: x,
                    x_offset_em: glyph.x_offset,
                    y_offset_em: glyph.y_offset,
                    cluster: glyph.start,
                });
                let char_index = char_starts.partition_point(|start| *start < glyph.start).min(char_starts.len() - 1);
                run.char_advances_em[char_index] += glyph.x_advance;
                x += glyph.x_advance;
            }
        }
    }
    run.advance_em = x;
    run
}

/// OS/2 `sTypoAscender`/`sTypoDescender` in em.
fn typo_ascent_descent(font: &swash::FontRef<'_>) -> Option<(f32, f32)> {
    let os2 = font.table(swash::Tag::from_be_bytes(*b"OS/2"))?;
    let head = font.table(swash::Tag::from_be_bytes(*b"head"))?;
    if os2.len() < 72 || head.len() < 20 {
        return None;
    }
    let units = f32::from(u16::from_be_bytes([head[18], head[19]]).max(1));
    let ascent = f32::from(i16::from_be_bytes([os2[68], os2[69]]));
    let descent = f32::from(i16::from_be_bytes([os2[70], os2[71]]));
    (ascent > 0.0).then_some((ascent / units, descent.abs() / units))
}

/// Splits a CSS `font-family` list, honoring quotes (`splitCssFontFamilies`).
pub fn parse_css_font_family(css: &str) -> Vec<FamilyName> {
    let mut families = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut push = |raw: &str| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return;
        }
        let unquoted = match trimmed.chars().next() {
            Some(q @ ('"' | '\'')) if trimmed.len() >= 2 && trimmed.ends_with(q) => &trimmed[1..trimmed.len() - 1],
            _ => trimmed,
        };
        let name = unquoted.trim();
        let quoted = unquoted.len() != trimmed.len();
        let generic = if quoted {
            None
        } else {
            match name.to_ascii_lowercase().as_str() {
                "sans-serif" | "system-ui" | "ui-sans-serif" => Some(FamilyName::SansSerif),
                "serif" | "ui-serif" => Some(FamilyName::Serif),
                "monospace" | "ui-monospace" => Some(FamilyName::Monospace),
                "cursive" => Some(FamilyName::Cursive),
                "fantasy" => Some(FamilyName::Fantasy),
                _ => None,
            }
        };
        families.push(generic.unwrap_or_else(|| FamilyName::Named(name.to_owned())));
    };
    for ch in css.chars() {
        match (ch, quote) {
            ('"' | '\'', None) => {
                quote = Some(ch);
                current.push(ch);
            }
            (c, Some(q)) if c == q => {
                quote = None;
                current.push(ch);
            }
            (',', None) => {
                push(&current);
                current.clear();
            }
            _ => current.push(ch),
        }
    }
    push(&current);
    families
}

/// `normalizeCssFontFamily`: NFKC, lowercase, collapsed whitespace.
pub fn normalize_family_key(name: &str) -> String {
    let trimmed = name.trim().trim_matches(|c| c == '"' || c == '\'');
    trimmed.nfkc().collect::<String>().to_lowercase().split_whitespace().collect::<Vec<_>>().join(" ")
}

fn quote_css_family(family: &str) -> String {
    format!("\"{}\"", family.replace('\\', "\\\\").replace('"', "\\\""))
}

fn contains_hangul(value: &str) -> bool {
    value.chars().any(|ch| ('\u{AC00}'..='\u{D7A3}').contains(&ch) || ('\u{3130}'..='\u{318F}').contains(&ch))
}

fn index_families(db: &fontdb::Database) -> (HashMap<String, FamilyInfo>, Vec<SystemFontFamily>) {
    struct Group {
        match_name: String,
        aliases: Vec<String>,
        weights: Vec<u16>,
        variable_range: Option<(f32, f32)>,
    }
    let mut groups: Vec<Group> = Vec::new();
    let mut by_key: HashMap<String, usize> = HashMap::new();

    for face in db.faces() {
        let Some((primary, _)) = face.families.first() else {
            continue;
        };
        let key = normalize_family_key(primary);
        let index = *by_key.entry(key).or_insert_with(|| {
            groups.push(Group {
                match_name: primary.clone(),
                aliases: Vec::new(),
                weights: Vec::new(),
                variable_range: None,
            });
            groups.len() - 1
        });
        let group = &mut groups[index];
        for (name, _) in &face.families {
            if !group.aliases.iter().any(|alias| normalize_family_key(alias) == normalize_family_key(name)) {
                group.aliases.push(name.clone());
            }
        }
        if !group.weights.contains(&face.weight.0) {
            group.weights.push(face.weight.0);
        }
        if group.variable_range.is_none()
            && let Some(Some(range)) = db.with_face_data(face.id, |data, face_index| {
                swash::FontRef::from_index(data, face_index as usize)
                    .and_then(|font| font.variations().find_by_tag(swash::Tag::from_be_bytes(*b"wght")))
                    .map(|axis| (axis.min_value(), axis.max_value()))
            })
        {
            group.variable_range = Some(range);
        }
    }

    let mut families = HashMap::new();
    let mut list = Vec::with_capacity(groups.len());
    for group in groups {
        let mut weights = group.weights.clone();
        weights.sort_unstable();
        let info = FamilyInfo {
            match_name: group.match_name.clone(),
            weights: weights.clone(),
            variable_range: group.variable_range,
        };
        for alias in &group.aliases {
            families.entry(normalize_family_key(alias)).or_insert_with(|| info.clone());
        }
        let display = group
            .aliases
            .iter()
            .find(|alias| contains_hangul(alias))
            .cloned()
            .unwrap_or_else(|| group.match_name.clone());
        let mut ordered = vec![group.match_name.clone()];
        ordered.extend(group.aliases.iter().filter(|alias| **alias != group.match_name).cloned());
        list.push(SystemFontFamily {
            css_family: ordered.iter().map(|name| quote_css_family(name)).collect::<Vec<_>>().join(", "),
            family: display,
            aliases: group.aliases,
            weights,
        });
    }
    list.sort_by_key(|family| family.family.to_lowercase());
    (families, list)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn css_family_parsing() {
        let families = parse_css_font_family("\"Malgun Gothic\", 'Apple SD Gothic Neo', sans-serif, \"serif\"");
        assert_eq!(
            families,
            vec![
                FamilyName::Named("Malgun Gothic".into()),
                FamilyName::Named("Apple SD Gothic Neo".into()),
                FamilyName::SansSerif,
                FamilyName::Named("serif".into()),
            ]
        );
        assert_eq!(
            parse_css_font_family("A, \"B, C\""),
            vec![FamilyName::Named("A".into()), FamilyName::Named("B, C".into())]
        );
    }

    #[test]
    fn css_weight_matching() {
        let weights = [300, 500, 700];
        assert_eq!(css_weight_match(&weights, 400), Some(500));
        assert_eq!(css_weight_match(&weights, 450), Some(500));
        assert_eq!(css_weight_match(&[300, 700], 400), Some(300));
        assert_eq!(css_weight_match(&[700, 900], 400), Some(700));
        assert_eq!(css_weight_match(&weights, 200), Some(300));
        assert_eq!(css_weight_match(&weights, 350), Some(300));
        assert_eq!(css_weight_match(&weights, 600), Some(700));
        assert_eq!(css_weight_match(&[300, 500], 800), Some(500));
        assert_eq!(css_weight_match(&[], 400), None);
    }

    #[test]
    fn family_key_normalization() {
        assert_eq!(normalize_family_key("  \"Apple  SD Gothic Neo\" "), "apple sd gothic neo");
        assert_eq!(normalize_family_key("ＡＢＣ"), "abc");
    }
}
