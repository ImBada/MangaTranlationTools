import type { FontPreset, FontSizePreset, TranslationBlock } from "../../../shared/types";
import { FONT_FILTER_EFFECT_DEFINITIONS } from "./fontFilterEffectMetadata";
import {
  buildFontPresetLinkFields,
  DEFAULT_FONT_PRESET_VALUES,
  FONT_PRESET_LINK_FIELD_BY_KEY,
  FONT_PRESET_LINK_FIELDS,
  FONT_PRESET_VALUE_KEYS,
  LINKABLE_FONT_PRESET_KEYS,
  normalizeFontPreset,
  type LinkableFontPresetKey
} from "../../../shared/fontPresets";
export {
  buildFontPresetLinkFields,
  FONT_PRESET_LINK_FIELDS,
  LINKABLE_FONT_PRESET_KEYS,
  type LinkableFontPresetKey
} from "../../../shared/fontPresets";

export type FontPresetPatch = Partial<Omit<FontPreset, "id" | "name" | "fontSizePresetId">>;
export type BlockFontPatch = FontPresetPatch & Partial<Pick<TranslationBlock, "textAlign" | "textPosition">>;

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
  const sourceValues: FontPresetPatch = {};
  for (const key of FONT_PRESET_VALUE_KEYS) {
    Object.assign(sourceValues, { [key]: source[key] });
  }
  return normalizeFontPreset({
    ...sourceValues,
    id: `font-preset-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    name,
    characterFontOverrides: cloneCharacterFontOverrides(source.characterFontOverrides ?? DEFAULT_FONT_PRESET.characterFontOverrides),
    fontSizePx: source.fontSizePx ?? DEFAULT_FONT_PRESET.fontSizePx,
    lineHeight: source.lineHeight ?? DEFAULT_FONT_PRESET.lineHeight
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

const FILTER_EFFECT_CHILD_PRESET_KEYS = new Set<LinkableFontPresetKey>(
  FONT_FILTER_EFFECT_DEFINITIONS.flatMap((definition) => [definition.enabledKey, definition.intensityKey])
);
const FILTER_EFFECT_PRESET_KEYS: LinkableFontPresetKey[] = [
  "filterEffectEnabled",
  ...FONT_FILTER_EFFECT_DEFINITIONS.flatMap((definition) => [definition.enabledKey, definition.intensityKey])
];

export function isBlockFontPresetValueLinked(block: TranslationBlock, key: LinkableFontPresetKey): boolean {
  if (FILTER_EFFECT_CHILD_PRESET_KEYS.has(key) && block.filterEffectEnabledLinkedToPreset === false) {
    return false;
  }
  return block[FONT_PRESET_LINK_FIELD_BY_KEY[key]] !== false;
}

export function buildFontPresetLinkPatch(key: LinkableFontPresetKey, linked: boolean): Partial<TranslationBlock> {
  return { [FONT_PRESET_LINK_FIELD_BY_KEY[key]]: linked };
}

export function buildFontPresetRelinkPatch(key: LinkableFontPresetKey, preset: FontPresetPatch): Partial<TranslationBlock> {
  const patch: Partial<TranslationBlock> = {};
  const keys = key === "filterEffectEnabled" ? FILTER_EFFECT_PRESET_KEYS : [key];
  for (const presetKey of keys) {
    Object.assign(patch, buildFontPresetLinkPatch(presetKey, true));
    const value = preset[presetKey];
    if (value !== undefined) {
      Object.assign(patch, { [presetKey]: value });
    }
  }
  return patch;
}

export function applyAssignedFontPresetToBlock(block: TranslationBlock, preset: FontPreset): TranslationBlock {
  return {
    ...applyFontPresetPatchToBlock(block, preset, { forceLinkedValues: true }),
    fontPresetId: preset.id,
    ...buildFontPresetLinkFields(true)
  };
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

  const nextBlock: TranslationBlock = {
    ...block,
    fontFamily: normalizedPatch.fontFamily ?? block.fontFamily,
    characterFontOverrides:
      normalizedPatch.characterFontOverrides !== undefined || fullFontPresetPatch
        ? cloneCharacterFontOverrides(normalizedPatch.characterFontOverrides)
        : block.characterFontOverrides
  };
  for (const key of LINKABLE_FONT_PRESET_KEYS) {
    const value = normalizedPatch[key];
    if (value !== undefined && (forceLinkedValues || isBlockFontPresetValueLinked(block, key))) {
      Object.assign(nextBlock, { [key]: value });
    }
  }
  return nextBlock;
}
