import { describe, expect, it } from "vitest";
import type { TranslationBlock } from "../src/shared/types";
import {
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
});
