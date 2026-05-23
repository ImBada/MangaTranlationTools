import type { BlockType, FontPreset, TranslationBlock } from "./types";

type DefaultFontPresetValues = Required<Omit<FontPreset, "id" | "name" | "fontSizePresetId" | "characterFontOverrides">>;

export const DEFAULT_OVERLAY_FONT_FAMILY = "\"Malgun Gothic\", \"Apple SD Gothic Neo\", sans-serif";
export const DEFAULT_OVERLAY_FONT_WEIGHT = 700;
export const DEFAULT_OVERLAY_FONT_STYLE = "normal";
export const DEFAULT_OVERLAY_TEXT_DECORATION = "none";
export const DEFAULT_FONT_RENDERING_MODE = "smooth";
export const DEFAULT_ENABLED_SHADOW_DISTANCE_PX = 4;
export const DEFAULT_FILTER_ZIGZAG_INTENSITY = 0.35;
export const DEFAULT_FILTER_WAVE_INTENSITY = 0.35;
export const DEFAULT_FILTER_GLITCH_INTENSITY = 0.35;
export const DEFAULT_FILTER_PIXELATE_INTENSITY = 0.35;
export const DEFAULT_FILTER_BLUR_INTENSITY = 0.35;
export const DEFAULT_FILTER_NOISE_INTENSITY = 0.35;
export const DEFAULT_FONT_BACKGROUND_SCALE = 1.15;
export const MIN_FONT_BACKGROUND_SCALE = 0.8;
export const MAX_FONT_BACKGROUND_SCALE = 2;
export const DEFAULT_FONT_BACKGROUND_COLOR = "#ffffff";
export const DEFAULT_FONT_BACKGROUND_RADIUS_PX = 12;
export const MIN_FONT_BACKGROUND_RADIUS_PX = 0;
export const MAX_FONT_BACKGROUND_RADIUS_PX = 96;
export const DEFAULT_ENABLED_FONT_BACKGROUND_OPACITY = 0.9;
export const DEFAULT_FONT_BACKGROUND_OPACITY = 0;
export const MIN_FONT_BACKGROUND_OPACITY = 0;
export const MAX_FONT_BACKGROUND_OPACITY = 1;

export const DEFAULT_FONT_PRESET_VALUES = {
  fontFamily: DEFAULT_OVERLAY_FONT_FAMILY,
  fontWeight: DEFAULT_OVERLAY_FONT_WEIGHT,
  fontStyle: DEFAULT_OVERLAY_FONT_STYLE,
  textDecoration: DEFAULT_OVERLAY_TEXT_DECORATION,
  fontRenderingMode: DEFAULT_FONT_RENDERING_MODE,
  fontSizePx: 24,
  lineHeight: 1.18,
  letterSpacingPx: 0,
  outlineColor: "#000000",
  outlineWidthPx: 0,
  secondaryOutlineEnabled: false,
  secondaryOutlineColor: "#ffffff",
  secondaryOutlineWidthPx: 0,
  shadowEnabled: false,
  shadowColor: "#000000",
  shadowOpacity: 1,
  shadowBlurPx: 0,
  shadowAngleDeg: 45,
  shadowDistancePx: 0,
  autoFitText: true,
  textColor: "#111111",
  screentoneFillEnabled: false,
  screentoneFillIntensity: 0.55,
  screentoneFillDensity: 0.55,
  screentoneFillAntialias: true,
  filterEffectEnabled: false,
  filterZigzagEnabled: false,
  filterZigzagIntensity: DEFAULT_FILTER_ZIGZAG_INTENSITY,
  filterWaveEnabled: false,
  filterWaveIntensity: DEFAULT_FILTER_WAVE_INTENSITY,
  filterGlitchEnabled: false,
  filterGlitchIntensity: DEFAULT_FILTER_GLITCH_INTENSITY,
  filterPixelateEnabled: false,
  filterPixelateIntensity: DEFAULT_FILTER_PIXELATE_INTENSITY,
  filterBlurEnabled: false,
  filterBlurIntensity: DEFAULT_FILTER_BLUR_INTENSITY,
  filterNoiseEnabled: false,
  filterNoiseIntensity: DEFAULT_FILTER_NOISE_INTENSITY,
  fontBackgroundEnabled: false,
  fontBackgroundScale: DEFAULT_FONT_BACKGROUND_SCALE,
  fontBackgroundColor: DEFAULT_FONT_BACKGROUND_COLOR,
  fontBackgroundRadiusPx: DEFAULT_FONT_BACKGROUND_RADIUS_PX,
  fontBackgroundOpacity: DEFAULT_FONT_BACKGROUND_OPACITY
} satisfies DefaultFontPresetValues;
export type FontPresetValueKey = keyof typeof DEFAULT_FONT_PRESET_VALUES;
export const FONT_PRESET_VALUE_KEYS = Object.keys(DEFAULT_FONT_PRESET_VALUES) as FontPresetValueKey[];

export const BLOCK_TYPE_FONT_PRESET_NAMES: readonly BlockType[] = ["speech", "sfx", "caption", "other"];
export const BLOCK_TYPE_FONT_PRESET_IDS = {
  speech: "font-preset-speech",
  sfx: "font-preset-sfx",
  caption: "font-preset-caption",
  other: "font-preset-other"
} satisfies Record<BlockType, string>;

export type LinkableFontPresetKey = Exclude<
  keyof FontPreset,
  "id" | "name" | "fontSizePresetId" | "fontFamily" | "characterFontOverrides"
>;

export const FONT_PRESET_LINK_FIELD_BY_KEY = {
  fontSizePx: "fontSizeLinkedToPreset",
  lineHeight: "lineHeightLinkedToPreset",
  letterSpacingPx: "letterSpacingLinkedToPreset",
  outlineColor: "outlineColorLinkedToPreset",
  outlineWidthPx: "outlineWidthLinkedToPreset",
  secondaryOutlineEnabled: "secondaryOutlineEnabledLinkedToPreset",
  secondaryOutlineColor: "secondaryOutlineColorLinkedToPreset",
  secondaryOutlineWidthPx: "secondaryOutlineWidthLinkedToPreset",
  shadowEnabled: "shadowEnabledLinkedToPreset",
  shadowColor: "shadowColorLinkedToPreset",
  shadowOpacity: "shadowOpacityLinkedToPreset",
  shadowBlurPx: "shadowBlurPxLinkedToPreset",
  shadowAngleDeg: "shadowAngleDegLinkedToPreset",
  shadowDistancePx: "shadowDistancePxLinkedToPreset",
  autoFitText: "autoFitTextLinkedToPreset",
  textColor: "textColorLinkedToPreset",
  screentoneFillEnabled: "screentoneFillEnabledLinkedToPreset",
  screentoneFillIntensity: "screentoneFillIntensityLinkedToPreset",
  screentoneFillDensity: "screentoneFillDensityLinkedToPreset",
  screentoneFillAntialias: "screentoneFillAntialiasLinkedToPreset",
  filterEffectEnabled: "filterEffectEnabledLinkedToPreset",
  filterZigzagEnabled: "filterZigzagEnabledLinkedToPreset",
  filterZigzagIntensity: "filterZigzagIntensityLinkedToPreset",
  filterWaveEnabled: "filterWaveEnabledLinkedToPreset",
  filterWaveIntensity: "filterWaveIntensityLinkedToPreset",
  filterGlitchEnabled: "filterGlitchEnabledLinkedToPreset",
  filterGlitchIntensity: "filterGlitchIntensityLinkedToPreset",
  filterPixelateEnabled: "filterPixelateEnabledLinkedToPreset",
  filterPixelateIntensity: "filterPixelateIntensityLinkedToPreset",
  filterBlurEnabled: "filterBlurEnabledLinkedToPreset",
  filterBlurIntensity: "filterBlurIntensityLinkedToPreset",
  filterNoiseEnabled: "filterNoiseEnabledLinkedToPreset",
  filterNoiseIntensity: "filterNoiseIntensityLinkedToPreset",
  fontBackgroundEnabled: "fontBackgroundEnabledLinkedToPreset",
  fontBackgroundScale: "fontBackgroundScaleLinkedToPreset",
  fontBackgroundColor: "fontBackgroundColorLinkedToPreset",
  fontBackgroundRadiusPx: "fontBackgroundRadiusPxLinkedToPreset",
  fontBackgroundOpacity: "fontBackgroundOpacityLinkedToPreset",
  fontWeight: "fontWeightLinkedToPreset",
  fontStyle: "fontStyleLinkedToPreset",
  textDecoration: "textDecorationLinkedToPreset",
  fontRenderingMode: "fontRenderingModeLinkedToPreset"
} satisfies Record<LinkableFontPresetKey, keyof TranslationBlock>;

export const FONT_PRESET_LINK_FIELDS = Object.values(FONT_PRESET_LINK_FIELD_BY_KEY) as (typeof FONT_PRESET_LINK_FIELD_BY_KEY)[LinkableFontPresetKey][];
export const LINKABLE_FONT_PRESET_KEYS = Object.keys(FONT_PRESET_LINK_FIELD_BY_KEY) as LinkableFontPresetKey[];

export function buildFontPresetLinkFields(linked: boolean | undefined): Partial<TranslationBlock> {
  return Object.fromEntries(FONT_PRESET_LINK_FIELDS.map((field) => [field, linked])) as Partial<TranslationBlock>;
}

export function createDefaultFontPreset(id: string, name: BlockType): FontPreset {
  return {
    id,
    name,
    ...DEFAULT_FONT_PRESET_VALUES
  };
}

export function normalizeFontPreset(preset: FontPreset): FontPreset {
  const normalized = { ...preset };
  for (const key of FONT_PRESET_VALUE_KEYS) {
    if (normalized[key] === undefined || normalized[key] === null) {
      Object.assign(normalized, { [key]: DEFAULT_FONT_PRESET_VALUES[key] });
    }
  }
  if (preset.secondaryOutlineEnabled === undefined && (preset.secondaryOutlineWidthPx ?? 0) > 0) {
    normalized.secondaryOutlineEnabled = true;
  }
  return normalized;
}

export function ensureBlockTypeFontPresets(fontPresets: FontPreset[] = []): FontPreset[] {
  const next = [...fontPresets];
  const usedIds = new Set(next.map((preset) => preset.id));

  for (const name of BLOCK_TYPE_FONT_PRESET_NAMES) {
    const baseId = BLOCK_TYPE_FONT_PRESET_IDS[name];
    const existingById = next.find((preset) => preset.id === baseId);
    if (existingById) {
      continue;
    }
    let id = baseId;
    let suffix = 2;
    while (usedIds.has(id)) {
      id = `${baseId}-${suffix}`;
      suffix += 1;
    }
    usedIds.add(id);
    next.push(createDefaultFontPreset(id, name));
  }

  return next;
}

export function applyBlockTypeFontPresetToBlock(block: TranslationBlock, fontPresets: FontPreset[]): TranslationBlock {
  const preset = fontPresets.find((candidate) => candidate.id === BLOCK_TYPE_FONT_PRESET_IDS[block.type]);
  if (!preset) {
    return block;
  }
  const normalizedPreset = normalizeFontPreset(preset);

  return {
    ...block,
    fontPresetId: normalizedPreset.id,
    ...buildFontPresetLinkFields(true),
    characterFontOverrides: normalizedPreset.characterFontOverrides?.map((override) => ({ ...override })),
    ...pickFontPresetBlockValues(normalizedPreset)
  };
}

function pickFontPresetBlockValues(preset: FontPreset): Partial<TranslationBlock> {
  const values: Partial<TranslationBlock> = {};
  for (const key of FONT_PRESET_VALUE_KEYS) {
    Object.assign(values, { [key]: preset[key] });
  }
  return values;
}
