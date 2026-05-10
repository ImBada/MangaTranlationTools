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
    canvas.width = width;
    canvas.height = height;

    context.setTransform(1, 0, 0, 1, 0, 0);
    context.clearRect(0, 0, width, height);
    const debugEnabled = isInpaintDebugLogEnabled();
    const drawStartMs = debugEnabled ? nowInpaintDebugMs() : 0;
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
    lastRenderedBlocksRef.current = new Map(currentPage.blocks.map((block) => [block.id, block]));
    lastRenderedBlockSummariesRef.current = new Map(
      currentPage.blocks.map((block) => [block.id, summarizeBlockRenderState(block)])
    );
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

    const dirtyRect = resolveBlockCanvasDirtyRect(blockForDirtyRect, currentPage);
    if (!dirtyRect) {
      lastHiddenBlockIdRef.current = hiddenBlockId ?? null;
      return;
    }

    context.clearRect(dirtyRect.x, dirtyRect.y, dirtyRect.width, dirtyRect.height);
    const intersectingBlockIds = currentPage.blocks
      .filter((block) => block.id !== hiddenBlockId && doesBlockIntersectCanvasRect(block, currentPage, dirtyRect))
      .map((block) => block.id);
    if (intersectingBlockIds.length === 0) {
      lastHiddenBlockIdRef.current = hiddenBlockId ?? null;
      return;
    }

    context.save();
    context.beginPath();
    context.rect(dirtyRect.x, dirtyRect.y, dirtyRect.width, dirtyRect.height);
    context.clip();
    drawOverlayBlocks(context, currentPage, {
      renderSize: { width: currentPage.width, height: currentPage.height },
      editingEnabled: currentEditingEnabled,
      includedBlockIds: new Set(intersectingBlockIds),
      fontWeightAvailability: currentFontWeightAvailability
    });
    context.restore();
    lastHiddenBlockIdRef.current = hiddenBlockId ?? null;
  }, [canvasRef, hiddenBlockId]);

  return <canvas ref={canvasRef} className="overlay-render-canvas" aria-hidden="true" />;
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
