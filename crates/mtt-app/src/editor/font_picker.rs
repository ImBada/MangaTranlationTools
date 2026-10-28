//! Font family picker (`FontFamilyPicker.tsx`): the built-in Korean-labelled choices plus
//! installed families, grouped 한국어/영어/일본어/기타, the five most recent picks on top,
//! NFKC whitespace-insensitive search, and a rendered "오늘의 번역 Aa 123" preview per row.

use std::collections::HashMap;
use std::sync::LazyLock;

use eframe::egui::{self, RichText, TextureHandle};
use mtt_core::font_presets::DEFAULT_OVERLAY_FONT_FAMILY;
use mtt_core::model::{BBox, TranslationBlock};
use unicode_normalization::UnicodeNormalization;

use crate::services::Services;

/// `FONT_FAMILY_OPTIONS`.
const BUILTIN: [(&str, &str); 5] = [
    ("맑은 고딕", DEFAULT_OVERLAY_FONT_FAMILY),
    ("Apple SD Gothic Neo", "\"Apple SD Gothic Neo\", \"Malgun Gothic\", sans-serif"),
    ("본고딕", "\"Noto Sans CJK KR\", \"Noto Sans KR\", \"Malgun Gothic\", sans-serif"),
    ("바탕", "Batang, \"AppleMyungjo\", serif"),
    ("돋움", "Dotum, \"Apple SD Gothic Neo\", sans-serif"),
];
const PREVIEW_TEXT: &str = "오늘의 번역 Aa 123";
/// Previews rendered per frame so scrolling stays smooth.
const PREVIEWS_PER_FRAME: usize = 6;

#[derive(Debug, Clone, PartialEq)]
pub struct FontOption {
    pub label: String,
    pub value: String,
    pub weights: Vec<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Group {
    Korean,
    English,
    Japanese,
    Other,
}

impl Group {
    fn label(self) -> &'static str {
        match self {
            Group::Korean => "한국어",
            Group::English => "영어",
            Group::Japanese => "일본어",
            Group::Other => "기타",
        }
    }
}

static KOREAN_WORDS: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\b(?:kr|korean|korea|hangul|hangeul|malgun|batang|dotum|gulim|gungsuh|nanum|kopub|pretendard|spoqa|seoul|jeju|hamchorom)\b")
        .expect("valid regex")
});
static JAPANESE_WORDS: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"\b(?:jp|japanese|japan|jis|hiragino|meiryo|osaka|kozuka)\b").expect("valid regex")
});

/// `detectFontLanguageGroup`.
fn language_group(option: &FontOption) -> Group {
    let name: String = format!("{} {}", option.label, option.value).nfkc().collect::<String>().to_lowercase();
    let has = |ranges: &[(u32, u32)]| name.chars().any(|c| ranges.iter().any(|(a, b)| (*a..=*b).contains(&(c as u32))));
    if has(&[(0x1100, 0x11ff), (0x3130, 0x318f), (0xac00, 0xd7af)])
        || KOREAN_WORDS.is_match(&name)
        || ["apple sd gothic", "noto sans cjk kr", "noto serif cjk kr", "source han sans k", "source han serif k"]
            .iter()
            .any(|needle| name.contains(needle))
    {
        return Group::Korean;
    }
    if has(&[(0x3040, 0x30ff)])
        || JAPANESE_WORDS.is_match(&name)
        || [
            "yu gothic",
            "yu mincho",
            "noto sans cjk jp",
            "noto serif cjk jp",
            "source han sans j",
            "source han serif j",
        ]
        .iter()
        .any(|needle| name.contains(needle))
    {
        return Group::Japanese;
    }
    if name.is_ascii() { Group::English } else { Group::Other }
}

/// `normalizeFontSearchText`.
fn search_key(value: &str) -> String {
    value.nfkc().collect::<String>().to_lowercase().chars().filter(|c| !c.is_whitespace()).collect()
}

/// An option with what grouping, sorting and search need, worked out once.
struct IndexedOption {
    option: FontOption,
    group: Group,
    sort_label: String,
    search: String,
}

impl IndexedOption {
    fn new(option: FontOption) -> Self {
        let group = language_group(&option);
        let sort_label = option.label.to_lowercase();
        let search = search_key(&format!("{} {}", option.label, option.value));
        Self { option, group, sort_label, search }
    }

    fn sort_key(&self) -> (Group, &str) {
        (self.group, &self.sort_label)
    }
}

/// `buildFontFamilyOptions` without the current value: built-ins plus installed families,
/// deduplicated and sorted by language group then label.
fn index_options(families: &[mtt_render::fonts::SystemFontFamily]) -> Vec<IndexedOption> {
    let mut options: Vec<FontOption> = BUILTIN
        .iter()
        .map(|(label, value)| FontOption { label: (*label).into(), value: (*value).into(), weights: Vec::new() })
        .collect();
    let mut by_value: HashMap<String, usize> =
        options.iter().enumerate().map(|(index, option)| (option.value.clone(), index)).collect();
    for family in families {
        match by_value.get(&family.css_family) {
            Some(&index) if options[index].weights.is_empty() => options[index].weights = family.weights.clone(),
            Some(_) => {}
            None => {
                by_value.insert(family.css_family.clone(), options.len());
                options.push(FontOption {
                    label: family.family.clone(),
                    value: family.css_family.clone(),
                    weights: family.weights.clone(),
                });
            }
        }
    }
    let mut indexed: Vec<IndexedOption> = options.into_iter().map(IndexedOption::new).collect();
    indexed.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
    indexed
}

/// The options in display order, with `selected` added in its place when it isn't one of
/// them (a family that isn't installed).
fn with_selected<'a>(index: &'a [IndexedOption], extra: Option<&'a IndexedOption>) -> Vec<&'a IndexedOption> {
    let mut options: Vec<&IndexedOption> = index.iter().collect();
    if let Some(extra) = extra {
        let at = options.partition_point(|option| option.sort_key() <= extra.sort_key());
        options.insert(at, extra);
    }
    options
}

#[cfg(test)]
fn build_options(families: &[mtt_render::fonts::SystemFontFamily], selected: &str) -> Vec<FontOption> {
    let index = index_options(families);
    let extra = (!selected.is_empty() && !index.iter().any(|option| option.option.value == selected)).then(|| {
        IndexedOption::new(FontOption { label: selected.into(), value: selected.into(), weights: Vec::new() })
    });
    with_selected(&index, extra.as_ref()).into_iter().map(|option| option.option.clone()).collect()
}

/// The option list, built once with the built-ins and again when the installed families
/// arrive (the font system loads in the background; its family list doesn't change after).
#[derive(Default)]
struct OptionIndex {
    with_system_fonts: bool,
    options: Vec<IndexedOption>,
}

impl OptionIndex {
    fn refresh(&mut self, services: &mut Services) {
        if self.with_system_fonts {
            return;
        }
        let system = services.fonts().and_then(|fonts| fonts.lock().ok().map(|f| index_options(f.system_families())));
        match system {
            Some(options) => {
                self.options = options;
                self.with_system_fonts = true;
            }
            None if self.options.is_empty() => self.options = index_options(&[]),
            None => {}
        }
    }
}

/// Rendered previews keyed by css family (`None` = nothing to draw).
#[derive(Default)]
pub struct FontPreviews {
    textures: HashMap<String, Option<TextureHandle>>,
    rendered_this_frame: usize,
    options: OptionIndex,
}

impl FontPreviews {
    fn get(&mut self, ctx: &egui::Context, services: &mut Services, value: &str) -> Option<TextureHandle> {
        if let Some(texture) = self.textures.get(value) {
            return texture.clone();
        }
        if self.rendered_this_frame >= PREVIEWS_PER_FRAME {
            ctx.request_repaint();
            return None;
        }
        self.rendered_this_frame += 1;
        let texture = render_preview(ctx, services, value);
        self.textures.insert(value.to_owned(), texture.clone());
        texture
    }
}

impl FontPreviews {
    /// Preset tag rendered in the preset's own style at ~18px (`buildPresetTagTextStyles`).
    pub fn preset_tag(
        &mut self,
        ctx: &egui::Context,
        services: &mut Services,
        preset: &mtt_core::model::FontPreset,
    ) -> Option<TextureHandle> {
        let key = format!("preset:{:x}", super::overlay::content_hash(preset));
        if let Some(texture) = self.textures.get(&key) {
            return texture.clone();
        }
        if self.rendered_this_frame >= PREVIEWS_PER_FRAME {
            ctx.request_repaint();
            return None;
        }
        self.rendered_this_frame += 1;
        let texture = render_preset_tag(ctx, services, preset, &key);
        self.textures.insert(key, texture.clone());
        texture
    }
}

fn render_preset_tag(
    ctx: &egui::Context,
    services: &mut Services,
    preset: &mtt_core::model::FontPreset,
    key: &str,
) -> Option<TextureHandle> {
    const TAG_FONT_SIZE: f64 = 18.0;
    let fonts = services.fonts()?;
    let mut fonts = fonts.lock().ok()?;
    let mut style = mtt_core::font_presets::normalize_font_preset(preset).style;
    let size = style.font_size_px.unwrap_or(24.0).max(1.0);
    let outline = style.outline_width_px.unwrap_or(0.0).max(0.0);
    let secondary_on = style.secondary_outline_enabled.unwrap_or(false);
    let secondary = if secondary_on { style.secondary_outline_width_px.unwrap_or(0.0).max(0.0) } else { 0.0 };
    let scale = TAG_FONT_SIZE / (size + (outline + secondary * 2.0) * 2.0).max(1.0);
    style.font_size_px = Some(size * scale);
    style.outline_width_px = Some(outline * scale);
    style.secondary_outline_width_px = Some(secondary * scale);
    style.letter_spacing_px = style.letter_spacing_px.map(|value| value * scale);
    style.line_height = Some(1.0);
    style.auto_fit_text = Some(false);
    style.shadow_enabled = Some(false);
    style.filter_effect_enabled = Some(false);
    style.font_background_enabled = Some(false);
    let block = TranslationBlock {
        id: "preset-tag".into(),
        bbox: BBox::new(0.0, 0.0, 1000.0, 1000.0),
        translated_text: preset.name.clone(),
        text_padding_px: Some(0.0),
        text_align: mtt_core::model::TextAlign::Left,
        opacity: 0.0,
        style,
        ..TranslationBlock::default()
    };
    let page = mtt_render::layout::Size::new(480.0, 30.0);
    let options = mtt_render::block::BlockRenderOptions { editing_enabled: false, scale: ctx.pixels_per_point() };
    let raster = mtt_render::block::render_block(&mut fonts, page, &block, options)?;
    Some(crate::textures::pixmap_texture(ctx, key, &trim_transparent(&raster.pixmap)?))
}

/// Crops a preview to its visible pixels (the raster covers the whole render box).
fn trim_transparent(pixmap: &tiny_skia::Pixmap) -> Option<tiny_skia::Pixmap> {
    let (width, height) = (pixmap.width(), pixmap.height());
    let data = pixmap.data();
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (width, height, 0, 0);
    for y in 0..height {
        for x in 0..width {
            if data[((y * width + x) * 4 + 3) as usize] > 0 {
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    if min_x > max_x || min_y > max_y {
        return None;
    }
    let rect = tiny_skia::IntRect::from_xywh(min_x as i32, min_y as i32, max_x - min_x + 1, max_y - min_y + 1)?;
    pixmap.clone_rect(rect)
}

fn render_preview(ctx: &egui::Context, services: &mut Services, value: &str) -> Option<TextureHandle> {
    let fonts = services.fonts()?;
    let mut fonts = fonts.lock().ok()?;
    let page = mtt_render::layout::Size::new(320.0, 28.0);
    let mut block = TranslationBlock {
        id: "font-preview".into(),
        bbox: BBox::new(0.0, 0.0, 1000.0, 1000.0),
        translated_text: PREVIEW_TEXT.into(),
        text_padding_px: Some(0.0),
        text_align: mtt_core::model::TextAlign::Left,
        opacity: 0.0,
        ..TranslationBlock::default()
    };
    block.style = mtt_core::font_presets::default_text_style();
    block.style.font_family = Some(value.to_owned());
    block.style.font_size_px = Some(16.0);
    block.style.auto_fit_text = Some(false);
    block.style.outline_width_px = Some(0.0);
    block.style.font_weight = Some(400.0);
    block.style.text_color = Some("#e5e7eb".into());
    let scale = ctx.pixels_per_point();
    let options = mtt_render::block::BlockRenderOptions { editing_enabled: false, scale };
    let raster = mtt_render::block::render_block(&mut fonts, page, &block, options)?;
    Some(crate::textures::pixmap_texture(ctx, &format!("font-preview-{value}"), &trim_transparent(&raster.pixmap)?))
}

/// The picker; returns the chosen option.
impl FontPreviews {
    /// Call once per frame before drawing previews.
    pub fn begin_frame(&mut self) {
        self.rendered_this_frame = 0;
    }
}

pub fn picker(
    ui: &mut egui::Ui,
    services: &mut Services,
    previews: &mut FontPreviews,
    current: &str,
    text_editing: &mut bool,
) -> Option<FontOption> {
    picker_with_id(ui, services, previews, current, text_editing, "font-family")
}

/// `picker` with its own id (several pickers in one window).
pub fn picker_with_id(
    ui: &mut egui::Ui,
    services: &mut Services,
    previews: &mut FontPreviews,
    current: &str,
    text_editing: &mut bool,
    id_salt: &str,
) -> Option<FontOption> {
    previews.options.refresh(services);
    let index = std::mem::take(&mut previews.options.options);
    let known = index.iter().find(|option| option.option.value == current);
    let selected_label = known.map_or(current.to_owned(), |option| option.option.label.clone());
    let extra = (known.is_none() && !current.is_empty())
        .then(|| IndexedOption::new(FontOption { label: current.into(), value: current.into(), weights: Vec::new() }));
    let id = ui.make_persistent_id((id_salt, "search"));
    let mut query: String = ui.data_mut(|data| data.get_temp(id).unwrap_or_default());
    let mut chosen = None;
    egui::ComboBox::from_id_salt(id_salt)
        // Clicking the search box must not close the list.
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .width(ui.available_width())
        .truncate()
        .selected_text(selected_label)
        .height(420.0)
        .show_ui(ui, |ui| {
            ui.set_min_width(280.0);
            let response = ui.add(egui::TextEdit::singleline(&mut query).hint_text("폰트 검색"));
            if response.has_focus() {
                *text_editing = true;
            }
            let needle = search_key(&query);
            let options = with_selected(&index, extra.as_ref());
            let recent: Vec<FontOption> = services
                .ui_state
                .recent_font_families
                .iter()
                .map(|recent| {
                    options.iter().find(|option| option.option.value == recent.value).map_or_else(
                        || FontOption { label: recent.label.clone(), value: recent.value.clone(), weights: Vec::new() },
                        |option| option.option.clone(),
                    )
                })
                .filter(|option| {
                    needle.is_empty() || search_key(&format!("{} {}", option.label, option.value)).contains(&needle)
                })
                .collect();
            let filtered: Vec<&IndexedOption> =
                options.into_iter().filter(|option| needle.is_empty() || option.search.contains(&needle)).collect();
            ui.label(RichText::new(format!("{}개", recent.len() + filtered.len())).weak().small());
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                if recent.is_empty() && filtered.is_empty() {
                    ui.label(RichText::new("검색 결과 없음").weak());
                }
                for option in &recent {
                    if option_row(ui, services, previews, option, current) {
                        chosen = Some(option.clone());
                    }
                }
                let mut last_group = None;
                for option in filtered {
                    if last_group != Some(option.group) {
                        last_group = Some(option.group);
                        ui.label(RichText::new(option.group.label()).small().strong());
                    }
                    if option_row(ui, services, previews, &option.option, current) {
                        chosen = Some(option.option.clone());
                    }
                }
            });
        });
    previews.options.options = index;
    ui.data_mut(|data| data.insert_temp(id, query));
    if let Some(option) = &chosen {
        services.remember_font(&option.label, &option.value);
    }
    chosen
}

fn option_row(
    ui: &mut egui::Ui,
    services: &mut Services,
    previews: &mut FontPreviews,
    option: &FontOption,
    current: &str,
) -> bool {
    let response = ui.add(egui::Button::selectable(option.value == current, &option.label).truncate());
    // Previews only for rows on screen.
    if ui.is_rect_visible(response.rect)
        && let Some(texture) = previews.get(ui.ctx(), services, &option.value)
    {
        let size = texture.size_vec2() / ui.ctx().pixels_per_point();
        ui.add(egui::Image::new((texture.id(), size)));
    }
    response.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn option(label: &str, value: &str) -> FontOption {
        FontOption { label: label.into(), value: value.into(), weights: Vec::new() }
    }

    #[test]
    fn language_groups_follow_ts_heuristics() {
        assert_eq!(language_group(&option("나눔고딕", "NanumGothic")), Group::Korean);
        assert_eq!(language_group(&option("Malgun Gothic", "\"Malgun Gothic\"")), Group::Korean);
        assert_eq!(language_group(&option("Hiragino Sans", "\"Hiragino Sans\"")), Group::Japanese);
        assert_eq!(language_group(&option("ヒラギノ角ゴ", "x")), Group::Japanese);
        assert_eq!(language_group(&option("Helvetica", "Helvetica")), Group::English);
        assert_eq!(language_group(&option("Ткань", "x")), Group::Other);
        assert_eq!(search_key(" Ａｐｐｌｅ  SD "), "applesd");
    }

    #[test]
    fn options_are_sorted_with_the_selection_in_place() {
        let family = |name: &str| mtt_render::fonts::SystemFontFamily {
            family: name.into(),
            aliases: Vec::new(),
            weights: vec![400],
            css_family: format!("\"{name}\""),
        };
        let families =
            [family("Zapfino"), family("나눔고딕"), family("arial"), family("Hiragino Sans"), family("Zapfino")];
        let options = build_options(&families, "\"Missing\"");
        let keys: Vec<(Group, String)> =
            options.iter().map(|option| (language_group(option), option.label.to_lowercase())).collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
        assert_eq!(options.iter().filter(|option| option.label == "Zapfino").count(), 1, "deduplicated");
        assert!(options.iter().any(|option| option.value == "\"Missing\""));
    }

    #[test]
    fn options_start_with_builtins_and_keep_unknown_selection() {
        let options = build_options(&[], "\"Custom Font\"");
        assert!(options.iter().any(|option| option.label == "맑은 고딕"));
        assert!(options.iter().any(|option| option.value == "\"Custom Font\""));
        assert_eq!(language_group(&options[0]), Group::Korean);
    }
}
