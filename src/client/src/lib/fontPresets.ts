import type { FontPreset, FontSizePreset, TranslationBlock } from "../../../shared/types";
import { DEFAULT_FONT_PRESET_VALUES, normalizeFontPreset } from "../../../shared/fontPresets";

export type FontPresetPatch = Partial<
  Pick<
    FontPreset,
    | "fontFamily"
    | "characterFontOverrides"
    | "fontWeight"
    | "fontStyle"
    | "textDecoration"
    | "fontSizePx"
    | "lineHeight"
    | "letterSpacingPx"
    | "outlineColor"
    | "outlineWidthPx"
    | "secondaryOutlineColor"
    | "secondaryOutlineWidthPx"
    | "shadowEnabled"
    | "shadowColor"
    | "shadowOpacity"
    | "shadowBlurPx"
    | "shadowAngleDeg"
    | "shadowDistancePx"
    | "autoFitText"
    | "textColor"
    | "screentoneFillEnabled"
    | "screentoneFillIntensity"
    | "screentoneFillDensity"
    | "screentoneFillAntialias"
  >
>;
export type BlockFontPatch = FontPresetPatch & Partial<Pick<TranslationBlock, "textAlign" | "textPosition">>;
export type LinkableFontPresetKey = Exclude<keyof FontPresetPatch, "fontFamily" | "characterFontOverrides">;

const PRESET_LINK_FIELD_BY_KEY = {
  fontSizePx: "fontSizeLinkedToPreset",
  lineHeight: "lineHeightLinkedToPreset",
  letterSpacingPx: "letterSpacingLinkedToPreset",
  outlineColor: "outlineColorLinkedToPreset",
  outlineWidthPx: "outlineWidthLinkedToPreset",
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
  fontWeight: "fontWeightLinkedToPreset",
  fontStyle: "fontStyleLinkedToPreset",
  textDecoration: "textDecorationLinkedToPreset"
} satisfies Record<LinkableFontPresetKey, keyof TranslationBlock>;

export const FONT_PRESET_LINK_FIELDS = Object.values(PRESET_LINK_FIELD_BY_KEY) as (typeof PRESET_LINK_FIELD_BY_KEY)[LinkableFontPresetKey][];

export const DEFAULT_FONT_PRESET: Omit<FontPreset, "id" | "name"> = DEFAULT_FONT_PRESET_VALUES;

export function buildAlphabeticPresetName(index: number): string {
  let remaining = Math.max(0, Math.floor(index));
  let name = "";

  do {
    name = String.fromCharCode(65 + (remaining % 26)) + name;
    remaining = Math.floor(remaining / 26) - 1;
  } while (remaining >= 0);

  return name;
}

export function buildNextFontSizePresetName(fontSizePresets: FontSizePreset[]): string {
  const usedNames = new Set(fontSizePresets.map((preset) => preset.name.trim()));
  let index = 0;
  let name = buildAlphabeticPresetName(index);

  while (usedNames.has(name)) {
    index += 1;
    name = buildAlphabeticPresetName(index);
  }

  return name;
}

export function createFontSizePreset(name: string, fontSizePx: number): FontSizePreset {
  return {
    id: `font-size-preset-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    name,
    fontSizePx
  };
}

export function createFontPreset(name: string, source: FontPresetPatch = DEFAULT_FONT_PRESET): FontPreset {
  return normalizeFontPreset({
    id: `font-preset-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    name,
    fontFamily: source.fontFamily,
    characterFontOverrides: cloneCharacterFontOverrides(source.characterFontOverrides ?? DEFAULT_FONT_PRESET.characterFontOverrides),
    fontWeight: source.fontWeight,
    fontStyle: source.fontStyle,
    textDecoration: source.textDecoration,
    fontSizePx: source.fontSizePx ?? DEFAULT_FONT_PRESET.fontSizePx,
    lineHeight: source.lineHeight ?? DEFAULT_FONT_PRESET.lineHeight,
    letterSpacingPx: source.letterSpacingPx,
    outlineColor: source.outlineColor,
    outlineWidthPx: source.outlineWidthPx,
    secondaryOutlineColor: source.secondaryOutlineColor,
    secondaryOutlineWidthPx: source.secondaryOutlineWidthPx,
    shadowEnabled: source.shadowEnabled,
    shadowColor: source.shadowColor,
    shadowOpacity: source.shadowOpacity,
    shadowBlurPx: source.shadowBlurPx,
    shadowAngleDeg: source.shadowAngleDeg,
    shadowDistancePx: source.shadowDistancePx,
    autoFitText: source.autoFitText,
    textColor: source.textColor,
    screentoneFillEnabled: source.screentoneFillEnabled,
    screentoneFillIntensity: source.screentoneFillIntensity,
    screentoneFillDensity: source.screentoneFillDensity,
    screentoneFillAntialias: source.screentoneFillAntialias
  });
}

export function resolveFontPresetSize(preset: FontPreset, fontSizePresets: FontSizePreset[] = []): number {
  if (!preset.fontSizePresetId) {
    return preset.fontSizePx;
  }
  return fontSizePresets.find((candidate) => candidate.id === preset.fontSizePresetId)?.fontSizePx ?? preset.fontSizePx;
}

export function resolveFontPreset(preset: FontPreset, fontSizePresets: FontSizePreset[] = []): FontPreset {
  const normalizedPreset = normalizeFontPreset(preset);
  const fontSizePx = resolveFontPresetSize(normalizedPreset, fontSizePresets);
  return fontSizePx === normalizedPreset.fontSizePx ? normalizedPreset : { ...normalizedPreset, fontSizePx };
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
  const nextBlock = { ...block };
  for (const field of FONT_PRESET_LINK_FIELDS) {
    delete nextBlock[field];
  }
  return nextBlock;
}

export function normalizeCharacterOverrideCharacter(value: string): string {
  const normalized = value.trim();
  return [...normalized][0] ?? "";
}

export function normalizeCharacterFontOverrides(
  overrides: readonly NonNullable<FontPreset["characterFontOverrides"]>[number][] | undefined
): NonNullable<FontPreset["characterFontOverrides"]> {
  const byCharacter = new Map<string, NonNullable<FontPreset["characterFontOverrides"]>[number]>();

  for (const override of overrides ?? []) {
    const character = normalizeCharacterOverrideCharacter(override.character);
    const fontFamily = override.fontFamily?.trim();
    if (!character || !fontFamily) {
      continue;
    }
    byCharacter.set(character, { character, fontFamily });
  }

  return [...byCharacter.values()];
}

function cloneCharacterFontOverrides(
  overrides: FontPreset["characterFontOverrides"] | TranslationBlock["characterFontOverrides"] | undefined
): NonNullable<FontPreset["characterFontOverrides"]> | undefined {
  const normalized = normalizeCharacterFontOverrides(overrides);
  return normalized.length > 0 ? normalized : undefined;
}

function isFullFontPresetPatch(patch: FontPresetPatch): patch is FontPreset {
  return "id" in patch && "name" in patch;
}

export function applyFontPresetPatchToBlock(
  block: TranslationBlock,
  patch: FontPresetPatch,
  options: { forceLinkedValues?: boolean } = {}
): TranslationBlock {
  const forceLinkedValues = options.forceLinkedValues ?? false;
  const normalizedPatch = isFullFontPresetPatch(patch) ? normalizeFontPreset(patch) : patch;
  const fullFontPresetPatch = isFullFontPresetPatch(normalizedPatch);

  return {
    ...block,
    fontFamily: normalizedPatch.fontFamily ?? block.fontFamily,
    characterFontOverrides:
      normalizedPatch.characterFontOverrides !== undefined || fullFontPresetPatch
        ? cloneCharacterFontOverrides(normalizedPatch.characterFontOverrides)
        : block.characterFontOverrides,
    fontWeight:
      normalizedPatch.fontWeight !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "fontWeight"))
        ? normalizedPatch.fontWeight
        : block.fontWeight,
    fontStyle:
      normalizedPatch.fontStyle !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "fontStyle"))
        ? normalizedPatch.fontStyle
        : block.fontStyle,
    textDecoration:
      normalizedPatch.textDecoration !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "textDecoration"))
        ? normalizedPatch.textDecoration
        : block.textDecoration,
    fontSizePx:
      normalizedPatch.fontSizePx !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "fontSizePx"))
        ? normalizedPatch.fontSizePx
        : block.fontSizePx,
    lineHeight:
      normalizedPatch.lineHeight !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "lineHeight"))
        ? normalizedPatch.lineHeight
        : block.lineHeight,
    letterSpacingPx:
      normalizedPatch.letterSpacingPx !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "letterSpacingPx"))
        ? normalizedPatch.letterSpacingPx
        : block.letterSpacingPx,
    outlineColor:
      normalizedPatch.outlineColor !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "outlineColor"))
        ? normalizedPatch.outlineColor
        : block.outlineColor,
    outlineWidthPx:
      normalizedPatch.outlineWidthPx !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "outlineWidthPx"))
        ? normalizedPatch.outlineWidthPx
        : block.outlineWidthPx,
    secondaryOutlineColor:
      normalizedPatch.secondaryOutlineColor !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "secondaryOutlineColor"))
        ? normalizedPatch.secondaryOutlineColor
        : block.secondaryOutlineColor,
    secondaryOutlineWidthPx:
      normalizedPatch.secondaryOutlineWidthPx !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "secondaryOutlineWidthPx"))
        ? normalizedPatch.secondaryOutlineWidthPx
        : block.secondaryOutlineWidthPx,
    shadowEnabled:
      normalizedPatch.shadowEnabled !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "shadowEnabled"))
        ? normalizedPatch.shadowEnabled
        : block.shadowEnabled,
    shadowColor:
      normalizedPatch.shadowColor !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "shadowColor"))
        ? normalizedPatch.shadowColor
        : block.shadowColor,
    shadowOpacity:
      normalizedPatch.shadowOpacity !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "shadowOpacity"))
        ? normalizedPatch.shadowOpacity
        : block.shadowOpacity,
    shadowBlurPx:
      normalizedPatch.shadowBlurPx !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "shadowBlurPx"))
        ? normalizedPatch.shadowBlurPx
        : block.shadowBlurPx,
    shadowAngleDeg:
      normalizedPatch.shadowAngleDeg !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "shadowAngleDeg"))
        ? normalizedPatch.shadowAngleDeg
        : block.shadowAngleDeg,
    shadowDistancePx:
      normalizedPatch.shadowDistancePx !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "shadowDistancePx"))
        ? normalizedPatch.shadowDistancePx
        : block.shadowDistancePx,
    autoFitText:
      normalizedPatch.autoFitText !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "autoFitText"))
        ? normalizedPatch.autoFitText
        : block.autoFitText,
    textColor:
      normalizedPatch.textColor !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "textColor"))
        ? normalizedPatch.textColor
        : block.textColor,
    screentoneFillEnabled:
      normalizedPatch.screentoneFillEnabled !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "screentoneFillEnabled"))
        ? normalizedPatch.screentoneFillEnabled
        : block.screentoneFillEnabled,
    screentoneFillIntensity:
      normalizedPatch.screentoneFillIntensity !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "screentoneFillIntensity"))
        ? normalizedPatch.screentoneFillIntensity
        : block.screentoneFillIntensity,
    screentoneFillDensity:
      normalizedPatch.screentoneFillDensity !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "screentoneFillDensity"))
        ? normalizedPatch.screentoneFillDensity
        : block.screentoneFillDensity,
    screentoneFillAntialias:
      normalizedPatch.screentoneFillAntialias !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, "screentoneFillAntialias"))
        ? normalizedPatch.screentoneFillAntialias
        : block.screentoneFillAntialias
  };
}
