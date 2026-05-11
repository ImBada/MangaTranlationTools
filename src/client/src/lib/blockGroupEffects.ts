import type { TranslationBlockGroup, TranslationBlockGroupEffect } from "../../../shared/types";

export const TRANSLATION_BLOCK_GROUP_DROP_SHADOW_EFFECT_TYPE = "drop-shadow";

export type TranslationBlockGroupDropShadowSettings = {
  blurPx: number;
  color: string;
  offsetX: number;
  offsetY: number;
  opacity: number;
};

export const DEFAULT_TRANSLATION_BLOCK_GROUP_DROP_SHADOW_SETTINGS: TranslationBlockGroupDropShadowSettings = {
  blurPx: 2,
  color: "#000000",
  offsetX: 4,
  offsetY: 4,
  opacity: 0.45
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
  return {
    blurPx: readFiniteNumber(settings.blurPx, DEFAULT_TRANSLATION_BLOCK_GROUP_DROP_SHADOW_SETTINGS.blurPx, 0, 80),
    color: readHexColor(settings.color, DEFAULT_TRANSLATION_BLOCK_GROUP_DROP_SHADOW_SETTINGS.color),
    offsetX: readFiniteNumber(settings.offsetX, DEFAULT_TRANSLATION_BLOCK_GROUP_DROP_SHADOW_SETTINGS.offsetX, -160, 160),
    offsetY: readFiniteNumber(settings.offsetY, DEFAULT_TRANSLATION_BLOCK_GROUP_DROP_SHADOW_SETTINGS.offsetY, -160, 160),
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
