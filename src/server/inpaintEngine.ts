import sharp from "sharp";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawn } from "node:child_process";
import type { InpaintEngine, InpaintSettings } from "../shared/types";

type DecodedImage = {
  data: Buffer;
  width: number;
  height: number;
};

type ImageRect = {
  x: number;
  y: number;
  width: number;
  height: number;
};

export type InpaintEngineRunOptions = {
  lamaCommand?: string;
  lamaArgs?: string[];
  settings?: InpaintSettings;
};

const MASK_ACTIVE_THRESHOLD = 0.03;
const DEFAULT_LAMA_ARGS = ["--input", "{source}", "--mask", "{mask}", "--output", "{output}"];
const DEFAULT_INPAINT_SETTINGS: InpaintSettings = {
  engine: "local-fill-fallback",
  paddingPx: 0,
  featherPx: 0,
  tileSize: 1024,
  artifactCleanupPx: 8
};
const ARTIFACT_CLEANUP_STRENGTH = 0.9;
const ARTIFACT_CLEANUP_MIN_LUMA = 190;
const ARTIFACT_CLEANUP_SAMPLE_MIN_LUMA = 225;
const ARTIFACT_CLEANUP_MAX_CHROMA = 24;
const ARTIFACT_CLEANUP_MAX_BG_DISTANCE = 90;
const ARTIFACT_CLEANUP_DARK_STROKE_LUMA = 150;
const NEIGHBORS = [
  [-1, -1],
  [0, -1],
  [1, -1],
  [-1, 0],
  [1, 0],
  [-1, 1],
  [0, 1],
  [1, 1]
] as const;

export async function runInpaintEngine(
  sourceDataUrl: string,
  maskDataUrl: string,
  engine: InpaintEngine,
  options: InpaintEngineRunOptions = {}
): Promise<string> {
  const settings = normalizeInpaintSettings(engine, options.settings);
  const source = await decodeRgba(sourceDataUrl);
  const mask = prepareMask(await decodeRgba(maskDataUrl, source.width, source.height), settings);
  const bounds = resolveMaskBounds(mask);

  if (engine === "lama") {
    return bounds ? runExternalLamaInpaint(source, mask, cropRectForBounds(bounds, source.width, source.height, settings), { ...options, settings }) : emptyResultLayer(source.width, source.height);
  }

  if (engine === "mask-fill-fallback") {
    return rgbaToPngDataUrl(mask.data, mask.width, mask.height);
  }

  if (!bounds) {
    return emptyResultLayer(source.width, source.height);
  }

  const rect = cropRectForBounds(bounds, source.width, source.height, settings);
  const result = localFillInpaint(cropImage(source, rect), cropImage(mask, rect));
  return maskedResultLayer(source, result, cropImage(mask, rect), rect, settings);
}

export function resolveLamaCommandFromEnv(env: NodeJS.ProcessEnv): InpaintEngineRunOptions {
  const lamaCommand = env.MANGA_TRANSLATOR_LAMA_COMMAND?.trim();
  const lamaArgsRaw = env.MANGA_TRANSLATOR_LAMA_ARGS?.trim();
  return {
    lamaCommand: lamaCommand || undefined,
    lamaArgs: lamaArgsRaw ? parseLamaArgs(lamaArgsRaw) : undefined
  };
}

async function runExternalLamaInpaint(source: DecodedImage, mask: DecodedImage, rect: ImageRect, options: InpaintEngineRunOptions): Promise<string> {
  if (!options.lamaCommand) {
    throw new Error("LaMa 인페인트 명령이 설정되지 않았습니다.");
  }

  const sourceCrop = cropImage(source, rect);
  const maskCrop = cropImage(mask, rect);
  const workDir = await mkdtemp(join(tmpdir(), "manga-inpaint-"));
  const sourcePath = join(workDir, "source.png");
  const maskPath = join(workDir, "mask.png");
  const outputPath = join(workDir, "output.png");
  try {
    await writeDecodedPngFile(sourceCrop, sourcePath);
    await writeDecodedPngFile(maskCrop, maskPath);
    await runCommand(
      options.lamaCommand,
      (options.lamaArgs ?? DEFAULT_LAMA_ARGS).map((arg) => applyLamaArgPlaceholders(arg, sourcePath, maskPath, outputPath))
    );
    const inpainted = await decodeRgba(`data:image/png;base64,${(await readFile(outputPath)).toString("base64")}`, rect.width, rect.height);
    return maskedResultLayer(source, inpainted, maskCrop, rect, options.settings);
  } finally {
    await rm(workDir, { recursive: true, force: true });
  }
}

async function writeDecodedPngFile(image: DecodedImage, outputPath: string): Promise<void> {
  await sharp(image.data, {
    raw: {
      width: image.width,
      height: image.height,
      channels: 4
    }
  }).png().toFile(outputPath);
}

function runCommand(command: string, args: string[]): Promise<void> {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      stdio: ["ignore", "pipe", "pipe"],
      windowsHide: true
    });
    const stdout: Buffer[] = [];
    const stderr: Buffer[] = [];
    child.stdout.on("data", (chunk: Buffer) => stdout.push(chunk));
    child.stderr.on("data", (chunk: Buffer) => stderr.push(chunk));
    child.on("error", reject);
    child.on("close", (code) => {
      if (code === 0) {
        resolve();
        return;
      }
      const detail = Buffer.concat(stderr).toString("utf8").trim() || Buffer.concat(stdout).toString("utf8").trim();
      reject(new Error(`LaMa 인페인트 명령이 실패했습니다. code=${code}${detail ? `: ${detail}` : ""}`));
    });
  });
}

function applyLamaArgPlaceholders(arg: string, sourcePath: string, maskPath: string, outputPath: string): string {
  return arg
    .replaceAll("{source}", sourcePath)
    .replaceAll("{mask}", maskPath)
    .replaceAll("{output}", outputPath);
}

function parseLamaArgs(raw: string): string[] {
  const parsed = JSON.parse(raw) as unknown;
  if (!Array.isArray(parsed) || !parsed.every((entry) => typeof entry === "string")) {
    throw new Error("MANGA_TRANSLATOR_LAMA_ARGS는 문자열 배열 JSON이어야 합니다.");
  }
  return parsed;
}

function normalizeInpaintSettings(engine: InpaintEngine, settings?: InpaintSettings): InpaintSettings {
  return {
    ...DEFAULT_INPAINT_SETTINGS,
    ...settings,
    engine,
    paddingPx: Math.max(0, Math.round(settings?.paddingPx ?? DEFAULT_INPAINT_SETTINGS.paddingPx)),
    featherPx: Math.max(0, Math.round(settings?.featherPx ?? DEFAULT_INPAINT_SETTINGS.featherPx)),
    tileSize: Math.max(128, Math.round(settings?.tileSize ?? DEFAULT_INPAINT_SETTINGS.tileSize)),
    artifactCleanupPx: Math.max(0, Math.round(settings?.artifactCleanupPx ?? DEFAULT_INPAINT_SETTINGS.artifactCleanupPx ?? 0))
  };
}

function prepareMask(mask: DecodedImage, settings: InpaintSettings): DecodedImage {
  const total = mask.width * mask.height;
  const binary = new Uint8Array(total);
  for (let index = 0; index < total; index += 1) {
    const offset = index * 4;
    const alpha = mask.data[offset + 3] / 255;
    const luma = (mask.data[offset] * 0.299 + mask.data[offset + 1] * 0.587 + mask.data[offset + 2] * 0.114) / 255;
    binary[index] = alpha * luma > MASK_ACTIVE_THRESHOLD ? 255 : 0;
  }

  const dilated = settings.paddingPx > 0 ? dilateMask(binary, mask.width, mask.height, settings.paddingPx) : binary;
  const coverage = settings.featherPx > 0 ? boxBlurMask(dilated, mask.width, mask.height, settings.featherPx) : dilated;
  const data = Buffer.alloc(total * 4);
  for (let index = 0; index < total; index += 1) {
    const offset = index * 4;
    const value = coverage[index];
    data[offset] = value;
    data[offset + 1] = value;
    data[offset + 2] = value;
    data[offset + 3] = 255;
  }

  return {
    data,
    width: mask.width,
    height: mask.height
  };
}

function dilateMask(mask: Uint8Array, width: number, height: number, radius: number): Uint8Array {
  const horizontal = new Uint8Array(mask.length);
  const output = new Uint8Array(mask.length);

  for (let y = 0; y < height; y += 1) {
    let activeCount = 0;
    for (let x = -radius; x <= radius; x += 1) {
      if (x >= 0 && x < width && mask[y * width + x] > 0) {
        activeCount += 1;
      }
    }
    for (let x = 0; x < width; x += 1) {
      horizontal[y * width + x] = activeCount > 0 ? 255 : 0;
      const removeX = x - radius;
      const addX = x + radius + 1;
      if (removeX >= 0 && mask[y * width + removeX] > 0) {
        activeCount -= 1;
      }
      if (addX < width && mask[y * width + addX] > 0) {
        activeCount += 1;
      }
    }
  }

  for (let x = 0; x < width; x += 1) {
    let activeCount = 0;
    for (let y = -radius; y <= radius; y += 1) {
      if (y >= 0 && y < height && horizontal[y * width + x] > 0) {
        activeCount += 1;
      }
    }
    for (let y = 0; y < height; y += 1) {
      output[y * width + x] = activeCount > 0 ? 255 : 0;
      const removeY = y - radius;
      const addY = y + radius + 1;
      if (removeY >= 0 && horizontal[removeY * width + x] > 0) {
        activeCount -= 1;
      }
      if (addY < height && horizontal[addY * width + x] > 0) {
        activeCount += 1;
      }
    }
  }

  return output;
}

function boxBlurMask(mask: Uint8Array, width: number, height: number, radius: number): Uint8Array {
  const horizontal = new Float32Array(mask.length);
  const output = new Uint8Array(mask.length);

  for (let y = 0; y < height; y += 1) {
    let sum = 0;
    for (let x = -radius; x <= radius; x += 1) {
      if (x >= 0 && x < width) {
        sum += mask[y * width + x];
      }
    }
    for (let x = 0; x < width; x += 1) {
      const left = Math.max(0, x - radius);
      const right = Math.min(width - 1, x + radius);
      horizontal[y * width + x] = sum / (right - left + 1);
      const removeX = x - radius;
      const addX = x + radius + 1;
      if (removeX >= 0) {
        sum -= mask[y * width + removeX];
      }
      if (addX < width) {
        sum += mask[y * width + addX];
      }
    }
  }

  for (let x = 0; x < width; x += 1) {
    let sum = 0;
    for (let y = -radius; y <= radius; y += 1) {
      if (y >= 0 && y < height) {
        sum += horizontal[y * width + x];
      }
    }
    for (let y = 0; y < height; y += 1) {
      const top = Math.max(0, y - radius);
      const bottom = Math.min(height - 1, y + radius);
      output[y * width + x] = Math.round(sum / (bottom - top + 1));
      const removeY = y - radius;
      const addY = y + radius + 1;
      if (removeY >= 0) {
        sum -= horizontal[removeY * width + x];
      }
      if (addY < height) {
        sum += horizontal[addY * width + x];
      }
    }
  }

  return output;
}

function resolveMaskBounds(mask: DecodedImage): ImageRect | null {
  let minX = mask.width;
  let minY = mask.height;
  let maxX = -1;
  let maxY = -1;

  for (let y = 0; y < mask.height; y += 1) {
    for (let x = 0; x < mask.width; x += 1) {
      const offset = (y * mask.width + x) * 4;
      if (mask.data[offset] <= 0) {
        continue;
      }
      minX = Math.min(minX, x);
      minY = Math.min(minY, y);
      maxX = Math.max(maxX, x);
      maxY = Math.max(maxY, y);
    }
  }

  if (maxX < minX || maxY < minY) {
    return null;
  }

  return {
    x: minX,
    y: minY,
    width: maxX - minX + 1,
    height: maxY - minY + 1
  };
}

function cropRectForBounds(bounds: ImageRect, width: number, height: number, settings: InpaintSettings): ImageRect {
  const cleanupPx = settings.artifactCleanupPx ?? 0;
  const contextMargin = Math.max(settings.paddingPx + settings.featherPx + cleanupPx + 32, Math.round(Math.min(settings.tileSize, 1024) * 0.25));
  const x = Math.max(0, bounds.x - contextMargin);
  const y = Math.max(0, bounds.y - contextMargin);
  const right = Math.min(width, bounds.x + bounds.width + contextMargin);
  const bottom = Math.min(height, bounds.y + bounds.height + contextMargin);
  return {
    x,
    y,
    width: right - x,
    height: bottom - y
  };
}

function cropImage(image: DecodedImage, rect: ImageRect): DecodedImage {
  const data = Buffer.alloc(rect.width * rect.height * 4);
  for (let y = 0; y < rect.height; y += 1) {
    const sourceStart = ((rect.y + y) * image.width + rect.x) * 4;
    const targetStart = y * rect.width * 4;
    image.data.copy(data, targetStart, sourceStart, sourceStart + rect.width * 4);
  }
  return {
    data,
    width: rect.width,
    height: rect.height
  };
}

async function emptyResultLayer(width: number, height: number): Promise<string> {
  return rgbaToPngDataUrl(Buffer.alloc(width * height * 4), width, height);
}

async function maskedResultLayer(source: DecodedImage, inpainted: DecodedImage, mask: DecodedImage, rect: ImageRect, settings?: InpaintSettings): Promise<string> {
  const output = Buffer.alloc(source.width * source.height * 4);
  const artifactCleanupPx = Math.max(0, Math.round(settings?.artifactCleanupPx ?? DEFAULT_INPAINT_SETTINGS.artifactCleanupPx ?? 0));
  const artifactCleanupMask = artifactCleanupPx > 0 ? buildArtifactCleanupMask(mask, artifactCleanupPx) : null;
  const artifactBackground = artifactCleanupMask ? estimateArtifactBackgroundColor(source, mask, rect) : null;

  for (let y = 0; y < rect.height; y += 1) {
    for (let x = 0; x < rect.width; x += 1) {
      const cropOffset = (y * rect.width + x) * 4;
      const amount = mask.data[cropOffset];
      const sourceOffset = ((rect.y + y) * source.width + rect.x + x) * 4;
      if (amount <= 0) {
        if (artifactCleanupMask && artifactBackground) {
          writeArtifactCleanupPixel(output, source, mask, rect, x, y, sourceOffset, artifactCleanupMask[y * rect.width + x], artifactBackground);
        }
        continue;
      }
      output[sourceOffset] = inpainted.data[cropOffset];
      output[sourceOffset + 1] = inpainted.data[cropOffset + 1];
      output[sourceOffset + 2] = inpainted.data[cropOffset + 2];
      output[sourceOffset + 3] = amount;
    }
  }

  return rgbaToPngDataUrl(output, source.width, source.height);
}

function buildArtifactCleanupMask(mask: DecodedImage, radius: number): Uint8Array {
  const total = mask.width * mask.height;
  const binary = new Uint8Array(total);
  for (let index = 0; index < total; index += 1) {
    binary[index] = mask.data[index * 4] > 0 ? 255 : 0;
  }

  const dilated = dilateMask(binary, mask.width, mask.height, radius);
  const featherRadius = Math.max(1, Math.round(radius / 2));
  return boxBlurMask(dilated, mask.width, mask.height, featherRadius);
}

function estimateArtifactBackgroundColor(source: DecodedImage, mask: DecodedImage, rect: ImageRect): [number, number, number] {
  let red = 0;
  let green = 0;
  let blue = 0;
  let weightSum = 0;

  for (let y = 0; y < rect.height; y += 1) {
    for (let x = 0; x < rect.width; x += 1) {
      const cropOffset = (y * rect.width + x) * 4;
      if (mask.data[cropOffset] > 0) {
        continue;
      }
      const sourceOffset = ((rect.y + y) * source.width + rect.x + x) * 4;
      const r = source.data[sourceOffset];
      const g = source.data[sourceOffset + 1];
      const b = source.data[sourceOffset + 2];
      const luma = rgbLuma(r, g, b);
      const chroma = rgbChroma(r, g, b);
      if (luma < ARTIFACT_CLEANUP_SAMPLE_MIN_LUMA || chroma > ARTIFACT_CLEANUP_MAX_CHROMA) {
        continue;
      }
      const weight = Math.max(0.1, (luma - ARTIFACT_CLEANUP_SAMPLE_MIN_LUMA) / (255 - ARTIFACT_CLEANUP_SAMPLE_MIN_LUMA));
      red += r * weight;
      green += g * weight;
      blue += b * weight;
      weightSum += weight;
    }
  }

  if (weightSum <= 0) {
    return [255, 255, 255];
  }
  return [red / weightSum, green / weightSum, blue / weightSum];
}

function writeArtifactCleanupPixel(
  output: Buffer,
  source: DecodedImage,
  mask: DecodedImage,
  rect: ImageRect,
  x: number,
  y: number,
  sourceOffset: number,
  cleanupAmount: number,
  background: [number, number, number]
): void {
  if (cleanupAmount <= 0 || hasDarkUnmaskedNeighbor(source, mask, rect, x, y)) {
    return;
  }

  const r = source.data[sourceOffset];
  const g = source.data[sourceOffset + 1];
  const b = source.data[sourceOffset + 2];
  const luma = rgbLuma(r, g, b);
  const chroma = rgbChroma(r, g, b);
  if (luma < ARTIFACT_CLEANUP_MIN_LUMA || chroma > ARTIFACT_CLEANUP_MAX_CHROMA) {
    return;
  }

  const backgroundDistance = rgbDistance([r, g, b], background);
  if (backgroundDistance > ARTIFACT_CLEANUP_MAX_BG_DISTANCE) {
    return;
  }

  const lumaWeight = clamp01((luma - ARTIFACT_CLEANUP_MIN_LUMA) / (255 - ARTIFACT_CLEANUP_MIN_LUMA));
  const backgroundWeight = clamp01(1 - backgroundDistance / ARTIFACT_CLEANUP_MAX_BG_DISTANCE);
  const alpha = Math.round(cleanupAmount * ARTIFACT_CLEANUP_STRENGTH * Math.max(lumaWeight, backgroundWeight));
  if (alpha <= 0) {
    return;
  }

  output[sourceOffset] = Math.round(background[0]);
  output[sourceOffset + 1] = Math.round(background[1]);
  output[sourceOffset + 2] = Math.round(background[2]);
  output[sourceOffset + 3] = alpha;
}

function hasDarkUnmaskedNeighbor(source: DecodedImage, mask: DecodedImage, rect: ImageRect, x: number, y: number): boolean {
  for (let dy = -1; dy <= 1; dy += 1) {
    for (let dx = -1; dx <= 1; dx += 1) {
      if (dx === 0 && dy === 0) {
        continue;
      }
      const nx = x + dx;
      const ny = y + dy;
      if (nx < 0 || ny < 0 || nx >= rect.width || ny >= rect.height) {
        continue;
      }
      const cropOffset = (ny * rect.width + nx) * 4;
      if (mask.data[cropOffset] > 0) {
        continue;
      }
      const sourceOffset = ((rect.y + ny) * source.width + rect.x + nx) * 4;
      if (rgbLuma(source.data[sourceOffset], source.data[sourceOffset + 1], source.data[sourceOffset + 2]) < ARTIFACT_CLEANUP_DARK_STROKE_LUMA) {
        return true;
      }
    }
  }
  return false;
}

function localFillInpaint(source: DecodedImage, mask: DecodedImage): DecodedImage {
  const total = source.width * source.height;
  const output = Buffer.from(source.data);
  const coverage = new Float32Array(total);
  const masked = new Uint8Array(total);
  const filled = new Uint8Array(total);
  const queue = new Int32Array(total);
  const fallbackColor = averageUnmaskedColor(source, mask);
  let head = 0;
  let tail = 0;

  for (let index = 0; index < total; index += 1) {
    const maskOffset = index * 4;
    const alpha = mask.data[maskOffset + 3] / 255;
    const luma = (mask.data[maskOffset] * 0.299 + mask.data[maskOffset + 1] * 0.587 + mask.data[maskOffset + 2] * 0.114) / 255;
    coverage[index] = alpha * luma;
    masked[index] = coverage[index] > MASK_ACTIVE_THRESHOLD ? 1 : 0;
  }

  for (let y = 0; y < source.height; y += 1) {
    for (let x = 0; x < source.width; x += 1) {
      const index = y * source.width + x;
      if (!masked[index] || !hasAvailableNeighbor(x, y, source.width, source.height, masked, filled)) {
        continue;
      }
      fillPixelFromNeighbors(index, x, y, source.width, source.height, output, masked, filled, fallbackColor);
      filled[index] = 1;
      queue[tail] = index;
      tail += 1;
    }
  }

  while (head < tail) {
    const index = queue[head];
    head += 1;
    const x = index % source.width;
    const y = Math.floor(index / source.width);
    for (const [dx, dy] of NEIGHBORS) {
      const nx = x + dx;
      const ny = y + dy;
      if (nx < 0 || ny < 0 || nx >= source.width || ny >= source.height) {
        continue;
      }
      const neighborIndex = ny * source.width + nx;
      if (!masked[neighborIndex] || filled[neighborIndex]) {
        continue;
      }
      fillPixelFromNeighbors(neighborIndex, nx, ny, source.width, source.height, output, masked, filled, fallbackColor);
      filled[neighborIndex] = 1;
      queue[tail] = neighborIndex;
      tail += 1;
    }
  }

  for (let index = 0; index < total; index += 1) {
    if (!masked[index]) {
      continue;
    }
    if (!filled[index]) {
      writeColor(output, index, fallbackColor);
    }
    const sourceOffset = index * 4;
    const amount = Math.max(0, Math.min(1, coverage[index]));
    output[sourceOffset] = Math.round(source.data[sourceOffset] * (1 - amount) + output[sourceOffset] * amount);
    output[sourceOffset + 1] = Math.round(source.data[sourceOffset + 1] * (1 - amount) + output[sourceOffset + 1] * amount);
    output[sourceOffset + 2] = Math.round(source.data[sourceOffset + 2] * (1 - amount) + output[sourceOffset + 2] * amount);
    output[sourceOffset + 3] = source.data[sourceOffset + 3];
  }

  return {
    data: output,
    width: source.width,
    height: source.height
  };
}

function hasAvailableNeighbor(
  x: number,
  y: number,
  width: number,
  height: number,
  masked: Uint8Array,
  filled: Uint8Array
): boolean {
  for (const [dx, dy] of NEIGHBORS) {
    const nx = x + dx;
    const ny = y + dy;
    if (nx < 0 || ny < 0 || nx >= width || ny >= height) {
      continue;
    }
    const index = ny * width + nx;
    if (!masked[index] || filled[index]) {
      return true;
    }
  }
  return false;
}

function fillPixelFromNeighbors(
  index: number,
  x: number,
  y: number,
  width: number,
  height: number,
  output: Buffer,
  masked: Uint8Array,
  filled: Uint8Array,
  fallbackColor: [number, number, number]
): void {
  let red = 0;
  let green = 0;
  let blue = 0;
  let count = 0;

  for (const [dx, dy] of NEIGHBORS) {
    const nx = x + dx;
    const ny = y + dy;
    if (nx < 0 || ny < 0 || nx >= width || ny >= height) {
      continue;
    }
    const neighborIndex = ny * width + nx;
    if (masked[neighborIndex] && !filled[neighborIndex]) {
      continue;
    }
    const offset = neighborIndex * 4;
    red += output[offset];
    green += output[offset + 1];
    blue += output[offset + 2];
    count += 1;
  }

  writeColor(output, index, count > 0 ? [red / count, green / count, blue / count] : fallbackColor);
}

function averageUnmaskedColor(source: DecodedImage, mask: DecodedImage): [number, number, number] {
  let red = 0;
  let green = 0;
  let blue = 0;
  let count = 0;

  for (let index = 0; index < source.width * source.height; index += 1) {
    const offset = index * 4;
    const alpha = mask.data[offset + 3] / 255;
    const luma = (mask.data[offset] * 0.299 + mask.data[offset + 1] * 0.587 + mask.data[offset + 2] * 0.114) / 255;
    if (alpha * luma > MASK_ACTIVE_THRESHOLD) {
      continue;
    }
    red += source.data[offset];
    green += source.data[offset + 1];
    blue += source.data[offset + 2];
    count += 1;
  }

  if (count === 0) {
    return [255, 255, 255];
  }
  return [red / count, green / count, blue / count];
}

function writeColor(output: Buffer, index: number, color: [number, number, number]): void {
  const offset = index * 4;
  output[offset] = Math.round(color[0]);
  output[offset + 1] = Math.round(color[1]);
  output[offset + 2] = Math.round(color[2]);
}

function rgbLuma(red: number, green: number, blue: number): number {
  return red * 0.299 + green * 0.587 + blue * 0.114;
}

function rgbChroma(red: number, green: number, blue: number): number {
  return Math.max(red, green, blue) - Math.min(red, green, blue);
}

function rgbDistance(a: [number, number, number], b: [number, number, number]): number {
  const dr = a[0] - b[0];
  const dg = a[1] - b[1];
  const db = a[2] - b[2];
  return Math.sqrt(dr * dr + dg * dg + db * db);
}

function clamp01(value: number): number {
  return Math.max(0, Math.min(1, value));
}

async function decodeRgba(dataUrl: string, width?: number, height?: number): Promise<DecodedImage> {
  const image = sharp(dataUrlToBuffer(dataUrl)).ensureAlpha();
  const pipeline = width && height ? image.resize(width, height, { fit: "fill" }) : image;
  const { data, info } = await pipeline.raw().toBuffer({ resolveWithObject: true });
  return {
    data,
    width: info.width,
    height: info.height
  };
}

async function rgbaToPngDataUrl(data: Buffer, width: number, height: number): Promise<string> {
  const buffer = await sharp(data, {
    raw: {
      width,
      height,
      channels: 4
    }
  }).png().toBuffer();
  return `data:image/png;base64,${buffer.toString("base64")}`;
}

function dataUrlToBuffer(dataUrl: string): Buffer {
  const match = /^data:image\/(?:png|jpeg|jpg|webp);base64,([A-Za-z0-9+/=]+)$/u.exec(dataUrl);
  if (!match) {
    throw new Error("이미지 데이터 URL이 올바르지 않습니다.");
  }
  return Buffer.from(match[1], "base64");
}
