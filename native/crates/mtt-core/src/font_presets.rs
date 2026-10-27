//! Font preset defaults and helpers (`src/shared/fontPresets.ts`).

use crate::model::{
    BlockType, FontCharacterOverride, FontPreset, FontRenderingMode, FontSizePreset, StyleKey, TextDecoration,
    TextFontStyle, TextStyle, TranslationBlock,
};

pub const DEFAULT_OVERLAY_FONT_FAMILY: &str = "\"Malgun Gothic\", \"Apple SD Gothic Neo\", sans-serif";
pub const DEFAULT_OVERLAY_FONT_WEIGHT: f64 = 700.0;
pub const DEFAULT_FONT_SIZE_PX: f64 = 24.0;
pub const DEFAULT_LINE_HEIGHT: f64 = 1.18;
pub const DEFAULT_ENABLED_SHADOW_DISTANCE_PX: f64 = 4.0;
pub const DEFAULT_FILTER_INTENSITY: f64 = 0.35;
pub const DEFAULT_FONT_BACKGROUND_SCALE: f64 = 1.15;
pub const MIN_FONT_BACKGROUND_SCALE: f64 = 0.8;
pub const MAX_FONT_BACKGROUND_SCALE: f64 = 2.0;
pub const DEFAULT_FONT_BACKGROUND_COLOR: &str = "#ffffff";
pub const DEFAULT_FONT_BACKGROUND_RADIUS_PX: f64 = 12.0;
pub const MIN_FONT_BACKGROUND_RADIUS_PX: f64 = 0.0;
pub const MAX_FONT_BACKGROUND_RADIUS_PX: f64 = 96.0;
pub const DEFAULT_ENABLED_FONT_BACKGROUND_OPACITY: f64 = 0.9;
pub const DEFAULT_FONT_BACKGROUND_OPACITY: f64 = 0.0;
pub const DEFAULT_SCREENTONE_FILL_INTENSITY: f64 = 0.55;
pub const DEFAULT_SCREENTONE_FILL_DENSITY: f64 = 0.55;

/// `DEFAULT_FONT_PRESET_VALUES` with every field set.
pub fn default_text_style() -> TextStyle {
    TextStyle {
        font_family: Some(DEFAULT_OVERLAY_FONT_FAMILY.to_owned()),
        character_font_overrides: None,
        font_weight: Some(DEFAULT_OVERLAY_FONT_WEIGHT),
        font_style: Some(TextFontStyle::Normal),
        text_decoration: Some(TextDecoration::None),
        font_rendering_mode: Some(FontRenderingMode::Smooth),
        font_size_px: Some(DEFAULT_FONT_SIZE_PX),
        line_height: Some(DEFAULT_LINE_HEIGHT),
        letter_spacing_px: Some(0.0),
        outline_color: Some("#000000".to_owned()),
        outline_width_px: Some(0.0),
        secondary_outline_enabled: Some(false),
        secondary_outline_color: Some("#ffffff".to_owned()),
        secondary_outline_width_px: Some(0.0),
        shadow_enabled: Some(false),
        shadow_color: Some("#000000".to_owned()),
        shadow_opacity: Some(1.0),
        shadow_blur_px: Some(0.0),
        shadow_angle_deg: Some(45.0),
        shadow_distance_px: Some(0.0),
        auto_fit_text: Some(true),
        text_color: Some("#111111".to_owned()),
        screentone_fill_enabled: Some(false),
        screentone_fill_intensity: Some(DEFAULT_SCREENTONE_FILL_INTENSITY),
        screentone_fill_density: Some(DEFAULT_SCREENTONE_FILL_DENSITY),
        screentone_fill_antialias: Some(true),
        filter_effect_enabled: Some(false),
        filter_zigzag_enabled: Some(false),
        filter_zigzag_intensity: Some(DEFAULT_FILTER_INTENSITY),
        filter_wave_enabled: Some(false),
        filter_wave_intensity: Some(DEFAULT_FILTER_INTENSITY),
        filter_glitch_enabled: Some(false),
        filter_glitch_intensity: Some(DEFAULT_FILTER_INTENSITY),
        filter_pixelate_enabled: Some(false),
        filter_pixelate_intensity: Some(DEFAULT_FILTER_INTENSITY),
        filter_blur_enabled: Some(false),
        filter_blur_intensity: Some(DEFAULT_FILTER_INTENSITY),
        filter_noise_enabled: Some(false),
        filter_noise_intensity: Some(DEFAULT_FILTER_INTENSITY),
        font_background_enabled: Some(false),
        font_background_scale: Some(DEFAULT_FONT_BACKGROUND_SCALE),
        font_background_color: Some(DEFAULT_FONT_BACKGROUND_COLOR.to_owned()),
        font_background_radius_px: Some(DEFAULT_FONT_BACKGROUND_RADIUS_PX),
        font_background_opacity: Some(DEFAULT_FONT_BACKGROUND_OPACITY),
    }
}

pub const BLOCK_TYPE_FONT_PRESET_NAMES: [BlockType; 4] =
    [BlockType::Speech, BlockType::Sfx, BlockType::Caption, BlockType::Other];

pub fn block_type_font_preset_id(block_type: BlockType) -> &'static str {
    match block_type {
        BlockType::Speech => "font-preset-speech",
        BlockType::Sfx => "font-preset-sfx",
        BlockType::Caption => "font-preset-caption",
        BlockType::Other => "font-preset-other",
    }
}

pub fn create_default_font_preset(id: &str, name: BlockType) -> FontPreset {
    FontPreset {
        id: id.to_owned(),
        name: name.as_str().to_owned(),
        style: default_text_style(),
        ..FontPreset::default()
    }
}

/// Fills unset preset values with defaults (`normalizeFontPreset`).
pub fn normalize_font_preset(preset: &FontPreset) -> FontPreset {
    let mut normalized = preset.clone();
    normalized.style.fill_missing_from(&default_text_style());
    if preset.style.secondary_outline_enabled.is_none() && preset.style.secondary_outline_width_px.unwrap_or(0.0) > 0.0
    {
        normalized.style.secondary_outline_enabled = Some(true);
    }
    normalized
}

/// Adds the four block-type presets when they are missing (`ensureBlockTypeFontPresets`).
pub fn ensure_block_type_font_presets(font_presets: &[FontPreset]) -> Vec<FontPreset> {
    let mut next = font_presets.to_vec();
    for name in BLOCK_TYPE_FONT_PRESET_NAMES {
        let base_id = block_type_font_preset_id(name);
        if next.iter().any(|preset| preset.id == base_id) {
            continue;
        }
        let mut id = base_id.to_owned();
        let mut suffix = 2;
        while next.iter().any(|preset| preset.id == id) {
            id = format!("{base_id}-{suffix}");
            suffix += 1;
        }
        next.push(create_default_font_preset(&id, name));
    }
    next
}

/// Filter child keys follow the master `filterEffectEnabled` link.
fn is_filter_child_key(key: StyleKey) -> bool {
    use StyleKey::*;
    matches!(
        key,
        FilterZigzagEnabled
            | FilterZigzagIntensity
            | FilterWaveEnabled
            | FilterWaveIntensity
            | FilterGlitchEnabled
            | FilterGlitchIntensity
            | FilterPixelateEnabled
            | FilterPixelateIntensity
            | FilterBlurEnabled
            | FilterBlurIntensity
            | FilterNoiseEnabled
            | FilterNoiseIntensity
    )
}

/// `isBlockFontPresetValueLinked`: a missing link flag means linked.
pub fn is_value_linked(block: &TranslationBlock, key: StyleKey) -> bool {
    if is_filter_child_key(key) && block.links.filter_effect_enabled_linked_to_preset == Some(false) {
        return false;
    }
    block.links.get(key) != Some(false)
}

/// Keys that relinking `key` also relinks (`buildFontPresetRelinkPatch`).
pub fn relink_keys(key: StyleKey) -> Vec<StyleKey> {
    if key == StyleKey::FilterEffectEnabled {
        StyleKey::ALL
            .iter()
            .copied()
            .filter(|k| *k == StyleKey::FilterEffectEnabled || is_filter_child_key(*k))
            .collect()
    } else {
        vec![key]
    }
}

/// `resolveFontPreset`: normalized preset with the size taken from its size preset.
pub fn resolve_font_preset(preset: &FontPreset, size_presets: &[FontSizePreset]) -> FontPreset {
    let mut normalized = normalize_font_preset(preset);
    if let Some(size) = preset
        .font_size_preset_id
        .as_deref()
        .and_then(|id| size_presets.iter().find(|candidate| candidate.id == id))
        .map(|candidate| candidate.font_size_px)
    {
        normalized.style.font_size_px = Some(size);
    }
    normalized
}

/// `applyFontPresetPatchToBlock`: copies the family and every linked value (or every value
/// when `force`).
pub fn apply_font_preset_values(block: &mut TranslationBlock, preset: &FontPreset, force: bool) {
    let normalized = normalize_font_preset(preset);
    if normalized.style.font_family.is_some() {
        block.style.font_family = normalized.style.font_family.clone();
    }
    block.style.character_font_overrides = normalized
        .style
        .character_font_overrides
        .as_ref()
        .map(|overrides| normalize_character_font_overrides(overrides))
        .filter(|overrides| !overrides.is_empty());
    for key in StyleKey::ALL {
        if force || is_value_linked(block, *key) {
            block.style.copy_key_from(&normalized.style, *key);
        }
    }
}

/// `applyAssignedFontPresetToBlock`: assigns the preset and links every field.
pub fn apply_font_preset_to_block(block: &mut TranslationBlock, preset: &FontPreset) {
    apply_font_preset_values(block, preset, true);
    block.font_preset_id = Some(preset.id.clone());
    block.links.set_all(Some(true));
}

/// Applies the preset matching the block type (`applyBlockTypeFontPresetToBlock`).
pub fn apply_block_type_font_preset_to_block(block: &mut TranslationBlock, font_presets: &[FontPreset]) {
    let preset_id = block_type_font_preset_id(block.block_type);
    if let Some(preset) = font_presets.iter().find(|preset| preset.id == preset_id) {
        apply_font_preset_to_block(block, preset);
    }
}

/// Re-applies an edited preset to a block assigned to it, keeping unlinked fields.
pub fn sync_linked_block_to_preset(block: &mut TranslationBlock, preset: &FontPreset) {
    if block.font_preset_id.as_deref() == Some(preset.id.as_str()) {
        apply_font_preset_values(block, preset, false);
    }
}

/// `normalizeCharacterFontOverrides`: one override per character, empty entries dropped.
pub fn normalize_character_font_overrides(overrides: &[FontCharacterOverride]) -> Vec<FontCharacterOverride> {
    let mut result: Vec<FontCharacterOverride> = Vec::new();
    for entry in overrides {
        let Some(character) = entry.character.trim().chars().next() else {
            continue;
        };
        let family = entry.font_family.trim();
        if family.is_empty() {
            continue;
        }
        let normalized = FontCharacterOverride { character: character.to_string(), font_family: family.to_owned() };
        match result.iter_mut().find(|existing| existing.character == normalized.character) {
            Some(existing) => *existing = normalized,
            None => result.push(normalized),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ensure_adds_missing_block_type_presets_with_unique_ids() {
        let existing =
            vec![FontPreset { id: "font-preset-sfx".into(), name: "custom".into(), ..FontPreset::default() }];
        let presets = ensure_block_type_font_presets(&existing);
        let ids: Vec<&str> = presets.iter().map(|preset| preset.id.as_str()).collect();
        assert_eq!(ids, ["font-preset-sfx", "font-preset-speech", "font-preset-caption", "font-preset-other"]);
        assert_eq!(presets[0].name, "custom", "existing preset must be untouched");
    }

    #[test]
    fn normalize_enables_secondary_outline_for_legacy_width() {
        let preset = FontPreset {
            style: TextStyle { secondary_outline_width_px: Some(2.0), ..TextStyle::default() },
            ..FontPreset::default()
        };
        let normalized = normalize_font_preset(&preset);
        assert_eq!(normalized.style.secondary_outline_enabled, Some(true));
        assert_eq!(normalized.style.font_size_px, Some(DEFAULT_FONT_SIZE_PX));
    }

    #[test]
    fn apply_links_every_field_and_sync_respects_unlinked_fields() {
        let mut preset = create_default_font_preset("p", BlockType::Speech);
        let mut block = TranslationBlock::default();
        apply_font_preset_to_block(&mut block, &preset);
        assert!(StyleKey::ALL.iter().all(|key| block.links.get(*key) == Some(true)));

        block.links.set(StyleKey::TextColor, Some(false));
        block.links.set(StyleKey::FontSizePx, None);
        block.style.text_color = Some("#ff0000".into());
        preset.style.text_color = Some("#00ff00".into());
        preset.style.font_size_px = Some(40.0);
        sync_linked_block_to_preset(&mut block, &preset);
        assert_eq!(block.style.text_color.as_deref(), Some("#ff0000"), "unlinked field kept");
        assert_eq!(block.style.font_size_px, Some(40.0), "missing link flag counts as linked");
    }

    #[test]
    fn filter_children_follow_master_link() {
        let mut block = TranslationBlock::default();
        block.links.filter_effect_enabled_linked_to_preset = Some(false);
        assert!(!is_value_linked(&block, StyleKey::FilterWaveIntensity));
        assert!(is_value_linked(&block, StyleKey::TextColor));
        assert_eq!(relink_keys(StyleKey::FilterEffectEnabled).len(), 13);
    }

    #[test]
    fn size_preset_overrides_preset_size() {
        let preset =
            FontPreset { font_size_preset_id: Some("s".into()), ..create_default_font_preset("p", BlockType::Speech) };
        let sizes = [FontSizePreset { id: "s".into(), name: "큰".into(), font_size_px: 36.0, ..Default::default() }];
        assert_eq!(resolve_font_preset(&preset, &sizes).style.font_size_px, Some(36.0));
    }

    #[test]
    fn character_overrides_normalized() {
        let overrides = vec![
            FontCharacterOverride { character: " ♥x".into(), font_family: "A".into() },
            FontCharacterOverride { character: "♥".into(), font_family: " B ".into() },
            FontCharacterOverride { character: "".into(), font_family: "C".into() },
        ];
        let normalized = normalize_character_font_overrides(&overrides);
        assert_eq!(normalized, vec![FontCharacterOverride { character: "♥".into(), font_family: "B".into() }]);
    }
}
