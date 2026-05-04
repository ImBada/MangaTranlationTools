import type { TextPosition, TranslationBlock } from "../../../shared/types";
import { bboxToPixels, clamp, clampTextPaddingPx, resolveBlockRenderBbox } from "../../../shared/geometry";
import {
  DEFAULT_OVERLAY_FONT_FAMILY,
  DEFAULT_OVERLAY_FONT_STYLE,
  DEFAULT_OVERLAY_FONT_WEIGHT,
  DEFAULT_OVERLAY_TEXT_DECORATION
} from "../../../shared/fontPresets";

export {
  DEFAULT_OVERLAY_FONT_FAMILY,
  DEFAULT_OVERLAY_FONT_STYLE,
  DEFAULT_OVERLAY_FONT_WEIGHT,
  DEFAULT_OVERLAY_TEXT_DECORATION
};

const MIN_FONT_SIZE_PX = 2;
const MAX_BLOCK_PADDING_PX = 14;
const MIN_INNER_SIZE_PX = 1;
const BLOCK_BORDER_PX = 1;
const TEXT_FIT_SAFETY_PX = 6;
const TEXT_MEASURE_GUARD_PX = TEXT_FIT_SAFETY_PX + 4;
export const DEFAULT_SCREENTONE_FILL_INTENSITY = 0.55;
export const DEFAULT_SCREENTONE_FILL_DENSITY = 0.55;
export const DEFAULT_OVERLAY_TEXT_POSITION: TextPosition = "center";

let measureCanvas: HTMLCanvasElement | null = null;

export type ViewportSize = {
  width: number;
  height: number;
};

export type PixelRect = {
  left: number;
  top: number;
  width: number;
  height: number;
};

export type BlockTextLayout = {
  rect: PixelRect;
  paddingPx: number;
  innerWidth: number;
  innerHeight: number;
  fitInnerWidth: number;
  fitInnerHeight: number;
  fontSizePx: number;
  overflow: boolean;
};

export type TextPositionFactors = {
  x: number;
  y: number;
};

export function resolveTextPosition(position: TextPosition | undefined): TextPosition {
  return position ?? DEFAULT_OVERLAY_TEXT_POSITION;
}

export function resolveTextPositionFactors(position: TextPosition | undefined): TextPositionFactors {
  switch (resolveTextPosition(position)) {
    case "top-left":
      return { x: 0, y: 0 };
    case "top":
      return { x: 0.5, y: 0 };
    case "top-right":
      return { x: 1, y: 0 };
    case "left":
      return { x: 0, y: 0.5 };
    case "right":
      return { x: 1, y: 0.5 };
    case "bottom-left":
      return { x: 0, y: 1 };
    case "bottom":
      return { x: 0.5, y: 1 };
    case "bottom-right":
      return { x: 1, y: 1 };
    case "center":
    default:
      return { x: 0.5, y: 0.5 };
  }
}

export function resolveOverlayFontSizePx(block: TranslationBlock, text: string, pageSize: ViewportSize, stageSize: ViewportSize): number {
  return resolveBlockTextLayout(block, text, pageSize, stageSize).fontSizePx;
}

export function resolveWrappedTextLines(
  block: Pick<TranslationBlock, "fontFamily" | "fontWeight" | "fontStyle">,
  text: string,
  fontSize: number,
  maxWidth: number
): string[] {
  const context = getMeasureContext();
  context.font = buildFont(fontSize, block);
  return wrapTextToWidth(context, text, maxWidth);
}

function resolveAutoBlockPaddingPx(rect: PixelRect): number {
  const shortestSide = Math.min(rect.width, rect.height);
  if (shortestSide <= 48) {
    return 0;
  }
  if (shortestSide <= 72) {
    return 1;
  }
  if (shortestSide <= 96) {
    return 2;
  }

  return Math.round(clamp(shortestSide * 0.06, 3, MAX_BLOCK_PADDING_PX));
}

export function resolveBlockTextLayout(
  block: TranslationBlock,
  text: string,
  pageSize: ViewportSize,
  stageSize: ViewportSize
): BlockTextLayout {
  const rect = resolveBlockRectPx(block, pageSize, stageSize);
  const paddingPx = resolveBlockPaddingPx(block, rect, pageSize, stageSize);
  const borderInsetPx = BLOCK_BORDER_PX * 2;
  const innerWidth = Math.max(MIN_INNER_SIZE_PX, rect.width - paddingPx * 2 - borderInsetPx);
  const innerHeight = Math.max(MIN_INNER_SIZE_PX, rect.height - paddingPx * 2 - borderInsetPx);
  const fitInnerWidth = Math.max(MIN_INNER_SIZE_PX, innerWidth - TEXT_MEASURE_GUARD_PX * 2);
  const fitInnerHeight = Math.max(MIN_INNER_SIZE_PX, innerHeight - TEXT_MEASURE_GUARD_PX * 2);
  const scale = Math.min(stageSize.width / Math.max(1, pageSize.width), stageSize.height / Math.max(1, pageSize.height));
  const preferredFontSize = Math.max(MIN_FONT_SIZE_PX, Math.floor(block.fontSizePx * scale));
  const maxFontSize = resolveAutoFitUpperBound(block, preferredFontSize, fitInnerWidth, fitInnerHeight);
  const fontSizePx = resolveTextFontSizePx(block, text, maxFontSize, fitInnerWidth, fitInnerHeight);

  return {
    rect,
    paddingPx,
    innerWidth,
    innerHeight,
    fitInnerWidth,
    fitInnerHeight,
    fontSizePx,
    overflow: text.trim() ? !doesTextFit(block, text, fontSizePx, fitInnerWidth, fitInnerHeight) : false
  };
}

export function resolveBlockRectPx(block: TranslationBlock, pageSize: ViewportSize, stageSize: ViewportSize): PixelRect {
  const pixelRect = bboxToPixels(resolveBlockRenderBbox(block), pageSize.width, pageSize.height);
  const scaleX = stageSize.width / Math.max(1, pageSize.width);
  const scaleY = stageSize.height / Math.max(1, pageSize.height);

  return {
    left: pixelRect.x * scaleX,
    top: pixelRect.y * scaleY,
    width: pixelRect.w * scaleX,
    height: pixelRect.h * scaleY
  };
}

export function resolveBlockPaddingPx(
  rect: PixelRect,
  pageSize?: ViewportSize,
  stageSize?: ViewportSize
): number;
export function resolveBlockPaddingPx(
  block: Pick<TranslationBlock, "textPaddingPx">,
  rect: PixelRect,
  pageSize: ViewportSize,
  stageSize: ViewportSize
): number;
export function resolveBlockPaddingPx(
  blockOrRect: Pick<TranslationBlock, "textPaddingPx"> | PixelRect,
  rectOrPageSize?: PixelRect | ViewportSize,
  pageSize?: ViewportSize,
  stageSize?: ViewportSize
): number {
  if ("textPaddingPx" in blockOrRect && rectOrPageSize && pageSize && stageSize) {
    const block = blockOrRect;
    if (typeof block.textPaddingPx === "number" && Number.isFinite(block.textPaddingPx)) {
      const scale = Math.min(stageSize.width / Math.max(1, pageSize.width), stageSize.height / Math.max(1, pageSize.height));
      return clampTextPaddingPx(block.textPaddingPx) * scale;
    }
    return resolveAutoBlockPaddingPx(rectOrPageSize as PixelRect);
  }

  return resolveAutoBlockPaddingPx(blockOrRect as PixelRect);
}

export function hexToRgba(hex: string, alpha: number): string {
  const value = hex.replace("#", "");
  const r = Number.parseInt(value.slice(0, 2), 16);
  const g = Number.parseInt(value.slice(2, 4), 16);
  const b = Number.parseInt(value.slice(4, 6), 16);
  return `rgba(${r}, ${g}, ${b}, ${clamp(alpha, 0, 1)})`;
}

export function normalizeScreentoneFillIntensity(value: number | undefined): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_SCREENTONE_FILL_INTENSITY;
  }
  return clamp(value ?? DEFAULT_SCREENTONE_FILL_INTENSITY, 0.05, 1);
}

export function normalizeScreentoneFillDensity(value: number | undefined): number {
  if (!Number.isFinite(value)) {
    return DEFAULT_SCREENTONE_FILL_DENSITY;
  }
  return clamp(value ?? DEFAULT_SCREENTONE_FILL_DENSITY, 0.05, 1);
}

export function resolveScreentoneTileSizePx(fontSizePx: number, density: number | undefined): number {
  const normalizedDensity = normalizeScreentoneFillDensity(density);
  const minRatio = 0.022;
  const maxRatio = 0.12;
  const ratio = maxRatio - normalizedDensity * (maxRatio - minRatio);
  return Math.max(3, fontSizePx * ratio);
}

export function resolveScreentoneDotRadiusPx(tileSizePx: number, intensity: number | undefined): number {
  const normalizedIntensity = normalizeScreentoneFillIntensity(intensity);
  return clamp(tileSizePx * (0.08 + normalizedIntensity * 0.4), 0.2, tileSizePx * 0.48);
}

export function buildScreentoneFillCssBackground(
  textColor: string,
  intensity: number | undefined,
  density: number | undefined,
  antialias: boolean | undefined,
  fontSizePx: number
): string {
  const normalizedIntensity = normalizeScreentoneFillIntensity(intensity);
  const tileSizePx = resolveScreentoneTileSizePx(fontSizePx, density);
  const dotRadiusPx = resolveScreentoneDotRadiusPx(tileSizePx, normalizedIntensity);
  const edgeRadiusPx = antialias === false ? dotRadiusPx : dotRadiusPx + 0.45;
  return `radial-gradient(circle at 50% 50%, ${textColor} 0 ${dotRadiusPx}px, transparent ${edgeRadiusPx}px), #ffffff`;
}

export function buildScreentoneFillCssSize(fontSizePx: number, density: number | undefined): string {
  const tileSizePx = resolveScreentoneTileSizePx(fontSizePx, density);
  return `${tileSizePx}px ${tileSizePx}px`;
}

function resolveTextFontSizePx(
  block: TranslationBlock,
  text: string,
  maxFontSize: number,
  innerWidth: number,
  innerHeight: number
): number {
  const capped = Math.max(MIN_FONT_SIZE_PX, Math.floor(maxFontSize));
  if (!(block.autoFitText ?? true) || !text.trim()) {
    return capped;
  }

  let low = MIN_FONT_SIZE_PX;
  let high = capped;
  let best = MIN_FONT_SIZE_PX;
  while (low <= high) {
    const mid = Math.floor((low + high) / 2);
    if (doesTextFit(block, text, mid, innerWidth, innerHeight)) {
      best = mid;
      low = mid + 1;
    } else {
      high = mid - 1;
    }
  }
  return Math.min(best, capped);
}

function doesTextFit(block: TranslationBlock, text: string, fontSize: number, innerWidth: number, innerHeight: number): boolean {
  if (block.renderDirection === "vertical") {
    return measureVerticalText(text, fontSize, innerWidth, innerHeight, fontSize * block.lineHeight).fits;
  }

  const context = getMeasureContext();
  context.font = buildFont(fontSize, block);
  const measured = measureWrappedText(context, text, innerWidth, fontSize * block.lineHeight);
  return measured.totalHeight <= innerHeight && measured.maxLineWidth <= innerWidth;
}

type TextWrapSegment = {
  text: string;
  separator: "" | " ";
};

function wrapTextToWidth(context: CanvasRenderingContext2D, text: string, maxWidth: number): string[] {
  const paragraphs = text.replace(/\r/g, "").split("\n");
  const lines: string[] = [];

  for (const paragraph of paragraphs) {
    const normalized = paragraph.replace(/\s+/g, " ").trim();
    if (!normalized) {
      lines.push("");
      continue;
    }

    const segments = splitTextWrapSegments(normalized);
    let current = segments[0]?.text ?? "";
    for (const segment of segments.slice(1)) {
      const candidate = `${current}${segment.separator}${segment.text}`;
      if (context.measureText(candidate).width <= maxWidth) {
        current = candidate;
        continue;
      }

      lines.push(current);
      current = segment.text;
    }

    if (current) {
      lines.push(current);
    }
  }

  return lines.length > 0 ? lines : [text];
}

function splitTextWrapSegments(text: string): TextWrapSegment[] {
  const segments: TextWrapSegment[] = [];

  for (const word of text.split(" ")) {
    const parts = splitWordAtSoftWrapMarks(word);
    for (const [index, part] of parts.entries()) {
      segments.push({
        text: part,
        separator: segments.length > 0 && index === 0 ? " " : ""
      });
    }
  }

  return segments;
}

function splitWordAtSoftWrapMarks(word: string): string[] {
  const parts = word.split(/((?:[…⋯]+|\.{3,}|[~～〜]+))/u).filter(Boolean);
  return parts.length > 0 ? parts : [word];
}

function measureWrappedText(
  context: CanvasRenderingContext2D,
  text: string,
  maxWidth: number,
  lineHeight: number
) : { lines: string[]; totalHeight: number; maxLineWidth: number } {
  const lines = wrapTextToWidth(context, text, maxWidth);
  return {
    lines,
    totalHeight: lines.length * lineHeight,
    maxLineWidth: lines.reduce((widest, line) => Math.max(widest, context.measureText(line).width), 0)
  };
}

function resolveAutoFitUpperBound(block: TranslationBlock, preferredFontSize: number, innerWidth: number, innerHeight: number): number {
  if (!(block.autoFitText ?? true)) {
    return preferredFontSize;
  }

  return Math.max(MIN_FONT_SIZE_PX, preferredFontSize, innerWidth, innerHeight);
}

function measureVerticalText(
  text: string,
  fontSize: number,
  maxWidth: number,
  maxHeight: number,
  lineHeight: number
): { columnCount: number; fits: boolean } {
  const compact = text.replace(/\r/g, "").replace(/\s+/g, "");
  if (!compact) {
    return { columnCount: 0, fits: true };
  }

  const charsPerColumn = Math.max(1, Math.floor(maxHeight / Math.max(fontSize, lineHeight)));
  const columnCount = Math.max(1, Math.ceil(compact.length / charsPerColumn));
  const estimatedColumnWidth = fontSize * 1.15;
  return {
    columnCount,
    fits: columnCount * estimatedColumnWidth <= maxWidth
  };
}

function getMeasureContext(): CanvasRenderingContext2D {
  if (typeof document === "undefined") {
    throw new Error("Document is not available for canvas text measurement");
  }

  measureCanvas ??= document.createElement("canvas");
  const context = measureCanvas.getContext("2d");
  if (!context) {
    throw new Error("Canvas context is not available");
  }
  return context;
}

export function buildOverlayCanvasFont(
  fontSize: number,
  block: Pick<TranslationBlock, "fontFamily" | "fontWeight" | "fontStyle">
): string {
  return buildFont(fontSize, block);
}

function buildFont(fontSize: number, block: Pick<TranslationBlock, "fontFamily" | "fontWeight" | "fontStyle">): string {
  return `${block.fontStyle ?? DEFAULT_OVERLAY_FONT_STYLE} ${block.fontWeight ?? DEFAULT_OVERLAY_FONT_WEIGHT} ${fontSize}px ${block.fontFamily ?? DEFAULT_OVERLAY_FONT_FAMILY}`;
}
