import { describe, expect, it } from "vitest";
import { clampStageViewScale, resolveStageFitSize, resolveStagePanBounds, resolveStagePanRange } from "../src/client/src/lib/stageFit";

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
    expect(clampStageViewScale(4)).toBe(2);
  });

  it("allows oversized pages to pan far enough to bring edges into view", () => {
    expect(resolveStagePanBounds({ width: 1200, height: 1800 }, { width: 800, height: 600 })).toEqual({
      width: 600,
      height: 900
    });
  });

  it("does not pan axes that already fit in the visible area", () => {
    expect(resolveStagePanBounds({ width: 600, height: 1800 }, { width: 800, height: 600 })).toEqual({
      width: 0,
      height: 900
    });
  });

  it("calculates exact pan limits from image and clip edges", () => {
    expect(resolveStagePanRange(
      { left: -200, top: -600, right: 1000, bottom: 1200, width: 1200, height: 1800 },
      { left: 0, top: 0, right: 800, bottom: 600, width: 800, height: 600 },
      0
    )).toEqual({
      minX: -200,
      maxX: 200,
      minY: -600,
      maxY: 600
    });
  });

  it("uses asymmetric pan limits when the image starts off-center", () => {
    expect(resolveStagePanRange(
      { left: 80, top: -600, right: 1280, bottom: 1200, width: 1200, height: 1800 },
      { left: 0, top: 0, right: 800, bottom: 600, width: 800, height: 600 },
      0
    )).toEqual({
      minX: -480,
      maxX: -80,
      minY: -600,
      maxY: 600
    });
  });

  it("adds a small edge margin beyond exact image edges", () => {
    expect(resolveStagePanRange(
      { left: -200, top: -600, right: 1000, bottom: 1200, width: 1200, height: 1800 },
      { left: 0, top: 0, right: 800, bottom: 600, width: 800, height: 600 },
      80
    )).toEqual({
      minX: -280,
      maxX: 280,
      minY: -680,
      maxY: 680
    });
  });
});
