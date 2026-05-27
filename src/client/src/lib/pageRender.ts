import type { MangaPage, TranslationBlock } from "../../../shared/types";
import { resolveBlockRotationDeg } from "../../../shared/geometry";
import {
  hasEnabledTranslationBlockGroupEffects,
  resolveTranslationBlockGroupDropShadowEffect,
  resolveTranslationBlockGroupDropShadowSettings,
  type TranslationBlockGroupDropShadowSettings
} from "./blockGroupEffects";
import { resolveTranslationBlockListItems } from "./blockGroups";
import {
  hexToRgba,
  buildOverlayCanvasFont,
  DEFAULT_OVERLAY_TEXT_DECORATION,
  DEFAULT_FONT_BACKGROUND_COLOR,
  resolveFontBackgroundScale,
  resolveScaledTextBackgroundRect,
  normalizeFontBackgroundOpacity,
  normalizeFontBackgroundRadiusPx,
  resolveFontBackgroundEnabled,
  resolveScreentoneDotRadiusPx,
  resolveScreentoneTileSizePx,
  resolveBlockTextLayout,
  measureTextWidthWithLetterSpacing,
  resolveSyntheticBoldStrokeWidthPx,
  resolveSyntheticItalicSkewX,
  resolveCharacterFontRuns,
  resolveSecondaryOutlineEnabled,
  resolveTextLetterSpacingPx,
  resolveTextPositionFactors,
  resolveWrappedTextLines,
  DEFAULT_FONT_RENDERING_MODE,
  type FontWeightAvailability
} from "./overlayLayout";
import {
  hasActiveFontFilterEffects,
  resolveFontFilterEffectMarginPx
} from "./fontFilterEffectMetadata";
import {
  applyFontFilterEffectsToCanvas,
  type FontFilterEffectBounds
} from "./fontFilterEffects";
import { hardenCanvasTextPixels } from "./hardTextRendering";
import { applyInpaintMaskPixelsToLayer, drawInpaintMaskImageForPixelRead } from "./inpaintMaskImages";

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
  fontWeightAvailability?: readonly FontWeightAvailability[];
};

export type OverlayRenderOptions = {
  renderSize: {
    width: number;
    height: number;
  };
  editingEnabled: boolean;
  includedBlockIds?: ReadonlySet<string>;
  fontWeightAvailability?: readonly FontWeightAvailability[];
};

export type OverlayCanvasDirtyRect = {
  height: number;
  width: number;
  x: number;
  y: number;
};

const CANVAS_TEXT_RENDER_FONT_SIZE_RATIO = 0.985;
const OVERLAY_BLOCK_BORDER_PX = 1;
const CANVAS_TEXT_RENDER_Y_OFFSET_RATIO = 0.04;
const CENTERED_ELLIPSIS = "…";
const CENTERED_PERIOD_ELLIPSIS = "...";
const DEFAULT_TEXT_OUTLINE_COLOR = "#000000";
const CENTERED_ELLIPSIS_DOT_RADIUS_RATIO = 0.065;
const CENTERED_ELLIPSIS_DOT_SPACING_RATIO = 0.26;
type CanvasPaint = CanvasRenderingContext2D["strokeStyle"];

type TextRenderStyle = {
  fontWeightAvailability: readonly FontWeightAvailability[];
  syntheticBoldWidthPx: number;
  syntheticItalicSkewX: number;
};

type PositionedTextRunSegment = TextRunSegment & {
  advance: number;
  x: number;
};

type TextDrawItem = {
  fontSize: number;
  text: string;
  x: number;
  y: number;
};

type DrawTextOptions = {
  includeFontBackground?: boolean;
  includeGlyphs?: boolean;
};

type CenteredEllipsisDotGeometry = {
  centers: [number, number, number];
  centerY: number;
  radius: number;
};

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
      editingEnabled: options.activeLayer === "overlay",
      fontWeightAvailability: options.fontWeightAvailability
    });
    context.restore();
  }

  return canvas.toDataURL("image/png");
}

export function drawOverlayBlocks(context: CanvasRenderingContext2D, page: MangaPage, options: OverlayRenderOptions): void {
  drawOverlayBlockGroupEffects(context, page, options);
  for (const block of page.blocks) {
    if (
      block.renderDirection === "hidden" ||
      (options.includedBlockIds && !options.includedBlockIds.has(block.id))
    ) {
      continue;
    }
    drawRenderedBlock(context, page, block, options);
  }
}

function drawOverlayBlockGroupEffects(context: CanvasRenderingContext2D, page: MangaPage, options: OverlayRenderOptions): void {
  if (!page.blockGroups?.some(hasEnabledTranslationBlockGroupEffects)) {
    return;
  }

  for (const item of resolveTranslationBlockListItems(page)) {
    if (item.kind !== "group") {
      continue;
    }

    const dropShadowEffect = resolveTranslationBlockGroupDropShadowEffect(item.group);
    if (!dropShadowEffect?.enabled) {
      continue;
    }

    const blocks = item.blocks
      .map(({ block }) => block)
      .filter((block) =>
        block.renderDirection !== "hidden" &&
        (!options.includedBlockIds || options.includedBlockIds.has(block.id))
      );
    if (blocks.length === 0) {
      continue;
    }

    drawOverlayBlockGroupDropShadow(context, page, blocks, resolveTranslationBlockGroupDropShadowSettings(dropShadowEffect), options);
  }
}

function drawOverlayBlockGroupDropShadow(
  context: CanvasRenderingContext2D,
  page: MangaPage,
  blocks: readonly TranslationBlock[],
  settings: TranslationBlockGroupDropShadowSettings,
  options: OverlayRenderOptions
): void {
  if (settings.opacity <= 0) {
    return;
  }

  const groupCanvas = document.createElement("canvas");
  groupCanvas.width = page.width;
  groupCanvas.height = page.height;
  const groupContext = groupCanvas.getContext("2d");
  if (!groupContext) {
    return;
  }

  for (const block of blocks) {
    drawRenderedBlock(groupContext, page, block, {
      fontWeightAvailability: options.fontWeightAvailability,
      editingEnabled: false,
      renderSize: options.renderSize
    });
  }

  const shadowCanvas = document.createElement("canvas");
  shadowCanvas.width = page.width;
  shadowCanvas.height = page.height;
  const shadowContext = shadowCanvas.getContext("2d");
  if (!shadowContext) {
    return;
  }

  shadowContext.drawImage(groupCanvas, 0, 0);
  shadowContext.globalCompositeOperation = "source-in";
  shadowContext.fillStyle = hexToRgba(settings.color, settings.opacity);
  shadowContext.fillRect(0, 0, page.width, page.height);
  shadowContext.globalCompositeOperation = "source-over";

  context.save();
  context.filter = settings.blurPx > 0 ? `blur(${settings.blurPx}px)` : "none";
  context.drawImage(
    shadowCanvas,
    Math.cos((settings.angleDeg * Math.PI) / 180) * settings.distancePx,
    Math.sin((settings.angleDeg * Math.PI) / 180) * settings.distancePx
  );
  context.restore();
}

export function doesBlockIntersectCanvasRect(
  block: TranslationBlock,
  page: Pick<MangaPage, "height" | "width">,
  rect: OverlayCanvasDirtyRect
): boolean {
  const blockRect = resolveBlockCanvasDirtyRect(block, page);
  if (!blockRect) {
    return false;
  }
  return (
    blockRect.x < rect.x + rect.width &&
    blockRect.x + blockRect.width > rect.x &&
    blockRect.y < rect.y + rect.height &&
    blockRect.y + blockRect.height > rect.y
  );
}

export function resolveBlockCanvasDirtyRect(
  block: TranslationBlock,
  page: Pick<MangaPage, "height" | "width">,
  resolvedLayout?: ReturnType<typeof resolveBlockTextLayout>
): OverlayCanvasDirtyRect | null {
  const text = block.translatedText || block.sourceText || "...";
  const pageSize = { width: page.width, height: page.height };
  const layout = resolvedLayout ?? resolveBlockTextLayout(block, text, pageSize, pageSize);
  const outlinePx = Math.max(0, block.outlineWidthPx ?? 0);
  const secondaryOutlinePx = resolveSecondaryOutlineEnabled(block) ? Math.max(0, block.secondaryOutlineWidthPx ?? 0) : 0;
  const shadowDistancePx = Math.max(0, block.shadowDistancePx ?? 0);
  const shadowBlurPx = resolveBlockShadowBlurPx(block);
  const filterMarginPx = resolveFontFilterEffectMarginPx(block, layout.fontSizePx);
  const fontBackgroundScale = resolveFontBackgroundScale(block);
  const fontBackgroundMarginPx = resolveFontBackgroundEnabled(block) && normalizeFontBackgroundOpacity(block.fontBackgroundOpacity) > 0
    ? Math.max(0, Math.max(layout.fitInnerWidth, layout.fitInnerHeight) * (fontBackgroundScale - 1) / 2)
    : 0;
  const margin = Math.ceil(Math.max(
    24,
    fontBackgroundMarginPx,
    layout.fontSizePx * 0.45 + outlinePx * 3 + secondaryOutlinePx * 4 + shadowDistancePx + shadowBlurPx * 3 + filterMarginPx + 8
  ));
  const expanded = {
    x: layout.rect.left - margin,
    y: layout.rect.top - margin,
    width: layout.rect.width + margin * 2,
    height: layout.rect.height + margin * 2
  };
  const rotationDeg = resolveBlockRotationDeg(block);
  const rotated = rotationDeg === 0
    ? expanded
    : resolveRotatedRectBounds(expanded, {
        x: layout.rect.left + layout.rect.width / 2,
        y: layout.rect.top + layout.rect.height / 2
      }, rotationDeg);
  const x = Math.max(0, Math.floor(rotated.x));
  const y = Math.max(0, Math.floor(rotated.y));
  const right = Math.min(page.width, Math.ceil(rotated.x + rotated.width));
  const bottom = Math.min(page.height, Math.ceil(rotated.y + rotated.height));
  if (right <= x || bottom <= y) {
    return null;
  }
  return {
    x,
    y,
    width: right - x,
    height: bottom - y
  };
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
  applyInpaintMaskToLayer(layerContext, mask, width, height);
  drawImageLayer(context, layer, width, height, opacity);
}

function applyInpaintMaskToLayer(
  layerContext: CanvasRenderingContext2D,
  mask: HTMLImageElement,
  width: number,
  height: number
): void {
  const maskCanvas = document.createElement("canvas");
  maskCanvas.width = width;
  maskCanvas.height = height;
  const maskContext = maskCanvas.getContext("2d", { willReadFrequently: true });
  if (!maskContext) {
    layerContext.clearRect(0, 0, width, height);
    return;
  }

  drawInpaintMaskImageForPixelRead(maskContext, mask, width, height);
  const layerPixels = layerContext.getImageData(0, 0, width, height);
  const maskPixels = maskContext.getImageData(0, 0, width, height).data;
  applyInpaintMaskPixelsToLayer(layerPixels.data, maskPixels);
  layerContext.putImageData(layerPixels, 0, 0);
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

  const fontWeightAvailability = options.fontWeightAvailability ?? [];
  const textRenderStyle = resolveTextRenderStyle(block, renderFontSizePx, options);
  const drawText = (targetContext: CanvasRenderingContext2D, drawOptions: DrawTextOptions = {}): void => {
    const includeFontBackground = drawOptions.includeFontBackground ?? true;
    const includeGlyphs = drawOptions.includeGlyphs ?? true;
    targetContext.fillStyle = block.textColor;
    targetContext.lineJoin = "round";
    targetContext.font = buildOverlayCanvasFont(renderFontSizePx, block, fontWeightAvailability);
    targetContext.textBaseline = "top";

    if (block.renderDirection === "vertical") {
      drawVerticalRenderedText(
        targetContext,
        block,
        text,
        left,
        top,
        layout.fitInnerWidth,
        layout.fitInnerHeight,
        renderFontSizePx,
        block.lineHeight,
        textRenderStyle,
        { includeFontBackground, includeGlyphs }
      );
      return;
    }

    drawHorizontalRenderedText(
      targetContext,
      block,
      text,
      left,
      top,
      layout.innerWidth,
      layout.innerHeight,
      layout.fitInnerWidth,
      renderFontSizePx,
      textRenderStyle,
      fontWeightAvailability,
      { includeFontBackground, includeGlyphs }
    );
  };

  const hardRenderingEnabled = (block.fontRenderingMode ?? DEFAULT_FONT_RENDERING_MODE) === "hard";

  if (hasActiveFontFilterEffects(block)) {
    const filterBounds = resolveBlockFontFilterEffectBounds(block, page, layout, renderFontSizePx);
    const filterCanvasWidth = Math.max(1, Math.ceil(filterBounds.width));
    const filterCanvasHeight = Math.max(1, Math.ceil(filterBounds.height));
    const textCanvas = document.createElement("canvas");
    textCanvas.width = filterCanvasWidth;
    textCanvas.height = filterCanvasHeight;
    const textContext = textCanvas.getContext("2d", { willReadFrequently: true });
    if (textContext) {
      textContext.save();
      textContext.translate(-filterBounds.x, -filterBounds.y);
      if (rotationDeg !== 0) {
        textContext.translate(centerX, centerY);
        textContext.rotate((rotationDeg * Math.PI) / 180);
        textContext.translate(-centerX, -centerY);
      }
      drawText(context, { includeFontBackground: true, includeGlyphs: false });
      drawText(textContext, { includeFontBackground: false, includeGlyphs: true });
      textContext.restore();
      applyFontFilterEffectsToCanvas(
        textContext,
        filterCanvasWidth,
        filterCanvasHeight,
        {
          height: filterCanvasHeight,
          width: filterCanvasWidth,
          x: 0,
          y: 0
        },
        block,
        renderFontSizePx
      );
      if (hardRenderingEnabled) {
        hardenCanvasTextPixels(textContext, filterCanvasWidth, filterCanvasHeight, block);
      }
      context.restore();
      context.save();
      context.imageSmoothingEnabled = !hardRenderingEnabled;
      context.drawImage(textCanvas, filterBounds.x, filterBounds.y);
      context.restore();
      return;
    }
  }

  if (hardRenderingEnabled) {
    const hardTextBounds = resolveBlockHardTextBounds(block, page, layout, renderFontSizePx);
    const textCanvas = document.createElement("canvas");
    textCanvas.width = hardTextBounds.width;
    textCanvas.height = hardTextBounds.height;
    const textContext = textCanvas.getContext("2d", { willReadFrequently: true });
    if (textContext) {
      textContext.translate(-hardTextBounds.x, -hardTextBounds.y);
      if (rotationDeg !== 0) {
        textContext.translate(centerX, centerY);
        textContext.rotate((rotationDeg * Math.PI) / 180);
        textContext.translate(-centerX, -centerY);
      }
      drawText(context, { includeFontBackground: true, includeGlyphs: false });
      drawText(textContext, { includeFontBackground: false, includeGlyphs: true });
      textContext.setTransform(1, 0, 0, 1, 0, 0);
      hardenCanvasTextPixels(textContext, hardTextBounds.width, hardTextBounds.height, block);
      context.restore();
      context.save();
      context.imageSmoothingEnabled = false;
      context.drawImage(textCanvas, hardTextBounds.x, hardTextBounds.y);
      context.restore();
      return;
    }
  }

  drawText(context);
  context.restore();
}

function fillRoundRect(
  context: CanvasRenderingContext2D,
  left: number,
  top: number,
  width: number,
  height: number,
  radiusPx: number
): void {
  const radius = Math.max(0, Math.min(radiusPx, width / 2, height / 2));
  if (radius <= 0) {
    context.fillRect(left, top, width, height);
    return;
  }

  context.beginPath();
  context.moveTo(left + radius, top);
  context.lineTo(left + width - radius, top);
  context.quadraticCurveTo(left + width, top, left + width, top + radius);
  context.lineTo(left + width, top + height - radius);
  context.quadraticCurveTo(left + width, top + height, left + width - radius, top + height);
  context.lineTo(left + radius, top + height);
  context.quadraticCurveTo(left, top + height, left, top + height - radius);
  context.lineTo(left, top + radius);
  context.quadraticCurveTo(left, top, left + radius, top);
  context.closePath();
  context.fill();
}

function resolveBlockHardTextBounds(
  block: TranslationBlock,
  page: Pick<MangaPage, "height" | "width">,
  layout: ReturnType<typeof resolveBlockTextLayout>,
  fontSizePx: number
): FontFilterEffectBounds {
  return resolveBlockTextEffectBounds(block, page, layout, fontSizePx + 8);
}

function resolveBlockFontFilterEffectBounds(
  block: TranslationBlock,
  page: Pick<MangaPage, "height" | "width">,
  layout: ReturnType<typeof resolveBlockTextLayout>,
  fontSizePx: number
): FontFilterEffectBounds {
  const fallbackMargin = resolveFontFilterEffectMarginPx(block, fontSizePx) + fontSizePx;
  return resolveBlockTextEffectBounds(block, page, layout, fallbackMargin);
}

function resolveBlockTextEffectBounds(
  block: TranslationBlock,
  page: Pick<MangaPage, "height" | "width">,
  layout: ReturnType<typeof resolveBlockTextLayout>,
  fallbackMargin: number
): FontFilterEffectBounds {
  const dirtyRect = resolveBlockCanvasDirtyRect(block, page, layout);
  const rect = dirtyRect ?? resolveExpandedLayoutRect(page, layout, fallbackMargin);
  return {
    height: Math.max(1, rect.height),
    width: Math.max(1, rect.width),
    x: rect.x,
    y: rect.y
  };
}

function resolveExpandedLayoutRect(
  page: Pick<MangaPage, "height" | "width">,
  layout: ReturnType<typeof resolveBlockTextLayout>,
  margin: number
): OverlayCanvasDirtyRect {
  const left = Math.max(0, Math.floor(layout.rect.left - margin));
  const top = Math.max(0, Math.floor(layout.rect.top - margin));
  const right = Math.min(page.width, Math.ceil(layout.rect.left + layout.rect.width + margin));
  const bottom = Math.min(page.height, Math.ceil(layout.rect.top + layout.rect.height + margin));
  return {
    height: Math.max(1, bottom - top),
    width: Math.max(1, right - left),
    x: left,
    y: top
  };
}

function resolveRotatedRectBounds(
  rect: OverlayCanvasDirtyRect,
  center: { x: number; y: number },
  rotationDeg: number
): OverlayCanvasDirtyRect {
  const angle = (rotationDeg * Math.PI) / 180;
  const cos = Math.cos(angle);
  const sin = Math.sin(angle);
  const corners = [
    { x: rect.x, y: rect.y },
    { x: rect.x + rect.width, y: rect.y },
    { x: rect.x + rect.width, y: rect.y + rect.height },
    { x: rect.x, y: rect.y + rect.height }
  ].map((point) => {
    const dx = point.x - center.x;
    const dy = point.y - center.y;
    return {
      x: center.x + dx * cos - dy * sin,
      y: center.y + dx * sin + dy * cos
    };
  });
  const minX = Math.min(...corners.map((point) => point.x));
  const maxX = Math.max(...corners.map((point) => point.x));
  const minY = Math.min(...corners.map((point) => point.y));
  const maxY = Math.max(...corners.map((point) => point.y));
  return {
    x: minX,
    y: minY,
    width: maxX - minX,
    height: maxY - minY
  };
}

function resolveTextRenderStyle(block: TranslationBlock, fontSize: number, options: OverlayRenderOptions): TextRenderStyle {
  return {
    fontWeightAvailability: options.fontWeightAvailability ?? [],
    syntheticBoldWidthPx: resolveSyntheticBoldStrokeWidthPx(block, fontSize, options.fontWeightAvailability),
    syntheticItalicSkewX: resolveSyntheticItalicSkewX(block)
  };
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
  fontSize: number,
  textRenderStyle: TextRenderStyle,
  fontWeightAvailability: readonly FontWeightAvailability[] = [],
  drawOptions: DrawTextOptions = {}
): void {
  const lines = resolveWrappedTextLines(block, text, fontSize, fitInnerWidth, fontWeightAvailability);
  const lineHeightPx = fontSize * block.lineHeight;
  const totalHeight = lines.length * lineHeightPx;
  const textPositionFactors = resolveTextPositionFactors(block.textPosition);
  const startY = top + (innerHeight - totalHeight) * textPositionFactors.y + Math.max(0, (lineHeightPx - fontSize) / 2);
  const maxSpacedLineWidth = lines.reduce(
    (widest, line) => Math.max(widest, measureTextWidthWithLetterSpacing(context, block, line, fontSize, fontWeightAvailability)),
    0
  );
  const textLeft = left + (innerWidth - maxSpacedLineWidth) * textPositionFactors.x;
  const x =
    block.textAlign === "left"
      ? textLeft
      : block.textAlign === "right"
        ? textLeft + maxSpacedLineWidth
        : textLeft + maxSpacedLineWidth / 2;

  context.textAlign = block.textAlign;
  if (drawOptions.includeFontBackground ?? true) {
    drawFontBackground(context, block, {
      left: textLeft,
      top: startY,
      width: maxSpacedLineWidth,
      height: totalHeight
    });
  }
  if (drawOptions.includeGlyphs === false) {
    return;
  }
  drawTextBlockItems(
    context,
    block,
    lines.map((line, index) => ({
      fontSize,
      text: line,
      x,
      y: startY + index * lineHeightPx
    })),
    textRenderStyle,
    fontWeightAvailability
  );
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
  lineHeight: number,
  textRenderStyle: TextRenderStyle,
  drawOptions: DrawTextOptions = {}
): void {
  const chars = [...text.replace(/\s+/g, "")];
  const lineHeightPx = fontSize * lineHeight;
  const letterSpacingPx = resolveTextLetterSpacingPx(block, fontSize);
  const charAdvancePx = Math.max(1, lineHeightPx + letterSpacingPx);
  const totalHeight = chars.length * lineHeightPx + Math.max(0, chars.length - 1) * (charAdvancePx - lineHeightPx);
  const textPositionFactors = resolveTextPositionFactors(block.textPosition);
  const startY = top + (innerHeight - totalHeight) * textPositionFactors.y;
  const x = left + (innerWidth - fontSize) * textPositionFactors.x + fontSize / 2;
  context.textAlign = "center";
  if (drawOptions.includeFontBackground ?? true) {
    drawFontBackground(context, block, {
      left: x - fontSize / 2,
      top: startY,
      width: fontSize,
      height: totalHeight
    });
  }
  if (drawOptions.includeGlyphs === false) {
    return;
  }
  drawTextBlockItems(
    context,
    block,
    chars.map((char, index) => ({
      fontSize,
      text: char,
      x,
      y: startY + index * charAdvancePx
    })),
    textRenderStyle,
    textRenderStyle.fontWeightAvailability
  );
}

function drawFontBackground(
  context: CanvasRenderingContext2D,
  block: TranslationBlock,
  textRect: { left: number; top: number; width: number; height: number }
): void {
  const opacity = normalizeFontBackgroundOpacity(block.fontBackgroundOpacity);
  if (!resolveFontBackgroundEnabled(block) || opacity <= 0 || textRect.width <= 0 || textRect.height <= 0) {
    return;
  }

  const rect = resolveScaledTextBackgroundRect(textRect, resolveFontBackgroundScale(block));
  context.save();
  context.fillStyle = hexToRgba(block.fontBackgroundColor ?? DEFAULT_FONT_BACKGROUND_COLOR, opacity);
  fillRoundRect(context, rect.left, rect.top, rect.width, rect.height, normalizeFontBackgroundRadiusPx(block.fontBackgroundRadiusPx));
  context.restore();
}

function drawTextBlockItems(
  context: CanvasRenderingContext2D,
  block: TranslationBlock,
  items: readonly TextDrawItem[],
  textRenderStyle: TextRenderStyle,
  fontWeightAvailability: readonly FontWeightAvailability[]
): void {
  for (const item of items) {
    drawTextItemPhase(context, block, item, "shadow", textRenderStyle);
  }
  for (const item of items) {
    drawTextItemPhase(context, block, item, "outline", textRenderStyle);
  }
  for (const item of items) {
    drawTextItemPhase(context, block, item, "fill", textRenderStyle);
  }
  for (const item of items) {
    drawTextDecoration(context, block, item.text, item.x, item.y, item.fontSize, fontWeightAvailability);
  }
}

function drawTextItemPhase(
  context: CanvasRenderingContext2D,
  block: TranslationBlock,
  item: TextDrawItem,
  phase: "fill" | "outline" | "shadow",
  textRenderStyle: TextRenderStyle
): void {
  if (drawCenteredEllipsisStyledSegmentsPhase(context, block, item, phase, textRenderStyle)) {
    return;
  }

  if (phase === "shadow") {
    drawTextShadow(context, block, item.text, item.x, item.y, item.fontSize, textRenderStyle);
    return;
  }
  if (phase === "outline") {
    strokeTextOutlines(context, block, item.text, item.x, item.y, item.fontSize, textRenderStyle);
    return;
  }
  fillTextBody(context, block, item.text, item.x, item.y, item.fontSize, textRenderStyle);
}

function drawTextDecoration(
  context: CanvasRenderingContext2D,
  block: TranslationBlock,
  text: string,
  x: number,
  y: number,
  fontSize: number,
  fontWeightAvailability: readonly FontWeightAvailability[]
): void {
  if ((block.textDecoration ?? DEFAULT_OVERLAY_TEXT_DECORATION) !== "underline") {
    return;
  }

  const width = measureTextWidthWithLetterSpacing(context, block, text, fontSize, fontWeightAvailability);
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

function strokeTextOutlines(
  context: CanvasRenderingContext2D,
  block: TranslationBlock,
  text: string,
  x: number,
  y: number,
  fontSize: number,
  textRenderStyle: TextRenderStyle
): void {
  const outlineWidthPx = resolveRenderedOutlineWidthPx(block, fontSize);
  const secondaryOutlineWidthPx = resolveRenderedSecondaryOutlineWidthPx(block, fontSize);

  if (secondaryOutlineWidthPx > 0) {
    context.save();
    context.strokeStyle = block.secondaryOutlineColor ?? "#ffffff";
    context.lineWidth = outlineWidthPx + secondaryOutlineWidthPx * 2 + textRenderStyle.syntheticBoldWidthPx;
    drawTextRun(context, block, text, x, y, fontSize, "stroke", textRenderStyle);
    context.restore();
  }

  if (outlineWidthPx > 0) {
    context.save();
    context.strokeStyle = block.outlineColor ?? DEFAULT_TEXT_OUTLINE_COLOR;
    context.lineWidth = outlineWidthPx + textRenderStyle.syntheticBoldWidthPx;
    drawTextRun(context, block, text, x, y, fontSize, "stroke", textRenderStyle);
    context.fillStyle = block.outlineColor ?? DEFAULT_TEXT_OUTLINE_COLOR;
    drawTextRun(context, block, text, x, y, fontSize, "fill", textRenderStyle);
    context.restore();
  }
}

function fillTextBody(
  context: CanvasRenderingContext2D,
  block: TranslationBlock,
  text: string,
  x: number,
  y: number,
  fontSize: number,
  textRenderStyle: TextRenderStyle
): void {
  if (block.screentoneFillEnabled ?? false) {
    context.save();
    context.fillStyle = "#ffffff";
    drawTextRun(context, block, text, x, y, fontSize, "fill", textRenderStyle);
    const pattern = createScreentonePattern(
      context,
      block.textColor,
      block.screentoneFillIntensity,
      block.screentoneFillDensity,
      block.screentoneFillAntialias,
      fontSize
    );
    context.fillStyle = pattern ?? block.textColor;
    strokeSyntheticBoldText(context, block, text, x, y, fontSize, context.fillStyle, textRenderStyle);
    drawTextRun(context, block, text, x, y, fontSize, "fill", textRenderStyle);
    context.restore();
    return;
  }

  strokeSyntheticBoldText(context, block, text, x, y, fontSize, context.fillStyle, textRenderStyle);
  drawTextRun(context, block, text, x, y, fontSize, "fill", textRenderStyle);
}

function drawCenteredEllipsisStyledSegmentsPhase(
  context: CanvasRenderingContext2D,
  block: TranslationBlock,
  item: TextDrawItem,
  phase: "fill" | "outline" | "shadow",
  textRenderStyle: TextRenderStyle
): boolean {
  if (!containsCenteredEllipsis(item.text)) {
    return false;
  }

  const positionedSegments = resolveTextRunSegmentPositions(context, block, item.text, item.x, item.fontSize, textRenderStyle);
  if (!positionedSegments.some((segment) => segment.centerEllipsis)) {
    return false;
  }

  context.save();
  context.textAlign = "left";
  const originalFont = context.font;
  for (const segment of positionedSegments) {
    applySegmentFont(context, block, segment.fontFamily, item.fontSize, textRenderStyle, originalFont);
    if (segment.centerEllipsis) {
      if (phase === "shadow") {
        drawCenteredEllipsisShadow(context, block, segment.x, item.y, item.fontSize, segment.advance, textRenderStyle);
      } else if (phase === "outline") {
        drawCenteredEllipsisOutlines(context, block, segment.x, item.y, item.fontSize, segment.advance, textRenderStyle);
      } else {
        drawCenteredEllipsisFill(context, block, segment.x, item.y, item.fontSize, segment.advance, textRenderStyle);
      }
    } else if (phase === "shadow") {
      drawTextShadow(context, block, segment.text, segment.x, item.y, item.fontSize, textRenderStyle);
    } else if (phase === "outline") {
      strokeTextOutlines(context, block, segment.text, segment.x, item.y, item.fontSize, textRenderStyle);
    } else {
      fillTextBody(context, block, segment.text, segment.x, item.y, item.fontSize, textRenderStyle);
    }
  }
  context.font = originalFont;
  context.restore();
  return true;
}

function drawCenteredEllipsisShadow(
  context: CanvasRenderingContext2D,
  block: TranslationBlock,
  x: number,
  y: number,
  fontSize: number,
  advance: number,
  textRenderStyle: TextRenderStyle
): void {
  if (!hasVisibleBlockTextShadow(block)) {
    return;
  }

  const shadowDistancePx = Math.max(0, block.shadowDistancePx ?? 0);
  const shadowBlurPx = resolveBlockShadowBlurPx(block);
  if (shadowDistancePx === 0 && shadowBlurPx === 0) {
    return;
  }

  const geometry = resolveCenteredEllipsisDotGeometry(context, x, y, fontSize, advance);
  const angleRad = ((block.shadowAngleDeg ?? 45) * Math.PI) / 180;
  const outlineWidthPx = resolveRenderedOutlineWidthPx(block, fontSize);
  const secondaryOutlineWidthPx = resolveRenderedSecondaryOutlineWidthPx(block, fontSize);
  const outlineRadiusPx = secondaryOutlineWidthPx > 0 ? outlineWidthPx / 2 + secondaryOutlineWidthPx : outlineWidthPx / 2;
  const shadowRadius = geometry.radius + textRenderStyle.syntheticBoldWidthPx / 2 + outlineRadiusPx;

  context.save();
  context.translate(Math.cos(angleRad) * shadowDistancePx, Math.sin(angleRad) * shadowDistancePx);
  context.filter = shadowBlurPx > 0 ? `blur(${shadowBlurPx}px)` : "none";
  withSyntheticItalicTransform(context, x, y, fontSize, textRenderStyle, () => {
    drawCenteredEllipsisDots(context, geometry, shadowRadius, resolveBlockShadowColor(block));
  });
  context.restore();
}

function drawCenteredEllipsisOutlines(
  context: CanvasRenderingContext2D,
  block: TranslationBlock,
  x: number,
  y: number,
  fontSize: number,
  advance: number,
  textRenderStyle: TextRenderStyle
): void {
  const geometry = resolveCenteredEllipsisDotGeometry(context, x, y, fontSize, advance);
  const outlineWidthPx = resolveRenderedOutlineWidthPx(block, fontSize);
  const secondaryOutlineWidthPx = resolveRenderedSecondaryOutlineWidthPx(block, fontSize);
  const syntheticBoldRadiusPx = textRenderStyle.syntheticBoldWidthPx / 2;
  const fillRadius = geometry.radius + syntheticBoldRadiusPx;
  const collapseKnockoutOutline = shouldCollapseCenteredEllipsisKnockoutOutline(block, fontSize);

  withSyntheticItalicTransform(context, x, y, fontSize, textRenderStyle, () => {
    if (secondaryOutlineWidthPx > 0) {
      drawCenteredEllipsisDots(
        context,
        geometry,
        geometry.radius + outlineWidthPx / 2 + secondaryOutlineWidthPx + syntheticBoldRadiusPx,
        block.secondaryOutlineColor ?? "#ffffff"
      );
    }

    if (outlineWidthPx > 0 && !collapseKnockoutOutline) {
      const primaryOutlinePaint = block.outlineColor ?? DEFAULT_TEXT_OUTLINE_COLOR;
      drawCenteredEllipsisDots(
        context,
        geometry,
        geometry.radius + outlineWidthPx / 2 + syntheticBoldRadiusPx,
        primaryOutlinePaint
      );
      drawCenteredEllipsisDots(context, geometry, fillRadius, primaryOutlinePaint);
    }
  });
}

function drawCenteredEllipsisFill(
  context: CanvasRenderingContext2D,
  block: TranslationBlock,
  x: number,
  y: number,
  fontSize: number,
  advance: number,
  textRenderStyle: TextRenderStyle
): void {
  const geometry = resolveCenteredEllipsisDotGeometry(context, x, y, fontSize, advance);
  const syntheticBoldRadiusPx = textRenderStyle.syntheticBoldWidthPx / 2;
  const fillRadius = geometry.radius + syntheticBoldRadiusPx;

  withSyntheticItalicTransform(context, x, y, fontSize, textRenderStyle, () => {
    if (block.screentoneFillEnabled ?? false) {
      drawCenteredEllipsisDots(context, geometry, fillRadius, "#ffffff");
      const pattern = createScreentonePattern(
        context,
        block.textColor,
        block.screentoneFillIntensity,
        block.screentoneFillDensity,
        block.screentoneFillAntialias,
        fontSize
      );
      drawCenteredEllipsisDots(context, geometry, fillRadius, pattern ?? block.textColor);
      return;
    }

    drawCenteredEllipsisDots(context, geometry, fillRadius, block.textColor);
  });
}

function resolveCenteredEllipsisDotGeometry(
  context: CanvasRenderingContext2D,
  x: number,
  y: number,
  fontSize: number,
  advance: number
): CenteredEllipsisDotGeometry {
  const ellipsisAdvance = Math.max(fontSize * 0.45, advance);
  const dotMetrics = context.measureText(".");
  const measuredDotHeight = dotMetrics.actualBoundingBoxAscent + dotMetrics.actualBoundingBoxDescent;
  const measuredRadius =
    Number.isFinite(measuredDotHeight) && measuredDotHeight > 0 && measuredDotHeight < fontSize * 0.4
      ? measuredDotHeight / 2
      : fontSize * CENTERED_ELLIPSIS_DOT_RADIUS_RATIO;
  const radius = Math.max(fontSize * 0.045, Math.min(fontSize * 0.09, measuredRadius));
  const spacing = Math.max(radius * 2.8, Math.min(fontSize * CENTERED_ELLIPSIS_DOT_SPACING_RATIO, ellipsisAdvance / 3));
  const centerX = x + ellipsisAdvance / 2;

  return {
    centers: [centerX - spacing, centerX, centerX + spacing],
    centerY: y + fontSize / 2,
    radius
  };
}

function drawCenteredEllipsisDots(
  context: CanvasRenderingContext2D,
  geometry: CenteredEllipsisDotGeometry,
  radius: number,
  paint: CanvasPaint
): void {
  if (radius <= 0) {
    return;
  }

  context.save();
  context.fillStyle = paint;
  context.beginPath();
  for (const centerX of geometry.centers) {
    context.moveTo(centerX + radius, geometry.centerY);
    context.arc(centerX, geometry.centerY, radius, 0, Math.PI * 2);
  }
  context.fill();
  context.restore();
}

function shouldCollapseCenteredEllipsisKnockoutOutline(block: TranslationBlock, fontSize: number): boolean {
  return (
    resolveRenderedOutlineWidthPx(block, fontSize) > 0 &&
    resolveRenderedSecondaryOutlineWidthPx(block, fontSize) > 0 &&
    colorsMatch(block.outlineColor, block.backgroundColor)
  );
}

function resolveTextRunSegmentPositions(
  context: CanvasRenderingContext2D,
  block: TranslationBlock,
  text: string,
  x: number,
  fontSize: number,
  textRenderStyle: TextRenderStyle
): PositionedTextRunSegment[] {
  const letterSpacingPx = resolveTextLetterSpacingPx(block, fontSize);
  const segments = splitTextRunSegments(block, text, letterSpacingPx !== 0);
  const originalFont = context.font;
  const segmentMetrics = segments.map((segment) => {
    applySegmentFont(context, block, segment.fontFamily, fontSize, textRenderStyle, originalFont);
    return {
      ...segment,
      advance: measureTextSegmentAdvance(context, segment.text, letterSpacingPx)
    };
  });
  const runWidth = segmentMetrics.reduce(
    (total, segment, index) => total + segment.advance + resolveInterSegmentSpacingPx(letterSpacingPx, index, segmentMetrics.length),
    0
  );

  let cursorX = context.textAlign === "right" ? x - runWidth : context.textAlign === "center" ? x - runWidth / 2 : x;
  const positionedSegments: PositionedTextRunSegment[] = [];
  for (const [index, segment] of segmentMetrics.entries()) {
    positionedSegments.push({ ...segment, x: cursorX });
    cursorX += segment.advance + resolveInterSegmentSpacingPx(letterSpacingPx, index, segmentMetrics.length);
  }

  context.font = originalFont;
  return positionedSegments;
}

function measureTextSegmentAdvance(context: Pick<CanvasRenderingContext2D, "measureText">, text: string, letterSpacingPx: number): number {
  const glyphCount = [...text].length;
  return context.measureText(text).width + (glyphCount > 1 ? letterSpacingPx * (glyphCount - 1) : 0);
}

function resolveInterSegmentSpacingPx(letterSpacingPx: number, segmentIndex: number, segmentCount: number): number {
  return letterSpacingPx !== 0 && segmentIndex < segmentCount - 1 ? letterSpacingPx : 0;
}

function drawTextShadow(
  context: CanvasRenderingContext2D,
  block: TranslationBlock,
  text: string,
  x: number,
  y: number,
  fontSize: number,
  textRenderStyle: TextRenderStyle
): void {
  if (!hasVisibleBlockTextShadow(block)) {
    return;
  }

  const shadowDistancePx = Math.max(0, block.shadowDistancePx ?? 0);
  const shadowBlurPx = resolveBlockShadowBlurPx(block);
  if (shadowDistancePx === 0 && shadowBlurPx === 0) {
    return;
  }

  const angleRad = ((block.shadowAngleDeg ?? 45) * Math.PI) / 180;
  const dx = Math.cos(angleRad) * shadowDistancePx;
  const dy = Math.sin(angleRad) * shadowDistancePx;
  const outlineWidthPx = resolveRenderedOutlineWidthPx(block, fontSize);
  const secondaryOutlineWidthPx = resolveRenderedSecondaryOutlineWidthPx(block, fontSize);
  const shadowColor = resolveBlockShadowColor(block);

  context.save();
  context.translate(dx, dy);
  context.filter = shadowBlurPx > 0 ? `blur(${shadowBlurPx}px)` : "none";

  if (secondaryOutlineWidthPx > 0) {
    context.save();
    context.strokeStyle = shadowColor;
    context.lineWidth = outlineWidthPx + secondaryOutlineWidthPx * 2 + textRenderStyle.syntheticBoldWidthPx;
    drawTextRun(context, block, text, x, y, fontSize, "stroke", textRenderStyle);
    context.restore();
  }

  if (outlineWidthPx > 0) {
    context.save();
    context.strokeStyle = shadowColor;
    context.lineWidth = outlineWidthPx + textRenderStyle.syntheticBoldWidthPx;
    drawTextRun(context, block, text, x, y, fontSize, "stroke", textRenderStyle);
    context.restore();
  }

  context.fillStyle = shadowColor;
  strokeSyntheticBoldText(context, block, text, x, y, fontSize, shadowColor, textRenderStyle);
  drawTextRun(context, block, text, x, y, fontSize, "fill", textRenderStyle);
  context.restore();
}

function hasVisibleBlockTextShadow(block: Pick<TranslationBlock, "shadowBlurPx" | "shadowDistancePx" | "shadowEnabled" | "shadowOpacity">): boolean {
  const shadowEnabled = block.shadowEnabled ?? ((block.shadowDistancePx ?? 0) > 0 || (block.shadowBlurPx ?? 0) > 0);
  return shadowEnabled && resolveBlockShadowOpacity(block) > 0;
}

function resolveBlockShadowColor(block: Pick<TranslationBlock, "shadowColor" | "shadowOpacity">): string {
  return hexToRgba(block.shadowColor ?? "#000000", resolveBlockShadowOpacity(block));
}

function resolveBlockShadowOpacity(block: Pick<TranslationBlock, "shadowOpacity">): number {
  const opacity = block.shadowOpacity ?? 1;
  return Number.isFinite(opacity) ? Math.max(0, Math.min(1, opacity)) : 1;
}

function resolveBlockShadowBlurPx(block: Pick<TranslationBlock, "shadowBlurPx">): number {
  const blurPx = block.shadowBlurPx ?? 0;
  return Number.isFinite(blurPx) ? Math.max(0, Math.min(80, blurPx)) : 0;
}

function strokeSyntheticBoldText(
  context: CanvasRenderingContext2D,
  block: TranslationBlock,
  text: string,
  x: number,
  y: number,
  fontSize: number,
  paint: CanvasPaint,
  textRenderStyle: TextRenderStyle
): void {
  if (textRenderStyle.syntheticBoldWidthPx <= 0) {
    return;
  }

  context.save();
  context.strokeStyle = paint;
  context.lineWidth = textRenderStyle.syntheticBoldWidthPx;
  drawTextRun(context, block, text, x, y, fontSize, "stroke", textRenderStyle);
  context.restore();
}

function drawTextRun(
  context: CanvasRenderingContext2D,
  block: TranslationBlock,
  text: string,
  x: number,
  y: number,
  fontSize: number,
  mode: "fill" | "stroke",
  textRenderStyle: TextRenderStyle
): void {
  withSyntheticItalicTransform(context, x, y, fontSize, textRenderStyle, () => {
    drawTextRunGlyphs(context, block, text, x, y, fontSize, mode, textRenderStyle);
  });
}

function withSyntheticItalicTransform(
  context: CanvasRenderingContext2D,
  x: number,
  y: number,
  fontSize: number,
  textRenderStyle: TextRenderStyle,
  draw: () => void
): void {
  if (textRenderStyle.syntheticItalicSkewX === 0) {
    draw();
    return;
  }

  context.save();
  const pivotY = y + fontSize / 2;
  context.translate(x, pivotY);
  context.transform(1, 0, textRenderStyle.syntheticItalicSkewX, 1, 0, 0);
  context.translate(-x, -pivotY);
  draw();
  context.restore();
}

function drawTextRunGlyphs(
  context: CanvasRenderingContext2D,
  block: TranslationBlock,
  text: string,
  x: number,
  y: number,
  fontSize: number,
  mode: "fill" | "stroke",
  textRenderStyle: TextRenderStyle
): void {
  const letterSpacingPx = resolveTextLetterSpacingPx(block, fontSize);
  const hasCenteredEllipsis = containsCenteredEllipsis(text);
  const hasCharacterFontOverrides = (block.characterFontOverrides?.length ?? 0) > 0;
  if (letterSpacingPx === 0 && !hasCenteredEllipsis && !hasCharacterFontOverrides) {
    drawTextGlyph(context, text, x, y, fontSize, mode);
    return;
  }
  const runWidth = measureTextWidthWithLetterSpacing(context, block, text, fontSize, textRenderStyle.fontWeightAvailability);
  let cursorX = context.textAlign === "right" ? x - runWidth : context.textAlign === "center" ? x - runWidth / 2 : x;
  const originalFont = context.font;

  context.save();
  context.textAlign = "left";
  if (letterSpacingPx === 0) {
    for (const segment of splitTextRunSegments(block, text)) {
      applySegmentFont(context, block, segment.fontFamily, fontSize, textRenderStyle, originalFont);
      drawTextGlyph(context, segment.text, cursorX, y, fontSize, mode, segment.centerEllipsis);
      cursorX += context.measureText(segment.text).width;
    }
  } else {
    for (const segment of splitTextRunSegments(block, text, true)) {
      applySegmentFont(context, block, segment.fontFamily, fontSize, textRenderStyle, originalFont);
      drawTextGlyph(context, segment.text, cursorX, y, fontSize, mode, segment.centerEllipsis);
      cursorX += context.measureText(segment.text).width + letterSpacingPx;
    }
  }
  context.font = originalFont;
  context.restore();
}

type TextRunSegment = {
  text: string;
  fontFamily?: string;
  centerEllipsis: boolean;
};

function splitTextRunSegments(block: TranslationBlock, text: string, forceSingleGlyph = false): TextRunSegment[] {
  const segments: TextRunSegment[] = [];
  const runs = resolveCharacterFontRuns(block, text);

  for (const run of runs) {
    for (const segment of splitTextRunByCenteredEllipsis(run.text)) {
      if (!forceSingleGlyph || isCenteredEllipsisSegment(segment)) {
        segments.push({ text: segment, fontFamily: run.fontFamily, centerEllipsis: isCenteredEllipsisSegment(segment) });
        continue;
      }

      for (const char of [...segment]) {
        segments.push({ text: char, fontFamily: run.fontFamily, centerEllipsis: false });
      }
    }
  }

  return segments;
}

function splitTextRunByCenteredEllipsis(text: string): string[] {
  return text.split(/(…|\.{3})/u).filter(Boolean);
}

function containsCenteredEllipsis(text: string): boolean {
  return text.includes(CENTERED_ELLIPSIS) || text.includes(CENTERED_PERIOD_ELLIPSIS);
}

function isCenteredEllipsisSegment(text: string): boolean {
  return text === CENTERED_ELLIPSIS || text === CENTERED_PERIOD_ELLIPSIS;
}

function applySegmentFont(
  context: CanvasRenderingContext2D,
  block: TranslationBlock,
  fontFamily: string | undefined,
  fontSize: number,
  textRenderStyle: TextRenderStyle,
  fallbackFont: string
): void {
  if (!fontFamily) {
    context.font = fallbackFont;
    return;
  }
  context.font = buildOverlayCanvasFont(fontSize, { ...block, fontFamily }, textRenderStyle.fontWeightAvailability);
}

function drawTextGlyph(
  context: CanvasRenderingContext2D,
  text: string,
  x: number,
  y: number,
  fontSize: number,
  mode: "fill" | "stroke",
  centerEllipsis = false
): void {
  const glyphY = y + resolveCenteredEllipsisYOffset(context, text, fontSize, centerEllipsis);
  if (mode === "stroke") {
    context.strokeText(text, x, glyphY);
  } else {
    context.fillText(text, x, glyphY);
  }
}

export function resolveCenteredEllipsisYOffset(
  context: Pick<CanvasRenderingContext2D, "measureText">,
  text: string,
  fontSize: number,
  centerEllipsis = false
): number {
  if (!centerEllipsis && !isCenteredEllipsisSegment(text)) {
    return 0;
  }

  const metrics = context.measureText(text);
  const inkHeight = metrics.actualBoundingBoxAscent + metrics.actualBoundingBoxDescent;
  if (!Number.isFinite(inkHeight) || inkHeight <= 0) {
    return 0;
  }
  const inkCenterFromY = (metrics.actualBoundingBoxDescent - metrics.actualBoundingBoxAscent) / 2;
  if (!Number.isFinite(inkCenterFromY)) {
    return 0;
  }
  return fontSize / 2 - inkCenterFromY;
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

function colorsMatch(left: string | undefined, right: string | undefined): boolean {
  const leftColor = parseOptionalHexColor(left);
  const rightColor = parseOptionalHexColor(right);
  return Boolean(leftColor && rightColor && leftColor.r === rightColor.r && leftColor.g === rightColor.g && leftColor.b === rightColor.b);
}

function parseOptionalHexColor(hex: string | undefined): { r: number; g: number; b: number } | null {
  if (!hex || !/^#[0-9a-f]{6}$/iu.test(hex)) {
    return null;
  }
  const color = parseHexColor(hex);
  if (!Number.isFinite(color.r) || !Number.isFinite(color.g) || !Number.isFinite(color.b)) {
    return null;
  }
  return color;
}

function resolveRenderedOutlineWidthPx(block: TranslationBlock, fontSize: number): number {
  const outlineWidthPx = Math.max(0, block.outlineWidthPx ?? 0);
  return Math.min(outlineWidthPx, fontSize * 0.35);
}

function resolveRenderedSecondaryOutlineWidthPx(block: TranslationBlock, fontSize: number): number {
  if (!resolveSecondaryOutlineEnabled(block)) {
    return 0;
  }
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
