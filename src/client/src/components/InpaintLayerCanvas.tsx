import React, { useRef, useState } from "react";
import type { ImageRect } from "../../../shared/types";
import { useCanvasImageSync, type CanvasImageSyncState } from "../hooks/useCanvasImageSync";
import { encodeCanvasSnapshotDataUrl } from "../lib/canvasSnapshotEncoding";
import { createUndoCanvasSnapshot, isInlineDataUrl } from "../lib/inpaintCanvasUndo";
import {
  createMaskIslandSelection,
  drawMaskSegment,
  eraseSelectedMaskIslands,
  renderMaskIslandSelectionPreview,
  resolveCanvasPoint,
  resolveSelectionRect,
  restoreMaskIslandSelection,
  selectTouchedMaskIslands,
  type DrawPoint,
  type MaskIslandSelection,
  type SelectionDragState
} from "../lib/inpaintLayerCanvas";
import {
  createInpaintDebugId,
  summarizeCanvasSyncState,
  summarizeDataUrl,
  summarizeError,
  writeInpaintDebugLog
} from "../lib/inpaintDiagnostics";
import type { InpaintLayerChangeOptions } from "../lib/inpaintLayerChange";
import { renderInpaintMaskCanvasForDisplay } from "../lib/inpaintMaskImages";

export type InpaintTool = "select" | "brush" | "eraser" | "autoEraser";

type InpaintLayerCanvasProps = {
  dataUrl?: string;
  pageSize: {
    width: number;
    height: number;
  };
  tool: InpaintTool;
  brushSize: number;
  disabled: boolean;
  selectionRect: ImageRect | null;
  onChange: (dataUrl: string | undefined, options?: InpaintLayerChangeOptions) => void;
  onEditEnd?: () => void;
  onEditStart?: () => void;
  onSelectionChange: (rect: ImageRect | null) => void;
};

type PendingMaskCanvasCommit = {
  brushSize: number;
  commitId: string;
  nextDataUrl: string | undefined;
  onChange: InpaintLayerCanvasProps["onChange"];
  previousDataUrl: string | undefined;
  resolved: boolean;
  sourceState: CanvasImageSyncState;
  tool: InpaintTool;
};

export function InpaintLayerCanvas({
  dataUrl,
  pageSize,
  tool,
  brushSize,
  disabled,
  selectionRect,
  onChange,
  onEditEnd,
  onEditStart,
  onSelectionChange
}: InpaintLayerCanvasProps): React.JSX.Element {
  const {
    canvasRef,
    drawingRef,
    markCanvasCommitted,
    markCanvasEdited,
    readCanvasSourceState,
    readCommittedCanvasState
  } = useCanvasImageSync({
    afterDraw: renderInpaintMaskCanvasForDisplay,
    dataUrl,
    loadErrorMessage: "인페인트 마스크를 불러오지 못했습니다.",
    pageSize,
    willReadFrequently: true
  });
  const changedRef = useRef(false);
  const lastPointRef = useRef<DrawPoint | null>(null);
  const autoEraseSelectionRef = useRef<MaskIslandSelection | null>(null);
  const selectionDragRef = useRef<SelectionDragState | null>(null);
  const undoDataUrlRef = useRef<string | undefined>(undefined);
  const undoCanvasSnapshotRef = useRef<HTMLCanvasElement | null>(null);
  const editSessionActiveRef = useRef(false);
  const activePointerIdRef = useRef<number | null>(null);
  const pendingCommitsRef = useRef<PendingMaskCanvasCommit[]>([]);
  const [previewSelectionRect, setPreviewSelectionRect] = useState<ImageRect | null>(null);

  const pointerEnabled = !disabled && tool !== "select";
  const autoEraseEnabled = !disabled && tool === "autoEraser";
  const selectionEnabled = !disabled && tool === "select";
  const activeSelectionRect = previewSelectionRect ?? selectionRect;

  const drawSegment = (from: DrawPoint, to: DrawPoint) => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d");
    if (!canvas || !context) {
      return;
    }

    drawMaskSegment(context, from, to, brushSize, tool === "eraser");
    changedRef.current = true;
  };

  const resolvePoint = (event: React.PointerEvent<HTMLCanvasElement>): DrawPoint => {
    return resolveCanvasPoint(event.clientX, event.clientY, event.currentTarget.getBoundingClientRect(), pageSize);
  };

  const startEditSession = React.useCallback(() => {
    if (editSessionActiveRef.current) {
      return;
    }
    editSessionActiveRef.current = true;
    onEditStart?.();
  }, [onEditStart]);

  const endEditSession = React.useCallback(() => {
    if (!editSessionActiveRef.current) {
      return;
    }
    editSessionActiveRef.current = false;
    onEditEnd?.();
  }, [onEditEnd]);

  React.useEffect(() => {
    return () => {
      endEditSession();
    };
  }, [endEditSession]);

  const flushResolvedMaskCommits = React.useCallback(() => {
    const queue = pendingCommitsRef.current;
    writeInpaintDebugLog("inpaint-mask:commit-flush-start", {
      queueLength: queue.length,
      ready: queue.filter((commit) => commit.resolved).length
    });
    while (queue.length > 0 && queue[0].resolved) {
      const commit = queue[0];
      queue.shift();
      const marked = markCanvasCommitted(commit.nextDataUrl, commit.sourceState);
      writeInpaintDebugLog("inpaint-mask:commit-apply", {
        brushSize: commit.brushSize,
        commitId: commit.commitId,
        marked,
        nextDataUrl: summarizeDataUrl(commit.nextDataUrl),
        previousDataUrl: summarizeDataUrl(commit.previousDataUrl),
        queueRemaining: queue.length,
        sourceState: summarizeCanvasSyncState(commit.sourceState),
        tool: commit.tool
      });
      commit.onChange(commit.nextDataUrl, {
        previousDataUrl: commit.previousDataUrl,
        previousMaskSourceDataUrl: commit.sourceState.dataUrl
      });

      if (queue.length > 0) {
        queue[0].previousDataUrl = commit.nextDataUrl;
        writeInpaintDebugLog("inpaint-mask:commit-carry-forward", {
          fromCommitId: commit.commitId,
          nextCommitId: queue[0].commitId,
          nextPreviousDataUrl: summarizeDataUrl(queue[0].previousDataUrl)
        });
      }
    }
  }, [markCanvasCommitted]);

  const commitMaskCanvasSnapshot = (canvas: HTMLCanvasElement, previousDataUrl: string | undefined) => {
    const sourceState = readCanvasSourceState();
    const commitId = createInpaintDebugId("mask-commit");
    const previousSnapshot = undoCanvasSnapshotRef.current;
    undoCanvasSnapshotRef.current = null;
    const previousDataUrlPromise = previousSnapshot
      ? encodeCanvasSnapshotDataUrl(previousSnapshot, { mode: "mask" })
      : Promise.resolve(previousDataUrl);
    const commit: PendingMaskCanvasCommit = {
      brushSize,
      commitId,
      nextDataUrl: undefined,
      onChange,
      previousDataUrl,
      resolved: false,
      sourceState,
      tool
    };
    pendingCommitsRef.current.push(commit);
    writeInpaintDebugLog("inpaint-mask:commit-enqueue", {
      brushSize,
      commitId,
      previousDataUrl: summarizeDataUrl(previousDataUrl),
      queueLength: pendingCommitsRef.current.length,
      sourceState: summarizeCanvasSyncState(sourceState),
      tool
    });
    void Promise.all([previousDataUrlPromise, encodeCanvasSnapshotDataUrl(canvas, { mode: "mask" })])
      .then(([resolvedPreviousDataUrl, nextDataUrl]) => {
        commit.previousDataUrl = resolvedPreviousDataUrl;
        commit.nextDataUrl = nextDataUrl;
        commit.resolved = true;
        writeInpaintDebugLog("inpaint-mask:commit-encoded", {
          commitId,
          nextDataUrl: summarizeDataUrl(nextDataUrl),
          previousDataUrl: summarizeDataUrl(resolvedPreviousDataUrl),
          queueLength: pendingCommitsRef.current.length
        });
        flushResolvedMaskCommits();
      })
      .catch((error) => {
        console.error(error);
        const fallbackDataUrl = isInlineDataUrl(previousDataUrl) ? previousDataUrl : undefined;
        commit.previousDataUrl = fallbackDataUrl;
        commit.nextDataUrl = fallbackDataUrl;
        commit.resolved = true;
        writeInpaintDebugLog("inpaint-mask:commit-encode-error", {
          commitId,
          error: summarizeError(error),
          fallbackDataUrl: summarizeDataUrl(fallbackDataUrl),
          queueLength: pendingCommitsRef.current.length
        });
        flushResolvedMaskCommits();
      });
  };

  const captureCanvasPointer = (canvas: HTMLCanvasElement, pointerId: number) => {
    try {
      canvas.setPointerCapture(pointerId);
    } catch {
      // The pointer may have started outside this canvas; drawing can continue without capture.
    }
  };

  const releaseCanvasPointer = (canvas: HTMLCanvasElement, pointerId: number) => {
    if (canvas.hasPointerCapture(pointerId)) {
      canvas.releasePointerCapture(pointerId);
    }
  };

  const commitChange = () => {
    const canvas = canvasRef.current;
    if (!canvas || !changedRef.current) {
      undoCanvasSnapshotRef.current = null;
      writeInpaintDebugLog("inpaint-mask:commit-skip", {
        changed: changedRef.current,
        hasCanvas: Boolean(canvas),
        pendingQueueLength: pendingCommitsRef.current.length,
        tool
      });
      return;
    }

    const previousDataUrl = undoDataUrlRef.current;
    undoDataUrlRef.current = undefined;
    changedRef.current = false;
    writeInpaintDebugLog("inpaint-mask:commit-request", {
      brushSize,
      previousDataUrl: summarizeDataUrl(previousDataUrl),
      tool
    });
    commitMaskCanvasSnapshot(canvas, previousDataUrl);
  };

  const updateAutoEraseSelection = (canvas: HTMLCanvasElement, from: DrawPoint, to: DrawPoint = from) => {
    const selection = autoEraseSelectionRef.current;
    if (!selection) {
      return;
    }

    const distance = Math.hypot(to.x - from.x, to.y - from.y);
    const steps = Math.max(1, Math.ceil(distance / Math.max(1, brushSize / 3)));
    let changed = false;
    for (let step = 0; step <= steps; step += 1) {
      const ratio = step / steps;
      changed = selectTouchedMaskIslands(selection, {
        x: from.x + (to.x - from.x) * ratio,
        y: from.y + (to.y - from.y) * ratio
      }, brushSize) || changed;
    }

    if (changed) {
      renderMaskIslandSelectionPreview(canvas, selection);
    }
  };

  const cancelAutoEraseSelection = (canvas: HTMLCanvasElement) => {
    const selection = autoEraseSelectionRef.current;
    autoEraseSelectionRef.current = null;
    const previousDataUrl = undoDataUrlRef.current;
    undoCanvasSnapshotRef.current = null;
    if (selection) {
      restoreMaskIslandSelection(canvas, selection);
    }
    undoDataUrlRef.current = undefined;
    drawingRef.current = false;
    lastPointRef.current = null;
    const marked = markCanvasCommitted(previousDataUrl);
    writeInpaintDebugLog("inpaint-mask:auto-eraser-cancel", {
      marked,
      previousDataUrl: summarizeDataUrl(previousDataUrl),
      restoredSelection: Boolean(selection)
    });
  };

  const commitAutoEraseSelection = (canvas: HTMLCanvasElement) => {
    const selection = autoEraseSelectionRef.current;
    if (!selection) {
      writeInpaintDebugLog("inpaint-mask:auto-eraser-commit-skip", {
        reason: "missing-selection"
      });
      return;
    }

    autoEraseSelectionRef.current = null;
    if (!eraseSelectedMaskIslands(canvas, selection)) {
      const previousDataUrl = undoDataUrlRef.current;
      undoCanvasSnapshotRef.current = null;
      restoreMaskIslandSelection(canvas, selection);
      undoDataUrlRef.current = undefined;
      drawingRef.current = false;
      lastPointRef.current = null;
      const marked = markCanvasCommitted(previousDataUrl);
      writeInpaintDebugLog("inpaint-mask:auto-eraser-commit-skip", {
        marked,
        previousDataUrl: summarizeDataUrl(previousDataUrl),
        reason: "no-selected-island-erased"
      });
      return;
    }

    const previousDataUrl = undoDataUrlRef.current;
    undoDataUrlRef.current = undefined;
    drawingRef.current = false;
    lastPointRef.current = null;
    writeInpaintDebugLog("inpaint-mask:auto-eraser-commit-request", {
      brushSize,
      previousDataUrl: summarizeDataUrl(previousDataUrl)
    });
    commitMaskCanvasSnapshot(canvas, previousDataUrl);
  };

  const startMaskStroke = (canvas: HTMLCanvasElement, point: DrawPoint, pointerId: number) => {
    if (autoEraseEnabled) {
      const selection = createMaskIslandSelection(canvas);
      if (!selection) {
        writeInpaintDebugLog("inpaint-mask:stroke-start-skip", {
          brushSize,
          point: summarizePoint(point),
          reason: "missing-auto-erase-selection",
          tool
        });
        return;
      }
      captureCanvasPointer(canvas, pointerId);
      undoDataUrlRef.current = readCommittedCanvasState()?.dataUrl ?? dataUrl;
      undoCanvasSnapshotRef.current = createUndoCanvasSnapshot(canvas, undoDataUrlRef.current);
      writeInpaintDebugLog("inpaint-mask:stroke-start", {
        brushSize,
        mode: "auto-eraser",
        point: summarizePoint(point),
        pointerId,
        previousDataUrl: summarizeDataUrl(undoDataUrlRef.current),
        queueLength: pendingCommitsRef.current.length,
        tool
      });
      startEditSession();
      autoEraseSelectionRef.current = selection;
      markCanvasEdited();
      drawingRef.current = true;
      activePointerIdRef.current = pointerId;
      lastPointRef.current = point;
      updateAutoEraseSelection(canvas, point);
      return;
    }

    captureCanvasPointer(canvas, pointerId);
    undoDataUrlRef.current = readCommittedCanvasState()?.dataUrl ?? dataUrl;
    undoCanvasSnapshotRef.current = createUndoCanvasSnapshot(canvas, undoDataUrlRef.current);
    writeInpaintDebugLog("inpaint-mask:stroke-start", {
      brushSize,
      mode: "draw",
      point: summarizePoint(point),
      pointerId,
      previousDataUrl: summarizeDataUrl(undoDataUrlRef.current),
      queueLength: pendingCommitsRef.current.length,
      tool
    });
    startEditSession();
    markCanvasEdited();
    drawingRef.current = true;
    activePointerIdRef.current = pointerId;
    lastPointRef.current = point;
    drawSegment(point, point);
  };

  const finishMaskStroke = (pointerId: number | null = null, point: DrawPoint | null = null) => {
    if (!drawingRef.current) {
      writeInpaintDebugLog("inpaint-mask:stroke-finish-skip", {
        pointerId,
        reason: "not-drawing",
        tool
      });
      return;
    }
    const activePointerId = activePointerIdRef.current;
    if (activePointerId !== null && pointerId !== null && activePointerId !== pointerId) {
      writeInpaintDebugLog("inpaint-mask:stroke-finish-skip", {
        activePointerId,
        pointerId,
        reason: "pointer-mismatch",
        tool
      });
      return;
    }

    const canvas = canvasRef.current;
    if (canvas && activePointerId !== null) {
      releaseCanvasPointer(canvas, activePointerId);
    }
    if (canvas && autoEraseSelectionRef.current) {
      if (point) {
        updateAutoEraseSelection(canvas, lastPointRef.current ?? point, point);
      }
      writeInpaintDebugLog("inpaint-mask:stroke-finish", {
        activePointerId,
        mode: "auto-eraser",
        point: summarizePoint(point),
        tool
      });
      commitAutoEraseSelection(canvas);
      activePointerIdRef.current = null;
      endEditSession();
      return;
    }

    drawingRef.current = false;
    activePointerIdRef.current = null;
    lastPointRef.current = null;
    writeInpaintDebugLog("inpaint-mask:stroke-finish", {
      activePointerId,
      mode: "draw",
      point: summarizePoint(point),
      tool
    });
    commitChange();
    endEditSession();
  };

  const cancelMaskStroke = (pointerId: number | null = null) => {
    if (!drawingRef.current) {
      writeInpaintDebugLog("inpaint-mask:stroke-cancel-skip", {
        pointerId,
        reason: "not-drawing",
        tool
      });
      return;
    }
    const activePointerId = activePointerIdRef.current;
    if (activePointerId !== null && pointerId !== null && activePointerId !== pointerId) {
      writeInpaintDebugLog("inpaint-mask:stroke-cancel-skip", {
        activePointerId,
        pointerId,
        reason: "pointer-mismatch",
        tool
      });
      return;
    }

    const canvas = canvasRef.current;
    if (canvas && activePointerId !== null) {
      releaseCanvasPointer(canvas, activePointerId);
    }
    activePointerIdRef.current = null;
    if (canvas && autoEraseSelectionRef.current) {
      writeInpaintDebugLog("inpaint-mask:stroke-cancel", {
        activePointerId,
        mode: "auto-eraser",
        tool
      });
      cancelAutoEraseSelection(canvas);
      endEditSession();
      return;
    }

    drawingRef.current = false;
    lastPointRef.current = null;
    writeInpaintDebugLog("inpaint-mask:stroke-cancel", {
      activePointerId,
      mode: "draw",
      tool
    });
    commitChange();
    endEditSession();
  };

  React.useEffect(() => {
    if (!disabled || !drawingRef.current) {
      return;
    }
    finishMaskStroke(activePointerIdRef.current);
  }, [disabled, finishMaskStroke]);

  React.useEffect(() => {
    const shouldIgnoreInactivePointer = (event: PointerEvent) => {
      if (!drawingRef.current) {
        return true;
      }
      const activePointerId = activePointerIdRef.current;
      return activePointerId !== null && activePointerId !== event.pointerId;
    };

    const finishActivePointer = (event: PointerEvent) => {
      if (shouldIgnoreInactivePointer(event)) {
        return;
      }
      const canvas = canvasRef.current;
      if (canvas && event.composedPath().includes(canvas)) {
        return;
      }
      const point = canvas
        ? resolveCanvasPoint(event.clientX, event.clientY, canvas.getBoundingClientRect(), pageSize)
        : null;
      finishMaskStroke(event.pointerId, point);
    };
    const cancelActivePointer = (event: PointerEvent) => {
      if (shouldIgnoreInactivePointer(event)) {
        return;
      }
      const canvas = canvasRef.current;
      if (canvas && event.composedPath().includes(canvas)) {
        return;
      }
      cancelMaskStroke(event.pointerId);
    };

    window.addEventListener("pointerup", finishActivePointer, true);
    window.addEventListener("pointercancel", cancelActivePointer, true);
    return () => {
      window.removeEventListener("pointerup", finishActivePointer, true);
      window.removeEventListener("pointercancel", cancelActivePointer, true);
    };
  }, [cancelMaskStroke, finishMaskStroke, pageSize]);

  return (
    <>
      <canvas
        ref={canvasRef}
        className={`inpaint-layer ${pointerEnabled ? "editing" : ""} ${selectionEnabled ? "selecting" : ""}`.trim()}
        aria-label="인페인트 레이어"
        onPointerDown={(event) => {
          if (selectionEnabled) {
            event.preventDefault();
            event.stopPropagation();
            event.currentTarget.setPointerCapture(event.pointerId);
            const point = resolvePoint(event);
            selectionDragRef.current = { start: point, current: point };
            setPreviewSelectionRect(resolveSelectionRect(point, point, pageSize));
            return;
          }
          if (!pointerEnabled) {
            return;
          }
          event.preventDefault();
          event.stopPropagation();
          const point = resolvePoint(event);
          startMaskStroke(event.currentTarget, point, event.pointerId);
        }}
        onPointerMove={(event) => {
          if (selectionEnabled) {
            event.preventDefault();
            event.stopPropagation();
            const point = resolvePoint(event);
            const drag = selectionDragRef.current;
            if (drag) {
              drag.current = point;
              setPreviewSelectionRect(resolveSelectionRect(drag.start, point, pageSize));
            }
            return;
          }
          if (!pointerEnabled && !drawingRef.current) {
            return;
          }
          event.preventDefault();
          event.stopPropagation();
          const point = resolvePoint(event);
          if (drawingRef.current && (disabled || (event.buttons & 1) === 0)) {
            finishMaskStroke(event.pointerId, point);
            return;
          }
          if (!drawingRef.current || !lastPointRef.current) {
            if (pointerEnabled && (event.buttons & 1) !== 0) {
              startMaskStroke(event.currentTarget, point, event.pointerId);
            }
            return;
          }
          if (autoEraseSelectionRef.current) {
            updateAutoEraseSelection(event.currentTarget, lastPointRef.current, point);
            lastPointRef.current = point;
            return;
          }
          drawSegment(lastPointRef.current, point);
          lastPointRef.current = point;
        }}
        onPointerUp={(event) => {
          if (selectionEnabled && selectionDragRef.current) {
            event.preventDefault();
            event.stopPropagation();
            const point = resolvePoint(event);
            const rect = resolveSelectionRect(selectionDragRef.current.start, point, pageSize);
            releaseCanvasPointer(event.currentTarget, event.pointerId);
            selectionDragRef.current = null;
            setPreviewSelectionRect(null);
            onSelectionChange(rect.width >= 2 && rect.height >= 2 ? rect : null);
            return;
          }
          if (!drawingRef.current) {
            return;
          }
          event.preventDefault();
          event.stopPropagation();
          const point = resolvePoint(event);
          finishMaskStroke(event.pointerId, point);
        }}
        onPointerCancel={(event) => {
          if (selectionEnabled && selectionDragRef.current) {
            releaseCanvasPointer(event.currentTarget, event.pointerId);
            selectionDragRef.current = null;
            setPreviewSelectionRect(null);
            return;
          }
          if (!drawingRef.current) {
            return;
          }
          cancelMaskStroke(event.pointerId);
        }}
      />
      {activeSelectionRect ? (
        <div
          className="inpaint-selection-rect"
          style={{
            left: `${(activeSelectionRect.x / Math.max(1, pageSize.width)) * 100}%`,
            top: `${(activeSelectionRect.y / Math.max(1, pageSize.height)) * 100}%`,
            width: `${(activeSelectionRect.width / Math.max(1, pageSize.width)) * 100}%`,
            height: `${(activeSelectionRect.height / Math.max(1, pageSize.height)) * 100}%`
          }}
        />
      ) : null}
    </>
  );
}

function summarizePoint(point: DrawPoint | null): { x: number; y: number } | null {
  if (!point) {
    return null;
  }
  return {
    x: Math.round(point.x * 100) / 100,
    y: Math.round(point.y * 100) / 100
  };
}
