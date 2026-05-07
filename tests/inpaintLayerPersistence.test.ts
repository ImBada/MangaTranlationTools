import { describe, expect, it } from "vitest";
import {
  resolveInpaintLayerUndoSnapshotSequence,
  resolveInpaintUndoDataUrlSequence
} from "../src/client/src/hooks/useInpaintLayerPersistence";

describe("inpaint layer persistence helpers", () => {
  it("keeps intermediate undo data URLs while removing no-op and adjacent duplicate states", () => {
    expect(resolveInpaintUndoDataUrlSequence(
      "before",
      ["mid-a", "mid-a", "mid-b", "after"],
      "after"
    )).toEqual(["before", "mid-a", "mid-b"]);
  });

  it("preserves undefined undo states when they represent a real transition", () => {
    expect(resolveInpaintUndoDataUrlSequence(
      undefined,
      ["mask-a", undefined, "mask-b"],
      "mask-c"
    )).toEqual([undefined, "mask-a", undefined, "mask-b"]);
  });

  it("removes duplicates that become adjacent after final-state entries are dropped", () => {
    expect(resolveInpaintUndoDataUrlSequence(
      "before",
      ["after", "before", "mid-a", "after", "mid-a"],
      "after"
    )).toEqual(["before", "mid-a"]);
  });

  it("keeps paired mask/result undo snapshots for mixed layer updates", () => {
    expect(resolveInpaintLayerUndoSnapshotSequence(
      { maskDataUrl: "mask-before", resultDataUrl: "result-before" },
      [
        { maskDataUrl: "mask-smart", resultDataUrl: "result-smart" },
        { maskDataUrl: "mask-smart", resultDataUrl: "result-smart" },
        { maskDataUrl: "mask-smart", resultDataUrl: "result-after" }
      ],
      { maskDataUrl: "mask-after", resultDataUrl: "result-after" }
    )).toEqual([
      { maskDataUrl: "mask-before", resultDataUrl: "result-before" },
      { maskDataUrl: "mask-smart", resultDataUrl: "result-smart" },
      { maskDataUrl: "mask-smart", resultDataUrl: "result-after" }
    ]);
  });

  it("removes layer duplicates that become adjacent after final-state snapshots are dropped", () => {
    expect(resolveInpaintLayerUndoSnapshotSequence(
      { maskDataUrl: "mask-before", resultDataUrl: "result-before" },
      [
        { maskDataUrl: "mask-after", resultDataUrl: "result-after" },
        { maskDataUrl: "mask-before", resultDataUrl: "result-before" },
        { maskDataUrl: "mask-mid", resultDataUrl: "result-mid" },
        { maskDataUrl: "mask-after", resultDataUrl: "result-after" },
        { maskDataUrl: "mask-mid", resultDataUrl: "result-mid" }
      ],
      { maskDataUrl: "mask-after", resultDataUrl: "result-after" }
    )).toEqual([
      { maskDataUrl: "mask-before", resultDataUrl: "result-before" },
      { maskDataUrl: "mask-mid", resultDataUrl: "result-mid" }
    ]);
  });

  it("drops layer undo snapshots that are identical to the final state", () => {
    expect(resolveInpaintLayerUndoSnapshotSequence(
      { maskDataUrl: undefined, resultDataUrl: undefined },
      [
        { maskDataUrl: "mask-a", resultDataUrl: "result-a" },
        { maskDataUrl: "mask-final", resultDataUrl: "result-final" }
      ],
      { maskDataUrl: "mask-final", resultDataUrl: "result-final" }
    )).toEqual([
      { maskDataUrl: undefined, resultDataUrl: undefined },
      { maskDataUrl: "mask-a", resultDataUrl: "result-a" }
    ]);
  });
});
