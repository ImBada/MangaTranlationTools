import { describe, expect, it } from "vitest";
import type { MangaPage, TranslationBlock } from "../src/shared/types";
import { collectFindReplaceMatches, countTextOccurrences, replaceTextLiteral } from "../src/client/src/lib/findReplace";

const baseBlock: TranslationBlock = {
  id: "block-1",
  type: "speech",
  bbox: { x: 100, y: 120, w: 200, h: 180 },
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
  opacity: 1
};

const basePage: MangaPage = {
  id: "page-1",
  name: "001.png",
  imagePath: "C:/page-1.png",
  dataUrl: "data:image/png;base64,aaa",
  width: 1200,
  height: 1800,
  blocks: [],
  analysisStatus: "completed",
  createdAt: "2026-04-19T00:00:00.000Z",
  updatedAt: "2026-04-19T00:00:00.000Z"
};

describe("find replace helpers", () => {
  it("replaces literal keywords without treating them as regex", () => {
    expect(replaceTextLiteral("a.b a.b", "a.b", "x")).toBe("x x");
  });

  it("counts non-overlapping literal occurrences", () => {
    expect(countTextOccurrences("아아아", "아")).toBe(3);
    expect(countTextOccurrences("aaaa", "aa")).toBe(2);
  });

  it("collects full translated sentences with before and after previews", () => {
    const pages: MangaPage[] = [
      {
        ...basePage,
        blocks: [
          { ...baseBlock, id: "block-1", translatedText: "나는 마왕이다.\n마왕은 강하다." },
          { ...baseBlock, id: "block-2", translatedText: "용사는 강하다." }
        ]
      }
    ];

    expect(collectFindReplaceMatches(pages, "마왕", "왕").map((match) => ({
      id: match.id,
      before: match.before,
      after: match.after,
      occurrenceCount: match.occurrenceCount
    }))).toEqual([
      {
        id: "page-1:block-1",
        before: "나는 마왕이다.\n마왕은 강하다.",
        after: "나는 왕이다.\n왕은 강하다.",
        occurrenceCount: 2
      }
    ]);
  });

  it("returns no matches for an empty keyword", () => {
    expect(collectFindReplaceMatches([{ ...basePage, blocks: [baseBlock] }], "", "x")).toEqual([]);
  });
});
