import { describe, expect, it } from "vitest";
import {
  DEFAULT_TRANSLATION_BLOCK_GROUP_DROP_SHADOW_SETTINGS,
  TRANSLATION_BLOCK_GROUP_DROP_SHADOW_EFFECT_TYPE,
  hasEnabledTranslationBlockGroupEffects,
  resolveTranslationBlockGroupDropShadowEffect,
  resolveTranslationBlockGroupDropShadowSettings,
  setTranslationBlockGroupDropShadowEnabled,
  updateTranslationBlockGroupDropShadowSettings
} from "./blockGroupEffects";

describe("blockGroupEffects", () => {
  it("creates and updates a group drop shadow effect without dropping other effects", () => {
    const effects = updateTranslationBlockGroupDropShadowSettings(
      [{ id: "effect-other", type: "sample", enabled: true, settings: { strength: 1 } }],
      { color: "#123abc", distancePx: 12 }
    );

    expect(effects[0]).toEqual({ id: "effect-other", type: "sample", enabled: true, settings: { strength: 1 } });
    expect(effects[1]).toMatchObject({
      type: TRANSLATION_BLOCK_GROUP_DROP_SHADOW_EFFECT_TYPE,
      enabled: true,
      settings: {
        ...DEFAULT_TRANSLATION_BLOCK_GROUP_DROP_SHADOW_SETTINGS,
        color: "#123abc",
        distancePx: 12
      }
    });

    const disabledEffects = setTranslationBlockGroupDropShadowEnabled(effects, false);
    expect(resolveTranslationBlockGroupDropShadowEffect({ effects: disabledEffects })?.enabled).toBe(false);
    expect(setTranslationBlockGroupDropShadowEnabled([], false)).toEqual([]);
  });

  it("normalizes invalid drop shadow settings", () => {
    expect(resolveTranslationBlockGroupDropShadowSettings({
      id: "effect-1",
      type: TRANSLATION_BLOCK_GROUP_DROP_SHADOW_EFFECT_TYPE,
      enabled: true,
      settings: {
        blurPx: -5,
        color: "black",
        angleDeg: 999,
        distancePx: 999,
        opacity: 2
      }
    })).toEqual({
      ...DEFAULT_TRANSLATION_BLOCK_GROUP_DROP_SHADOW_SETTINGS,
      angleDeg: 360,
      blurPx: 0,
      distancePx: 80,
      opacity: 1
    });
  });

  it("reads legacy offset shadow settings as angle and distance", () => {
    const settings = resolveTranslationBlockGroupDropShadowSettings({
      id: "effect-1",
      type: TRANSLATION_BLOCK_GROUP_DROP_SHADOW_EFFECT_TYPE,
      enabled: true,
      settings: {
        offsetX: 3,
        offsetY: 4
      }
    });

    expect(settings.angleDeg).toBeCloseTo(53.1301, 4);
    expect(settings.distancePx).toBe(5);
  });

  it("treats missing legacy effect arrays as disabled", () => {
    expect(resolveTranslationBlockGroupDropShadowEffect({} as { effects: never[] })).toBeNull();
    expect(hasEnabledTranslationBlockGroupEffects({} as { effects: never[] })).toBe(false);
  });
});
