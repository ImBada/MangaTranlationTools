import { describe, expect, it } from "vitest";
import type { MangaPage, TranslationBlock } from "../src/shared/types";
import {
  applyTranslationBlockFontStyle,
  bringTranslationBlockToFront,
  bringTranslationBlocksToFront,
  createInpaintMaskUndoSnapshot,
  extractTranslationBlockFontStyle,
  parseTranslationBlockFontStyleFromClipboard,
  parseTranslationBlockFromClipboard,
  serializeTranslationBlockFontStyleForClipboard,
  serializeTranslationBlockForClipboard,
  splitTextBySelection
} from "../src/client/src/lib/editorUtils";

const block: TranslationBlock = {
  id: "block-1",
  type: "speech",
  bbox: { x: 100, y: 120, w: 200, h: 180 },
  renderBbox: { x: 90, y: 110, w: 220, h: 200 },
  sourceText: "こんにちは",
  translatedText: "안녕",
  confidence: 1,
  sourceDirection: "vertical",
  renderDirection: "horizontal",
  fontSizePx: 24,
  lineHeight: 1.2,
  textAlign: "center",
  textColor: "#111111",
  backgroundColor: "#fffdf5",
  opacity: 0.88
};

describe("editor utils", () => {
  it("round-trips translation blocks through clipboard text", () => {
    const serialized = serializeTranslationBlockForClipboard(block);
    expect(parseTranslationBlockFromClipboard(serialized)).toEqual(block);
  });

  it("rejects unrelated clipboard text", () => {
    expect(parseTranslationBlockFromClipboard("plain text")).toBeNull();
    expect(parseTranslationBlockFromClipboard(JSON.stringify({ kind: "other", block }))).toBeNull();
  });

  it("splits selected text into remaining and selected parts", () => {
    expect(splitTextBySelection("첫 줄\n둘째 줄\n셋째 줄", 4, 8)).toEqual({
      selectedText: "둘째 줄",
      remainingText: "첫 줄\n\n셋째 줄"
    });
  });

  it("normalizes reversed and out-of-range text selections", () => {
    expect(splitTextBySelection("abcdef", 99, 2)).toEqual({
      selectedText: "cdef",
      remainingText: "ab"
    });
    expect(splitTextBySelection("abcdef", 3, 3)).toBeNull();
  });

  it("round-trips font style clipboard values including cleared optional fields", () => {
    const styledBlock: TranslationBlock = {
      ...block,
      fontPresetId: "preset-1",
      fontSizeLinkedToPreset: false,
      fontWeightLinkedToPreset: true,
      lineHeightLinkedToPreset: undefined,
      fontFamily: "Arial",
      fontWeight: 800,
      fontStyle: "italic",
      textDecoration: "underline",
      fontSizePx: 36,
      lineHeight: 1.35,
      letterSpacingPx: undefined,
      outlineColor: "#ffffff",
      outlineWidthPx: 2,
      secondaryOutlineColor: "#223344",
      secondaryOutlineWidthPx: 3,
      shadowEnabled: true,
      shadowColor: "#112233",
      shadowOpacity: 0.6,
      shadowBlurPx: 4,
      shadowAngleDeg: 120,
      shadowDistancePx: 6,
      textAlign: "right",
      textPosition: "bottom-right",
      textColor: "#fefefe",
      screentoneFillEnabled: true,
      screentoneFillIntensity: 0.8,
      screentoneFillDensity: 0.35,
      screentoneFillAntialias: false
    };

    const parsed = parseTranslationBlockFontStyleFromClipboard(
      serializeTranslationBlockFontStyleForClipboard(extractTranslationBlockFontStyle(styledBlock))
    );

    expect(parsed).toMatchObject({
      fontPresetId: "preset-1",
      fontSizeLinkedToPreset: false,
      fontWeightLinkedToPreset: true,
      fontFamily: "Arial",
      fontWeight: 800,
      fontStyle: "italic",
      textDecoration: "underline",
      fontSizePx: 36,
      lineHeight: 1.35,
      outlineWidthPx: 2,
      shadowEnabled: true,
      shadowOpacity: 0.6,
      shadowBlurPx: 4,
      textAlign: "right",
      textPosition: "bottom-right",
      screentoneFillAntialias: false
    });
    expect(parsed).not.toBeNull();
    expect(Object.prototype.hasOwnProperty.call(parsed, "letterSpacingPx")).toBe(true);
    expect(parsed?.letterSpacingPx).toBeUndefined();
    expect(Object.prototype.hasOwnProperty.call(parsed, "lineHeightLinkedToPreset")).toBe(true);
    expect(parsed?.lineHeightLinkedToPreset).toBeUndefined();
  });

  it("applies copied font style without replacing block content or geometry", () => {
    const target: TranslationBlock = {
      ...block,
      id: "target",
      translatedText: "keep me",
      fontPresetId: "old-preset",
      fontSizeLinkedToPreset: true,
      fontSizePx: 12,
      lineHeight: 1,
      textAlign: "left"
    };
    const style = extractTranslationBlockFontStyle({
      ...block,
      fontSizePx: 42,
      lineHeight: 1.4,
      textAlign: "right",
      textColor: "#ffffff"
    });

    const applied = applyTranslationBlockFontStyle(target, style);

    expect(applied).toMatchObject({
      id: "target",
      bbox: target.bbox,
      translatedText: "keep me",
      fontSizePx: 42,
      lineHeight: 1.4,
      textAlign: "right",
      textColor: "#ffffff"
    });
    expect(applied.fontPresetId).toBeUndefined();
    expect(applied.fontSizeLinkedToPreset).toBeUndefined();
  });

  it("copies font preset links with font style", () => {
    const target: TranslationBlock = {
      ...block,
      id: "target",
      fontPresetId: "old-preset",
      fontSizeLinkedToPreset: false,
      fontWeightLinkedToPreset: false,
      fontSizePx: 12,
      fontWeight: 400
    };
    const style = extractTranslationBlockFontStyle({
      ...block,
      fontPresetId: "source-preset",
      fontSizeLinkedToPreset: true,
      fontWeightLinkedToPreset: undefined,
      fontSizePx: 40,
      fontWeight: 900
    });

    const applied = applyTranslationBlockFontStyle(target, style);

    expect(applied).toMatchObject({
      id: "target",
      fontPresetId: "source-preset",
      fontSizeLinkedToPreset: true,
      fontSizePx: 40,
      fontWeight: 900
    });
    expect(Object.prototype.hasOwnProperty.call(applied, "fontWeightLinkedToPreset")).toBe(true);
    expect(applied.fontWeightLinkedToPreset).toBeUndefined();
  });

  it("moves a translation block to the front render order", () => {
    const blocks = [
      { ...block, id: "back" },
      { ...block, id: "middle" },
      { ...block, id: "front" }
    ];

    expect(bringTranslationBlockToFront(blocks, "middle").map((candidate) => candidate.id)).toEqual(["back", "front", "middle"]);
  });

  it("keeps block order identity when the requested block is already front", () => {
    const blocks = [
      { ...block, id: "back" },
      { ...block, id: "front" }
    ];

    expect(bringTranslationBlockToFront(blocks, "front")).toBe(blocks);
  });

  it("moves multiple translation blocks to the front while preserving their relative order", () => {
    const blocks = [
      { ...block, id: "back" },
      { ...block, id: "group-a" },
      { ...block, id: "middle" },
      { ...block, id: "group-b" },
      { ...block, id: "front" }
    ];

    expect(bringTranslationBlocksToFront(blocks, ["group-b", "group-a"]).map((candidate) => candidate.id)).toEqual([
      "back",
      "middle",
      "front",
      "group-a",
      "group-b"
    ]);
  });

  it("allows inpaint undo snapshots to override captured layer pixels", () => {
    const page: MangaPage = {
      id: "page-1",
      name: "page.png",
      imagePath: "/tmp/page.png",
      dataUrl: "/api/source",
      width: 100,
      height: 200,
      blocks: [],
      analysisStatus: "completed",
      createdAt: "2026-01-01T00:00:00.000Z",
      updatedAt: "2026-01-01T00:00:00.000Z",
      inpaintMaskPath: "/tmp/mask.png",
      inpaintMaskDataUrl: "/api/mask"
    };

    expect(createInpaintMaskUndoSnapshot(page, { inpaintMaskDataUrl: "data:image/png;base64,old" })).toMatchObject({
      inpaintMaskPath: "/tmp/mask.png",
      inpaintMaskDataUrl: "data:image/png;base64,old"
    });
  });
});
