import { describe, expect, it } from "vitest";
import {
  isExpectedPreviousInpaintMask,
  isExpectedPreviousInpaintResult,
  resolveInpaintUndoDataUrlSequence
} from "../src/client/src/hooks/useInpaintLayerPersistence";

describe("inpaint layer persistence helpers", () => {
  it("accepts a saved mask image URL as the previous source for canvas data URL edits", () => {
    expect(isExpectedPreviousInpaintMask(
      "/api/library/chapters/chapter/pages/page/images/inpaint-mask",
      "data:image/png;base64,previous-canvas-mask",
      "/api/library/chapters/chapter/pages/page/images/inpaint-mask"
    )).toBe(true);
  });

  it("accepts a saved result image URL as the previous source for canvas data URL edits", () => {
    expect(isExpectedPreviousInpaintResult(
      "/api/library/chapters/chapter/pages/page/images/inpaint-result",
      "data:image/png;base64,previous-canvas-result",
      "/api/library/chapters/chapter/pages/page/images/inpaint-result"
    )).toBe(true);
  });

  it("rejects stale mask edits when neither the previous data URL nor source URL matches the live mask", () => {
    expect(isExpectedPreviousInpaintMask(
      "/api/library/chapters/chapter/pages/page/images/inpaint-mask?updatedAt=newer",
      "data:image/png;base64,previous-canvas-mask",
      "/api/library/chapters/chapter/pages/page/images/inpaint-mask?updatedAt=older"
    )).toBe(false);
  });

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

});
