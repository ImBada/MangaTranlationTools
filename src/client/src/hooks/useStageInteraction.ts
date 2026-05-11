import React from "react";
import type { BBox, ChapterSnapshot, ImageRect, MangaPage, TranslationBlock } from "../../../shared/types";
import {
  applyEditableBlockBbox,
  clampRotationDeg,
  resolveBlockRotationDeg,
  resolveEditableBlockBbox
} from "../../../shared/geometry";
import { isBlockDuplicateModifier } from "../lib/editorShortcuts";
import { resolveTranslationBlockDragBlockIds } from "../lib/blockGroups";
import { angleBetweenPointsDeg, bringTranslationBlocksToFront, bringTranslationBlockToFront, isEditableTarget } from "../lib/editorUtils";
import {
  createInpaintDebugId,
  isInpaintDebugLogEnabled,
  nowInpaintDebugMs,
  roundInpaintDebugMs,
  setActiveTranslationBlockDragDebugId,
  writeInpaintDebugLog
} from "../lib/inpaintDiagnostics";
import type { ActiveLayer } from "../lib/layerState";
import type { ViewportSize } from "../lib/overlayLayout";
import { clampStageViewScale } from "../lib/stageFit";
import { useStageSize } from "./useStageSize";

type DragMode = "move" | "resize" | "rotate";

type DragState = {
  mode: DragMode;
  blockId: string;
  blockIds: string[];
  pointerId: number;
  previewActive: boolean;
  captureElement: Element | null;
  startX: number;
  startY: number;
  startBbox: BBox;
  startBboxesByBlockId: Map<string, BBox>;
  startRotationDeg: number;
  startAngleDeg: number;
  centerX: number;
  centerY: number;
  undoRecorded: boolean;
  debug: TranslationBlockDragDebugState | null;
};

type TranslationBlockDragDebugState = {
  id: string;
  blockCount: number;
  handlerTotalMs: number;
  lastMoveAtMs: number;
  lastSummaryAtMs: number;
  maxInputGapMs: number;
  maxRectReadMs: number;
  maxTotalMs: number;
  maxUpdateMs: number;
  mode: DragMode;
  moveCount: number;
  pageId: string;
  rectReadTotalMs: number;
  slowMoveCount: number;
  startAtMs: number;
  updateTotalMs: number;
};

type UseStageInteractionOptions = {
  activeLayer: ActiveLayer;
  currentChapter: ChapterSnapshot | null;
  duplicateBlock: (block: TranslationBlock) => void;
  recordTranslationUndoSnapshot: (label: string) => boolean;
  selectedBlockId: string | null;
  selectedBlockIds: string[];
  selectedPage: MangaPage | null;
  selectedPageEditLocked: boolean;
  setSelectedBlockId: React.Dispatch<React.SetStateAction<string | null>>;
  updateCurrentChapter: (pageId: string | undefined, updater: (chapter: ChapterSnapshot) => ChapterSnapshot) => void;
};

type UseStageInteractionState = {
  activeBlockDragId: string | null;
  fitStageToWorkspace: () => void;
  handleZoomToolDrag: (scale: number) => void;
  imageRef: React.RefObject<HTMLCanvasElement | null>;
  onBlockPointerDown: (event: React.PointerEvent, block: TranslationBlock, mode: DragMode) => void;
  onSelectedBlockRangeChange: (blockId: string, rect: ImageRect) => void;
  onStagePointerMove: (event: React.PointerEvent) => void;
  onStagePointerUp: (event: React.PointerEvent) => void;
  stageRef: React.RefObject<HTMLDivElement | null>;
  stageSize: ViewportSize | null;
  stageViewResetKey: number;
  stageViewScale: number | null;
  stageZoomLabel: string;
  showOriginalStageSize: () => void;
  zoomInStage: () => void;
  zoomOutStage: () => void;
};

const STAGE_ZOOM_STEP = 1.2;
const DRAG_DEBUG_INPUT_GAP_MS = 34;
const DRAG_DEBUG_RECT_READ_MS = 4;
const DRAG_DEBUG_SUMMARY_INTERVAL_MS = 250;
const DRAG_DEBUG_TOTAL_HANDLER_MS = 12;
const DRAG_DEBUG_UPDATE_MS = 8;

function isBlockDuplicateModifierPressed(event: React.PointerEvent): boolean {
  const platform = typeof navigator === "undefined" ? "" : navigator.platform;
  return isBlockDuplicateModifier(event, platform);
}

function blurActiveEditableElement(): void {
  if (typeof document === "undefined" || typeof HTMLElement === "undefined") {
    return;
  }

  const activeElement = document.activeElement;
  if (!(activeElement instanceof HTMLElement) || !isEditableTarget(activeElement)) {
    return;
  }

  activeElement.blur();
}

function trySetPointerCapture(element: Element, pointerId: number): void {
  try {
    element.setPointerCapture(pointerId);
  } catch {
    // Some automated or synthetic pointer events do not create an active pointer.
  }
}

function tryReleasePointerCapture(element: Element | null, pointerId: number): void {
  try {
    if (element?.hasPointerCapture(pointerId)) {
      element.releasePointerCapture(pointerId);
    }
  } catch {
    // Ignore stale pointer capture state after cancellation or synthetic events.
  }
}

function summarizeDebugBbox(bbox: BBox): BBox {
  return {
    x: Math.round(bbox.x * 10) / 10,
    y: Math.round(bbox.y * 10) / 10,
    w: Math.round(bbox.w * 10) / 10,
    h: Math.round(bbox.h * 10) / 10
  };
}

function recordTranslationBlockDragMoveDebug(
  debug: TranslationBlockDragDebugState,
  detail: {
    blockFound: boolean;
    dx: number;
    dy: number;
    handlerMs: number;
    inputGapMs: number;
    nextBbox: BBox;
    nextRotationDeg: number | null;
    pointerId: number;
    rectHeight: number;
    rectReadMs: number;
    rectWidth: number;
    shiftKey: boolean;
    targetBlockIndex: number;
    updateMs: number;
  }
): void {
  debug.moveCount += 1;
  debug.slowMoveCount += isSlowTranslationBlockDragMove(detail) ? 1 : 0;
  debug.handlerTotalMs += detail.handlerMs;
  debug.rectReadTotalMs += detail.rectReadMs;
  debug.updateTotalMs += detail.updateMs;
  debug.maxInputGapMs = Math.max(debug.maxInputGapMs, detail.inputGapMs);
  debug.maxRectReadMs = Math.max(debug.maxRectReadMs, detail.rectReadMs);
  debug.maxTotalMs = Math.max(debug.maxTotalMs, detail.handlerMs);
  debug.maxUpdateMs = Math.max(debug.maxUpdateMs, detail.updateMs);
  debug.lastMoveAtMs = nowInpaintDebugMs();

  const slowReasons = resolveTranslationBlockDragSlowReasons(detail);
  if (slowReasons.length > 0 && (debug.slowMoveCount <= 8 || debug.slowMoveCount % 10 === 0)) {
    writeInpaintDebugLog("translation-block-drag:move-slow", {
      blockFound: detail.blockFound,
      blockIndex: detail.targetBlockIndex,
      blockCount: debug.blockCount,
      debugId: debug.id,
      dx: roundInpaintDebugMs(detail.dx),
      dy: roundInpaintDebugMs(detail.dy),
      handlerMs: roundInpaintDebugMs(detail.handlerMs),
      inputGapMs: roundInpaintDebugMs(detail.inputGapMs),
      mode: debug.mode,
      moveCount: debug.moveCount,
      nextBbox: summarizeDebugBbox(detail.nextBbox),
      nextRotationDeg: detail.nextRotationDeg === null ? null : roundInpaintDebugMs(detail.nextRotationDeg),
      pageId: debug.pageId,
      pointerId: detail.pointerId,
      rectHeight: Math.round(detail.rectHeight),
      rectReadMs: roundInpaintDebugMs(detail.rectReadMs),
      rectWidth: Math.round(detail.rectWidth),
      shiftKey: detail.shiftKey,
      slowMoveCount: debug.slowMoveCount,
      slowReasons,
      updateMs: roundInpaintDebugMs(detail.updateMs)
    });
  }

  const now = nowInpaintDebugMs();
  if (now - debug.lastSummaryAtMs < DRAG_DEBUG_SUMMARY_INTERVAL_MS && debug.moveCount % 20 !== 0) {
    return;
  }
  debug.lastSummaryAtMs = now;
  writeInpaintDebugLog("translation-block-drag:move-summary", summarizeTranslationBlockDragDebug(debug));
}

function isSlowTranslationBlockDragMove(detail: {
  handlerMs: number;
  inputGapMs: number;
  rectReadMs: number;
  updateMs: number;
}): boolean {
  return (
    detail.handlerMs >= DRAG_DEBUG_TOTAL_HANDLER_MS ||
    detail.inputGapMs >= DRAG_DEBUG_INPUT_GAP_MS ||
    detail.rectReadMs >= DRAG_DEBUG_RECT_READ_MS ||
    detail.updateMs >= DRAG_DEBUG_UPDATE_MS
  );
}

function resolveTranslationBlockDragSlowReasons(detail: {
  handlerMs: number;
  inputGapMs: number;
  rectReadMs: number;
  updateMs: number;
}): string[] {
  const reasons: string[] = [];
  if (detail.inputGapMs >= DRAG_DEBUG_INPUT_GAP_MS) {
    reasons.push("pointer-event-gap");
  }
  if (detail.handlerMs >= DRAG_DEBUG_TOTAL_HANDLER_MS) {
    reasons.push("pointermove-handler");
  }
  if (detail.rectReadMs >= DRAG_DEBUG_RECT_READ_MS) {
    reasons.push("stage-layout-read");
  }
  if (detail.updateMs >= DRAG_DEBUG_UPDATE_MS) {
    reasons.push("chapter-state-update");
  }
  return reasons;
}

function summarizeTranslationBlockDragDebug(debug: TranslationBlockDragDebugState): Record<string, unknown> {
  const elapsedMs = Math.max(0, nowInpaintDebugMs() - debug.startAtMs);
  const moveCount = Math.max(1, debug.moveCount);
  return {
    averageHandlerMs: roundInpaintDebugMs(debug.handlerTotalMs / moveCount),
    averageRectReadMs: roundInpaintDebugMs(debug.rectReadTotalMs / moveCount),
    averageUpdateMs: roundInpaintDebugMs(debug.updateTotalMs / moveCount),
    blockCount: debug.blockCount,
    debugId: debug.id,
    elapsedMs: roundInpaintDebugMs(elapsedMs),
    maxInputGapMs: roundInpaintDebugMs(debug.maxInputGapMs),
    maxRectReadMs: roundInpaintDebugMs(debug.maxRectReadMs),
    maxTotalMs: roundInpaintDebugMs(debug.maxTotalMs),
    maxUpdateMs: roundInpaintDebugMs(debug.maxUpdateMs),
    mode: debug.mode,
    moveCount: debug.moveCount,
    pageId: debug.pageId,
    slowMoveCount: debug.slowMoveCount
  };
}

export function useStageInteraction({
  activeLayer,
  currentChapter,
  duplicateBlock,
  recordTranslationUndoSnapshot,
  selectedBlockId,
  selectedBlockIds,
  selectedPage,
  selectedPageEditLocked,
  setSelectedBlockId,
  updateCurrentChapter
}: UseStageInteractionOptions): UseStageInteractionState {
  const [stageViewScale, setStageViewScale] = React.useState<number | null>(null);
  const [stageViewResetKey, setStageViewResetKey] = React.useState(0);
  const [activeBlockDragId, setActiveBlockDragId] = React.useState<string | null>(null);
  const stageRef = React.useRef<HTMLDivElement | null>(null);
  const imageRef = React.useRef<HTMLCanvasElement | null>(null);
  const dragRef = React.useRef<DragState | null>(null);
  const selectedPageSize = React.useMemo(
    () => (selectedPage ? { width: selectedPage.width, height: selectedPage.height } : null),
    [selectedPage?.height, selectedPage?.width]
  );
  const stageSize = useStageSize(imageRef, selectedPageSize);
  const currentStageScale = typeof stageViewScale === "number"
    ? stageViewScale
    : selectedPage && stageSize
      ? stageSize.width / Math.max(1, selectedPage.width)
      : 1;
  const stageZoomLabel = `${Math.round(currentStageScale * 100)}%`;

  React.useEffect(() => () => {
    setActiveTranslationBlockDragDebugId(null);
  }, []);

  const zoomStage = React.useCallback((factor: number) => {
    setStageViewScale((current) => clampStageViewScale((current ?? currentStageScale) * factor));
  }, [currentStageScale]);

  const handleZoomToolDrag = React.useCallback((scale: number) => {
    setStageViewScale(clampStageViewScale(scale));
  }, []);

  const zoomInStage = React.useCallback(() => {
    zoomStage(STAGE_ZOOM_STEP);
  }, [zoomStage]);

  const zoomOutStage = React.useCallback(() => {
    zoomStage(1 / STAGE_ZOOM_STEP);
  }, [zoomStage]);

  const fitStageToWorkspace = React.useCallback(() => {
    setStageViewScale(null);
    setStageViewResetKey((current) => current + 1);
  }, []);

  const showOriginalStageSize = React.useCallback(() => {
    setStageViewScale(1);
  }, []);

  const onBlockPointerDown = React.useCallback((event: React.PointerEvent, block: TranslationBlock, mode: DragMode) => {
    const debugEnabled = isInpaintDebugLogEnabled();
    if (!stageRef.current || selectedPageEditLocked || activeLayer !== "overlay") {
      if (debugEnabled) {
        writeInpaintDebugLog("translation-block-drag:start-skip", {
          activeLayer,
          blockId: block.id,
          hasStage: Boolean(stageRef.current),
          mode,
          selectedPageEditLocked
        });
      }
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    blurActiveEditableElement();
    if (mode === "move" && isBlockDuplicateModifierPressed(event)) {
      if (debugEnabled) {
        writeInpaintDebugLog("translation-block-drag:duplicate-shortcut", {
          blockId: block.id,
          pageId: selectedPage?.id,
          pointerId: event.pointerId
        });
      }
      duplicateBlock(block);
      return;
    }
    setSelectedBlockId(block.id);
    const target = resolveEditableBlockBbox(block);
    const dragBlockIds = mode === "move" && selectedPage
      ? resolveTranslationBlockDragBlockIds(selectedPage, block.id, selectedBlockId, selectedBlockIds)
      : [block.id];
    const startBboxesByBlockId = new Map<string, BBox>(
      dragBlockIds.flatMap((blockId) => {
        const dragBlock = selectedPage?.blocks.find((candidate) => candidate.id === blockId) ?? (blockId === block.id ? block : null);
        return dragBlock ? [[blockId, resolveEditableBlockBbox(dragBlock).bbox] as [string, BBox]] : [];
      })
    );
    if (!startBboxesByBlockId.has(block.id)) {
      startBboxesByBlockId.set(block.id, target.bbox);
    }
    const stageRect = stageRef.current.getBoundingClientRect();
    const centerX = stageRect.left + ((target.bbox.x + target.bbox.w / 2) / 1000) * stageRect.width;
    const centerY = stageRect.top + ((target.bbox.y + target.bbox.h / 2) / 1000) * stageRect.height;
    const debugStartMs = debugEnabled && selectedPage ? nowInpaintDebugMs() : 0;
    const debug = debugStartMs > 0 && selectedPage
      ? {
          id: createInpaintDebugId("translation-block-drag"),
          blockCount: selectedPage.blocks.length,
          handlerTotalMs: 0,
          lastMoveAtMs: debugStartMs,
          lastSummaryAtMs: debugStartMs,
          maxInputGapMs: 0,
          maxRectReadMs: 0,
          maxTotalMs: 0,
          maxUpdateMs: 0,
          mode,
          moveCount: 0,
          pageId: selectedPage.id,
          rectReadTotalMs: 0,
          slowMoveCount: 0,
          startAtMs: debugStartMs,
          updateTotalMs: 0
        }
      : null;
    if (debug) {
      setActiveTranslationBlockDragDebugId(debug.id);
      writeInpaintDebugLog("translation-block-drag:start", {
        blockCount: debug.blockCount,
        blockId: block.id,
        bringToFrontDeferred: true,
        debugId: debug.id,
        mode,
        orderUndoRecorded: false,
        pageId: debug.pageId,
        pointerId: event.pointerId,
        shiftKey: event.shiftKey,
        stageRect: {
          height: Math.round(stageRect.height),
          width: Math.round(stageRect.width)
        },
        startBbox: summarizeDebugBbox(target.bbox)
      });
    }
    dragRef.current = {
      mode,
      blockId: block.id,
      blockIds: [...startBboxesByBlockId.keys()],
      pointerId: event.pointerId,
      previewActive: false,
      captureElement: event.currentTarget,
      startX: event.clientX,
      startY: event.clientY,
      startBbox: target.bbox,
      startBboxesByBlockId,
      startRotationDeg: resolveBlockRotationDeg(block),
      startAngleDeg: angleBetweenPointsDeg(centerX, centerY, event.clientX, event.clientY),
      centerX,
      centerY,
      undoRecorded: false,
      debug
    };
    trySetPointerCapture(event.currentTarget, event.pointerId);
  }, [activeLayer, duplicateBlock, selectedBlockId, selectedBlockIds, selectedPage, selectedPageEditLocked, setSelectedBlockId]);

  const onStagePointerMove = React.useCallback((event: React.PointerEvent) => {
    const drag = dragRef.current;
    const page = selectedPage;
    const stage = stageRef.current;
    if (!drag || !page || !stage || !currentChapter || selectedPageEditLocked) {
      return;
    }
    if (event.pointerId !== drag.pointerId) {
      return;
    }
    const debug = drag.debug;
    const handlerStartMs = debug ? nowInpaintDebugMs() : 0;
    const inputGapMs = debug ? Math.max(0, handlerStartMs - debug.lastMoveAtMs) : 0;
    const rectStartMs = debug ? nowInpaintDebugMs() : 0;
    const rect = stage.getBoundingClientRect();
    const rectReadMs = debug ? nowInpaintDebugMs() - rectStartMs : 0;
    const dx = ((event.clientX - drag.startX) / Math.max(1, rect.width)) * 1000;
    const dy = ((event.clientY - drag.startY) / Math.max(1, rect.height)) * 1000;
    const moveDy = event.shiftKey ? 0 : dy;
    const next =
      drag.mode === "move"
        ? {
            ...drag.startBbox,
            x: drag.startBbox.x + dx,
            y: drag.startBbox.y + moveDy
          }
        : drag.mode === "resize"
          ? {
              ...drag.startBbox,
              w: drag.startBbox.w + dx,
              h: drag.startBbox.h + dy
            }
          : drag.startBbox;
    const nextRotationDeg =
      drag.mode === "rotate"
        ? clampRotationDeg(drag.startRotationDeg + angleBetweenPointsDeg(drag.centerX, drag.centerY, event.clientX, event.clientY) - drag.startAngleDeg)
        : null;

    if (!drag.previewActive) {
      setActiveBlockDragId(drag.blockId);
      drag.previewActive = true;
    }

    if (!drag.undoRecorded) {
      recordTranslationUndoSnapshot(drag.mode === "move" && drag.blockIds.length > 1 ? "번역 블록 여러 개 위치 변경" : "번역 블록 위치 변경");
      drag.undoRecorded = true;
    }

    const targetBlockIndex = page.blocks.findIndex((block) => block.id === drag.blockId);
    const blockFound = targetBlockIndex >= 0;
    const updateStartMs = debug ? nowInpaintDebugMs() : 0;
    updateCurrentChapter(page.id, (chapter) => ({
      ...chapter,
      pages: chapter.pages.map((candidate) =>
        candidate.id !== page.id
          ? candidate
          : {
              ...candidate,
              updatedAt: new Date().toISOString(),
              blocks: (drag.mode === "move" && drag.blockIds.length > 1
                ? bringTranslationBlocksToFront(candidate.blocks, drag.blockIds)
                : bringTranslationBlockToFront(candidate.blocks, drag.blockId)
              ).map((block) => {
                if (drag.mode === "move") {
                  const startBbox = drag.startBboxesByBlockId.get(block.id);
                  return startBbox
                    ? applyEditableBlockBbox(block, {
                        ...startBbox,
                        x: startBbox.x + dx,
                        y: startBbox.y + moveDy
                      })
                    : block;
                }
                if (block.id !== drag.blockId) {
                  return block;
                }
                return nextRotationDeg === null
                  ? applyEditableBlockBbox(block, next)
                  : { ...block, rotationDeg: nextRotationDeg };
              })
            }
      )
    }));
    if (debug) {
      const updateMs = nowInpaintDebugMs() - updateStartMs;
      const handlerMs = nowInpaintDebugMs() - handlerStartMs;
      recordTranslationBlockDragMoveDebug(debug, {
        blockFound,
        dx,
        dy,
        handlerMs,
        inputGapMs,
        nextBbox: next,
        nextRotationDeg,
        pointerId: event.pointerId,
        rectHeight: rect.height,
        rectReadMs,
        rectWidth: rect.width,
        shiftKey: event.shiftKey,
        targetBlockIndex,
        updateMs
      });
    }
  }, [currentChapter, recordTranslationUndoSnapshot, selectedPage, selectedPageEditLocked, updateCurrentChapter]);

  const onStagePointerUp = React.useCallback((event: React.PointerEvent) => {
    const drag = dragRef.current;
    if (drag && event.pointerId !== drag.pointerId) {
      return;
    }
    tryReleasePointerCapture(drag?.captureElement ?? null, event.pointerId);
    dragRef.current = null;
    setActiveBlockDragId(null);
    if (drag?.debug) {
      writeInpaintDebugLog("translation-block-drag:end", {
        ...summarizeTranslationBlockDragDebug(drag.debug),
        pointerId: event.pointerId,
        pointerMatchesDrag: event.pointerId === drag.pointerId
      });
      setActiveTranslationBlockDragDebugId(null);
    }
  }, []);

  const onSelectedBlockRangeChange = React.useCallback((blockId: string, selectionRect: ImageRect) => {
    const page = selectedPage;
    if (!page || !currentChapter || selectedPageEditLocked) {
      return;
    }
    const nextBbox = {
      x: (selectionRect.x / Math.max(1, page.width)) * 1000,
      y: (selectionRect.y / Math.max(1, page.height)) * 1000,
      w: (selectionRect.width / Math.max(1, page.width)) * 1000,
      h: (selectionRect.height / Math.max(1, page.height)) * 1000
    };

    recordTranslationUndoSnapshot("번역 블록 범위 변경");
    setSelectedBlockId(blockId);
    updateCurrentChapter(page.id, (chapter) => ({
      ...chapter,
      pages: chapter.pages.map((candidate) =>
        candidate.id !== page.id
          ? candidate
          : {
              ...candidate,
              updatedAt: new Date().toISOString(),
              blocks: candidate.blocks.map((block) =>
                block.id === blockId ? applyEditableBlockBbox(block, nextBbox) : block
              )
            }
      )
    }));
  }, [currentChapter, recordTranslationUndoSnapshot, selectedPage, selectedPageEditLocked, setSelectedBlockId, updateCurrentChapter]);

  return {
    activeBlockDragId,
    fitStageToWorkspace,
    handleZoomToolDrag,
    imageRef,
    onBlockPointerDown,
    onSelectedBlockRangeChange,
    onStagePointerMove,
    onStagePointerUp,
    stageRef,
    stageSize,
    stageViewResetKey,
    stageViewScale,
    stageZoomLabel,
    showOriginalStageSize,
    zoomInStage,
    zoomOutStage
  };
}
