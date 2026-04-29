import type { FontPreset, TranslationBlock } from "../../../shared/types";
import { DEFAULT_OVERLAY_FONT_FAMILY } from "./overlayLayout";

export type FontPresetPatch = Partial<
  Pick<FontPreset, "fontFamily" | "fontSizePx" | "lineHeight" | "outlineColor" | "outlineWidthPx" | "autoFitText" | "textColor">
>;
export type BlockFontPatch = FontPresetPatch & Partial<Pick<TranslationBlock, "textAlign">>;
export type LinkableFontPresetKey = Exclude<keyof FontPresetPatch, "fontFamily">;

const PRESET_LINK_FIELD_BY_KEY = {
  fontSizePx: "fontSizeLinkedToPreset",
  lineHeight: "lineHeightLinkedToPreset",
  outlineColor: "outlineColorLinkedToPreset",
  outlineWidthPx: "outlineWidthLinkedToPreset",
  autoFitText: "autoFitTextLinkedToPreset",
  textColor: "textColorLinkedToPreset"
} satisfies Record<LinkableFontPresetKey, keyof TranslationBlock>;

export const DEFAULT_FONT_PRESET: Omit<FontPreset, "id" | "name"> = {
  fontFamily: DEFAULT_OVERLAY_FONT_FAMILY,
  fontSizePx: 24,
  lineHeight: 1.18,
  outlineColor: "#000000",
  outlineWidthPx: 0,
  autoFitText: true,
  textColor: "#111111"
};

export function createFontPreset(name: string, source: FontPresetPatch = DEFAULT_FONT_PRESET): FontPreset {
  return {
    id: `font-preset-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    name,
    fontFamily: source.fontFamily ?? DEFAULT_FONT_PRESET.fontFamily,
    fontSizePx: source.fontSizePx ?? DEFAULT_FONT_PRESET.fontSizePx,
    lineHeight: source.lineHeight ?? DEFAULT_FONT_PRESET.lineHeight,
    outlineColor: source.outlineColor ?? DEFAULT_FONT_PRESET.outlineColor,
    outlineWidthPx: source.outlineWidthPx ?? DEFAULT_FONT_PRESET.outlineWidthPx,
    autoFitText: source.autoFitText ?? DEFAULT_FONT_PRESET.autoFitText,
    textColor: source.textColor ?? DEFAULT_FONT_PRESET.textColor
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
    autoFitTextLinkedToPreset: _autoFitTextLinkedToPreset,
    textColorLinkedToPreset: _textColorLinkedToPreset,
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
    autoFitText:
      patch.autoFitText !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "autoFitText"))
        ? patch.autoFitText
        : block.autoFitText,
    textColor:
      patch.textColor !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "textColor"))
        ? patch.textColor
        : block.textColor
  };
}
