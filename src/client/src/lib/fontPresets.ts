import type { FontPreset, TranslationBlock } from "../../../shared/types";
import { DEFAULT_FONT_PRESET_VALUES } from "../../../shared/fontPresets";

export type FontPresetPatch = Partial<
  Pick<
    FontPreset,
    | "fontFamily"
    | "fontWeight"
    | "fontStyle"
    | "textDecoration"
    | "fontSizePx"
    | "lineHeight"
    | "outlineColor"
    | "outlineWidthPx"
    | "secondaryOutlineColor"
    | "secondaryOutlineWidthPx"
    | "autoFitText"
    | "textColor"
    | "screentoneFillEnabled"
    | "screentoneFillIntensity"
    | "screentoneFillDensity"
    | "screentoneFillAntialias"
  >
>;
export type BlockFontPatch = FontPresetPatch & Partial<Pick<TranslationBlock, "textAlign" | "textPosition">>;
export type LinkableFontPresetKey = Exclude<keyof FontPresetPatch, "fontFamily">;

const PRESET_LINK_FIELD_BY_KEY = {
  fontSizePx: "fontSizeLinkedToPreset",
  lineHeight: "lineHeightLinkedToPreset",
  outlineColor: "outlineColorLinkedToPreset",
  outlineWidthPx: "outlineWidthLinkedToPreset",
  secondaryOutlineColor: "secondaryOutlineColorLinkedToPreset",
  secondaryOutlineWidthPx: "secondaryOutlineWidthLinkedToPreset",
  autoFitText: "autoFitTextLinkedToPreset",
  textColor: "textColorLinkedToPreset",
  screentoneFillEnabled: "screentoneFillEnabledLinkedToPreset",
  screentoneFillIntensity: "screentoneFillIntensityLinkedToPreset",
  screentoneFillDensity: "screentoneFillDensityLinkedToPreset",
  screentoneFillAntialias: "screentoneFillAntialiasLinkedToPreset",
  fontWeight: "fontWeightLinkedToPreset",
  fontStyle: "fontStyleLinkedToPreset",
  textDecoration: "textDecorationLinkedToPreset"
} satisfies Record<LinkableFontPresetKey, keyof TranslationBlock>;

export const DEFAULT_FONT_PRESET: Omit<FontPreset, "id" | "name"> = DEFAULT_FONT_PRESET_VALUES;

export function createFontPreset(name: string, source: FontPresetPatch = DEFAULT_FONT_PRESET): FontPreset {
  return {
    id: `font-preset-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    name,
    fontFamily: source.fontFamily ?? DEFAULT_FONT_PRESET.fontFamily,
    fontWeight: source.fontWeight ?? DEFAULT_FONT_PRESET.fontWeight,
    fontStyle: source.fontStyle ?? DEFAULT_FONT_PRESET.fontStyle,
    textDecoration: source.textDecoration ?? DEFAULT_FONT_PRESET.textDecoration,
    fontSizePx: source.fontSizePx ?? DEFAULT_FONT_PRESET.fontSizePx,
    lineHeight: source.lineHeight ?? DEFAULT_FONT_PRESET.lineHeight,
    outlineColor: source.outlineColor ?? DEFAULT_FONT_PRESET.outlineColor,
    outlineWidthPx: source.outlineWidthPx ?? DEFAULT_FONT_PRESET.outlineWidthPx,
    secondaryOutlineColor: source.secondaryOutlineColor ?? DEFAULT_FONT_PRESET.secondaryOutlineColor,
    secondaryOutlineWidthPx: source.secondaryOutlineWidthPx ?? DEFAULT_FONT_PRESET.secondaryOutlineWidthPx,
    autoFitText: source.autoFitText ?? DEFAULT_FONT_PRESET.autoFitText,
    textColor: source.textColor ?? DEFAULT_FONT_PRESET.textColor,
    screentoneFillEnabled: source.screentoneFillEnabled ?? DEFAULT_FONT_PRESET.screentoneFillEnabled,
    screentoneFillIntensity: source.screentoneFillIntensity ?? DEFAULT_FONT_PRESET.screentoneFillIntensity,
    screentoneFillDensity: source.screentoneFillDensity ?? DEFAULT_FONT_PRESET.screentoneFillDensity,
    screentoneFillAntialias: source.screentoneFillAntialias ?? DEFAULT_FONT_PRESET.screentoneFillAntialias
  };
}

export function isBlockFontSizeLinkedToPreset(block: Pick<TranslationBlock, "fontSizeLinkedToPreset">): boolean {
  return block.fontSizeLinkedToPreset !== false;
}

export function isBlockFontPresetValueLinked(block: TranslationBlock, key: LinkableFontPresetKey): boolean {
  return block[PRESET_LINK_FIELD_BY_KEY[key]] !== false;
}

export function buildFontPresetLinkPatch(key: LinkableFontPresetKey, linked: boolean): Partial<TranslationBlock> {
  return { [PRESET_LINK_FIELD_BY_KEY[key]]: linked };
}

export function clearFontPresetLinkFields(block: TranslationBlock): TranslationBlock {
  const {
    fontSizeLinkedToPreset: _fontSizeLinkedToPreset,
    lineHeightLinkedToPreset: _lineHeightLinkedToPreset,
    outlineColorLinkedToPreset: _outlineColorLinkedToPreset,
    outlineWidthLinkedToPreset: _outlineWidthLinkedToPreset,
    secondaryOutlineColorLinkedToPreset: _secondaryOutlineColorLinkedToPreset,
    secondaryOutlineWidthLinkedToPreset: _secondaryOutlineWidthLinkedToPreset,
    autoFitTextLinkedToPreset: _autoFitTextLinkedToPreset,
    textColorLinkedToPreset: _textColorLinkedToPreset,
    screentoneFillEnabledLinkedToPreset: _screentoneFillEnabledLinkedToPreset,
    screentoneFillIntensityLinkedToPreset: _screentoneFillIntensityLinkedToPreset,
    screentoneFillDensityLinkedToPreset: _screentoneFillDensityLinkedToPreset,
    screentoneFillAntialiasLinkedToPreset: _screentoneFillAntialiasLinkedToPreset,
    fontWeightLinkedToPreset: _fontWeightLinkedToPreset,
    fontStyleLinkedToPreset: _fontStyleLinkedToPreset,
    textDecorationLinkedToPreset: _textDecorationLinkedToPreset,
    ...rest
  } = block;
  return rest;
}

export function applyFontPresetPatchToBlock(
  block: TranslationBlock,
  patch: FontPresetPatch,
  options: { forceLinkedValues?: boolean } = {}
): TranslationBlock {
  const forceLinkedValues = options.forceLinkedValues ?? false;

  return {
    ...block,
    fontFamily: patch.fontFamily ?? block.fontFamily,
    fontWeight:
      patch.fontWeight !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "fontWeight"))
        ? patch.fontWeight
        : block.fontWeight,
    fontStyle:
      patch.fontStyle !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "fontStyle"))
        ? patch.fontStyle
        : block.fontStyle,
    textDecoration:
      patch.textDecoration !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "textDecoration"))
        ? patch.textDecoration
        : block.textDecoration,
    fontSizePx:
      patch.fontSizePx !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "fontSizePx"))
        ? patch.fontSizePx
        : block.fontSizePx,
    lineHeight:
      patch.lineHeight !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "lineHeight"))
        ? patch.lineHeight
        : block.lineHeight,
    outlineColor:
      patch.outlineColor !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "outlineColor"))
        ? patch.outlineColor
        : block.outlineColor,
    outlineWidthPx:
      patch.outlineWidthPx !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "outlineWidthPx"))
        ? patch.outlineWidthPx
        : block.outlineWidthPx,
    secondaryOutlineColor:
      patch.secondaryOutlineColor !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "secondaryOutlineColor"))
        ? patch.secondaryOutlineColor
        : block.secondaryOutlineColor,
    secondaryOutlineWidthPx:
      patch.secondaryOutlineWidthPx !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "secondaryOutlineWidthPx"))
        ? patch.secondaryOutlineWidthPx
        : block.secondaryOutlineWidthPx,
    autoFitText:
      patch.autoFitText !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "autoFitText"))
        ? patch.autoFitText
        : block.autoFitText,
    textColor:
      patch.textColor !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "textColor"))
        ? patch.textColor
        : block.textColor,
    screentoneFillEnabled:
      patch.screentoneFillEnabled !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "screentoneFillEnabled"))
        ? patch.screentoneFillEnabled
        : block.screentoneFillEnabled,
    screentoneFillIntensity:
      patch.screentoneFillIntensity !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "screentoneFillIntensity"))
        ? patch.screentoneFillIntensity
        : block.screentoneFillIntensity,
    screentoneFillDensity:
      patch.screentoneFillDensity !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "screentoneFillDensity"))
        ? patch.screentoneFillDensity
        : block.screentoneFillDensity,
    screentoneFillAntialias:
      patch.screentoneFillAntialias !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "screentoneFillAntialias"))
        ? patch.screentoneFillAntialias
        : block.screentoneFillAntialias
  };
}
