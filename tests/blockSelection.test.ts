import { describe, expect, it } from "vitest";
import type { MangaPage, TranslationBlock } from "../src/shared/types";
import {
  resolveCurrentTranslationBlockSelection,
  resolveSelectedTranslationBlocks,
  resolveShiftSelectedTranslationBlockIds,
  resolveToggledTranslationBlockIds,
  resolveTranslationBlockRangeSelection,
  resolveTranslationBlockIdsInSelection
} from "../src/client/src/lib/blockSelection";

const baseBlock: TranslationBlock = {
  id: "block-1",
  type: "speech",
  bbox: { x: 100, y: 100, w: 120, h: 120 },
  sourceText: "",
  translatedText: "",
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

const page: MangaPage = {
  id: "page-1",
  name: "page.png",
  imagePath: "/tmp/page.png",
  dataUrl: "/api/source",
  width: 1000,
  height: 1000,
  blocks: [
    baseBlock,
    {
      ...baseBlock,
      id: "block-2",
      bbox: { x: 320, y: 120, w: 100, h: 100 }
    },
    {
      ...baseBlock,
      id: "hidden",
      bbox: { x: 130, y: 130, w: 80, h: 80 },
      renderDirection: "hidden"
    },
    {
      ...baseBlock,
      id: "render-box",
      bbox: { x: 700, y: 700, w: 80, h: 80 },
      renderBbox: { x: 450, y: 450, w: 120, h: 120 }
    }
  ],
  analysisStatus: "completed",
  createdAt: "2026-01-01T00:00:00.000Z",
  updatedAt: "2026-01-01T00:00:00.000Z"
};

describe("block selection", () => {
  it("selects visible translation blocks intersecting the drag rectangle", () => {
    expect(resolveTranslationBlockIdsInSelection(page, { x: 150, y: 150, width: 240, height: 120 })).toEqual(["block-1", "block-2"]);
  });

  it("uses render boxes and ignores hidden blocks", () => {
    expect(resolveTranslationBlockIdsInSelection(page, { x: 440, y: 440, width: 80, height: 80 })).toEqual(["render-box"]);
  });

  it("resolves selected blocks in requested order and drops stale ids", () => {
    expect(resolveSelectedTranslationBlocks(page, ["block-2", "missing", "block-1"]).map((block) => block.id)).toEqual(["block-2", "block-1"]);
  });

  it("toggles block selection with shift-click while preserving selection order", () => {
    expect(resolveShiftSelectedTranslationBlockIds("block-1", [], "block-2")).toEqual(["block-1", "block-2"]);
    expect(resolveShiftSelectedTranslationBlockIds("block-1", ["block-1", "block-2"], "render-box")).toEqual([
      "block-1",
      "block-2",
      "render-box"
    ]);
    expect(resolveShiftSelectedTranslationBlockIds("block-1", ["block-1", "block-2"], "block-2")).toEqual(["block-1"]);
    expect(resolveShiftSelectedTranslationBlockIds("block-1", [], "block-1")).toEqual([]);
  });

  it("toggles block selection with drag range results", () => {
    expect(resolveToggledTranslationBlockIds("block-1", [], ["block-1", "block-2"])).toEqual(["block-2"]);
    expect(resolveToggledTranslationBlockIds("block-1", ["block-1", "block-2"], ["block-2", "render-box"])).toEqual([
      "block-1",
      "render-box"
    ]);
    expect(resolveToggledTranslationBlockIds("block-1", ["block-1", "block-2"], ["block-1", "block-2"])).toEqual([]);
    expect(resolveToggledTranslationBlockIds(null, [], ["block-2", "render-box"])).toEqual([
      "block-2",
      "render-box"
    ]);
  });

  it("resolves the current selection from single and grouped block state", () => {
    expect(resolveCurrentTranslationBlockSelection("block-1", [])).toEqual(["block-1"]);
    expect(resolveCurrentTranslationBlockSelection("block-1", ["block-1", "block-2"])).toEqual([
      "block-1",
      "block-2"
    ]);
    expect(resolveCurrentTranslationBlockSelection(null, [])).toEqual([]);
  });

  it("resolves contiguous block ranges in display order", () => {
    const blockIdsInOrder = page.blocks.map((block) => block.id);

    expect(resolveTranslationBlockRangeSelection(blockIdsInOrder, "block-1", "hidden")).toEqual([
      "block-1",
      "block-2",
      "hidden"
    ]);
    expect(resolveTranslationBlockRangeSelection(blockIdsInOrder, "render-box", "block-2")).toEqual([
      "block-2",
      "hidden",
      "render-box"
    ]);
    expect(resolveTranslationBlockRangeSelection(blockIdsInOrder, null, "block-2")).toEqual(["block-2"]);
    expect(resolveTranslationBlockRangeSelection(blockIdsInOrder, "block-1", "missing")).toEqual([]);
  });
});
