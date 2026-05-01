import { describe, expect, it } from "vitest";
import { applyFontPresetPatchToBlock, isBlockFontPresetValueLinked, isBlockFontSizeLinkedToPreset } from "../src/client/src/lib/fontPresets";
import type { FontPreset, TranslationBlock } from "../src/shared/types";

describe("font preset block links", () => {
  it("keeps block values when those values are unlinked from the preset", () => {
    const block = createBlock({ fontSizePx: 18, fontSizeLinkedToPreset: false, lineHeight: 1.1, lineHeightLinkedToPreset: false });
    const preset = createPreset({
      fontSizePx: 32,
      lineHeight: 1.3,
      outlineColor: "#ffffff",
      outlineWidthPx: 3,
      secondaryOutlineColor: "#ff0000",
      secondaryOutlineWidthPx: 5,
      screentoneFillEnabled: true,
      screentoneFillIntensity: 0.8,
      screentoneFillDensity: 0.35,
      screentoneFillAntialias: false
    });

    expect(applyFontPresetPatchToBlock(block, preset)).toMatchObject({
      fontSizePx: 18,
      lineHeight: 1.1,
      outlineColor: "#ffffff",
      outlineWidthPx: 3,
      secondaryOutlineColor: "#ff0000",
      secondaryOutlineWidthPx: 5,
      screentoneFillEnabled: true,
      screentoneFillIntensity: 0.8,
      screentoneFillDensity: 0.35,
      screentoneFillAntialias: false
    });
  });

  it("updates font size for linked blocks and when forced", () => {
    const unlinkedBlock = createBlock({ fontSizePx: 18, fontSizeLinkedToPreset: false });
    const linkedBlock = createBlock({ fontSizePx: 18 });
    const preset = createPreset({ fontSizePx: 32 });

    expect(isBlockFontSizeLinkedToPreset(linkedBlock)).toBe(true);
    expect(isBlockFontPresetValueLinked(linkedBlock, "lineHeight")).toBe(true);
    expect(applyFontPresetPatchToBlock(linkedBlock, preset).fontSizePx).toBe(32);
    expect(applyFontPresetPatchToBlock(unlinkedBlock, preset, { forceLinkedValues: true }).fontSizePx).toBe(32);
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
    screentoneFillEnabled: false,
    screentoneFillIntensity: 0.55,
    screentoneFillDensity: 0.55,
    screentoneFillAntialias: true,
    backgroundColor: "#fffdf5",
    opacity: 1,
    ...patch
  };
}

function createPreset(patch: Partial<FontPreset> = {}): FontPreset {
  return {
    id: "preset-1",
    name: "Preset",
    fontFamily: "Preset Sans",
    fontSizePx: 24,
    lineHeight: 1.18,
    autoFitText: true,
    ...patch
  };
}
