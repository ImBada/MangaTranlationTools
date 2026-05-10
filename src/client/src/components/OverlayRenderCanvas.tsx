import React, { useEffect, useRef } from "react";
import type { MangaPage, TranslationBlock } from "../../../shared/types";
import {
  getActiveTranslationBlockDragDebugId,
  isInpaintDebugLogEnabled,
  nowInpaintDebugMs,
  roundInpaintDebugMs,
  writeInpaintDebugLog
} from "../lib/inpaintDiagnostics";
import {
  doesBlockIntersectCanvasRect,
  drawOverlayBlocks,
  resolveBlockCanvasDirtyRect
} from "../lib/pageRender";
import type { FontWeightAvailability, ViewportSize } from "../lib/overlayLayout";

type OverlayRenderCanvasProps = {
  canvasRef?: React.RefObject<HTMLCanvasElement | null>;
  hiddenBlockId?: string | null;
  page: MangaPage;
  stageSize: ViewportSize;
  editingEnabled: boolean;
  fontWeightAvailability: readonly FontWeightAvailability[];
};

const OVERLAY_CANVAS_DEBUG_SLOW_DRAW_MS = 16;
const OVERLAY_CANVAS_DEBUG_SUMMARY_INTERVAL_MS = 500;
const OVERLAY_CANVAS_RENDER_CACHE_MAX_ENTRIES = 2;

export function OverlayRenderCanvas({
  canvasRef: externalCanvasRef,
  hiddenBlockId,
  page,
  stageSize,
  editingEnabled,
  fontWeightAvailability
}: OverlayRenderCanvasProps): React.JSX.Element {
  const internalCanvasRef = useRef<HTMLCanvasElement | null>(null);
  const canvasRef = externalCanvasRef ?? internalCanvasRef;
  const lastRenderedBlocksRef = useRef<Map<string, TranslationBlock>>(new Map());
  const lastRenderedBlockSummariesRef = useRef<Map<string, Record<string, unknown>>>(new Map());
  const lastHiddenBlockIdRef = useRef<string | null>(null);
  const renderCacheRef = useRef<Map<string, HTMLCanvasElement>>(new Map());
  const warmingRenderCacheKeysRef = useRef<Set<string>>(new Set());
  const renderKey = React.useMemo(
    () => createOverlayRenderKey(page, editingEnabled, fontWeightAvailability, hiddenBlockId, lastRenderedBlockSummariesRef.current),
    [editingEnabled, hiddenBlockId, fontWeightAvailability, page]
  );
  const renderStateRef = useRef({
    editingEnabled,
    hiddenBlockId,
    fontWeightAvailability,
    page
  });
  renderStateRef.current = {
    editingEnabled,
    hiddenBlockId,
    fontWeightAvailability,
    page
  };
  const debugStatsRef = useRef({
    drawCount: 0,
    lastSummaryAtMs: 0,
    maxDrawMs: 0,
    slowDrawCount: 0,
    totalDrawMs: 0
  });

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) {
      return;
    }
    canvas.style.width = `${Math.max(1, Math.round(stageSize.width))}px`;
    canvas.style.height = `${Math.max(1, Math.round(stageSize.height))}px`;
  }, [canvasRef, stageSize.height, stageSize.width]);

  React.useLayoutEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d", { willReadFrequently: true });
    if (!canvas || !context) {
      return;
    }

    const {
      editingEnabled: currentEditingEnabled,
      hiddenBlockId: currentHiddenBlockId,
      fontWeightAvailability: currentFontWeightAvailability,
      page: currentPage
    } = renderStateRef.current;
    const width = Math.max(1, currentPage.width);
    const height = Math.max(1, currentPage.height);
    const debugEnabled = isInpaintDebugLogEnabled();
    const drawStartMs = debugEnabled ? nowInpaintDebugMs() : 0;
    const hiddenDirtyBlock = currentHiddenBlockId
      ? lastRenderedBlocksRef.current.get(currentHiddenBlockId) ??
        currentPage.blocks.find((block) => block.id === currentHiddenBlockId) ??
        null
      : null;
    const cachedCanvas = renderCacheRef.current.get(renderKey);
    const renderCacheHit = Boolean(cachedCanvas && cachedCanvas.width === width && cachedCanvas.height === height);
    if (cachedCanvas && renderCacheHit) {
      copyOverlayCanvas(canvas, context, cachedCanvas, width, height);
    } else {
      canvas.width = width;
      canvas.height = height;
      context.setTransform(1, 0, 0, 1, 0, 0);
      context.clearRect(0, 0, width, height);
      const includedBlockIds = currentHiddenBlockId
        ? new Set(
            currentPage.blocks
              .filter((block) => block.id !== currentHiddenBlockId)
              .map((block) => block.id)
          )
        : undefined;
      drawOverlayBlocks(context, currentPage, {
        renderSize: { width: currentPage.width, height: currentPage.height },
        editingEnabled: currentEditingEnabled,
        includedBlockIds,
        fontWeightAvailability: currentFontWeightAvailability
      });
      if (!currentHiddenBlockId) {
        storeOverlayRenderCache(renderCacheRef.current, renderKey, canvas, width, height);
      }
    }
    if (currentHiddenBlockId && hiddenDirtyBlock) {
      refreshOverlayBlockDirtyRect(context, currentPage, {
        blockForDirtyRect: hiddenDirtyBlock,
        editingEnabled: currentEditingEnabled,
        excludedBlockId: currentHiddenBlockId,
        fontWeightAvailability: currentFontWeightAvailability
      });
    }

    const nextRenderedBlocks = createRenderedBlockMap(
      currentPage,
      currentHiddenBlockId,
      lastRenderedBlocksRef.current
    );
    const nextRenderedBlockSummaries = createRenderedBlockSummaryMap(
      currentPage,
      currentHiddenBlockId,
      lastRenderedBlockSummariesRef.current
    );
    lastRenderedBlocksRef.current = nextRenderedBlocks;
    lastRenderedBlockSummariesRef.current = nextRenderedBlockSummaries;
    if (!currentHiddenBlockId) {
      scheduleOverlayRenderCacheWarm(renderCacheRef.current, warmingRenderCacheKeysRef.current, {
        editingEnabled: !currentEditingEnabled,
        fontWeightAvailability: currentFontWeightAvailability,
        page: currentPage,
        renderKey: createOverlayRenderKey(
          currentPage,
          !currentEditingEnabled,
          currentFontWeightAvailability,
          null,
          nextRenderedBlockSummaries
        )
      });
    }
    if (!debugEnabled) {
      return;
    }

    const drawMs = nowInpaintDebugMs() - drawStartMs;
    const stats = debugStatsRef.current;
    stats.drawCount += 1;
    stats.totalDrawMs += drawMs;
    stats.maxDrawMs = Math.max(stats.maxDrawMs, drawMs);
    const activeDragDebugId = getActiveTranslationBlockDragDebugId();
    const nowMs = nowInpaintDebugMs();
    const shouldLogSlowDraw = drawMs >= OVERLAY_CANVAS_DEBUG_SLOW_DRAW_MS;
    if (shouldLogSlowDraw) {
      stats.slowDrawCount += 1;
    }
    const shouldWriteSlowDraw = shouldLogSlowDraw && (stats.slowDrawCount <= 8 || stats.slowDrawCount % 10 === 0);
    const shouldLogSummary = activeDragDebugId && nowMs - stats.lastSummaryAtMs >= OVERLAY_CANVAS_DEBUG_SUMMARY_INTERVAL_MS;
    if (!shouldWriteSlowDraw && !shouldLogSummary) {
      return;
    }

    stats.lastSummaryAtMs = nowMs;
    writeInpaintDebugLog(shouldWriteSlowDraw ? "overlay-render-canvas:draw-slow" : "overlay-render-canvas:draw-summary", () => ({
      activeDragDebugId,
      averageDrawMs: roundInpaintDebugMs(stats.totalDrawMs / Math.max(1, stats.drawCount)),
      blockCount: currentPage.blocks.length,
      canvasHeight: height,
      canvasWidth: width,
      drawCount: stats.drawCount,
      drawMs: roundInpaintDebugMs(drawMs),
      editingEnabled: currentEditingEnabled,
      cacheHit: renderCacheHit,
      cacheSize: renderCacheRef.current.size,
      maxDrawMs: roundInpaintDebugMs(stats.maxDrawMs),
      pageId: currentPage.id,
      slowDrawCount: stats.slowDrawCount,
      hiddenBlockId: currentHiddenBlockId ?? null,
      totalTextLength: currentPage.blocks.reduce((total, block) => total + (block.translatedText || block.sourceText || "").length, 0),
      visibleBlockCount: currentPage.blocks.filter((block) => block.renderDirection !== "hidden").length
    }));
  }, [canvasRef, renderKey]);

  React.useLayoutEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d", { willReadFrequently: true });
    const blockIdToRefresh = hiddenBlockId ?? lastHiddenBlockIdRef.current;
    const {
      editingEnabled: currentEditingEnabled,
      fontWeightAvailability: currentFontWeightAvailability,
      page: currentPage
    } = renderStateRef.current;
    const currentBlock = blockIdToRefresh
      ? currentPage.blocks.find((block) => block.id === blockIdToRefresh) ?? null
      : null;
    const blockForDirtyRect = hiddenBlockId && blockIdToRefresh
      ? lastRenderedBlocksRef.current.get(blockIdToRefresh) ?? currentBlock
      : currentBlock;
    if (!canvas || !context || !blockIdToRefresh || !blockForDirtyRect) {
      lastHiddenBlockIdRef.current = hiddenBlockId ?? null;
      return;
    }

    refreshOverlayBlockDirtyRect(context, currentPage, {
      blockForDirtyRect,
      editingEnabled: currentEditingEnabled,
      excludedBlockId: hiddenBlockId ?? null,
      fontWeightAvailability: currentFontWeightAvailability
    });
    lastHiddenBlockIdRef.current = hiddenBlockId ?? null;
  }, [canvasRef, hiddenBlockId]);

  return <canvas ref={canvasRef} className="overlay-render-canvas" aria-hidden="true" />;
}

function copyOverlayCanvas(
  targetCanvas: HTMLCanvasElement,
  targetContext: CanvasRenderingContext2D,
  sourceCanvas: HTMLCanvasElement,
  width: number,
  height: number
): void {
  if (targetCanvas.width !== width) {
    targetCanvas.width = width;
  }
  if (targetCanvas.height !== height) {
    targetCanvas.height = height;
  }
  targetContext.setTransform(1, 0, 0, 1, 0, 0);
  targetContext.clearRect(0, 0, width, height);
  targetContext.drawImage(sourceCanvas, 0, 0);
}

function storeOverlayRenderCache(
  cache: Map<string, HTMLCanvasElement>,
  renderKey: string,
  sourceCanvas: HTMLCanvasElement,
  width: number,
  height: number
): void {
  const cachedCanvas = document.createElement("canvas");
  cachedCanvas.width = width;
  cachedCanvas.height = height;
  const cachedContext = cachedCanvas.getContext("2d", { willReadFrequently: true });
  if (!cachedContext) {
    return;
  }
  cachedContext.drawImage(sourceCanvas, 0, 0);
  rememberOverlayRenderCache(cache, renderKey, cachedCanvas);
}

function rememberOverlayRenderCache(
  cache: Map<string, HTMLCanvasElement>,
  renderKey: string,
  cachedCanvas: HTMLCanvasElement
): void {
  cache.delete(renderKey);
  cache.set(renderKey, cachedCanvas);
  while (cache.size > OVERLAY_CANVAS_RENDER_CACHE_MAX_ENTRIES) {
    const oldestKey = cache.keys().next().value;
    if (!oldestKey) {
      break;
    }
    cache.delete(oldestKey);
  }
}

function scheduleOverlayRenderCacheWarm(
  cache: Map<string, HTMLCanvasElement>,
  warmingKeys: Set<string>,
  options: {
    editingEnabled: boolean;
    fontWeightAvailability: readonly FontWeightAvailability[];
    page: MangaPage;
    renderKey: string;
  }
): void {
  if (cache.has(options.renderKey) || warmingKeys.has(options.renderKey)) {
    return;
  }

  warmingKeys.add(options.renderKey);
  const warmCache = (): void => {
    try {
      if (cache.has(options.renderKey)) {
        return;
      }

      const width = Math.max(1, options.page.width);
      const height = Math.max(1, options.page.height);
      const cachedCanvas = document.createElement("canvas");
      cachedCanvas.width = width;
      cachedCanvas.height = height;
      const cachedContext = cachedCanvas.getContext("2d", { willReadFrequently: true });
      if (!cachedContext) {
        return;
      }

      drawOverlayBlocks(cachedContext, options.page, {
        renderSize: { width: options.page.width, height: options.page.height },
        editingEnabled: options.editingEnabled,
        fontWeightAvailability: options.fontWeightAvailability
      });
      rememberOverlayRenderCache(cache, options.renderKey, cachedCanvas);
    } finally {
      warmingKeys.delete(options.renderKey);
    }
  };

  const idleWindow = window as Window & {
    requestIdleCallback?: (callback: () => void, options?: { timeout?: number }) => number;
  };
  if (idleWindow.requestIdleCallback) {
    idleWindow.requestIdleCallback(warmCache);
    return;
  }
  window.setTimeout(warmCache, 200);
}

function refreshOverlayBlockDirtyRect(
  context: CanvasRenderingContext2D,
  page: MangaPage,
  options: {
    blockForDirtyRect: TranslationBlock;
    editingEnabled: boolean;
    excludedBlockId?: string | null;
    fontWeightAvailability: readonly FontWeightAvailability[];
  }
): void {
  const dirtyRect = resolveBlockCanvasDirtyRect(options.blockForDirtyRect, page);
  if (!dirtyRect) {
    return;
  }

  context.clearRect(dirtyRect.x, dirtyRect.y, dirtyRect.width, dirtyRect.height);
  const intersectingBlockIds = page.blocks
    .filter((block) => block.id !== options.excludedBlockId && doesBlockIntersectCanvasRect(block, page, dirtyRect))
    .map((block) => block.id);
  if (intersectingBlockIds.length === 0) {
    return;
  }

  context.save();
  context.beginPath();
  context.rect(dirtyRect.x, dirtyRect.y, dirtyRect.width, dirtyRect.height);
  context.clip();
  drawOverlayBlocks(context, page, {
    renderSize: { width: page.width, height: page.height },
    editingEnabled: options.editingEnabled,
    includedBlockIds: new Set(intersectingBlockIds),
    fontWeightAvailability: options.fontWeightAvailability
  });
  context.restore();
}

function createRenderedBlockMap(
  page: MangaPage,
  hiddenBlockId: string | null | undefined,
  previousBlocks: ReadonlyMap<string, TranslationBlock>
): Map<string, TranslationBlock> {
  return new Map(
    page.blocks.map((block) => [
      block.id,
      block.id === hiddenBlockId
        ? previousBlocks.get(block.id) ?? block
        : block
    ])
  );
}

function createRenderedBlockSummaryMap(
  page: MangaPage,
  hiddenBlockId: string | null | undefined,
  previousBlockSummaries: ReadonlyMap<string, Record<string, unknown>>
): Map<string, Record<string, unknown>> {
  return new Map(
    page.blocks.map((block) => [
      block.id,
      block.id === hiddenBlockId
        ? previousBlockSummaries.get(block.id) ?? summarizeBlockRenderState(block)
        : summarizeBlockRenderState(block)
    ])
  );
}

function createOverlayRenderKey(
  page: MangaPage,
  editingEnabled: boolean,
  fontWeightAvailability: readonly FontWeightAvailability[],
  hiddenBlockId: string | null | undefined,
  lastRenderedBlockSummaries: ReadonlyMap<string, Record<string, unknown>>
): string {
  const currentBlocksById = new Map(page.blocks.map((block) => [block.id, block]));
  const blockIds = hiddenBlockId
    ? [
        ...lastRenderedBlockSummaries.keys(),
        ...page.blocks
          .map((block) => block.id)
          .filter((blockId) => !lastRenderedBlockSummaries.has(blockId))
      ]
    : page.blocks.map((block) => block.id);
  return JSON.stringify({
    editingEnabled,
    fontWeightAvailability,
    pageHeight: page.height,
    pageId: page.id,
    pageWidth: page.width,
    blocks: blockIds.map((blockId) => {
      const currentBlock = currentBlocksById.get(blockId);
      if (!currentBlock) {
        return lastRenderedBlockSummaries.get(blockId) ?? { id: blockId, removed: true };
      }
      return blockId === hiddenBlockId
        ? lastRenderedBlockSummaries.get(blockId) ?? summarizeBlockRenderState(currentBlock)
        : summarizeBlockRenderState(currentBlock);
    })
  });
}

function summarizeBlockRenderState(block: TranslationBlock): Record<string, unknown> {
  return {
    autoFitText: block.autoFitText,
    backgroundColor: block.backgroundColor,
    bbox: block.bbox,
    bboxSpace: block.bboxSpace,
    characterFontOverrides: block.characterFontOverrides,
    fontFamily: block.fontFamily,
    fontSizePx: block.fontSizePx,
    fontStyle: block.fontStyle,
    fontWeight: block.fontWeight,
    id: block.id,
    letterSpacingPx: block.letterSpacingPx,
    lineHeight: block.lineHeight,
    opacity: block.opacity,
    outlineColor: block.outlineColor,
    outlineWidthPx: block.outlineWidthPx,
    renderDirection: block.renderDirection,
    renderBbox: block.renderBbox,
    renderBboxSpace: block.renderBboxSpace,
    rotationDeg: block.rotationDeg,
    screentoneFillAntialias: block.screentoneFillAntialias,
    screentoneFillDensity: block.screentoneFillDensity,
    screentoneFillEnabled: block.screentoneFillEnabled,
    screentoneFillIntensity: block.screentoneFillIntensity,
    secondaryOutlineColor: block.secondaryOutlineColor,
    secondaryOutlineWidthPx: block.secondaryOutlineWidthPx,
    shadowAngleDeg: block.shadowAngleDeg,
    shadowColor: block.shadowColor,
    shadowDistancePx: block.shadowDistancePx,
    shadowEnabled: block.shadowEnabled,
    sourceText: block.sourceText,
    textAlign: block.textAlign,
    textColor: block.textColor,
    textDecoration: block.textDecoration,
    textPaddingPx: block.textPaddingPx,
    textPosition: block.textPosition,
    translatedText: block.translatedText
  };
}
