import { describe, expect, it } from "vitest";
import {
  coalescePendingInpaintLayerSaves,
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
