import {
  FONT_FILTER_EFFECT_DEFINITIONS,
  resolveBlurRadiusPx,
  resolveFilterGlitchIntensity,
  resolveFilterNoiseIntensity,
  resolveFilterWaveIntensity,
  resolveFontFilterFontSizePx,
  resolveGlitchChannelSplitPx,
  resolveGlitchDisplacementPx,
  resolvePixelateBlockSizePx,
  resolveWaveDisplacementPx,
  resolveZigzagDisplacementPx,
  type FontFilterEffectIntensityKey,
  type FontFilterEffectValues
} from "./fontFilterEffectMetadata";

export type FontFilterEffectBounds = {
  height: number;
  width: number;
  x: number;
  y: number;
};

const ZIGZAG_TREMBLE_BANDS_PER_FONT_SIZE = 12;
const ZIGZAG_VERTICAL_EDGE_FADE_RATIO = 0.06;
const ZIGZAG_TREMBLE_ALTERNATE_WEIGHT = 0.62;
const ZIGZAG_TREMBLE_NOISE_WEIGHT = 0.34;
const ZIGZAG_TREMBLE_MICRO_SHAKE = 0.14;

const NOISE_COLOR_RANGE = 48;
const NOISE_ALPHA_RANGE = 0.14;

type ClampedFilterBounds = {
  height: number;
  left: number;
  top: number;
  width: number;
};

type FontFilterPixelTransform = (
  source: Uint8ClampedArray,
  width: number,
  height: number,
  intensity: number | undefined,
  fontSizePx: number
) => Uint8ClampedArray | null;

const FONT_FILTER_PIXEL_TRANSFORMS = {
  filterZigzagIntensity: applyZigzagFilterToPixels,
  filterWaveIntensity: applyWaveFilterToPixels,
  filterGlitchIntensity: applyGlitchFilterToPixels,
  filterPixelateIntensity: applyPixelateFilterToPixels,
  filterBlurIntensity: applyBlurFilterToPixels,
  filterNoiseIntensity: applyNoiseFilterToPixels
} satisfies Record<FontFilterEffectIntensityKey, FontFilterPixelTransform>;

export function applyFontFilterEffectsToCanvas(
  context: CanvasRenderingContext2D,
  canvasWidth: number,
  canvasHeight: number,
  bounds: FontFilterEffectBounds,
  values: FontFilterEffectValues,
  fontSizePx: number
): void {
  const activeDefinitions = FONT_FILTER_EFFECT_DEFINITIONS.filter((definition) => definition.hasActive(values));
  if (activeDefinitions.length === 0) {
    return;
  }

  const clampedBounds = resolveClampedBounds(canvasWidth, canvasHeight, bounds);
  if (!clampedBounds) {
    return;
  }

  const sourceImage = context.getImageData(clampedBounds.left, clampedBounds.top, clampedBounds.width, clampedBounds.height);
  let currentPixels: Uint8ClampedArray<ArrayBufferLike> = sourceImage.data;
  let changed = false;

  for (const definition of activeDefinitions) {
    const nextPixels = FONT_FILTER_PIXEL_TRANSFORMS[definition.intensityKey](
      currentPixels,
      clampedBounds.width,
      clampedBounds.height,
      values[definition.intensityKey],
      fontSizePx
    );
    if (nextPixels) {
      currentPixels = nextPixels;
      changed = true;
    }
  }

  if (!changed) {
    return;
  }

  const output = context.createImageData(clampedBounds.width, clampedBounds.height);
  output.data.set(currentPixels);
  context.clearRect(clampedBounds.left, clampedBounds.top, clampedBounds.width, clampedBounds.height);
  context.putImageData(output, clampedBounds.left, clampedBounds.top);
}

function applyZigzagFilterToPixels(
  source: Uint8ClampedArray,
  width: number,
  height: number,
  intensity: number | undefined,
  fontSizePx: number
): Uint8ClampedArray | null {
  const displacementPx = resolveZigzagDisplacementPx(fontSizePx, intensity);
  if (displacementPx <= 0) {
    return null;
  }
  return applyHorizontalDisplacementFilterToPixels(source, width, height, (y, sourceHeight) =>
    resolveZigzagTrembleOffset(y + 0.5, sourceHeight, fontSizePx) * displacementPx
  );
}

function applyWaveFilterToPixels(
  source: Uint8ClampedArray,
  width: number,
  height: number,
  intensity: number | undefined,
  fontSizePx: number
): Uint8ClampedArray | null {
  const resolvedIntensity = resolveFilterWaveIntensity(intensity);
  const displacementPx = resolveWaveDisplacementPx(fontSizePx, intensity);
  if (displacementPx <= 0) {
    return null;
  }
  const waveLengthPx = Math.max(4, resolveFontFilterFontSizePx(fontSizePx) * (1.15 - resolvedIntensity * 0.45));
  return applyHorizontalDisplacementFilterToPixels(source, width, height, (y) =>
    Math.sin(((y + 0.5) / waveLengthPx) * Math.PI * 2) * displacementPx
  );
}

function applyGlitchFilterToPixels(
  source: Uint8ClampedArray,
  width: number,
  height: number,
  intensity: number | undefined,
  fontSizePx: number
): Uint8ClampedArray | null {
  const resolvedIntensity = resolveFilterGlitchIntensity(intensity);
  const displacementPx = resolveGlitchDisplacementPx(fontSizePx, intensity);
  const channelSplitPx = resolveGlitchChannelSplitPx(fontSizePx, intensity);
  if (resolvedIntensity <= 0 || (displacementPx <= 0 && channelSplitPx <= 0)) {
    return null;
  }

  const output = new Uint8ClampedArray(source.length);
  for (let y = 0; y < height; y += 1) {
    const rowOffsetX = resolveGlitchRowOffset(y + 0.5, fontSizePx, resolvedIntensity, displacementPx);
    for (let x = 0; x < width; x += 1) {
      const outputOffset = (y * width + x) * 4;
      output[outputOffset] = Math.round(sampleImageDataBilinearChannel(source, width, height, x + rowOffsetX + channelSplitPx, y, 0));
      output[outputOffset + 1] = Math.round(sampleImageDataBilinearChannel(source, width, height, x + rowOffsetX, y, 1));
      output[outputOffset + 2] = Math.round(sampleImageDataBilinearChannel(source, width, height, x + rowOffsetX - channelSplitPx, y, 2));
      output[outputOffset + 3] = Math.round(sampleImageDataBilinearChannel(source, width, height, x + rowOffsetX, y, 3));
    }
  }
  return output;
}

function applyPixelateFilterToPixels(
  source: Uint8ClampedArray,
  width: number,
  height: number,
  intensity: number | undefined,
  fontSizePx: number
): Uint8ClampedArray | null {
  const blockSizePx = resolvePixelateBlockSizePx(fontSizePx, intensity);
  if (blockSizePx <= 1) {
    return null;
  }

  const output = new Uint8ClampedArray(source.length);
  for (let y = 0; y < height; y += 1) {
    const sourceY = Math.floor(clamp(Math.floor(y / blockSizePx) * blockSizePx + Math.floor(blockSizePx / 2), 0, height - 1));
    for (let x = 0; x < width; x += 1) {
      const sourceX = Math.floor(clamp(Math.floor(x / blockSizePx) * blockSizePx + Math.floor(blockSizePx / 2), 0, width - 1));
      const sourceOffset = (sourceY * width + sourceX) * 4;
      const outputOffset = (y * width + x) * 4;
      output[outputOffset] = source[sourceOffset];
      output[outputOffset + 1] = source[sourceOffset + 1];
      output[outputOffset + 2] = source[sourceOffset + 2];
      output[outputOffset + 3] = source[sourceOffset + 3];
    }
  }
  return output;
}

function applyBlurFilterToPixels(
  source: Uint8ClampedArray,
  width: number,
  height: number,
  intensity: number | undefined,
  fontSizePx: number
): Uint8ClampedArray | null {
  const radiusPx = resolveBlurRadiusPx(fontSizePx, intensity);
  if (radiusPx <= 0) {
    return null;
  }

  const output = new Uint8ClampedArray(source.length);
  boxBlurPremultipliedImageData(source, output, width, height, radiusPx);
  return output;
}

function applyNoiseFilterToPixels(
  source: Uint8ClampedArray,
  width: number,
  height: number,
  intensity: number | undefined,
  fontSizePx: number
): Uint8ClampedArray | null {
  const resolvedIntensity = resolveFilterNoiseIntensity(intensity);
  if (resolvedIntensity <= 0) {
    return null;
  }

  const output = new Uint8ClampedArray(source);
  const colorAmount = NOISE_COLOR_RANGE * resolvedIntensity;
  const alphaAmount = NOISE_ALPHA_RANGE * resolvedIntensity;
  const fontNoiseScale = resolveFontFilterFontSizePx(fontSizePx) * 0.37;

  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const offset = (y * width + x) * 4;
      const alpha = source[offset + 3];
      if (alpha <= 0) {
        continue;
      }
      const noise = resolveSignedNoise(x * 12.9898 + y * 78.233 + fontNoiseScale);
      const colorNoise = noise * colorAmount;
      output[offset] = clampByte(source[offset] + colorNoise);
      output[offset + 1] = clampByte(source[offset + 1] + colorNoise);
      output[offset + 2] = clampByte(source[offset + 2] + colorNoise);
      output[offset + 3] = clampByte(alpha * (1 + noise * alphaAmount));
    }
  }
  return output;
}

function applyHorizontalDisplacementFilterToPixels(
  source: Uint8ClampedArray,
  width: number,
  height: number,
  resolveOffsetX: (y: number, height: number) => number
): Uint8ClampedArray {
  const output = new Uint8ClampedArray(source.length);

  for (let y = 0; y < height; y += 1) {
    const offsetX = resolveOffsetX(y, height);
    for (let x = 0; x < width; x += 1) {
      sampleImageDataBilinear(
        source,
        width,
        height,
        x + offsetX,
        y,
        output,
        (y * width + x) * 4
      );
    }
  }

  return output;
}

function resolveClampedBounds(
  canvasWidth: number,
  canvasHeight: number,
  bounds: FontFilterEffectBounds
): ClampedFilterBounds | null {
  const left = Math.max(0, Math.floor(bounds.x));
  const top = Math.max(0, Math.floor(bounds.y));
  const right = Math.min(canvasWidth, Math.ceil(bounds.x + bounds.width));
  const bottom = Math.min(canvasHeight, Math.ceil(bounds.y + bounds.height));
  const width = right - left;
  const height = bottom - top;
  if (width <= 0 || height <= 0) {
    return null;
  }
  return { height, left, top, width };
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

  const bandHeightPx = Math.max(1, resolveFontFilterFontSizePx(fontSizePx) / ZIGZAG_TREMBLE_BANDS_PER_FONT_SIZE);
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

function resolveGlitchRowOffset(y: number, fontSizePx: number, intensity: number, displacementPx: number): number {
  const bandHeightPx = Math.max(1, resolveFontFilterFontSizePx(fontSizePx) * (0.24 - intensity * 0.08));
  const bandIndex = Math.floor(y / bandHeightPx);
  const bandNoise = resolveSignedNoise(bandIndex * 3.17);
  const gate = Math.abs(resolveSignedNoise(bandIndex + 19.31)) < 0.22 ? 0.35 : 1;
  const snappedNoise = Math.round(bandNoise * 2) / 2;
  return (snappedNoise + bandNoise * 0.3) * displacementPx * gate;
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
  for (let channel = 0; channel < 4; channel += 1) {
    output[outputOffset + channel] = Math.round(sampleImageDataBilinearChannel(source, width, height, x, y, channel));
  }
}

function sampleImageDataBilinearChannel(
  source: Uint8ClampedArray,
  width: number,
  height: number,
  x: number,
  y: number,
  channel: number
): number {
  const clampedX = clamp(x, 0, width - 1);
  const clampedY = clamp(y, 0, height - 1);
  const x0 = Math.floor(clampedX);
  const y0 = Math.floor(clampedY);
  const x1 = Math.min(width - 1, x0 + 1);
  const y1 = Math.min(height - 1, y0 + 1);
  const tx = clampedX - x0;
  const ty = clampedY - y0;
  const topLeft = (y0 * width + x0) * 4 + channel;
  const topRight = (y0 * width + x1) * 4 + channel;
  const bottomLeft = (y1 * width + x0) * 4 + channel;
  const bottomRight = (y1 * width + x1) * 4 + channel;
  const top = source[topLeft] * (1 - tx) + source[topRight] * tx;
  const bottom = source[bottomLeft] * (1 - tx) + source[bottomRight] * tx;
  return top * (1 - ty) + bottom * ty;
}

function boxBlurPremultipliedImageData(
  source: Uint8ClampedArray,
  output: Uint8ClampedArray,
  width: number,
  height: number,
  radius: number
): void {
  const premultiplied = new Float32Array(source.length);
  for (let offset = 0; offset < source.length; offset += 4) {
    const alpha = source[offset + 3] / 255;
    premultiplied[offset] = source[offset] * alpha;
    premultiplied[offset + 1] = source[offset + 1] * alpha;
    premultiplied[offset + 2] = source[offset + 2] * alpha;
    premultiplied[offset + 3] = source[offset + 3];
  }

  const horizontal = new Float32Array(source.length);
  const blurred = new Float32Array(source.length);
  blurFloatImageDataHorizontal(premultiplied, horizontal, width, height, radius);
  blurFloatImageDataVertical(horizontal, blurred, width, height, radius);

  for (let offset = 0; offset < blurred.length; offset += 4) {
    const alpha = clamp(blurred[offset + 3], 0, 255);
    output[offset + 3] = clampByte(alpha);
    if (alpha <= 0) {
      continue;
    }
    const alphaRatio = alpha / 255;
    output[offset] = clampByte(blurred[offset] / alphaRatio);
    output[offset + 1] = clampByte(blurred[offset + 1] / alphaRatio);
    output[offset + 2] = clampByte(blurred[offset + 2] / alphaRatio);
  }
}

function blurFloatImageDataHorizontal(
  source: Float32Array,
  output: Float32Array,
  width: number,
  height: number,
  radius: number
): void {
  const windowSize = radius * 2 + 1;
  for (let y = 0; y < height; y += 1) {
    for (let channel = 0; channel < 4; channel += 1) {
      let sum = 0;
      for (let offsetX = -radius; offsetX <= radius; offsetX += 1) {
        sum += source[(y * width + Math.floor(clamp(offsetX, 0, width - 1))) * 4 + channel];
      }
      for (let x = 0; x < width; x += 1) {
        output[(y * width + x) * 4 + channel] = sum / windowSize;
        if (x < width - 1) {
          const removeX = Math.floor(clamp(x - radius, 0, width - 1));
          const addX = Math.floor(clamp(x + radius + 1, 0, width - 1));
          sum += source[(y * width + addX) * 4 + channel] - source[(y * width + removeX) * 4 + channel];
        }
      }
    }
  }
}

function blurFloatImageDataVertical(
  source: Float32Array,
  output: Float32Array,
  width: number,
  height: number,
  radius: number
): void {
  const windowSize = radius * 2 + 1;
  for (let x = 0; x < width; x += 1) {
    for (let channel = 0; channel < 4; channel += 1) {
      let sum = 0;
      for (let offsetY = -radius; offsetY <= radius; offsetY += 1) {
        sum += source[(Math.floor(clamp(offsetY, 0, height - 1)) * width + x) * 4 + channel];
      }
      for (let y = 0; y < height; y += 1) {
        output[(y * width + x) * 4 + channel] = sum / windowSize;
        if (y < height - 1) {
          const removeY = Math.floor(clamp(y - radius, 0, height - 1));
          const addY = Math.floor(clamp(y + radius + 1, 0, height - 1));
          sum += source[(addY * width + x) * 4 + channel] - source[(removeY * width + x) * 4 + channel];
        }
      }
    }
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

function clampByte(value: number): number {
  return Math.round(clamp(value, 0, 255));
}

function resolveSignedNoise(value: number): number {
  const raw = Math.sin((value + 1) * 127.1) * 43758.5453123;
  return (raw - Math.floor(raw)) * 2 - 1;
}
