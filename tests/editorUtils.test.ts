import { describe, expect, it } from "vitest";
import type { MangaPage, TranslationBlock } from "../src/shared/types";
import {
  createInpaintMaskUndoSnapshot,
  parseTranslationBlockFromClipboard,
  serializeTranslationBlockForClipboard
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
