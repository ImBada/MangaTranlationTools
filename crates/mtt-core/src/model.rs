//! On-disk data model shared with the TypeScript app (`src/shared/types.ts`).
//!
//! Every persisted struct keeps unknown JSON keys in `extra` so a chapter edited by the
//! native app can still be opened by the TypeScript app without losing fields.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};

pub type JsonMap = Map<String, Value>;

/// Declares a string-valued enum that falls back to its default variant for unknown or
/// non-string JSON values instead of failing the whole document.
macro_rules! string_enum {
    (
        $(#[$meta:meta])*
        pub enum $name:ident {
            $($variant:ident => $value:literal),+ $(,)?
        }
        default = $default:ident
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum $name {
            $($variant),+
        }

        impl Default for $name {
            fn default() -> Self {
                Self::$default
            }
        }

        impl $name {
            pub const ALL: &'static [$name] = &[$(Self::$variant),+];

            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $value),+
                }
            }

            pub fn parse(value: &str) -> Option<Self> {
                match value {
                    $($value => Some(Self::$variant),)+
                    _ => None,
                }
            }
        }

        impl Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.serialize_str(self.as_str())
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = Value::deserialize(deserializer)?;
                Ok(value
                    .as_str()
                    .and_then(|text| Self::parse(text).or_else(|| Self::parse(&text.trim().to_ascii_lowercase())))
                    .unwrap_or_default())
            }
        }
    };
}

string_enum! {
    pub enum BlockType {
        Speech => "speech",
        Sfx => "sfx",
        Caption => "caption",
        Other => "other",
    }
    default = Other
}

string_enum! {
    pub enum SourceTextDirection {
        Horizontal => "horizontal",
        Vertical => "vertical",
    }
    default = Horizontal
}

string_enum! {
    pub enum RenderTextDirection {
        Horizontal => "horizontal",
        Vertical => "vertical",
        Hidden => "hidden",
    }
    default = Horizontal
}

string_enum! {
    pub enum TextPosition {
        TopLeft => "top-left",
        Top => "top",
        TopRight => "top-right",
        Left => "left",
        Center => "center",
        Right => "right",
        BottomLeft => "bottom-left",
        Bottom => "bottom",
        BottomRight => "bottom-right",
    }
    default = Center
}

string_enum! {
    pub enum TextAlign {
        Left => "left",
        Center => "center",
        Right => "right",
    }
    default = Center
}

string_enum! {
    pub enum TextFontStyle {
        Normal => "normal",
        Italic => "italic",
    }
    default = Normal
}

string_enum! {
    pub enum TextDecoration {
        None => "none",
        Underline => "underline",
    }
    default = None
}

string_enum! {
    pub enum FontRenderingMode {
        Smooth => "smooth",
        Hard => "hard",
    }
    default = Smooth
}

string_enum! {
    pub enum PageAnalysisStatus {
        Idle => "idle",
        Running => "running",
        Completed => "completed",
        Failed => "failed",
    }
    default = Idle
}

string_enum! {
    pub enum InpaintStatus {
        Idle => "idle",
        Running => "running",
        Completed => "completed",
        Failed => "failed",
    }
    default = Idle
}

string_enum! {
    pub enum ChapterStatus {
        Idle => "idle",
        Running => "running",
        Completed => "completed",
        Partial => "partial",
        Failed => "failed",
    }
    default = Idle
}

string_enum! {
    pub enum ImportSourceKind {
        Images => "images",
        Folder => "folder",
        Zip => "zip",
        ZipFolder => "zip-folder",
        Pdf => "pdf",
    }
    default = Images
}

string_enum! {
    pub enum InpaintEngine {
        Lama => "lama",
        OpencvFallback => "opencv-fallback",
        LocalFillFallback => "local-fill-fallback",
        MaskFillFallback => "mask-fill-fallback",
    }
    default = LocalFillFallback
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BBox {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl BBox {
    pub const fn new(x: f64, y: f64, w: f64, h: f64) -> Self {
        Self { x, y, w, h }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontCharacterOverride {
    #[serde(default)]
    pub character: String,
    #[serde(default)]
    pub font_family: String,
}

/// Declares the linkable font-preset style fields once and derives the style struct,
/// the per-block "linked to preset" flags, and key-based accessors from the same list.
macro_rules! define_text_style {
    ($($field:ident: $ty:ty => $link:ident, $key:ident;)+) => {
        /// Text style values shared by translation blocks and font presets.
        #[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub struct TextStyle {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub font_family: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub character_font_overrides: Option<Vec<FontCharacterOverride>>,
            $(
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub $field: Option<$ty>,
            )+
        }

        /// `*LinkedToPreset` flags stored on a translation block.
        #[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
        #[serde(rename_all = "camelCase")]
        pub struct PresetLinks {
            $(
                #[serde(default, skip_serializing_if = "Option::is_none")]
                pub $link: Option<bool>,
            )+
        }

        /// A linkable style field (every [`TextStyle`] field except the font family and
        /// per-character overrides).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum StyleKey {
            $($key,)+
        }

        impl StyleKey {
            pub const ALL: &'static [StyleKey] = &[$(StyleKey::$key,)+];
        }

        impl TextStyle {
            /// Copies one linkable field from `source`.
            pub fn copy_key_from(&mut self, source: &TextStyle, key: StyleKey) {
                match key {
                    $(StyleKey::$key => self.$field = source.$field.clone(),)+
                }
            }

            pub fn has_key(&self, key: StyleKey) -> bool {
                match key {
                    $(StyleKey::$key => self.$field.is_some(),)+
                }
            }

            /// Fills every unset linkable field from `defaults`.
            pub fn fill_missing_from(&mut self, defaults: &TextStyle) {
                if self.font_family.is_none() {
                    self.font_family = defaults.font_family.clone();
                }
                $(
                    if self.$field.is_none() {
                        self.$field = defaults.$field.clone();
                    }
                )+
            }
        }

        impl PresetLinks {
            pub fn get(&self, key: StyleKey) -> Option<bool> {
                match key {
                    $(StyleKey::$key => self.$link,)+
                }
            }

            pub fn set(&mut self, key: StyleKey, linked: Option<bool>) {
                match key {
                    $(StyleKey::$key => self.$link = linked,)+
                }
            }

            pub fn set_all(&mut self, linked: Option<bool>) {
                $(self.$link = linked;)+
            }
        }
    };
}

define_text_style! {
    font_weight: f64 => font_weight_linked_to_preset, FontWeight;
    font_style: TextFontStyle => font_style_linked_to_preset, FontStyle;
    text_decoration: TextDecoration => text_decoration_linked_to_preset, TextDecoration;
    font_rendering_mode: FontRenderingMode => font_rendering_mode_linked_to_preset, FontRenderingMode;
    font_size_px: f64 => font_size_linked_to_preset, FontSizePx;
    line_height: f64 => line_height_linked_to_preset, LineHeight;
    letter_spacing_px: f64 => letter_spacing_linked_to_preset, LetterSpacingPx;
    outline_color: String => outline_color_linked_to_preset, OutlineColor;
    outline_width_px: f64 => outline_width_linked_to_preset, OutlineWidthPx;
    secondary_outline_enabled: bool => secondary_outline_enabled_linked_to_preset, SecondaryOutlineEnabled;
    secondary_outline_color: String => secondary_outline_color_linked_to_preset, SecondaryOutlineColor;
    secondary_outline_width_px: f64 => secondary_outline_width_linked_to_preset, SecondaryOutlineWidthPx;
    shadow_enabled: bool => shadow_enabled_linked_to_preset, ShadowEnabled;
    shadow_color: String => shadow_color_linked_to_preset, ShadowColor;
    shadow_opacity: f64 => shadow_opacity_linked_to_preset, ShadowOpacity;
    shadow_blur_px: f64 => shadow_blur_px_linked_to_preset, ShadowBlurPx;
    shadow_angle_deg: f64 => shadow_angle_deg_linked_to_preset, ShadowAngleDeg;
    shadow_distance_px: f64 => shadow_distance_px_linked_to_preset, ShadowDistancePx;
    auto_fit_text: bool => auto_fit_text_linked_to_preset, AutoFitText;
    text_color: String => text_color_linked_to_preset, TextColor;
    screentone_fill_enabled: bool => screentone_fill_enabled_linked_to_preset, ScreentoneFillEnabled;
    screentone_fill_intensity: f64 => screentone_fill_intensity_linked_to_preset, ScreentoneFillIntensity;
    screentone_fill_density: f64 => screentone_fill_density_linked_to_preset, ScreentoneFillDensity;
    screentone_fill_antialias: bool => screentone_fill_antialias_linked_to_preset, ScreentoneFillAntialias;
    filter_effect_enabled: bool => filter_effect_enabled_linked_to_preset, FilterEffectEnabled;
    filter_zigzag_enabled: bool => filter_zigzag_enabled_linked_to_preset, FilterZigzagEnabled;
    filter_zigzag_intensity: f64 => filter_zigzag_intensity_linked_to_preset, FilterZigzagIntensity;
    filter_wave_enabled: bool => filter_wave_enabled_linked_to_preset, FilterWaveEnabled;
    filter_wave_intensity: f64 => filter_wave_intensity_linked_to_preset, FilterWaveIntensity;
    filter_glitch_enabled: bool => filter_glitch_enabled_linked_to_preset, FilterGlitchEnabled;
    filter_glitch_intensity: f64 => filter_glitch_intensity_linked_to_preset, FilterGlitchIntensity;
    filter_pixelate_enabled: bool => filter_pixelate_enabled_linked_to_preset, FilterPixelateEnabled;
    filter_pixelate_intensity: f64 => filter_pixelate_intensity_linked_to_preset, FilterPixelateIntensity;
    filter_blur_enabled: bool => filter_blur_enabled_linked_to_preset, FilterBlurEnabled;
    filter_blur_intensity: f64 => filter_blur_intensity_linked_to_preset, FilterBlurIntensity;
    filter_noise_enabled: bool => filter_noise_enabled_linked_to_preset, FilterNoiseEnabled;
    filter_noise_intensity: f64 => filter_noise_intensity_linked_to_preset, FilterNoiseIntensity;
    font_background_enabled: bool => font_background_enabled_linked_to_preset, FontBackgroundEnabled;
    font_background_scale: f64 => font_background_scale_linked_to_preset, FontBackgroundScale;
    font_background_color: String => font_background_color_linked_to_preset, FontBackgroundColor;
    font_background_radius_px: f64 => font_background_radius_px_linked_to_preset, FontBackgroundRadiusPx;
    font_background_opacity: f64 => font_background_opacity_linked_to_preset, FontBackgroundOpacity;
}

pub const DEFAULT_BLOCK_BACKGROUND_COLOR: &str = "#ffffff";
pub const DEFAULT_TEXT_COLOR: &str = "#111111";

fn default_background_color() -> String {
    DEFAULT_BLOCK_BACKGROUND_COLOR.to_owned()
}

fn default_opacity() -> f64 {
    1.0
}

/// Typesetting rules new blocks get. Each version keeps what the one before it does, so
/// pages made on an earlier version keep their look:
/// - 2 caps auto-fit growth by the source glyph size, balances line lengths (preferring
///   breaks after punctuation) and stops shrinking at a readable size.
/// - 3 keeps brackets and closing marks off the wrong end of a line, breaks words too long
///   for a line between syllables, picks the bold face of the font that draws, counts
///   characters by grapheme cluster, and sets vertical text centred, with sideways marks,
///   short runs across and a column per line break.
pub const LAYOUT_VERSION: u32 = 3;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationBlock {
    #[serde(default)]
    pub id: String,
    #[serde(rename = "type", default)]
    pub block_type: BlockType,
    #[serde(default)]
    pub bbox: BBox,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render_bbox: Option<BBox>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bbox_space: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub render_bbox_space: Option<String>,
    #[serde(default)]
    pub source_text: String,
    #[serde(default)]
    pub translated_text: String,
    #[serde(default)]
    pub confidence: f64,
    #[serde(default)]
    pub source_direction: SourceTextDirection,
    #[serde(default)]
    pub render_direction: RenderTextDirection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotation_deg: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_preset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_padding_px: Option<f64>,
    #[serde(default)]
    pub text_align: TextAlign,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_position: Option<TextPosition>,
    #[serde(default = "default_background_color")]
    pub background_color: String,
    #[serde(default = "default_opacity")]
    pub opacity: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inpainted: Option<bool>,
    /// [`TranslationBlock::content_fingerprint`] when the translation made the block, so
    /// later hand edits can be told apart (see [`TranslationBlock::is_user_work`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generated_fingerprint: Option<String>,
    /// Typesetting rules the block was made with ([`LAYOUT_VERSION`]); missing means the
    /// original rules, so pages typeset before keep their look.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_version: Option<u32>,
    /// Where text fits in the render box, row by row: the usable `[left, right]` span of
    /// evenly spaced rows from its top to its bottom, as fractions of its width (an empty
    /// span where nothing fits). Set from the balloon around the block, so lines can follow
    /// its outline; it scales with the box.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_shape: Option<Vec<[f64; 2]>>,
    #[serde(flatten)]
    pub links: PresetLinks,
    #[serde(flatten)]
    pub style: TextStyle,
    #[serde(flatten)]
    pub extra: JsonMap,
}

impl Default for TranslationBlock {
    fn default() -> Self {
        Self {
            id: String::new(),
            block_type: BlockType::default(),
            bbox: BBox::default(),
            render_bbox: None,
            bbox_space: None,
            render_bbox_space: None,
            source_text: String::new(),
            translated_text: String::new(),
            confidence: 0.0,
            source_direction: SourceTextDirection::default(),
            render_direction: RenderTextDirection::default(),
            rotation_deg: None,
            font_preset_id: None,
            text_padding_px: None,
            text_align: TextAlign::default(),
            text_position: None,
            background_color: default_background_color(),
            opacity: default_opacity(),
            inpainted: None,
            generated_fingerprint: None,
            layout_version: None,
            text_shape: None,
            links: PresetLinks::default(),
            style: TextStyle::default(),
            extra: JsonMap::new(),
        }
    }
}

impl TranslationBlock {
    /// Text drawn on the page: the translation, falling back to the source text.
    pub fn display_text(&self) -> &str {
        if !self.translated_text.is_empty() {
            &self.translated_text
        } else if !self.source_text.is_empty() {
            &self.source_text
        } else {
            "..."
        }
    }

    pub fn font_size_px(&self) -> f64 {
        self.style.font_size_px.unwrap_or(crate::font_presets::DEFAULT_FONT_SIZE_PX)
    }

    pub fn line_height(&self) -> f64 {
        self.style.line_height.unwrap_or(crate::font_presets::DEFAULT_LINE_HEIGHT)
    }

    pub fn text_color(&self) -> &str {
        self.style.text_color.as_deref().unwrap_or(DEFAULT_TEXT_COLOR)
    }

    /// The typesetting rules to use (see [`LAYOUT_VERSION`]).
    pub fn layout_version(&self) -> u32 {
        self.layout_version.unwrap_or(1)
    }

    /// Hash of what a hand edit changes: the id (so copies differ), texts (as saved), boxes,
    /// rotation, placement and preset. Style fields are left out because editing a preset
    /// rewrites them on every linked block.
    pub fn content_fingerprint(&self) -> String {
        let content = serde_json::json!([
            self.id,
            self.source_text,
            crate::text::normalize_korean_text(&self.translated_text),
            self.bbox,
            self.render_bbox,
            self.rotation_deg,
            self.font_preset_id,
            self.text_align,
            self.text_position,
            self.render_direction,
            self.text_padding_px,
        ]);
        // FNV-1a: stable across builds, unlike `std`'s hasher.
        let hash = content
            .to_string()
            .bytes()
            .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3));
        format!("{hash:016x}")
    }

    /// Records the block's current content as what the translation produced.
    pub fn mark_generated(&mut self) {
        self.generated_fingerprint = Some(self.content_fingerprint());
    }

    /// Whether the block holds the user's own work: changed since the translation made it,
    /// or made by hand. Blocks translated before fingerprints existed count as generated
    /// when they still have the translation's id (`{page_id}-block-{n}`).
    pub fn is_user_work(&self, page_id: &str) -> bool {
        match &self.generated_fingerprint {
            Some(fingerprint) => *fingerprint != self.content_fingerprint(),
            None => !self
                .id
                .strip_prefix(page_id)
                .and_then(|rest| rest.strip_prefix("-block-"))
                .is_some_and(|number| !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationBlockGroupEffect {
    #[serde(default)]
    pub id: String,
    #[serde(rename = "type", default)]
    pub effect_type: String,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<JsonMap>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationBlockGroup {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub block_ids: Vec<String>,
    #[serde(default)]
    pub effects: Vec<TranslationBlockGroupEffect>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontPreset {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size_preset_id: Option<String>,
    #[serde(flatten)]
    pub style: TextStyle,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FontSizePreset {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub font_size_px: f64,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InpaintSettings {
    #[serde(default)]
    pub engine: InpaintEngine,
    #[serde(default)]
    pub padding_px: f64,
    #[serde(default)]
    pub feather_px: f64,
    #[serde(default = "default_tile_size")]
    pub tile_size: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_cleanup_px: Option<f64>,
    /// Snap the fill to the page's own colors with hard edges, for pages drawn without
    /// anti-aliasing (1-bit black and white).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub aliased: bool,
    #[serde(flatten)]
    pub extra: JsonMap,
}

fn default_tile_size() -> f64 {
    1024.0
}

impl Default for InpaintSettings {
    fn default() -> Self {
        Self {
            engine: InpaintEngine::LocalFillFallback,
            padding_px: 0.0,
            feather_px: 0.0,
            tile_size: default_tile_size(),
            artifact_cleanup_px: Some(8.0),
            aliased: false,
            extra: JsonMap::new(),
        }
    }
}

/// A page as stored inside `chapter.json` (`LibraryPageRecord`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageRecord {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub image_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inpaint_mask_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inpaint_result_path: Option<String>,
    /// Legacy inline mask payload; superseded by `inpaint_mask_path`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inpaint_mask_data_url: Option<String>,
    /// Legacy inline result payload; superseded by `inpaint_result_path`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inpaint_result_data_url: Option<String>,
    /// Deprecated single-layer inpaint payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inpaint_layer_data_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inpaint_status: Option<InpaintStatus>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inpaint_settings: Option<InpaintSettings>,
    #[serde(default, deserialize_with = "de_u32_lenient")]
    pub width: u32,
    #[serde(default, deserialize_with = "de_u32_lenient")]
    pub height: u32,
    #[serde(default)]
    pub blocks: Vec<TranslationBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub block_groups: Option<Vec<TranslationBlockGroup>>,
    #[serde(default)]
    pub analysis_status: PageAnalysisStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress_completed: Option<bool>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(flatten)]
    pub extra: JsonMap,
}

impl PageRecord {
    pub fn is_completed(&self) -> bool {
        self.progress_completed.unwrap_or(false)
    }
}

/// `chapter.json` (`LibraryChapter`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chapter {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub work_id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub source_kind: ImportSourceKind,
    #[serde(default)]
    pub status: ChapterStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_opened_page_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_presets: Option<Vec<FontPreset>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub font_size_presets: Option<Vec<FontSizePreset>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub favorite_font_preset_ids: Option<Vec<String>>,
    /// Inpaint this chapter without anti-aliasing (`InpaintSettings::aliased`).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub inpaint_aliased: bool,
    #[serde(default)]
    pub page_order: Vec<String>,
    #[serde(default)]
    pub pages: Vec<PageRecord>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    #[serde(flatten)]
    pub extra: JsonMap,
}

impl Chapter {
    /// Pages sorted by `page_order`; pages missing from the order keep their stored order
    /// at the end (`reorderRecords`).
    pub fn ordered_page_indices(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = Vec::with_capacity(self.pages.len());
        let mut used = vec![false; self.pages.len()];
        for page_id in &self.page_order {
            if let Some(index) = self.pages.iter().position(|page| &page.id == page_id)
                && !used[index]
            {
                used[index] = true;
                indices.push(index);
            }
        }
        indices.extend((0..self.pages.len()).filter(|index| !used[*index]));
        indices
    }

    /// Reorders `pages` to follow `page_order`, leaving `page_order` itself untouched
    /// (`chapter.pages = reorderRecords(chapter.pages, chapter.pageOrder)`).
    pub(crate) fn apply_page_order(&mut self) {
        let order = self.ordered_page_indices();
        let mut pages: Vec<Option<PageRecord>> = std::mem::take(&mut self.pages).into_iter().map(Some).collect();
        self.pages = order.into_iter().filter_map(|index| pages[index].take()).collect();
    }

    /// Reorders `pages` to follow `page_order` and rewrites `page_order` to match.
    pub fn normalize_page_order(&mut self) {
        self.apply_page_order();
        self.page_order = self.pages.iter().map(|page| page.id.clone()).collect();
    }

    pub fn page(&self, page_id: &str) -> Option<&PageRecord> {
        self.pages.iter().find(|page| page.id == page_id)
    }

    pub fn page_mut(&mut self, page_id: &str) -> Option<&mut PageRecord> {
        self.pages.iter_mut().find(|page| page.id == page_id)
    }
}

/// `work.json` (`LibraryWork`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Work {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub chapter_order: Vec<String>,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub updated_at: String,
    /// Names, terms and tone for this work, added to every translation prompt.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub translation_notes: String,
    #[serde(flatten)]
    pub extra: JsonMap,
}

/// `library/index.json`.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryIndexFile {
    #[serde(default)]
    pub work_order: Vec<String>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

fn de_u32_lenient<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u32, D::Error> {
    let value = Value::deserialize(deserializer)?;
    Ok(value
        .as_f64()
        .filter(|number| number.is_finite() && *number >= 0.0)
        .map(|number| number.round().min(f64::from(u32::MAX)) as u32)
        .unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn block_round_trip_keeps_style_links_and_unknown_fields() {
        let raw = json!({
            "id": "b1",
            "type": "speech",
            "bbox": { "x": 1, "y": 2, "w": 3, "h": 4 },
            "sourceText": "こんにちは",
            "translatedText": "안녕하세요",
            "confidence": 0.9,
            "sourceDirection": "vertical",
            "renderDirection": "horizontal",
            "fontSizePx": 24,
            "lineHeight": 1.18,
            "fontSizeLinkedToPreset": true,
            "shadowBlurPxLinkedToPreset": false,
            "fontBackgroundRadiusPx": 12,
            "textAlign": "center",
            "textColor": "#111111",
            "backgroundColor": "#ffffff",
            "opacity": 1,
            "futureField": { "nested": [1, 2] }
        });
        let block: TranslationBlock = serde_json::from_value(raw.clone()).unwrap();
        assert_eq!(block.block_type, BlockType::Speech);
        assert_eq!(block.style.font_size_px, Some(24.0));
        assert_eq!(block.links.font_size_linked_to_preset, Some(true));
        assert_eq!(block.links.get(StyleKey::ShadowBlurPx), Some(false));
        assert_eq!(block.style.font_background_radius_px, Some(12.0));
        assert!(block.extra.contains_key("futureField"));
        assert!(!block.extra.contains_key("fontSizePx"), "style keys must not leak into extra");
        assert!(!block.extra.contains_key("fontSizeLinkedToPreset"), "link keys must not leak into extra");

        let back = serde_json::to_value(&block).unwrap();
        for key in ["futureField", "fontSizeLinkedToPreset", "shadowBlurPxLinkedToPreset", "fontBackgroundRadiusPx"] {
            assert_eq!(back.get(key).map(normalize_number), raw.get(key).map(normalize_number), "{key}");
        }
    }

    #[test]
    fn unknown_enum_values_fall_back_to_defaults() {
        let block: TranslationBlock = serde_json::from_value(json!({
            "type": "balloon",
            "renderDirection": 3,
            "textAlign": "RIGHT"
        }))
        .unwrap();
        assert_eq!(block.block_type, BlockType::Other);
        assert_eq!(block.render_direction, RenderTextDirection::Horizontal);
        assert_eq!(block.text_align, TextAlign::Right);
    }

    #[test]
    fn page_dimensions_accept_floats() {
        let page: PageRecord = serde_json::from_value(json!({ "width": 1071.6, "height": "x" })).unwrap();
        assert_eq!(page.width, 1072);
        assert_eq!(page.height, 0);
    }

    #[test]
    fn chapter_page_order_puts_unlisted_pages_last() {
        let mut chapter = Chapter {
            page_order: vec!["c".into(), "a".into(), "missing".into()],
            pages: ["a", "b", "c"].iter().map(|id| PageRecord { id: (*id).into(), ..PageRecord::default() }).collect(),
            ..Chapter::default()
        };
        chapter.normalize_page_order();
        let ids: Vec<&str> = chapter.pages.iter().map(|page| page.id.as_str()).collect();
        assert_eq!(ids, ["c", "a", "b"]);
        assert_eq!(chapter.page_order, ["c", "a", "b"]);
    }

    fn normalize_number(value: &Value) -> Value {
        match value.as_f64() {
            Some(number) => json!(number),
            None => value.clone(),
        }
    }
}
