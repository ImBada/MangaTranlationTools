import { describe, expect, it } from "vitest";
import { resolveCenteredEllipsisYOffset, resolveInpaintMaskCoverageAlpha } from "../src/client/src/lib/pageRender";

describe("page render glyph positioning", () => {
  it("moves low ellipsis glyphs up to the center of the font box", () => {
    const context = makeMeasureContext({
      actualBoundingBoxAscent: -70,
      actualBoundingBoxDescent: 80
    });

    expect(resolveCenteredEllipsisYOffset(context, "…", 100)).toBe(-25);
    expect(resolveCenteredEllipsisYOffset(context, "...", 100)).toBe(-25);
  });

  it("leaves already centered ellipsis glyphs and other text unchanged", () => {
    const context = makeMeasureContext({
      actualBoundingBoxAscent: -45,
      actualBoundingBoxDescent: 55
    });

    expect(resolveCenteredEllipsisYOffset(context, "…", 100)).toBe(0);
    expect(resolveCenteredEllipsisYOffset(context, "가", 100)).toBe(0);
  });

  it("does not adjust ellipsis when ink metrics are unavailable", () => {
    const context = makeMeasureContext({
      actualBoundingBoxAscent: 0,
      actualBoundingBoxDescent: 0
    });

    expect(resolveCenteredEllipsisYOffset(context, "…", 100)).toBe(0);
  });
});

describe("page render mask compositing", () => {
  it("uses mask luminance as well as alpha when compositing inpaint results", () => {
    const pixels = new Uint8ClampedArray([
      0, 0, 0, 255,
      255, 255, 255, 255,
      255, 255, 255, 128
    ]);

    expect(resolveInpaintMaskCoverageAlpha(pixels, 0)).toBe(0);
    expect(resolveInpaintMaskCoverageAlpha(pixels, 4)).toBe(1);
    expect(resolveInpaintMaskCoverageAlpha(pixels, 8)).toBeCloseTo(128 / 255);
  });
});

function makeMeasureContext(metrics: Pick<TextMetrics, "actualBoundingBoxAscent" | "actualBoundingBoxDescent">): Pick<CanvasRenderingContext2D, "measureText"> {
  return {
    measureText: () => ({
      width: 100,
      ...metrics
    }) as TextMetrics
  };
}
