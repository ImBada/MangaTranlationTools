import { describe, expect, it, vi } from "vitest";
import { DEFAULT_FILTER_ZIGZAG_INTENSITY } from "../src/shared/fontPresets";
import {
  applyZigzagFilterToCanvas,
  buildFontFilterEffectSettingPatch,
  hasActiveFontZigzagFilter,
  resolveEnabledFontZigzagIntensity,
  resolveFilterZigzagIntensity,
  resolveFontFilterEffectMarginPx
} from "../src/client/src/lib/fontFilterEffects";

describe("font filter effects", () => {
  it("normalizes zigzag intensity and active state", () => {
    expect(resolveFilterZigzagIntensity(-1)).toBe(0);
    expect(resolveFilterZigzagIntensity(2)).toBe(1);
    expect(resolveFilterZigzagIntensity(Number.NaN)).toBe(DEFAULT_FILTER_ZIGZAG_INTENSITY);
    expect(resolveEnabledFontZigzagIntensity(undefined)).toBe(DEFAULT_FILTER_ZIGZAG_INTENSITY);
    expect(resolveEnabledFontZigzagIntensity(0)).toBe(DEFAULT_FILTER_ZIGZAG_INTENSITY);
    expect(resolveEnabledFontZigzagIntensity(0.72)).toBe(0.72);

    expect(hasActiveFontZigzagFilter({
      filterEffectEnabled: true,
      filterZigzagEnabled: true,
      filterZigzagIntensity: 0
    })).toBe(false);
    expect(hasActiveFontZigzagFilter({
      filterEffectEnabled: true,
      filterZigzagEnabled: true,
      filterZigzagIntensity: 0.1
    })).toBe(true);
  });

  it("keeps filter effect master state separate from zigzag child patches", () => {
    expect(buildFontFilterEffectSettingPatch({ zigzagEnabled: false })).toEqual({
      filterZigzagEnabled: false
    });
    expect(buildFontFilterEffectSettingPatch({
      zigzagEnabled: true,
      zigzagIntensity: 0.42
    })).toEqual({
      filterZigzagEnabled: true,
      filterZigzagIntensity: 0.42
    });
    expect(buildFontFilterEffectSettingPatch({ zigzagEnabled: false })).not.toHaveProperty("filterEffectEnabled");
  });

  it("expands canvas dirty margins for visible displacement", () => {
    expect(resolveFontFilterEffectMarginPx({
      filterEffectEnabled: false,
      filterZigzagEnabled: true,
      filterZigzagIntensity: 1
    }, 24)).toBe(0);
    expect(resolveFontFilterEffectMarginPx({
      filterEffectEnabled: true,
      filterZigzagEnabled: true,
      filterZigzagIntensity: 1
    }, 24)).toBeGreaterThan(0);
  });

  it("applies zigzag displacement to canvas pixels", () => {
    const width = 16;
    const height = 16;
    const source = new Uint8ClampedArray(width * height * 4);
    for (let index = 0; index < source.length; index += 4) {
      const pixel = index / 4;
      source[index] = pixel % 256;
      source[index + 1] = (pixel * 3) % 256;
      source[index + 2] = 200;
      source[index + 3] = 255;
    }
    const context = createCanvasContextMock(source, width, height);

    applyZigzagFilterToCanvas(
      context,
      width,
      height,
      { height, width, x: 0, y: 0 },
      0.5,
      8
    );

    expect(context.clearRect).toHaveBeenCalledWith(0, 0, width, height);
    expect(context.putImageData).toHaveBeenCalledTimes(1);
    const output = context.putImageData.mock.calls[0]?.[0].data as Uint8ClampedArray | undefined;
    expect(output).toBeDefined();
    expect(output?.some((value: number, index: number) => value !== source[index])).toBe(true);
  });

  it("distorts horizontally without mixing vertical rows", () => {
    const width = 12;
    const height = 12;
    const source = new Uint8ClampedArray(width * height * 4);
    for (let y = 0; y < height; y += 1) {
      for (let x = 0; x < width; x += 1) {
        const index = (y * width + x) * 4;
        source[index] = y * 16;
        source[index + 1] = 40;
        source[index + 2] = 90;
        source[index + 3] = 255;
      }
    }
    const context = createCanvasContextMock(source, width, height);

    applyZigzagFilterToCanvas(
      context,
      width,
      height,
      { height, width, x: 0, y: 0 },
      1,
      40
    );

    const output = context.putImageData.mock.calls[0]?.[0].data as Uint8ClampedArray | undefined;
    expect(output).toBeDefined();
    for (let y = 0; y < height; y += 1) {
      for (let x = 0; x < width; x += 1) {
        expect(output?.[(y * width + x) * 4]).toBe(y * 16);
      }
    }
  });

  it("keeps tremble sampling clamped at canvas bounds", () => {
    const width = 12;
    const height = 12;
    const source = new Uint8ClampedArray(width * height * 4);
    for (let index = 0; index < source.length; index += 4) {
      source[index] = 80;
      source[index + 1] = 120;
      source[index + 2] = 180;
      source[index + 3] = 255;
    }
    const context = createCanvasContextMock(source, width, height);

    applyZigzagFilterToCanvas(
      context,
      width,
      height,
      { height, width, x: 0, y: 0 },
      1,
      40
    );

    const output = context.putImageData.mock.calls[0]?.[0].data as Uint8ClampedArray | undefined;
    expect(output).toBeDefined();
    for (let index = 3; index < (output?.length ?? 0); index += 4) {
      expect(output?.[index]).toBe(255);
    }
  });
});

type CanvasContextMock = CanvasRenderingContext2D & {
  clearRect: ReturnType<typeof vi.fn>;
  createImageData: ReturnType<typeof vi.fn>;
  getImageData: ReturnType<typeof vi.fn>;
  putImageData: ReturnType<typeof vi.fn>;
};

function createCanvasContextMock(source: Uint8ClampedArray, width: number, height: number): CanvasContextMock {
  return {
    clearRect: vi.fn(),
    createImageData: vi.fn((nextWidth: number, nextHeight: number) => createImageDataMock(nextWidth, nextHeight)),
    getImageData: vi.fn(() => ({
      data: new Uint8ClampedArray(source),
      height,
      width
    } as ImageData)),
    putImageData: vi.fn()
  } as unknown as CanvasContextMock;
}

function createImageDataMock(width: number, height: number): ImageData {
  return {
    colorSpace: "srgb",
    data: new Uint8ClampedArray(width * height * 4),
    height,
    width
  } as ImageData;
}
