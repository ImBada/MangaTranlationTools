import { describe, expect, it } from "vitest";
import { hardenTextPixelData } from "../src/client/src/lib/hardTextRendering";
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

describe("hard text rendering pixels", () => {
  it("snaps antialiased text pixels to the nearest text palette color", () => {
    const pixels = new Uint8ClampedArray([
      18, 18, 18, 180,
      230, 230, 230, 180,
      18, 18, 18, 64
    ]);

    hardenTextPixelData(pixels, 3, 1, {
      outlineColor: "#000000",
      outlineWidthPx: 0,
      secondaryOutlineColor: "#ffffff",
      shadowColor: "#000000",
      textColor: "#111111"
    });

    expect([...pixels]).toEqual([
      17, 17, 17, 255,
      255, 255, 255, 255,
      0, 0, 0, 0
    ]);
  });

  it("fills one-pixel transparent gaps between text fill and primary outline", () => {
    const pixels = new Uint8ClampedArray(3 * 3 * 4);
    setPixel(pixels, 3, 0, 1, [0, 0, 0, 255]);
    setPixel(pixels, 3, 2, 1, [17, 17, 17, 255]);

    hardenTextPixelData(pixels, 3, 3, {
      outlineColor: "#000000",
      outlineWidthPx: 2,
      secondaryOutlineColor: "#ffffff",
      shadowColor: "#000000",
      textColor: "#111111"
    });

    expect(readPixel(pixels, 3, 1, 1)).toEqual([0, 0, 0, 255]);
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

function setPixel(pixels: Uint8ClampedArray, width: number, x: number, y: number, rgba: [number, number, number, number]): void {
  const offset = (y * width + x) * 4;
  pixels.set(rgba, offset);
}

function readPixel(pixels: Uint8ClampedArray, width: number, x: number, y: number): [number, number, number, number] {
  const offset = (y * width + x) * 4;
  return [pixels[offset], pixels[offset + 1], pixels[offset + 2], pixels[offset + 3]];
}
