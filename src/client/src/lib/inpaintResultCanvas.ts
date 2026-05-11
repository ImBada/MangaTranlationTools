import type { ImageRect } from "../../../shared/types";

export type DrawPoint = {
  x: number;
  y: number;
};

export type RgbaColor = {
  r: number;
  g: number;
  b: number;
  a: number;
};

export function resolveSelectionRect(
  from: DrawPoint,
  to: DrawPoint,
  pageSize: { width: number; height: number }
): ImageRect {
  const left = Math.max(0, Math.min(from.x, to.x));
  const top = Math.max(0, Math.min(from.y, to.y));
  const right = Math.min(pageSize.width, Math.max(from.x, to.x));
  const bottom = Math.min(pageSize.height, Math.max(from.y, to.y));
  return {
    x: Math.floor(left),
    y: Math.floor(top),
    width: Math.max(0, Math.ceil(right) - Math.floor(left)),
    height: Math.max(0, Math.ceil(bottom) - Math.floor(top))
  };
}

export function resolveBrushBounds(point: DrawPoint, radius: number, width: number, height: number): { x: number; y: number; width: number; height: number } {
  const x = Math.max(0, Math.floor(point.x - radius));
  const y = Math.max(0, Math.floor(point.y - radius));
  const right = Math.min(width, Math.ceil(point.x + radius));
  const bottom = Math.min(height, Math.ceil(point.y + radius));
  return {
    x,
    y,
    width: Math.max(0, right - x),
    height: Math.max(0, bottom - y)
  };
}

export function brushMaskAlpha(x: number, y: number, center: DrawPoint, radius: number, hardness: number): number {
  const distance = Math.hypot(x - center.x, y - center.y);
  if (distance >= radius) {
    return 0;
  }
  const normalizedHardness = clamp01(hardness);
  if (normalizedHardness >= 1) {
    return 1;
  }
  const hardRadius = radius * normalizedHardness;
  if (distance <= hardRadius) {
    return 1;
  }
  const featherProgress = (distance - hardRadius) / Math.max(1, radius - hardRadius);
  const softness = 1 - normalizedHardness;
  return Math.pow(1 - featherProgress, 1 + softness * 2);
}

export function sampleBlur(source: Uint8ClampedArray, width: number, height: number, x: number, y: number): [number, number, number] {
  let red = 0;
  let green = 0;
  let blue = 0;
  let count = 0;
  for (let dy = -1; dy <= 1; dy += 1) {
    for (let dx = -1; dx <= 1; dx += 1) {
      const sx = Math.max(0, Math.min(width - 1, x + dx));
      const sy = Math.max(0, Math.min(height - 1, y + dy));
      const offset = (sy * width + sx) * 4;
      red += source[offset];
      green += source[offset + 1];
      blue += source[offset + 2];
      count += 1;
    }
  }
  return [red / count, green / count, blue / count];
}

export function sampleSharpen(source: Uint8ClampedArray, width: number, height: number, x: number, y: number): [number, number, number] {
  const center = readRgb(source, width, height, x, y);
  const left = readRgb(source, width, height, x - 1, y);
  const right = readRgb(source, width, height, x + 1, y);
  const top = readRgb(source, width, height, x, y - 1);
  const bottom = readRgb(source, width, height, x, y + 1);
  return [
    clampByte(center[0] * 5 - left[0] - right[0] - top[0] - bottom[0]),
    clampByte(center[1] * 5 - left[1] - right[1] - top[1] - bottom[1]),
    clampByte(center[2] * 5 - left[2] - right[2] - top[2] - bottom[2])
  ];
}

export function parseHexColor(value: string): RgbaColor {
  const normalized = /^#[0-9a-f]{6}$/iu.test(value) ? value.slice(1) : "ffffff";
  return {
    r: Number.parseInt(normalized.slice(0, 2), 16),
    g: Number.parseInt(normalized.slice(2, 4), 16),
    b: Number.parseInt(normalized.slice(4, 6), 16),
    a: 1
  };
}

export function blendChannel(source: number, target: number, amount: number): number {
  return clampByte(source + (target - source) * amount);
}

export function clampByte(value: number): number {
  return Math.max(0, Math.min(255, Math.round(value)));
}

export function clamp01(value: number): number {
  return Math.max(0, Math.min(1, value));
}

export function isCanvasBlank(canvas: HTMLCanvasElement): boolean {
  const context = canvas.getContext("2d");
  if (!context) {
    return true;
  }

  const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
  for (let index = 3; index < pixels.length; index += 4) {
    if (pixels[index] !== 0) {
      return false;
    }
  }
  return true;
}

function readRgb(source: Uint8ClampedArray, width: number, height: number, x: number, y: number): [number, number, number] {
  const sx = Math.max(0, Math.min(width - 1, x));
  const sy = Math.max(0, Math.min(height - 1, y));
  const offset = (sy * width + sx) * 4;
  return [source[offset], source[offset + 1], source[offset + 2]];
}
