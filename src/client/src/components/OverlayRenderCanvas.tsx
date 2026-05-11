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
import { hasEnabledTranslationBlockGroupEffects } from "../lib/blockGroupEffects";
import type { FontWeightAvailability, ViewportSize } from "../lib/overlayLayout";

type OverlayRenderCanvasProps = {
  canvasRef?: React.RefObject<HTMLCanvasElement | null>;
  hiddenBlockIds?: readonly string[] | null;
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
  hiddenBlockIds,
  page,
  stageSize,
  editingEnabled,
  fontWeightAvailability
}: OverlayRenderCanvasProps): React.JSX.Element {
  const internalCanvasRef = useRef<HTMLCanvasElement | null>(null);
  const canvasRef = externalCanvasRef ?? internalCanvasRef;
  const lastRenderedBlocksRef = useRef<Map<string, TranslationBlock>>(new Map());
  const lastRenderedBlockSummariesRef = useRef<Map<string, Record<string, unknown>>>(new Map());
  const lastHiddenBlockIdsRef = useRef<readonly string[]>([]);
  const renderCacheRef = useRef<Map<string, HTMLCanvasElement>>(new Map());
  const warmingRenderCacheKeysRef = useRef<Set<string>>(new Set());
  const hiddenBlockIdSet = React.useMemo(() => new Set(hiddenBlockIds ?? []), [hiddenBlockIds]);
  const hiddenBlockKey = React.useMemo(() => [...hiddenBlockIdSet].join("\u0000"), [hiddenBlockIdSet]);
  const renderKey = React.useMemo(
    () => createOverlayRenderKey(page, editingEnabled, fontWeightAvailability, hiddenBlockIdSet, lastRenderedBlockSummariesRef.current),
    [editingEnabled, fontWeightAvailability, hiddenBlockIdSet, page]
  );
  const renderStateRef = useRef({
    editingEnabled,
    hiddenBlockIds: hiddenBlockIds ?? [],
    fontWeightAvailability,
    page
  });
  renderStateRef.current = {
    editingEnabled,
    hiddenBlockIds: hiddenBlockIds ?? [],
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
      hiddenBlockIds: currentHiddenBlockIds,
      fontWeightAvailability: currentFontWeightAvailability,
      page: currentPage
    } = renderStateRef.current;
    const currentHiddenBlockIdSet = new Set(currentHiddenBlockIds);
    const width = Math.max(1, currentPage.width);
    const height = Math.max(1, currentPage.height);
    const debugEnabled = isInpaintDebugLogEnabled();
    const drawStartMs = debugEnabled ? nowInpaintDebugMs() : 0;
    const hiddenDirtyBlocks = resolveHiddenDirtyBlocks(
      currentHiddenBlockIds,
      currentPage,
      lastRenderedBlocksRef.current
    );
    const hiddenGroupEffectRedrawRequired =
      currentHiddenBlockIdSet.size > 0 &&
      Boolean(currentPage.blockGroups?.some(hasEnabledTranslationBlockGroupEffects));
    const cachedCanvas = hiddenGroupEffectRedrawRequired ? undefined : renderCacheRef.current.get(renderKey);
    const renderCacheHit = Boolean(cachedCanvas && cachedCanvas.width === width && cachedCanvas.height === height);
    if (cachedCanvas && renderCacheHit) {
      copyOverlayCanvas(canvas, context, cachedCanvas, width, height);
    } else {
      canvas.width = width;
      canvas.height = height;
      context.setTransform(1, 0, 0, 1, 0, 0);
      context.clearRect(0, 0, width, height);
      const includedBlockIds = resolveIncludedOverlayBlockIds(currentPage, currentHiddenBlockIdSet);
      drawOverlayBlocks(context, currentPage, {
        renderSize: { width: currentPage.width, height: currentPage.height },
        editingEnabled: currentEditingEnabled,
        includedBlockIds,
        fontWeightAvailability: currentFontWeightAvailability
      });
      if (currentHiddenBlockIdSet.size === 0) {
        storeOverlayRenderCache(renderCacheRef.current, renderKey, canvas, width, height);
      }
    }
    if (hiddenDirtyBlocks.length > 0 && !hiddenGroupEffectRedrawRequired) {
      refreshOverlayBlockDirtyRects(context, currentPage, {
        blocksForDirtyRect: hiddenDirtyBlocks,
        editingEnabled: currentEditingEnabled,
        excludedBlockIds: currentHiddenBlockIdSet,
        fontWeightAvailability: currentFontWeightAvailability
      });
    }

    const nextRenderedBlocks = createRenderedBlockMap(
      currentPage,
      currentHiddenBlockIdSet,
      lastRenderedBlocksRef.current
    );
    const nextRenderedBlockSummaries = createRenderedBlockSummaryMap(
      currentPage,
      currentHiddenBlockIdSet,
      lastRenderedBlockSummariesRef.current
    );
    lastRenderedBlocksRef.current = nextRenderedBlocks;
    lastRenderedBlockSummariesRef.current = nextRenderedBlockSummaries;
    if (currentHiddenBlockIdSet.size === 0) {
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
      hiddenBlockIds: currentHiddenBlockIds,
      totalTextLength: currentPage.blocks.reduce((total, block) => total + (block.translatedText || block.sourceText || "").length, 0),
      visibleBlockCount: currentPage.blocks.filter((block) => block.renderDirection !== "hidden").length
    }));
  }, [canvasRef, hiddenBlockKey, renderKey]);

  React.useLayoutEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d", { willReadFrequently: true });
    const blockIdsToRefresh = new Set([...(hiddenBlockIds ?? []), ...lastHiddenBlockIdsRef.current]);
    const {
      editingEnabled: currentEditingEnabled,
      fontWeightAvailability: currentFontWeightAvailability,
      page: currentPage
    } = renderStateRef.current;
    if (!canvas || !context) {
      lastHiddenBlockIdsRef.current = hiddenBlockIds ?? [];
      return;
    }
    if (blockIdsToRefresh.size === 0) {
      lastHiddenBlockIdsRef.current = hiddenBlockIds ?? [];
      return;
    }
    if (currentPage.blockGroups?.some(hasEnabledTranslationBlockGroupEffects)) {
      lastHiddenBlockIdsRef.current = hiddenBlockIds ?? [];
      return;
    }
    const currentHiddenBlockIdSet = new Set(hiddenBlockIds ?? []);
    const blocksForDirtyRect = [...blockIdsToRefresh].flatMap((blockId) => {
      const currentBlock = currentPage.blocks.find((block) => block.id === blockId) ?? null;
      const blockForDirtyRect = currentHiddenBlockIdSet.has(blockId)
        ? lastRenderedBlocksRef.current.get(blockId) ?? currentBlock
        : currentBlock;
      return blockForDirtyRect ? [blockForDirtyRect] : [];
    });
    if (blocksForDirtyRect.length === 0) {
      lastHiddenBlockIdsRef.current = hiddenBlockIds ?? [];
      return;
    }

    refreshOverlayBlockDirtyRects(context, currentPage, {
      blocksForDirtyRect,
      editingEnabled: currentEditingEnabled,
      excludedBlockIds: currentHiddenBlockIdSet,
      fontWeightAvailability: currentFontWeightAvailability
    });
    lastHiddenBlockIdsRef.current = hiddenBlockIds ?? [];
  }, [canvasRef, hiddenBlockIds]);

  return <canvas ref={canvasRef} className="overlay-render-canvas" aria-hidden="true" />;
}

function resolveIncludedOverlayBlockIds(page: MangaPage, hiddenBlockIds: ReadonlySet<string>): Set<string> | undefined {
  return hiddenBlockIds.size > 0
    ? new Set(
        page.blocks
          .filter((block) => !hiddenBlockIds.has(block.id))
          .map((block) => block.id)
      )
    : undefined;
}

function resolveHiddenDirtyBlocks(
  hiddenBlockIds: readonly string[],
  page: MangaPage,
  previousBlocks: ReadonlyMap<string, TranslationBlock>
): TranslationBlock[] {
  return hiddenBlockIds.flatMap((blockId) => {
    const block =
      previousBlocks.get(blockId) ??
      page.blocks.find((candidate) => candidate.id === blockId) ??
      null;
    return block ? [block] : [];
  });
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

function refreshOverlayBlockDirtyRects(
  context: CanvasRenderingContext2D,
  page: MangaPage,
  options: {
    blocksForDirtyRect: readonly TranslationBlock[];
    editingEnabled: boolean;
    excludedBlockIds?: ReadonlySet<string>;
    fontWeightAvailability: readonly FontWeightAvailability[];
  }
): void {
  for (const blockForDirtyRect of options.blocksForDirtyRect) {
    const dirtyRect = resolveBlockCanvasDirtyRect(blockForDirtyRect, page);
    if (!dirtyRect) {
      continue;
    }

    context.clearRect(dirtyRect.x, dirtyRect.y, dirtyRect.width, dirtyRect.height);
    const intersectingBlockIds = page.blocks
      .filter((block) => !options.excludedBlockIds?.has(block.id) && doesBlockIntersectCanvasRect(block, page, dirtyRect))
      .map((block) => block.id);
    if (intersectingBlockIds.length === 0) {
      continue;
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
}

function createRenderedBlockMap(
  page: MangaPage,
  hiddenBlockIds: ReadonlySet<string>,
  previousBlocks: ReadonlyMap<string, TranslationBlock>
): Map<string, TranslationBlock> {
  return new Map(
    page.blocks.map((block) => [
      block.id,
      hiddenBlockIds.has(block.id)
        ? previousBlocks.get(block.id) ?? block
        : block
    ])
  );
}

function createRenderedBlockSummaryMap(
  page: MangaPage,
  hiddenBlockIds: ReadonlySet<string>,
  previousBlockSummaries: ReadonlyMap<string, Record<string, unknown>>
): Map<string, Record<string, unknown>> {
  return new Map(
    page.blocks.map((block) => [
      block.id,
      hiddenBlockIds.has(block.id)
        ? previousBlockSummaries.get(block.id) ?? summarizeBlockRenderState(block)
        : summarizeBlockRenderState(block)
    ])
  );
}

function createOverlayRenderKey(
  page: MangaPage,
  editingEnabled: boolean,
  fontWeightAvailability: readonly FontWeightAvailability[],
  hiddenBlockIds: ReadonlySet<string> | null,
  lastRenderedBlockSummaries: ReadonlyMap<string, Record<string, unknown>>
): string {
  const currentBlocksById = new Map(page.blocks.map((block) => [block.id, block]));
  const hiddenBlockIdSet = hiddenBlockIds && hiddenBlockIds.size > 0 ? hiddenBlockIds : null;
  const blockIds = hiddenBlockIdSet
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
    blockGroups: summarizeBlockGroupRenderState(page),
    blocks: blockIds.map((blockId) => {
      const currentBlock = currentBlocksById.get(blockId);
      if (!currentBlock) {
        return lastRenderedBlockSummaries.get(blockId) ?? { id: blockId, removed: true };
      }
      return hiddenBlockIdSet?.has(blockId)
        ? lastRenderedBlockSummaries.get(blockId) ?? summarizeBlockRenderState(currentBlock)
        : summarizeBlockRenderState(currentBlock);
    })
  });
}

function summarizeBlockGroupRenderState(page: MangaPage): Record<string, unknown>[] {
  return (page.blockGroups ?? []).map((group) => ({
    blockIds: group.blockIds,
    effects: (group.effects ?? []).map((effect) => ({
      enabled: effect.enabled,
      id: effect.id,
      settings: effect.settings,
      type: effect.type
    })),
    id: group.id
  }));
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
    shadowBlurPx: block.shadowBlurPx,
    shadowColor: block.shadowColor,
    shadowDistancePx: block.shadowDistancePx,
    shadowEnabled: block.shadowEnabled,
    shadowOpacity: block.shadowOpacity,
    sourceText: block.sourceText,
    textAlign: block.textAlign,
    textColor: block.textColor,
    textDecoration: block.textDecoration,
    textPaddingPx: block.textPaddingPx,
    textPosition: block.textPosition,
    translatedText: block.translatedText
  };
}
