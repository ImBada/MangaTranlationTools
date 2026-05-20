import { DEFAULT_FILTER_ZIGZAG_INTENSITY } from "../../../shared/fontPresets";

export type FontFilterEffectValues = {
  filterEffectEnabled?: boolean;
  filterZigzagEnabled?: boolean;
  filterZigzagIntensity?: number;
};

export type FontFilterEffectControlPatch = {
  zigzagEnabled?: boolean;
  zigzagIntensity?: number;
};

export type ZigzagFilterBounds = {
  height: number;
  width: number;
  x: number;
  y: number;
};

export const FILTER_ZIGZAG_INTENSITY_MIN = 0;
export const FILTER_ZIGZAG_INTENSITY_MAX = 1;
export const FILTER_ZIGZAG_INTENSITY_STEP = 0.01;

const ZIGZAG_DISPLACEMENT_FONT_SIZE_RATIO = 0.18;
const ZIGZAG_TREMBLE_BANDS_PER_FONT_SIZE = 12;
const ZIGZAG_VERTICAL_EDGE_FADE_RATIO = 0.06;
const ZIGZAG_TREMBLE_ALTERNATE_WEIGHT = 0.62;
const ZIGZAG_TREMBLE_NOISE_WEIGHT = 0.34;
const ZIGZAG_TREMBLE_MICRO_SHAKE = 0.14;

export function resolveFilterZigzagIntensity(intensity: number | undefined): number {
  const value = intensity ?? DEFAULT_FILTER_ZIGZAG_INTENSITY;
  return Number.isFinite(value)
    ? Math.max(FILTER_ZIGZAG_INTENSITY_MIN, Math.min(FILTER_ZIGZAG_INTENSITY_MAX, value))
    : DEFAULT_FILTER_ZIGZAG_INTENSITY;
}

export function resolveEnabledFontZigzagIntensity(intensity: number | undefined): number {
  const resolvedIntensity = resolveFilterZigzagIntensity(intensity);
  return resolvedIntensity > 0 ? resolvedIntensity : DEFAULT_FILTER_ZIGZAG_INTENSITY;
}

export function hasActiveFontZigzagFilter(values: FontFilterEffectValues): boolean {
  return Boolean(
    values.filterEffectEnabled &&
    values.filterZigzagEnabled &&
    resolveFilterZigzagIntensity(values.filterZigzagIntensity) > 0
  );
}

export function buildFontFilterEffectSettingPatch(patch: FontFilterEffectControlPatch): Partial<FontFilterEffectValues> {
  const values: Partial<FontFilterEffectValues> = {};
  if (patch.zigzagEnabled !== undefined) {
    values.filterZigzagEnabled = patch.zigzagEnabled;
  }
  if (patch.zigzagIntensity !== undefined) {
    values.filterZigzagIntensity = patch.zigzagIntensity;
  }
  return values;
}

export function resolveZigzagDisplacementPx(fontSizePx: number, intensity: number | undefined): number {
  const fontSize = Number.isFinite(fontSizePx) ? Math.max(1, fontSizePx) : 1;
  return fontSize * ZIGZAG_DISPLACEMENT_FONT_SIZE_RATIO * resolveFilterZigzagIntensity(intensity);
}

export function resolveFontFilterEffectMarginPx(values: FontFilterEffectValues, fontSizePx: number): number {
  if (!hasActiveFontZigzagFilter(values)) {
    return 0;
  }
  return Math.ceil(resolveZigzagDisplacementPx(fontSizePx, values.filterZigzagIntensity) * 2 + 2);
}

export function applyZigzagFilterToCanvas(
  context: CanvasRenderingContext2D,
  canvasWidth: number,
  canvasHeight: number,
  bounds: ZigzagFilterBounds,
  intensity: number | undefined,
  fontSizePx: number
): void {
  const displacementPx = resolveZigzagDisplacementPx(fontSizePx, intensity);
  if (displacementPx <= 0) {
    return;
  }

  const left = Math.max(0, Math.floor(bounds.x));
  const top = Math.max(0, Math.floor(bounds.y));
  const right = Math.min(canvasWidth, Math.ceil(bounds.x + bounds.width));
  const bottom = Math.min(canvasHeight, Math.ceil(bounds.y + bounds.height));
  const width = right - left;
  const height = bottom - top;
  if (width <= 0 || height <= 0) {
    return;
  }

  const source = context.getImageData(left, top, width, height);
  const output = context.createImageData(width, height);

  for (let y = 0; y < height; y += 1) {
    const offsetX = resolveZigzagTrembleOffset(y + 0.5, height, fontSizePx) * displacementPx;
    for (let x = 0; x < width; x += 1) {
      const sourceX = x + offsetX;
      const sourceY = y;
      sampleImageDataBilinear(source.data, width, height, sourceX, sourceY, output.data, (y * width + x) * 4);
    }
  }

  context.clearRect(left, top, width, height);
  context.putImageData(output, left, top);
}

function resolveZigzagTrembleOffset(y: number, height: number, fontSizePx: number): number {
  const normalizedY = height > 1 ? y / height : 0;
  const edgeFade = Math.min(
    smoothstep(0, ZIGZAG_VERTICAL_EDGE_FADE_RATIO, normalizedY),
    1 - smoothstep(1 - ZIGZAG_VERTICAL_EDGE_FADE_RATIO, 1, normalizedY)
  );
  if (edgeFade <= 0) {
    return 0;
  }

  const bandHeightPx = Math.max(1, fontSizePx / ZIGZAG_TREMBLE_BANDS_PER_FONT_SIZE);
  const bandPosition = y / bandHeightPx;
  const bandIndex = Math.floor(bandPosition);
  const bandProgress = bandPosition - bandIndex;
  const mix = smoothstep(0.18, 0.82, bandProgress);
  const currentOffset = resolveZigzagTrembleBandOffset(bandIndex);
  const nextOffset = resolveZigzagTrembleBandOffset(bandIndex + 1);
  const bandOffset = currentOffset * (1 - mix) + nextOffset * mix;
  const microShake = Math.sin(bandPosition * Math.PI * 3) * ZIGZAG_TREMBLE_MICRO_SHAKE;
  return clamp(bandOffset + microShake, -1, 1) * edgeFade;
}

function resolveZigzagTrembleBandOffset(bandIndex: number): number {
  const alternatingOffset = bandIndex % 2 === 0 ? ZIGZAG_TREMBLE_ALTERNATE_WEIGHT : -ZIGZAG_TREMBLE_ALTERNATE_WEIGHT;
  const noiseOffset = resolveSignedNoise(bandIndex) * ZIGZAG_TREMBLE_NOISE_WEIGHT;
  return clamp(alternatingOffset + noiseOffset, -1, 1);
}

function sampleImageDataBilinear(
  source: Uint8ClampedArray,
  width: number,
  height: number,
  x: number,
  y: number,
  output: Uint8ClampedArray,
  outputOffset: number
): void {
  const clampedX = clamp(x, 0, width - 1);
  const clampedY = clamp(y, 0, height - 1);
  const x0 = Math.floor(clampedX);
  const y0 = Math.floor(clampedY);
  const x1 = Math.min(width - 1, x0 + 1);
  const y1 = Math.min(height - 1, y0 + 1);
  const tx = clampedX - x0;
  const ty = clampedY - y0;
  const topLeft = (y0 * width + x0) * 4;
  const topRight = (y0 * width + x1) * 4;
  const bottomLeft = (y1 * width + x0) * 4;
  const bottomRight = (y1 * width + x1) * 4;

  for (let channel = 0; channel < 4; channel += 1) {
    const top = source[topLeft + channel] * (1 - tx) + source[topRight + channel] * tx;
    const bottom = source[bottomLeft + channel] * (1 - tx) + source[bottomRight + channel] * tx;
    output[outputOffset + channel] = Math.round(top * (1 - ty) + bottom * ty);
  }
}

function smoothstep(edge0: number, edge1: number, value: number): number {
  if (edge0 === edge1) {
    return value < edge0 ? 0 : 1;
  }
  const t = clamp((value - edge0) / (edge1 - edge0), 0, 1);
  return t * t * (3 - 2 * t);
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

function resolveSignedNoise(value: number): number {
  const raw = Math.sin((value + 1) * 127.1) * 43758.5453123;
  return (raw - Math.floor(raw)) * 2 - 1;
}
