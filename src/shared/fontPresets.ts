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

  return {
    ...block,
    fontPresetId: preset.id,
    ...buildLinkedFontPresetFields(),
    fontFamily: preset.fontFamily,
    characterFontOverrides: preset.characterFontOverrides?.map((override) => ({ ...override })),
    fontWeight: preset.fontWeight,
    fontStyle: preset.fontStyle,
    textDecoration: preset.textDecoration,
    fontSizePx: preset.fontSizePx,
    lineHeight: preset.lineHeight,
    letterSpacingPx: preset.letterSpacingPx,
    outlineColor: preset.outlineColor,
    outlineWidthPx: preset.outlineWidthPx,
    secondaryOutlineColor: preset.secondaryOutlineColor,
    secondaryOutlineWidthPx: preset.secondaryOutlineWidthPx,
    shadowEnabled: preset.shadowEnabled,
    shadowColor: preset.shadowColor,
    shadowOpacity: preset.shadowOpacity,
    shadowBlurPx: preset.shadowBlurPx,
    shadowAngleDeg: preset.shadowAngleDeg,
    shadowDistancePx: preset.shadowDistancePx,
    autoFitText: preset.autoFitText,
    textColor: preset.textColor ?? block.textColor,
    screentoneFillEnabled: preset.screentoneFillEnabled,
    screentoneFillIntensity: preset.screentoneFillIntensity,
    screentoneFillDensity: preset.screentoneFillDensity,
    screentoneFillAntialias: preset.screentoneFillAntialias
  };
}

function buildLinkedFontPresetFields(): Partial<TranslationBlock> {
  return Object.fromEntries(BLOCK_FONT_PRESET_LINK_FIELDS.map((field) => [field, true])) as Partial<TranslationBlock>;
}
