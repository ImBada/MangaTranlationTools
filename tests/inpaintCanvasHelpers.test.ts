import { describe, expect, it, vi } from "vitest";
import {
  drawMaskSegment,
  isCanvasBlank as isMaskCanvasBlank,
  resolveCanvasPoint,
  resolveSelectionRect as resolveMaskSelectionRect
} from "../src/client/src/lib/inpaintLayerCanvas";
import {
  blendChannel,
  brushMaskAlpha,
  clamp01,
  clampByte,
  isCanvasBlank as isResultCanvasBlank,
  parseHexColor,
  resolveBrushBounds,
  resolveSelectionRect as resolveResultSelectionRect,
  rgbaToCss,
  sampleBlur,
  sampleSharpen
} from "../src/client/src/lib/inpaintResultCanvas";

describe("inpaint layer canvas helpers", () => {
  it("maps pointer coordinates into page pixel coordinates", () => {
    const point = resolveCanvasPoint(
      150,
      260,
      { left: 50, top: 60, width: 200, height: 400 } as DOMRect,
      { width: 1000, height: 2000 }
    );

    expect(point).toEqual({ x: 500, y: 1000 });
  });

  it("clamps selection rectangles to page bounds regardless of drag direction", () => {
    expect(resolveMaskSelectionRect(
      { x: 900.8, y: 100.2 },
      { x: -50.4, y: 650.6 },
      { width: 800, height: 600 }
    )).toEqual({
      x: 0,
      y: 100,
      width: 800,
      height: 500
    });

    expect(resolveMaskSelectionRect(
      { x: -120.3, y: -80.2 },
      { x: 240.4, y: 180.6 },
      { width: 800, height: 600 }
    )).toEqual({
      x: 0,
      y: 0,
      width: 241,
      height: 181
    });
  });

  it("configures mask drawing for brush and eraser strokes", () => {
    const context = createCanvasContextMock();

    drawMaskSegment(context, { x: 3, y: 5 }, { x: 11, y: 17 }, 0, true);

    expect(context.lineWidth).toBe(1);
    expect(context.globalCompositeOperation).toBe("destination-out");
    expect(context.strokeStyle).toBe("#ffffff");
    expect(context.moveTo).toHaveBeenCalledWith(3, 5);
    expect(context.lineTo).toHaveBeenCalledWith(11, 17);
    expect(context.stroke).toHaveBeenCalledTimes(1);
  });

  it("detects blank and nonblank mask canvases by alpha channel", () => {
    expect(isMaskCanvasBlank(createCanvasMock([0, 0, 0, 0, 255, 255, 255, 0]))).toBe(true);
    expect(isMaskCanvasBlank(createCanvasMock([0, 0, 0, 0, 255, 255, 255, 1]))).toBe(false);
  });
});

describe("inpaint result canvas helpers", () => {
  it("resolves and clamps result selection rectangles", () => {
    expect(resolveResultSelectionRect(
      { x: 20.2, y: 120.7 },
      { x: 360.1, y: -30.4 },
      { width: 300, height: 200 }
    )).toEqual({
      x: 20,
      y: 0,
      width: 280,
      height: 121
    });
  });

  it("clips brush bounds at image edges", () => {
    expect(resolveBrushBounds({ x: 3.4, y: 48.8 }, 10, 100, 50)).toEqual({
      x: 0,
      y: 38,
      width: 14,
      height: 12
    });
  });

  it("calculates feathered brush mask alpha", () => {
    const center = { x: 50, y: 50 };

    expect(brushMaskAlpha(50, 50, center, 20, 0.5)).toBe(1);
    expect(brushMaskAlpha(60, 50, center, 20, 0.5)).toBe(1);
    expect(brushMaskAlpha(65, 50, center, 20, 0.5)).toBeCloseTo(0.5);
    expect(brushMaskAlpha(70, 50, center, 20, 0.5)).toBe(0);
  });

  it("samples blur with edge clamping", () => {
    const source = rgbImage(3, 3, [
      [10, 20, 30], [20, 30, 40], [30, 40, 50],
      [40, 50, 60], [50, 60, 70], [60, 70, 80],
      [70, 80, 90], [80, 90, 100], [90, 100, 110]
    ]);

    expect(sampleBlur(source, 3, 3, 1, 1)).toEqual([50, 60, 70]);
    expect(sampleBlur(source, 3, 3, 0, 0)).toEqual([23.333333333333332, 33.333333333333336, 43.333333333333336]);
  });

  it("samples sharpen and clamps channels", () => {
    const source = rgbImage(3, 3, [
      [10, 10, 10], [10, 10, 10], [10, 10, 10],
      [10, 10, 10], [100, 120, 200], [10, 10, 10],
      [10, 10, 10], [10, 10, 10], [10, 10, 10]
    ]);

    expect(sampleSharpen(source, 3, 3, 1, 1)).toEqual([255, 255, 255]);
    expect(sampleSharpen(source, 3, 3, 0, 0)).toEqual([10, 10, 10]);
  });

  it("normalizes color and channel values", () => {
    expect(parseHexColor("#0a1B2c")).toEqual({ r: 10, g: 27, b: 44, a: 1 });
    expect(parseHexColor("bad")).toEqual({ r: 255, g: 255, b: 255, a: 1 });
    expect(rgbaToCss({ r: 10, g: 20, b: 30, a: 0.5 }, 0.4)).toBe("rgba(10, 20, 30, 0.2)");
    expect(blendChannel(10, 20, 0.25)).toBe(13);
    expect(clampByte(300.2)).toBe(255);
    expect(clampByte(-4)).toBe(0);
    expect(clamp01(2)).toBe(1);
    expect(clamp01(-1)).toBe(0);
  });

  it("detects blank and nonblank result canvases by alpha channel", () => {
    expect(isResultCanvasBlank(createCanvasMock([12, 34, 56, 0]))).toBe(true);
    expect(isResultCanvasBlank(createCanvasMock([12, 34, 56, 128]))).toBe(false);
  });
});

function createCanvasContextMock(): CanvasRenderingContext2D {
  return {
    save: vi.fn(),
    restore: vi.fn(),
    beginPath: vi.fn(),
    moveTo: vi.fn(),
    lineTo: vi.fn(),
    stroke: vi.fn()
  } as unknown as CanvasRenderingContext2D;
}

function createCanvasMock(alphaPixels: number[]): HTMLCanvasElement {
  return {
    width: Math.max(1, alphaPixels.length / 4),
    height: 1,
    getContext: () => ({
      getImageData: () => ({ data: new Uint8ClampedArray(alphaPixels) })
    })
  } as unknown as HTMLCanvasElement;
}

function rgbImage(width: number, height: number, pixels: [number, number, number][]): Uint8ClampedArray {
  const data = new Uint8ClampedArray(width * height * 4);
  for (let index = 0; index < pixels.length; index += 1) {
    data[index * 4] = pixels[index][0];
    data[index * 4 + 1] = pixels[index][1];
    data[index * 4 + 2] = pixels[index][2];
    data[index * 4 + 3] = 255;
  }
  return data;
}
