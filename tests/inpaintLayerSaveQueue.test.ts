import { describe, expect, it } from "vitest";
import type { ChapterSnapshot, MangaPage } from "../src/shared/types";
import {
  coalescePendingInpaintLayerSaves,
  preservePendingInpaintSaveDataUrls,
  requeueFailedInpaintLayerSave,
  type PendingInpaintLayerSave
} from "../src/client/src/hooks/useInpaintLayerSaveQueue";

type PendingMaskSave = Extract<PendingInpaintLayerSave, { kind: "mask" }>;
type PendingResultSave = Extract<PendingInpaintLayerSave, { kind: "result" }>;
type PendingLayersSave = Extract<PendingInpaintLayerSave, { kind: "layers" }>;

describe("inpaint layer save queue helpers", () => {
  it("keeps failed saves ahead of unrelated queued saves", () => {
    const failed = maskSave({ pageId: "page-a" });
    const queued = resultSave({ pageId: "page-b" });

    expect(requeueFailedInpaintLayerSave(failed, [queued])).toEqual([failed, queued]);
  });

  it("drops a failed save when a newer same-kind save supersedes it", () => {
    const failed = maskSave({ dataUrl: "old" });
    const queued = maskSave({ dataUrl: "new" });

    expect(requeueFailedInpaintLayerSave(failed, [queued])).toEqual([queued]);
  });

  it("keeps failed full-layer saves when only a partial save is queued", () => {
    const failed = layersSave({ maskDataUrl: "mask-a", resultDataUrl: "result-a" });
    const queued = resultSave({ dataUrl: "result-b" });

    expect(requeueFailedInpaintLayerSave(failed, [queued])).toEqual([failed, queued]);
  });

  it("lets full-layer saves supersede older queued saves for the same page", () => {
    const queued = [
      maskSave({ dataUrl: "mask-a" }),
      resultSave({ dataUrl: "result-a" }),
      resultSave({ pageId: "page-b", dataUrl: "result-b" })
    ];
    const pending = layersSave({ maskDataUrl: "mask-next", resultDataUrl: "result-next" });

    expect(coalescePendingInpaintLayerSaves(queued, pending)).toEqual([
      resultSave({ pageId: "page-b", dataUrl: "result-b" }),
      pending
    ]);
  });

  it("keeps local result data URLs when merging saved result responses", () => {
    const current = chapterSnapshot({
      pages: [
        mangaPage({
          inpaintMaskDataUrl: "data:image/png;base64,current-mask",
          inpaintMaskPath: "mask.png",
          inpaintResultDataUrl: "data:image/png;base64,current-result",
          inpaintResultPath: "result.png"
        })
      ]
    });
    const saved = chapterSnapshot({
      pages: [
        mangaPage({
          inpaintMaskDataUrl: "/api/pages/page-a/inpaint-mask",
          inpaintMaskPath: "mask.png",
          inpaintResultDataUrl: "/api/pages/page-a/inpaint-result",
          inpaintResultPath: "result.png"
        })
      ]
    });

    const merged = preservePendingInpaintSaveDataUrls(
      saved,
      resultSave({ dataUrl: "data:image/png;base64,pending-result" }),
      current
    );

    expect(merged.pages[0]).toMatchObject({
      inpaintMaskDataUrl: "data:image/png;base64,current-mask",
      inpaintMaskPath: "mask.png",
      inpaintResultDataUrl: "data:image/png;base64,pending-result",
      inpaintResultPath: "result.png"
    });
  });

  it("falls back to saved mask URLs when no current chapter is available", () => {
    const saved = chapterSnapshot({
      pages: [
        mangaPage({
          inpaintMaskDataUrl: "/api/pages/page-a/inpaint-mask",
          inpaintMaskPath: "mask.png",
          inpaintResultDataUrl: "/api/pages/page-a/inpaint-result",
          inpaintResultPath: "result.png"
        })
      ]
    });

    const merged = preservePendingInpaintSaveDataUrls(
      saved,
      resultSave({ dataUrl: "data:image/png;base64,pending-result" }),
      null
    );

    expect(merged.pages[0]).toMatchObject({
      inpaintMaskDataUrl: "/api/pages/page-a/inpaint-mask",
      inpaintResultDataUrl: "data:image/png;base64,pending-result"
    });
  });

  it("keeps local mask and result data URLs when merging saved mask responses", () => {
    const current = chapterSnapshot({
      pages: [
        mangaPage({
          inpaintMaskDataUrl: "data:image/png;base64,current-mask",
          inpaintMaskPath: "mask.png",
          inpaintResultDataUrl: "data:image/png;base64,current-result",
          inpaintResultPath: "result.png"
        })
      ]
    });
    const saved = chapterSnapshot({
      pages: [
        mangaPage({
          inpaintMaskDataUrl: "/api/pages/page-a/inpaint-mask",
          inpaintMaskPath: "mask.png",
          inpaintResultDataUrl: "/api/pages/page-a/inpaint-result",
          inpaintResultPath: "result.png"
        })
      ]
    });

    const merged = preservePendingInpaintSaveDataUrls(
      saved,
      maskSave({ dataUrl: "data:image/png;base64,pending-mask" }),
      current
    );

    expect(merged.pages[0]).toMatchObject({
      inpaintMaskDataUrl: "data:image/png;base64,pending-mask",
      inpaintMaskPath: "mask.png",
      inpaintResultDataUrl: "data:image/png;base64,current-result",
      inpaintResultPath: "result.png"
    });
  });
});

function maskSave(overrides: Partial<PendingMaskSave> = {}): PendingMaskSave {
  return {
    kind: "mask",
    chapterId: "chapter-a",
    pageId: "page-a",
    dataUrl: "mask",
    ...overrides
  };
}

function resultSave(overrides: Partial<PendingResultSave> = {}): PendingResultSave {
  return {
    kind: "result",
    chapterId: "chapter-a",
    pageId: "page-a",
    dataUrl: "result",
    ...overrides
  };
}

function layersSave(overrides: Partial<PendingLayersSave> = {}): PendingLayersSave {
  return {
    kind: "layers",
    chapterId: "chapter-a",
    pageId: "page-a",
    maskDataUrl: "mask",
    resultDataUrl: "result",
    ...overrides
  };
}

function chapterSnapshot(overrides: Partial<ChapterSnapshot> = {}): ChapterSnapshot {
  return {
    id: "chapter-a",
    workId: "work-a",
    title: "Chapter A",
    sourceKind: "images",
    status: "idle",
    pageOrder: ["page-a"],
    pages: [mangaPage()],
    createdAt: "2026-05-09T00:00:00.000Z",
    updatedAt: "2026-05-09T00:00:00.000Z",
    ...overrides
  };
}

function mangaPage(overrides: Partial<MangaPage> = {}): MangaPage {
  return {
    id: "page-a",
    name: "Page A",
    imagePath: "page.png",
    dataUrl: "data:image/png;base64,page",
    width: 100,
    height: 100,
    blocks: [],
    analysisStatus: "idle",
    createdAt: "2026-05-09T00:00:00.000Z",
    updatedAt: "2026-05-09T00:00:00.000Z",
    ...overrides
  };
}
