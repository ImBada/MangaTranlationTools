import type { MangaPage, TranslationBlock } from "../../../shared/types";
import { resolveBlockRotationDeg } from "../../../shared/geometry";
import {
  hexToRgba,
  buildOverlayCanvasFont,
  DEFAULT_OVERLAY_TEXT_DECORATION,
  resolveScreentoneDotRadiusPx,
  resolveScreentoneTileSizePx,
  resolveBlockTextLayout,
  resolveWrappedTextLines
} from "./overlayLayout";

export type RenderLayerVisibility = {
  image: boolean;
  inpaint: boolean;
  inpaintResult: boolean;
  inpaintMask: boolean;
  overlay: boolean;
};

export type RenderLayerOpacity = {
  image: number;
  inpaint: number;
  inpaintResult: number;
  inpaintMask: number;
  overlay: number;
};

export type RenderPageOptions = {
  layerVisibility: RenderLayerVisibility;
  layerOpacity: RenderLayerOpacity;
  activeLayer: "output" | "image" | "inpaint" | "inpaintResult" | "inpaintMask" | "overlay";
};

export type OverlayRenderOptions = {
  renderSize: {
    width: number;
    height: number;
  };
  editingEnabled: boolean;
};

const CANVAS_TEXT_RENDER_FONT_SIZE_RATIO = 0.985;
const OVERLAY_BLOCK_BORDER_PX = 1;
const CANVAS_TEXT_RENDER_Y_OFFSET_RATIO = 0.04;
const DEFAULT_TEXT_OUTLINE_COLOR = "#000000";

export async function renderPageToPngDataUrl(page: MangaPage, options: RenderPageOptions): Promise<string> {
  const pageSize = { width: page.width, height: page.height };
  const canvas = document.createElement("canvas");
  canvas.width = page.width;
  canvas.height = page.height;
  const context = canvas.getContext("2d");
  if (!context) {
    throw new Error("페이지 렌더 캔버스를 만들지 못했습니다.");
  }

  if (options.layerVisibility.image) {
    const sourceImage = await loadImage(page.dataUrl);
    drawImageLayer(context, sourceImage, page.width, page.height, options.layerOpacity.image);
  }

  if (options.layerVisibility.inpaint) {
    const inpaintMaskDataUrl = page.inpaintMaskDataUrl ?? page.inpaintLayerDataUrl;
    if (options.layerVisibility.inpaintResult && page.inpaintResultDataUrl) {
      const inpaintResultLayer = await loadImage(page.inpaintResultDataUrl);
      if (options.activeLayer === "inpaintResult") {
        drawImageLayer(context, inpaintResultLayer, page.width, page.height, options.layerOpacity.inpaint * options.layerOpacity.inpaintResult);
      } else {
        const inpaintMaskLayer = inpaintMaskDataUrl ? await loadImage(inpaintMaskDataUrl) : null;
        drawImageLayerMasked(
          context,
          inpaintResultLayer,
          inpaintMaskLayer,
          page.width,
          page.height,
          options.layerOpacity.inpaint * options.layerOpacity.inpaintResult
        );
      }
    }
    if (options.layerVisibility.inpaintMask && inpaintMaskDataUrl) {
      const inpaintMaskLayer = await loadImage(inpaintMaskDataUrl);
      drawImageLayer(context, inpaintMaskLayer, page.width, page.height, options.layerOpacity.inpaint * options.layerOpacity.inpaintMask);
    }
  }

  if (options.layerVisibility.overlay) {
    context.save();
    context.globalAlpha = Math.max(0, Math.min(1, options.layerOpacity.overlay));
    drawOverlayBlocks(context, page, {
      renderSize: pageSize,
      editingEnabled: options.activeLayer === "overlay"
    });
    context.restore();
  }

  return canvas.toDataURL("image/png");
}

export function drawOverlayBlocks(context: CanvasRenderingContext2D, page: MangaPage, options: OverlayRenderOptions): void {
  for (const block of page.blocks) {
    if (block.renderDirection === "hidden") {
      continue;
    }
    drawRenderedBlock(context, page, block, options);
  }
}

function drawImageLayer(
  context: CanvasRenderingContext2D,
  image: CanvasImageSource,
  width: number,
  height: number,
  opacity: number
): void {
  context.save();
  context.globalAlpha = Math.max(0, Math.min(1, opacity));
  context.drawImage(image, 0, 0, width, height);
  context.restore();
}

function drawImageLayerMasked(
  context: CanvasRenderingContext2D,
  image: HTMLImageElement,
  mask: HTMLImageElement | null,
  width: number,
  height: number,
  opacity: number
): void {
  if (!mask) {
    drawImageLayer(context, image, width, height, opacity);
    return;
  }

  const layer = document.createElement("canvas");
  layer.width = width;
  layer.height = height;
  const layerContext = layer.getContext("2d");
  if (!layerContext) {
    return;
  }

  layerContext.drawImage(image, 0, 0, width, height);
  layerContext.globalCompositeOperation = "destination-in";
  layerContext.drawImage(mask, 0, 0, width, height);
  drawImageLayer(context, layer, width, height, opacity);
}

function drawRenderedBlock(
  context: CanvasRenderingContext2D,
  page: MangaPage,
  block: TranslationBlock,
  options: OverlayRenderOptions
): void {
  const text = block.translatedText || block.sourceText || "...";
  const pageSize = { width: page.width, height: page.height };
  const layout = resolveBlockTextLayout(block, text, pageSize, options.renderSize);
  const renderFontSizePx = layout.fontSizePx * CANVAS_TEXT_RENDER_FONT_SIZE_RATIO;
  const yOffset = renderFontSizePx * CANVAS_TEXT_RENDER_Y_OFFSET_RATIO;
  const left = layout.rect.left + OVERLAY_BLOCK_BORDER_PX + layout.paddingPx;
  const top = layout.rect.top + OVERLAY_BLOCK_BORDER_PX + layout.paddingPx + yOffset;
  const rotationDeg = resolveBlockRotationDeg(block);
  const centerX = layout.rect.left + layout.rect.width / 2;
  const centerY = layout.rect.top + layout.rect.height / 2;

  context.save();
  if (rotationDeg !== 0) {
    context.translate(centerX, centerY);
    context.rotate((rotationDeg * Math.PI) / 180);
    context.translate(-centerX, -centerY);
  }
  if (options.editingEnabled) {
    context.fillStyle = hexToRgba(block.backgroundColor, block.opacity);
    context.fillRect(layout.rect.left, layout.rect.top, layout.rect.width, layout.rect.height);
  }

  context.fillStyle = block.textColor;
  context.lineJoin = "round";
  context.font = buildOverlayCanvasFont(renderFontSizePx, block);
  context.textBaseline = "top";

  if (block.renderDirection === "vertical") {
    drawVerticalRenderedText(context, block, text, left, top, layout.fitInnerWidth, layout.fitInnerHeight, renderFontSizePx, block.lineHeight);
  } else {
    drawHorizontalRenderedText(context, block, text, left, top, layout.innerWidth, layout.innerHeight, layout.fitInnerWidth, renderFontSizePx);
  }

  context.restore();
}

function drawHorizontalRenderedText(
  context: CanvasRenderingContext2D,
  block: TranslationBlock,
  text: string,
  left: number,
  top: number,
  innerWidth: number,
  innerHeight: number,
  fitInnerWidth: number,
  fontSize: number
): void {
  const lines = resolveWrappedTextLines(block, text, fontSize, fitInnerWidth);
  const lineHeightPx = fontSize * block.lineHeight;
  const totalHeight = lines.length * lineHeightPx;
  const startY = top + Math.max(0, (innerHeight - totalHeight) / 2) + Math.max(0, (lineHeightPx - fontSize) / 2);
  const x =
    block.textAlign === "left"
      ? left
      : block.textAlign === "right"
        ? left + innerWidth
        : left + innerWidth / 2;

  context.textAlign = block.textAlign;
  for (const [index, line] of lines.entries()) {
    const y = startY + index * lineHeightPx;
    drawFilledText(context, block, line, x, y, fontSize);
    drawTextDecoration(context, block, line, x, y, fontSize);
  }
}

function drawVerticalRenderedText(
  context: CanvasRenderingContext2D,
  block: TranslationBlock,
  text: string,
  left: number,
  top: number,
  innerWidth: number,
  innerHeight: number,
  fontSize: number,
  lineHeight: number
): void {
  const chars = [...text.replace(/\s+/g, "")];
  const lineHeightPx = fontSize * lineHeight;
  const totalHeight = chars.length * lineHeightPx;
  const startY = top + Math.max(0, (innerHeight - totalHeight) / 2);
  const x = left + innerWidth / 2;
  context.textAlign = "center";
  for (const [index, char] of chars.entries()) {
    const y = startY + index * lineHeightPx;
    drawFilledText(context, block, char, x, y, fontSize);
    drawTextDecoration(context, block, char, x, y, fontSize);
  }
}

function drawTextDecoration(context: CanvasRenderingContext2D, block: TranslationBlock, text: string, x: number, y: number, fontSize: number): void {
  if ((block.textDecoration ?? DEFAULT_OVERLAY_TEXT_DECORATION) !== "underline") {
    return;
  }

  const width = context.measureText(text).width;
  const startX = context.textAlign === "right" ? x - width : context.textAlign === "center" ? x - width / 2 : x;
  const underlineY = y + fontSize * 1.05;
  context.save();
  context.strokeStyle = block.textColor;
  context.lineWidth = Math.max(1, fontSize * 0.06);
  context.beginPath();
  context.moveTo(startX, underlineY);
  context.lineTo(startX + width, underlineY);
  context.stroke();
  context.restore();
}

function strokeTextOutlines(context: CanvasRenderingContext2D, block: TranslationBlock, text: string, x: number, y: number, fontSize: number): void {
  const outlineWidthPx = resolveRenderedOutlineWidthPx(block, fontSize);
  const secondaryOutlineWidthPx = resolveRenderedSecondaryOutlineWidthPx(block, fontSize);

  if (secondaryOutlineWidthPx > 0) {
    context.save();
    context.strokeStyle = block.secondaryOutlineColor ?? "#ffffff";
    context.lineWidth = outlineWidthPx + secondaryOutlineWidthPx * 2;
    context.strokeText(text, x, y);
    context.restore();
  }

  if (outlineWidthPx > 0) {
    context.save();
    context.strokeStyle = block.outlineColor ?? DEFAULT_TEXT_OUTLINE_COLOR;
    context.lineWidth = outlineWidthPx;
    context.strokeText(text, x, y);
    context.restore();
  }
}

function drawOutlinedText(context: CanvasRenderingContext2D, block: TranslationBlock, text: string, x: number, y: number, fontSize: number): void {
  strokeTextOutlines(context, block, text, x, y, fontSize);
  context.fillText(text, x, y);
}

function drawFilledText(context: CanvasRenderingContext2D, block: TranslationBlock, text: string, x: number, y: number, fontSize: number): void {
  if (!(block.screentoneFillEnabled ?? false)) {
    drawOutlinedText(context, block, text, x, y, fontSize);
    return;
  }

  strokeTextOutlines(context, block, text, x, y, fontSize);

  context.save();
  context.fillStyle = "#ffffff";
  context.fillText(text, x, y);
  const pattern = createScreentonePattern(
    context,
    block.textColor,
    block.screentoneFillIntensity,
    block.screentoneFillDensity,
    block.screentoneFillAntialias,
    fontSize
  );
  context.fillStyle = pattern ?? block.textColor;
  context.fillText(text, x, y);
  context.restore();
}

function createScreentonePattern(
  context: CanvasRenderingContext2D,
  textColor: string,
  intensity: number | undefined,
  density: number | undefined,
  antialias: boolean | undefined,
  fontSize: number
): CanvasPattern | null {
  const tileSizePx = resolveScreentoneTileSizePx(fontSize, density);
  const dotRadiusPx = resolveScreentoneDotRadiusPx(tileSizePx, intensity);
  const tileCanvasSizePx = Math.max(3, Math.ceil(tileSizePx));
  const tile = document.createElement("canvas");
  tile.width = tileCanvasSizePx;
  tile.height = tileCanvasSizePx;
  const tileContext = tile.getContext("2d");
  if (!tileContext) {
    return null;
  }

  if (antialias === false) {
    drawHardScreentoneDot(tileContext, textColor, tileCanvasSizePx, tileSizePx, dotRadiusPx);
  } else {
    tileContext.fillStyle = "#ffffff";
    tileContext.fillRect(0, 0, tileCanvasSizePx, tileCanvasSizePx);
    tileContext.fillStyle = textColor;
    tileContext.beginPath();
    tileContext.arc(tileSizePx / 2, tileSizePx / 2, dotRadiusPx, 0, Math.PI * 2);
    tileContext.fill();
  }
  return context.createPattern(tile, "repeat");
}

function drawHardScreentoneDot(
  context: CanvasRenderingContext2D,
  textColor: string,
  tileCanvasSizePx: number,
  tileSizePx: number,
  dotRadiusPx: number
): void {
  const image = context.createImageData(tileCanvasSizePx, tileCanvasSizePx);
  const color = parseHexColor(textColor);
  const center = tileSizePx / 2;
  const radiusSquared = dotRadiusPx * dotRadiusPx;

  for (let y = 0; y < tileCanvasSizePx; y += 1) {
    for (let x = 0; x < tileCanvasSizePx; x += 1) {
      const index = (y * tileCanvasSizePx + x) * 4;
      const dx = x + 0.5 - center;
      const dy = y + 0.5 - center;
      const inside = x + 0.5 <= tileSizePx && y + 0.5 <= tileSizePx && dx * dx + dy * dy <= radiusSquared;
      image.data[index] = inside ? color.r : 255;
      image.data[index + 1] = inside ? color.g : 255;
      image.data[index + 2] = inside ? color.b : 255;
      image.data[index + 3] = 255;
    }
  }

  context.putImageData(image, 0, 0);
}

function parseHexColor(hex: string): { r: number; g: number; b: number } {
  const normalized = hex.replace("#", "");
  return {
    r: Number.parseInt(normalized.slice(0, 2), 16),
    g: Number.parseInt(normalized.slice(2, 4), 16),
    b: Number.parseInt(normalized.slice(4, 6), 16)
  };
}

function resolveRenderedOutlineWidthPx(block: TranslationBlock, fontSize: number): number {
  const outlineWidthPx = Math.max(0, block.outlineWidthPx ?? 0);
  return Math.min(outlineWidthPx, fontSize * 0.35);
}

function resolveRenderedSecondaryOutlineWidthPx(block: TranslationBlock, fontSize: number): number {
  const outlineWidthPx = Math.max(0, block.secondaryOutlineWidthPx ?? 0);
  return Math.min(outlineWidthPx, fontSize * 0.35);
}

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("이미지를 불러오지 못했습니다."));
    image.src = src;
  });
}
