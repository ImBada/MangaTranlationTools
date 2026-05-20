import { describe, expect, it } from "vitest";
import { applyBlockTypeFontPresetToBlock, BLOCK_TYPE_FONT_PRESET_IDS, ensureBlockTypeFontPresets } from "../src/shared/fontPresets";
import type { FontPreset, TranslationBlock } from "../src/shared/types";

describe("block type font presets", () => {
  it("creates one default preset for every block type", () => {
    const presets = ensureBlockTypeFontPresets();

    expect(presets.map((preset) => preset.name)).toEqual(["speech", "sfx", "caption", "other"]);
    expect(presets.every((preset) => preset.fontSizePx === 24)).toBe(true);
    expect(presets.every((preset) => preset.lineHeight === 1.18)).toBe(true);
  });

  it("reuses existing block type presets by stable id even when renamed", () => {
    const existing: FontPreset = {
      id: BLOCK_TYPE_FONT_PRESET_IDS.speech,
      name: "대사",
      fontSizePx: 36,
      lineHeight: 1.4,
      textColor: "#ff0000"
    };

    const presets = ensureBlockTypeFontPresets([existing]);

    expect(presets.filter((preset) => preset.id === BLOCK_TYPE_FONT_PRESET_IDS.speech)).toEqual([existing]);
    expect(presets).toHaveLength(4);
  });

  it("applies the matching preset id to a translated block with linked values", () => {
    const preset: FontPreset = {
      id: BLOCK_TYPE_FONT_PRESET_IDS.sfx,
      name: "효과음",
      fontFamily: "Preset Sans",
      fontSizePx: 40,
      lineHeight: 1.3,
      outlineColor: "#ffffff",
      outlineWidthPx: 2,
      shadowEnabled: true,
      shadowColor: "#334455",
      shadowOpacity: 0.7,
      shadowBlurPx: 2,
      shadowAngleDeg: 30,
      shadowDistancePx: 4,
      textColor: "#222222",
      autoFitText: true,
      filterEffectEnabled: true,
      filterZigzagEnabled: true,
      filterZigzagIntensity: 0.65,
      filterWaveEnabled: true,
      filterWaveIntensity: 0.55,
      filterGlitchEnabled: true,
      filterGlitchIntensity: 0.45,
      filterPixelateEnabled: true,
      filterPixelateIntensity: 0.35,
      filterBlurEnabled: true,
      filterBlurIntensity: 0.25,
      filterNoiseEnabled: true,
      filterNoiseIntensity: 0.15
    };
    const block = createBlock({ type: "sfx", fontSizePx: 18, lineHeight: 1.1 });

    expect(applyBlockTypeFontPresetToBlock(block, [preset])).toMatchObject({
      fontPresetId: BLOCK_TYPE_FONT_PRESET_IDS.sfx,
      fontSizeLinkedToPreset: true,
      lineHeightLinkedToPreset: true,
      textColorLinkedToPreset: true,
      fontFamily: "Preset Sans",
      fontSizePx: 40,
      lineHeight: 1.3,
      outlineColor: "#ffffff",
      outlineWidthPx: 2,
      shadowEnabled: true,
      shadowColor: "#334455",
      shadowOpacity: 0.7,
      shadowBlurPx: 2,
      shadowAngleDeg: 30,
      shadowDistancePx: 4,
      textColor: "#222222",
      autoFitText: true,
      filterEffectEnabled: true,
      filterZigzagEnabled: true,
      filterZigzagIntensity: 0.65,
      filterWaveEnabled: true,
      filterWaveIntensity: 0.55,
      filterGlitchEnabled: true,
      filterGlitchIntensity: 0.45,
      filterPixelateEnabled: true,
      filterPixelateIntensity: 0.35,
      filterBlurEnabled: true,
      filterBlurIntensity: 0.25,
      filterNoiseEnabled: true,
      filterNoiseIntensity: 0.15
    });
  });

  it("normalizes legacy block type presets when applying them to translated blocks", () => {
    const preset: FontPreset = {
      id: BLOCK_TYPE_FONT_PRESET_IDS.speech,
      name: "대사",
      fontSizePx: 36,
      lineHeight: 1.4
    };
    const block = createBlock({
      shadowEnabled: true,
      shadowColor: "#334455",
      shadowOpacity: 0.7,
      shadowBlurPx: 2,
      shadowAngleDeg: 30,
      shadowDistancePx: 4
    });

    expect(applyBlockTypeFontPresetToBlock(block, [preset])).toMatchObject({
      fontPresetId: BLOCK_TYPE_FONT_PRESET_IDS.speech,
      fontSizePx: 36,
      lineHeight: 1.4,
      shadowEnabled: false,
      shadowColor: "#000000",
      shadowOpacity: 1,
      shadowBlurPx: 0,
      shadowAngleDeg: 45,
      shadowDistancePx: 0,
      filterEffectEnabled: false,
      filterZigzagEnabled: false,
      filterZigzagIntensity: 0.35,
      filterWaveEnabled: false,
      filterWaveIntensity: 0.35,
      filterGlitchEnabled: false,
      filterGlitchIntensity: 0.35,
      filterPixelateEnabled: false,
      filterPixelateIntensity: 0.35,
      filterBlurEnabled: false,
      filterBlurIntensity: 0.35,
      filterNoiseEnabled: false,
      filterNoiseIntensity: 0.35
    });
  });
});

function createBlock(patch: Partial<TranslationBlock> = {}): TranslationBlock {
  return {
    id: "block-1",
    type: "speech",
    bbox: { x: 0, y: 0, w: 100, h: 100 },
    sourceText: "",
    translatedText: "",
    confidence: 1,
    sourceDirection: "vertical",
    renderDirection: "horizontal",
    fontSizePx: 24,
    lineHeight: 1.18,
    textAlign: "center",
    textColor: "#111111",
    backgroundColor: "#fffdf5",
    opacity: 1,
    ...patch
  };
}
