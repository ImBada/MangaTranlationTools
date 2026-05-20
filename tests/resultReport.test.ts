import { describe, expect, it } from "vitest";
import type { MangaPage, TranslationBlock } from "../src/shared/types";
import { buildResultReportHtml, createResultReportRow, resolveBlockReportRects } from "../src/client/src/lib/resultReport";

describe("result report helpers", () => {
  it("uses source bbox for original crop and render bbox for output crop", () => {
    const page = createPage({
      width: 2000,
      height: 3000,
      blocks: [
        createBlock({
          bbox: { x: 100, y: 200, w: 300, h: 400 },
          renderBbox: { x: 150, y: 250, w: 350, h: 450 }
        })
      ]
    });

    expect(resolveBlockReportRects(page, page.blocks[0])).toEqual({
      sourceRect: { x: 200, y: 600, width: 600, height: 1200 },
      outputRect: { x: 300, y: 750, width: 700, height: 1350 }
    });
  });

  it("builds sortable filterable escaped report markup", () => {
    const page = createPage({ name: "<page>" });
    const block = createBlock({ sourceText: "<원문>", translatedText: "\"번역\"" });
    const { sourceRect, outputRect } = resolveBlockReportRects(page, block);
    const row = createResultReportRow({
      block,
      blockIndex: 1,
      fontPreset: { id: "preset-1", name: "기본", fontSizePx: 24, lineHeight: 1.2 },
      outputCropDataUrl: "data:image/png;base64,b3V0",
      outputRect,
      page,
      pageIndex: 1,
      rowIndex: 1,
      sourceCropDataUrl: "data:image/png;base64,c3Jj",
      sourceRect
    });

    const html = buildResultReportHtml({
      chapterTitle: "테스트 <화>",
      generatedAt: "2026. 5. 11.",
      rows: [row]
    });

    expect(html).toContain("테스트 &lt;화&gt; 결과 보고서");
    expect(html).toContain("data-global-filter");
    expect(html).toContain("data-filter-count");
    expect(html).toContain("data-sort-key=\"fontPresetName\"");
    expect(html).toContain("data-filter-key=\"fontPresetName\"");
    expect(html).toContain("data-focus-block");
    expect(html).toContain("manga-result-report:focus-block");
    expect(html).toContain("전체 컬럼 검색");
    expect(html).toContain("&lt;원문&gt;");
    expect(html).toContain("&quot;번역&quot;");
  });

  it("formats filter effect state with normalized active values", () => {
    const page = createPage();
    const block = createBlock({
      filterEffectEnabled: true,
      filterZigzagEnabled: true,
      filterZigzagIntensity: 1.5
    });
    const { sourceRect, outputRect } = resolveBlockReportRects(page, block);

    expect(createResultReportRow({
      block,
      blockIndex: 1,
      fontPreset: undefined,
      outputCropDataUrl: "",
      outputRect,
      page,
      pageIndex: 1,
      rowIndex: 1,
      sourceCropDataUrl: "",
      sourceRect
    }).filterEffect).toBe("지그재그 강도 100%");

    expect(createResultReportRow({
      block: { ...block, filterZigzagIntensity: 0 },
      blockIndex: 1,
      fontPreset: undefined,
      outputCropDataUrl: "",
      outputRect,
      page,
      pageIndex: 1,
      rowIndex: 1,
      sourceCropDataUrl: "",
      sourceRect
    }).filterEffect).toBe("켜짐 / 활성 필터 없음");

    expect(createResultReportRow({
      block: {
        ...block,
        filterWaveEnabled: true,
        filterWaveIntensity: 0.25,
        filterNoiseEnabled: true,
        filterNoiseIntensity: 0.5
      },
      blockIndex: 1,
      fontPreset: undefined,
      outputCropDataUrl: "",
      outputRect,
      page,
      pageIndex: 1,
      rowIndex: 1,
      sourceCropDataUrl: "",
      sourceRect
    }).filterEffect).toBe("지그재그 강도 100%, 물결 강도 25%, 노이즈 강도 50%");
  });
});

function createPage(patch: Partial<MangaPage> = {}): MangaPage {
  return {
    id: "page-1",
    name: "page 1",
    imagePath: "/tmp/page.png",
    dataUrl: "data:image/png;base64,",
    width: 1000,
    height: 1000,
    blocks: [createBlock()],
    analysisStatus: "completed",
    createdAt: "2026-05-11T00:00:00.000Z",
    updatedAt: "2026-05-11T00:00:00.000Z",
    ...patch
  };
}

function createBlock(patch: Partial<TranslationBlock> = {}): TranslationBlock {
  return {
    id: "block-1",
    type: "speech",
    bbox: { x: 100, y: 100, w: 200, h: 200 },
    sourceText: "",
    translatedText: "",
    confidence: 1,
    sourceDirection: "vertical",
    renderDirection: "horizontal",
    fontPresetId: "preset-1",
    fontSizePx: 24,
    lineHeight: 1.2,
    textAlign: "center",
    textColor: "#111111",
    backgroundColor: "#ffffff",
    opacity: 0.8,
    ...patch
  };
}
