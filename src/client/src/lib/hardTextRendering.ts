import type { TranslationBlock } from "../../../shared/types";

type HardTextPaletteColor = {
  r: number;
  g: number;
  b: number;
};

const DEFAULT_TEXT_OUTLINE_COLOR = "#000000";
const HARD_TEXT_ALPHA_THRESHOLD = 128;
const HARD_TEXT_COLOR_DISTANCE_THRESHOLD_SQUARED = 64 ** 2 * 3;

export type HardTextPixelValues = Pick<
  TranslationBlock,
  "outlineColor" | "outlineWidthPx" | "secondaryOutlineColor" | "shadowColor" | "textColor"
>;

export function hardenCanvasTextPixels(
  context: CanvasRenderingContext2D,
  width: number,
  height: number,
  values: HardTextPixelValues
): void {
  const pixels = context.getImageData(0, 0, width, height);
  hardenTextPixelData(pixels.data, width, height, values);
  context.putImageData(pixels, 0, 0);
}

export function hardenTextPixelData(
  pixels: Uint8ClampedArray,
  width: number,
  height: number,
  values: HardTextPixelValues
): void {
  const palette = resolveHardTextPalette(values);
  for (let offset = 3; offset < pixels.length; offset += 4) {
    const alpha = pixels[offset];
    if (alpha < HARD_TEXT_ALPHA_THRESHOLD) {
      pixels[offset - 3] = 0;
      pixels[offset - 2] = 0;
      pixels[offset - 1] = 0;
      pixels[offset] = 0;
      continue;
    }

    const snapped = findNearestHardTextPaletteColor(
      palette,
      pixels[offset - 3],
      pixels[offset - 2],
      pixels[offset - 1]
    );
    if (!snapped) {
      pixels[offset - 3] = 0;
      pixels[offset - 2] = 0;
      pixels[offset - 1] = 0;
      pixels[offset] = 0;
      continue;
    }

    pixels[offset - 3] = snapped.r;
    pixels[offset - 2] = snapped.g;
    pixels[offset - 1] = snapped.b;
    pixels[offset] = 255;
  }
  fillHardTextPrimaryOutlineInteriorGaps(pixels, width, height, values);
}

function fillHardTextPrimaryOutlineInteriorGaps(
  pixels: Uint8ClampedArray,
  width: number,
  height: number,
  values: Pick<TranslationBlock, "outlineColor" | "outlineWidthPx" | "textColor">
): void {
  if ((values.outlineWidthPx ?? 0) <= 0) {
    return;
  }

  const outlineColor = parseOptionalHexColor(values.outlineColor ?? DEFAULT_TEXT_OUTLINE_COLOR);
  const textColor = parseOptionalHexColor(values.textColor);
  if (!outlineColor || !textColor) {
    return;
  }

  const source = new Uint8ClampedArray(pixels);
  for (let y = 1; y < height - 1; y += 1) {
    for (let x = 1; x < width - 1; x += 1) {
      const offset = (y * width + x) * 4;
      if (source[offset + 3] !== 0) {
        continue;
      }
      if (!hasNeighborColor(source, width, x, y, outlineColor) || !hasNeighborColor(source, width, x, y, textColor)) {
        continue;
      }
      pixels[offset] = outlineColor.r;
      pixels[offset + 1] = outlineColor.g;
      pixels[offset + 2] = outlineColor.b;
      pixels[offset + 3] = 255;
    }
  }
}

function hasNeighborColor(
  pixels: Uint8ClampedArray,
  width: number,
  x: number,
  y: number,
  color: HardTextPaletteColor
): boolean {
  for (let dy = -1; dy <= 1; dy += 1) {
    for (let dx = -1; dx <= 1; dx += 1) {
      if (dx === 0 && dy === 0) {
        continue;
      }
      const offset = ((y + dy) * width + x + dx) * 4;
      if (
        pixels[offset + 3] === 255 &&
        pixels[offset] === color.r &&
        pixels[offset + 1] === color.g &&
        pixels[offset + 2] === color.b
      ) {
        return true;
      }
    }
  }
  return false;
}

function resolveHardTextPalette(
  values: Pick<TranslationBlock, "outlineColor" | "secondaryOutlineColor" | "shadowColor" | "textColor">
): HardTextPaletteColor[] {
  const colors = [
    values.textColor,
    values.outlineColor ?? DEFAULT_TEXT_OUTLINE_COLOR,
    values.secondaryOutlineColor ?? "#ffffff",
    values.shadowColor ?? "#000000",
    "#ffffff"
  ];
  const palette: HardTextPaletteColor[] = [];
  const used = new Set<string>();
  for (const color of colors) {
    const parsed = parseOptionalHexColor(color);
    if (!parsed) {
      continue;
    }
    const key = `${parsed.r},${parsed.g},${parsed.b}`;
    if (used.has(key)) {
      continue;
    }
    used.add(key);
    palette.push(parsed);
  }
  return palette;
}

function findNearestHardTextPaletteColor(
  palette: readonly HardTextPaletteColor[],
  r: number,
  g: number,
  b: number
): HardTextPaletteColor | null {
  if (palette.length === 0) {
    return null;
  }

  let nearest = palette[0];
  let nearestDistance = Number.POSITIVE_INFINITY;
  for (const color of palette) {
    const distance = (color.r - r) ** 2 + (color.g - g) ** 2 + (color.b - b) ** 2;
    if (distance < nearestDistance) {
      nearest = color;
      nearestDistance = distance;
    }
  }
  return nearestDistance <= HARD_TEXT_COLOR_DISTANCE_THRESHOLD_SQUARED ? nearest : null;
}

function parseHexColor(hex: string): { r: number; g: number; b: number } {
  const normalized = hex.replace("#", "");
  return {
    r: Number.parseInt(normalized.slice(0, 2), 16),
    g: Number.parseInt(normalized.slice(2, 4), 16),
    b: Number.parseInt(normalized.slice(4, 6), 16)
  };
}

function parseOptionalHexColor(hex: string | undefined): HardTextPaletteColor | null {
  if (!hex || !/^#[0-9a-f]{6}$/iu.test(hex)) {
    return null;
  }
  const color = parseHexColor(hex);
  if (!Number.isFinite(color.r) || !Number.isFinite(color.g) || !Number.isFinite(color.b)) {
    return null;
  }
  return color;
}
