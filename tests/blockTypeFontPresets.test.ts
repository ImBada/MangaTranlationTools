import { describe, expect, it } from "vitest";
import { applyBlockTypeFontPresetToBlock, ensureBlockTypeFontPresets } from "../src/shared/fontPresets";
import type { FontPreset, TranslationBlock } from "../src/shared/types";

describe("block type font presets", () => {
  it("creates one default preset for every block type", () => {
    const presets = ensureBlockTypeFontPresets();

    expect(presets.map((preset) => preset.name)).toEqual(["speech", "sfx", "caption", "other"]);
    expect(presets.every((preset) => preset.fontSizePx === 24)).toBe(true);
    expect(presets.every((preset) => preset.lineHeight === 1.18)).toBe(true);
  });

  it("reuses existing presets with matching block type names", () => {
    const existing: FontPreset = {
      id: "custom-speech",
      name: "speech",
      fontSizePx: 36,
      lineHeight: 1.4,
      textColor: "#ff0000"
    };

    const presets = ensureBlockTypeFontPresets([existing]);

    expect(presets.filter((preset) => preset.name === "speech")).toEqual([existing]);
  });

  it("applies the matching preset to a translated block with linked values", () => {
    const preset: FontPreset = {
      id: "custom-sfx",
      name: "sfx",
      fontFamily: "Preset Sans",
      fontSizePx: 40,
      lineHeight: 1.3,
      outlineColor: "#ffffff",
      outlineWidthPx: 2,
      textColor: "#222222",
      autoFitText: true
    };
    const block = createBlock({ type: "sfx", fontSizePx: 18, lineHeight: 1.1 });

    expect(applyBlockTypeFontPresetToBlock(block, [preset])).toMatchObject({
      fontPresetId: "custom-sfx",
      fontSizeLinkedToPreset: true,
      lineHeightLinkedToPreset: true,
      textColorLinkedToPreset: true,
      fontFamily: "Preset Sans",
      fontSizePx: 40,
      lineHeight: 1.3,
      outlineColor: "#ffffff",
      outlineWidthPx: 2,
      textColor: "#222222",
      autoFitText: true
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
