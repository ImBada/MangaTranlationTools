import { describe, expect, it } from "vitest";
import {
  clampStageViewScale,
  resolveStageDragZoomScale,
  resolveStageFitSize,
  resolveStageZoomAnchorPanOffset
} from "../src/client/src/lib/stageFit";

describe("stage fit sizing", () => {
  it("fits tall manga pages inside the available height", () => {
    const fit = resolveStageFitSize({ width: 1200, height: 1800 }, { width: 900, height: 600 });

    expect(fit.width).toBe(400);
    expect(fit.height).toBe(600);
  });

  it("fits wide pages inside the available width", () => {
    const fit = resolveStageFitSize({ width: 1600, height: 900 }, { width: 800, height: 700 });

    expect(fit.width).toBe(800);
    expect(fit.height).toBe(450);
  });

  it("keeps the existing maximum width and avoids upscaling", () => {
    expect(resolveStageFitSize({ width: 1600, height: 2400 }, { width: 2000, height: 3000 })).toEqual({
      width: 1040,
      height: 1560
    });
    expect(resolveStageFitSize({ width: 640, height: 960 }, { width: 1200, height: 1200 })).toEqual({
      width: 640,
      height: 960
    });
  });

  it("uses an explicit view scale for zoom and original-size viewing", () => {
    expect(resolveStageFitSize({ width: 1200, height: 1800 }, { width: 500, height: 500 }, { viewScale: 1 })).toEqual({
      width: 1200,
      height: 1800
    });
    expect(resolveStageFitSize({ width: 1200, height: 1800 }, { width: 500, height: 500 }, { viewScale: 0.25 })).toEqual({
      width: 300,
      height: 450
    });
  });

  it("clamps explicit zoom scales", () => {
    expect(clampStageViewScale(0.01)).toBe(0.1);
    expect(clampStageViewScale(12)).toBe(10);
  });

  it("resolves smooth drag zoom from horizontal movement", () => {
    expect(resolveStageDragZoomScale(1, 240)).toBe(2);
    expect(resolveStageDragZoomScale(1, -240)).toBe(0.5);
    expect(resolveStageDragZoomScale(1, 120)).toBeCloseTo(Math.SQRT2);
  });

  it("clamps drag zoom to supported bounds", () => {
    expect(resolveStageDragZoomScale(1, 2000)).toBe(10);
    expect(resolveStageDragZoomScale(1, -2000)).toBe(0.1);
  });

  it("offsets pan so drag zoom stays anchored to the clicked point", () => {
    const pan = resolveStageZoomAnchorPanOffset({
      anchorClientX: 620,
      anchorClientY: 430,
      centerClientX: 500,
      centerClientY: 350,
      contentX: 120,
      contentY: 80,
      startScale: 1,
      nextScale: 2
    });

    expect(pan).toEqual({ x: -120, y: -80 });
    expect(500 + pan.x + 120 * 2).toBe(620);
    expect(350 + pan.y + 80 * 2).toBe(430);
  });

  it("anchors drag zoom against workspace scale instead of canvas bounds", () => {
    const pan = resolveStageZoomAnchorPanOffset({
      anchorClientX: 900,
      anchorClientY: 160,
      centerClientX: 500,
      centerClientY: 350,
      contentX: 400,
      contentY: -190,
      startScale: 0.5,
      nextScale: 1
    });

    expect(pan).toEqual({ x: -400, y: 190 });
    expect(500 + pan.x + 400 * 2).toBe(900);
    expect(350 + pan.y - 190 * 2).toBe(160);
  });
});
