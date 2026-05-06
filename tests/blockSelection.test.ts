import { describe, expect, it } from "vitest";
import type { MangaPage, TranslationBlock } from "../src/shared/types";
import { resolveSelectedTranslationBlocks, resolveTranslationBlockIdsInSelection } from "../src/client/src/lib/blockSelection";

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
});
