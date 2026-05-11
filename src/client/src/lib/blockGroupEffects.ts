import type { TranslationBlockGroup, TranslationBlockGroupEffect } from "../../../shared/types";
import { DEFAULT_ENABLED_SHADOW_DISTANCE_PX, DEFAULT_FONT_PRESET_VALUES } from "../../../shared/fontPresets";

export const TRANSLATION_BLOCK_GROUP_DROP_SHADOW_EFFECT_TYPE = "drop-shadow";

export type TranslationBlockGroupDropShadowSettings = {
  angleDeg: number;
  blurPx: number;
  color: string;
  distancePx: number;
  opacity: number;
};

export const DEFAULT_TRANSLATION_BLOCK_GROUP_DROP_SHADOW_SETTINGS: TranslationBlockGroupDropShadowSettings = {
  angleDeg: DEFAULT_FONT_PRESET_VALUES.shadowAngleDeg ?? 45,
  blurPx: DEFAULT_FONT_PRESET_VALUES.shadowBlurPx ?? 0,
  color: DEFAULT_FONT_PRESET_VALUES.shadowColor ?? "#000000",
  distancePx: DEFAULT_ENABLED_SHADOW_DISTANCE_PX,
  opacity: DEFAULT_FONT_PRESET_VALUES.shadowOpacity ?? 1
};

export function resolveTranslationBlockGroupDropShadowEffect(
  group: Pick<TranslationBlockGroup, "effects"> | null | undefined
): TranslationBlockGroupEffect | null {
  return (group?.effects ?? []).find((effect) => effect.type === TRANSLATION_BLOCK_GROUP_DROP_SHADOW_EFFECT_TYPE) ?? null;
}

export function resolveTranslationBlockGroupDropShadowSettings(
  effect: TranslationBlockGroupEffect | null | undefined
): TranslationBlockGroupDropShadowSettings {
  const settings = effect?.settings ?? {};
  const legacyOffset = resolveLegacyDropShadowOffset(settings);
  return {
    angleDeg: readFiniteNumber(settings.angleDeg, legacyOffset?.angleDeg ?? DEFAULT_TRANSLATION_BLOCK_GROUP_DROP_SHADOW_SETTINGS.angleDeg, -360, 360),
    blurPx: readFiniteNumber(settings.blurPx, DEFAULT_TRANSLATION_BLOCK_GROUP_DROP_SHADOW_SETTINGS.blurPx, 0, 80),
    color: readHexColor(settings.color, DEFAULT_TRANSLATION_BLOCK_GROUP_DROP_SHADOW_SETTINGS.color),
    distancePx: readFiniteNumber(settings.distancePx, legacyOffset?.distancePx ?? DEFAULT_TRANSLATION_BLOCK_GROUP_DROP_SHADOW_SETTINGS.distancePx, 0, 80),
    opacity: readFiniteNumber(settings.opacity, DEFAULT_TRANSLATION_BLOCK_GROUP_DROP_SHADOW_SETTINGS.opacity, 0, 1)
  };
}

export function hasEnabledTranslationBlockGroupEffects(group: Pick<TranslationBlockGroup, "effects"> | null | undefined): boolean {
  const dropShadowEffect = resolveTranslationBlockGroupDropShadowEffect(group);
  return Boolean(dropShadowEffect?.enabled);
}

export function setTranslationBlockGroupDropShadowEnabled(
  effects: readonly TranslationBlockGroupEffect[],
  enabled: boolean
): TranslationBlockGroupEffect[] {
  return upsertTranslationBlockGroupDropShadowEffect(effects, {}, enabled);
}

export function updateTranslationBlockGroupDropShadowSettings(
  effects: readonly TranslationBlockGroupEffect[],
  patch: Partial<TranslationBlockGroupDropShadowSettings>
): TranslationBlockGroupEffect[] {
  return upsertTranslationBlockGroupDropShadowEffect(effects, patch, true);
}

function upsertTranslationBlockGroupDropShadowEffect(
  effects: readonly TranslationBlockGroupEffect[],
  patch: Partial<TranslationBlockGroupDropShadowSettings>,
  enabled: boolean
): TranslationBlockGroupEffect[] {
  const currentIndex = effects.findIndex((effect) => effect.type === TRANSLATION_BLOCK_GROUP_DROP_SHADOW_EFFECT_TYPE);
  const currentEffect = currentIndex >= 0 ? effects[currentIndex] : null;
  if (!currentEffect && !enabled) {
    return effects.map(cloneTranslationBlockGroupEffect);
  }

  const nextSettings = {
    ...resolveTranslationBlockGroupDropShadowSettings(currentEffect),
    ...patch
  };
  const normalizedNextSettings = resolveTranslationBlockGroupDropShadowSettings({
    id: currentEffect?.id ?? "",
    type: TRANSLATION_BLOCK_GROUP_DROP_SHADOW_EFFECT_TYPE,
    enabled,
    settings: nextSettings
  });
  const nextEffect: TranslationBlockGroupEffect = {
    id: currentEffect?.id ?? createTranslationBlockGroupEffectId(),
    type: TRANSLATION_BLOCK_GROUP_DROP_SHADOW_EFFECT_TYPE,
    enabled,
    settings: normalizedNextSettings
  };

  if (currentIndex < 0) {
    return [...effects.map(cloneTranslationBlockGroupEffect), nextEffect];
  }

  return effects.map((effect, index) => (
    index === currentIndex ? nextEffect : cloneTranslationBlockGroupEffect(effect)
  ));
}

export function cloneTranslationBlockGroupEffect(effect: TranslationBlockGroupEffect): TranslationBlockGroupEffect {
  return {
    ...effect,
    settings: effect.settings ? { ...effect.settings } : undefined
  };
}

function createTranslationBlockGroupEffectId(): string {
  const randomUUID = globalThis.crypto?.randomUUID?.bind(globalThis.crypto);
  return `text-block-group-effect-${randomUUID ? randomUUID() : `${Date.now()}-${Math.random().toString(36).slice(2, 10)}`}`;
}

function readFiniteNumber(value: unknown, fallback: number, min: number, max: number): number {
  const numericValue = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(numericValue)) {
    return fallback;
  }
  return Math.min(max, Math.max(min, numericValue));
}

function readHexColor(value: unknown, fallback: string): string {
  return typeof value === "string" && /^#[0-9a-f]{6}$/i.test(value) ? value : fallback;
}

function resolveLegacyDropShadowOffset(settings: Record<string, unknown>): Pick<TranslationBlockGroupDropShadowSettings, "angleDeg" | "distancePx"> | null {
  const offsetX = readOptionalFiniteNumber(settings.offsetX);
  const offsetY = readOptionalFiniteNumber(settings.offsetY);
  if (offsetX === null && offsetY === null) {
    return null;
  }

  const dx = offsetX ?? 0;
  const dy = offsetY ?? 0;
  return {
    angleDeg: (Math.atan2(dy, dx) * 180) / Math.PI,
    distancePx: Math.hypot(dx, dy)
  };
}

function readOptionalFiniteNumber(value: unknown): number | null {
  const numericValue = typeof value === "number" ? value : Number(value);
  return Number.isFinite(numericValue) ? numericValue : null;
}
