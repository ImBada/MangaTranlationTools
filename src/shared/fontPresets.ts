import type { BlockType, FontPreset, TranslationBlock } from "./types";

export const DEFAULT_OVERLAY_FONT_FAMILY = "\"Malgun Gothic\", \"Apple SD Gothic Neo\", sans-serif";
export const DEFAULT_OVERLAY_FONT_WEIGHT = 700;
export const DEFAULT_OVERLAY_FONT_STYLE = "normal";
export const DEFAULT_OVERLAY_TEXT_DECORATION = "none";
export const DEFAULT_ENABLED_SHADOW_DISTANCE_PX = 4;

export const DEFAULT_FONT_PRESET_VALUES: Omit<FontPreset, "id" | "name"> = {
  fontFamily: DEFAULT_OVERLAY_FONT_FAMILY,
  fontWeight: DEFAULT_OVERLAY_FONT_WEIGHT,
  fontStyle: DEFAULT_OVERLAY_FONT_STYLE,
  textDecoration: DEFAULT_OVERLAY_TEXT_DECORATION,
  fontSizePx: 24,
  lineHeight: 1.18,
  letterSpacingPx: 0,
  outlineColor: "#000000",
  outlineWidthPx: 0,
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
  screentoneFillAntialias: true
};

export const BLOCK_TYPE_FONT_PRESET_NAMES: readonly BlockType[] = ["speech", "sfx", "caption", "other"];
export const BLOCK_TYPE_FONT_PRESET_IDS = {
  speech: "font-preset-speech",
  sfx: "font-preset-sfx",
  caption: "font-preset-caption",
  other: "font-preset-other"
} satisfies Record<BlockType, string>;

const BLOCK_FONT_PRESET_LINK_FIELDS = [
  "fontSizeLinkedToPreset",
  "lineHeightLinkedToPreset",
  "letterSpacingLinkedToPreset",
  "outlineColorLinkedToPreset",
  "outlineWidthLinkedToPreset",
  "secondaryOutlineColorLinkedToPreset",
  "secondaryOutlineWidthLinkedToPreset",
  "shadowEnabledLinkedToPreset",
  "shadowColorLinkedToPreset",
  "shadowOpacityLinkedToPreset",
  "shadowBlurPxLinkedToPreset",
  "shadowAngleDegLinkedToPreset",
  "shadowDistancePxLinkedToPreset",
  "autoFitTextLinkedToPreset",
  "textColorLinkedToPreset",
  "screentoneFillEnabledLinkedToPreset",
  "screentoneFillIntensityLinkedToPreset",
  "screentoneFillDensityLinkedToPreset",
  "screentoneFillAntialiasLinkedToPreset",
  "fontWeightLinkedToPreset",
  "fontStyleLinkedToPreset",
  "textDecorationLinkedToPreset"
] satisfies readonly (keyof TranslationBlock)[];

export function createDefaultFontPreset(id: string, name: BlockType): FontPreset {
  return {
    id,
    name,
    ...DEFAULT_FONT_PRESET_VALUES
  };
}

export function normalizeFontPreset(preset: FontPreset): FontPreset {
  return {
    ...preset,
    fontFamily: preset.fontFamily ?? DEFAULT_FONT_PRESET_VALUES.fontFamily,
    fontWeight: preset.fontWeight ?? DEFAULT_FONT_PRESET_VALUES.fontWeight,
    fontStyle: preset.fontStyle ?? DEFAULT_FONT_PRESET_VALUES.fontStyle,
    textDecoration: preset.textDecoration ?? DEFAULT_FONT_PRESET_VALUES.textDecoration,
    fontSizePx: preset.fontSizePx ?? DEFAULT_FONT_PRESET_VALUES.fontSizePx,
    lineHeight: preset.lineHeight ?? DEFAULT_FONT_PRESET_VALUES.lineHeight,
    letterSpacingPx: preset.letterSpacingPx ?? DEFAULT_FONT_PRESET_VALUES.letterSpacingPx,
    outlineColor: preset.outlineColor ?? DEFAULT_FONT_PRESET_VALUES.outlineColor,
    outlineWidthPx: preset.outlineWidthPx ?? DEFAULT_FONT_PRESET_VALUES.outlineWidthPx,
    secondaryOutlineColor: preset.secondaryOutlineColor ?? DEFAULT_FONT_PRESET_VALUES.secondaryOutlineColor,
    secondaryOutlineWidthPx: preset.secondaryOutlineWidthPx ?? DEFAULT_FONT_PRESET_VALUES.secondaryOutlineWidthPx,
    shadowEnabled: preset.shadowEnabled ?? DEFAULT_FONT_PRESET_VALUES.shadowEnabled,
    shadowColor: preset.shadowColor ?? DEFAULT_FONT_PRESET_VALUES.shadowColor,
    shadowOpacity: preset.shadowOpacity ?? DEFAULT_FONT_PRESET_VALUES.shadowOpacity,
    shadowBlurPx: preset.shadowBlurPx ?? DEFAULT_FONT_PRESET_VALUES.shadowBlurPx,
    shadowAngleDeg: preset.shadowAngleDeg ?? DEFAULT_FONT_PRESET_VALUES.shadowAngleDeg,
    shadowDistancePx: preset.shadowDistancePx ?? DEFAULT_FONT_PRESET_VALUES.shadowDistancePx,
    autoFitText: preset.autoFitText ?? DEFAULT_FONT_PRESET_VALUES.autoFitText,
    textColor: preset.textColor ?? DEFAULT_FONT_PRESET_VALUES.textColor,
    screentoneFillEnabled: preset.screentoneFillEnabled ?? DEFAULT_FONT_PRESET_VALUES.screentoneFillEnabled,
    screentoneFillIntensity: preset.screentoneFillIntensity ?? DEFAULT_FONT_PRESET_VALUES.screentoneFillIntensity,
    screentoneFillDensity: preset.screentoneFillDensity ?? DEFAULT_FONT_PRESET_VALUES.screentoneFillDensity,
    screentoneFillAntialias: preset.screentoneFillAntialias ?? DEFAULT_FONT_PRESET_VALUES.screentoneFillAntialias
  };
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
    ...buildLinkedFontPresetFields(),
    fontFamily: normalizedPreset.fontFamily,
    characterFontOverrides: normalizedPreset.characterFontOverrides?.map((override) => ({ ...override })),
    fontWeight: normalizedPreset.fontWeight,
    fontStyle: normalizedPreset.fontStyle,
    textDecoration: normalizedPreset.textDecoration,
    fontSizePx: normalizedPreset.fontSizePx,
    lineHeight: normalizedPreset.lineHeight,
    letterSpacingPx: normalizedPreset.letterSpacingPx,
    outlineColor: normalizedPreset.outlineColor,
    outlineWidthPx: normalizedPreset.outlineWidthPx,
    secondaryOutlineColor: normalizedPreset.secondaryOutlineColor,
    secondaryOutlineWidthPx: normalizedPreset.secondaryOutlineWidthPx,
    shadowEnabled: normalizedPreset.shadowEnabled,
    shadowColor: normalizedPreset.shadowColor,
    shadowOpacity: normalizedPreset.shadowOpacity,
    shadowBlurPx: normalizedPreset.shadowBlurPx,
    shadowAngleDeg: normalizedPreset.shadowAngleDeg,
    shadowDistancePx: normalizedPreset.shadowDistancePx,
    autoFitText: normalizedPreset.autoFitText,
    textColor: normalizedPreset.textColor ?? block.textColor,
    screentoneFillEnabled: normalizedPreset.screentoneFillEnabled,
    screentoneFillIntensity: normalizedPreset.screentoneFillIntensity,
    screentoneFillDensity: normalizedPreset.screentoneFillDensity,
    screentoneFillAntialias: normalizedPreset.screentoneFillAntialias
  };
}

function buildLinkedFontPresetFields(): Partial<TranslationBlock> {
  return Object.fromEntries(BLOCK_FONT_PRESET_LINK_FIELDS.map((field) => [field, true])) as Partial<TranslationBlock>;
}
