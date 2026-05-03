import { describe, expect, it } from "vitest";
import { clampStageViewScale, resolveStageFitSize } from "../src/client/src/lib/stageFit";

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
});
