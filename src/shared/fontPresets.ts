import type { BlockType, FontPreset, TranslationBlock } from "./types";

export const DEFAULT_OVERLAY_FONT_FAMILY = "\"Malgun Gothic\", \"Apple SD Gothic Neo\", sans-serif";

export const DEFAULT_FONT_PRESET_VALUES: Omit<FontPreset, "id" | "name"> = {
  fontFamily: DEFAULT_OVERLAY_FONT_FAMILY,
  fontSizePx: 24,
  lineHeight: 1.18,
  outlineColor: "#000000",
  outlineWidthPx: 0,
  secondaryOutlineColor: "#ffffff",
  secondaryOutlineWidthPx: 0,
  autoFitText: true,
  textColor: "#111111",
  screentoneFillEnabled: false,
  screentoneFillIntensity: 0.55,
  screentoneFillDensity: 0.55,
  screentoneFillAntialias: true
};

export const BLOCK_TYPE_FONT_PRESET_NAMES: readonly BlockType[] = ["speech", "sfx", "caption", "other"];

const BLOCK_FONT_PRESET_LINK_FIELDS = [
  "fontSizeLinkedToPreset",
  "lineHeightLinkedToPreset",
  "outlineColorLinkedToPreset",
  "outlineWidthLinkedToPreset",
  "secondaryOutlineColorLinkedToPreset",
  "secondaryOutlineWidthLinkedToPreset",
  "autoFitTextLinkedToPreset",
  "textColorLinkedToPreset",
  "screentoneFillEnabledLinkedToPreset",
  "screentoneFillIntensityLinkedToPreset",
  "screentoneFillDensityLinkedToPreset",
  "screentoneFillAntialiasLinkedToPreset"
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
    if (next.some((preset) => preset.name === name)) {
      continue;
    }
    const baseId = `font-preset-${name}`;
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
  const preset = fontPresets.find((candidate) => candidate.name === block.type);
  if (!preset) {
    return block;
  }

  return {
    ...block,
    fontPresetId: preset.id,
    ...buildLinkedFontPresetFields(),
    fontFamily: preset.fontFamily,
    fontSizePx: preset.fontSizePx,
    lineHeight: preset.lineHeight,
    outlineColor: preset.outlineColor,
    outlineWidthPx: preset.outlineWidthPx,
    secondaryOutlineColor: preset.secondaryOutlineColor,
    secondaryOutlineWidthPx: preset.secondaryOutlineWidthPx,
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
