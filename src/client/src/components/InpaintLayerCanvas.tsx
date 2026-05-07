import React, { useRef, useState } from "react";
import type { ImageRect } from "../../../shared/types";
import { useCanvasImageSync, type CanvasImageSyncState } from "../hooks/useCanvasImageSync";
import { encodeCanvasSnapshotDataUrl } from "../lib/canvasSnapshotEncoding";
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
  nextDataUrl: string | undefined;
  onChange: InpaintLayerCanvasProps["onChange"];
  previousDataUrl: string | undefined;
  resolved: boolean;
  sourceState: CanvasImageSyncState;
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
    pageSize
  });
  const changedRef = useRef(false);
  const lastPointRef = useRef<DrawPoint | null>(null);
  const autoEraseSelectionRef = useRef<MaskIslandSelection | null>(null);
  const selectionDragRef = useRef<SelectionDragState | null>(null);
  const undoDataUrlRef = useRef<string | undefined>(undefined);
  const editSessionActiveRef = useRef(false);
  const pendingCommitsRef = useRef<PendingMaskCanvasCommit[]>([]);
  const intermediateUndoDataUrlsRef = useRef<(string | undefined)[]>([]);
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

  const appendIntermediateUndoDataUrl = (dataUrl: string | undefined) => {
    const undoDataUrls = intermediateUndoDataUrlsRef.current;
    if (undoDataUrls.length === 0 || undoDataUrls[undoDataUrls.length - 1] !== dataUrl) {
      undoDataUrls.push(dataUrl);
    }
  };

  const flushResolvedMaskCommits = React.useCallback(() => {
    const queue = pendingCommitsRef.current;
    while (queue.length > 0 && queue[0].resolved) {
      const commit = queue[0];
      queue.shift();
      if (queue.length > 0) {
        appendIntermediateUndoDataUrl(commit.nextDataUrl);
        continue;
      }

      const intermediateUndoDataUrls = [...intermediateUndoDataUrlsRef.current];
      intermediateUndoDataUrlsRef.current = [];
      markCanvasCommitted(commit.nextDataUrl, commit.sourceState);
      commit.onChange(commit.nextDataUrl, {
        previousDataUrl: commit.previousDataUrl,
        intermediateUndoDataUrls
      });
    }
  }, [markCanvasCommitted]);

  const commitMaskCanvasSnapshot = (canvas: HTMLCanvasElement, previousDataUrl: string | undefined) => {
    const commit: PendingMaskCanvasCommit = {
      nextDataUrl: undefined,
      onChange,
      previousDataUrl,
      resolved: false,
      sourceState: readCanvasSourceState()
    };
    pendingCommitsRef.current.push(commit);
    void encodeCanvasSnapshotDataUrl(canvas, { mode: "mask" })
      .then((nextDataUrl) => {
        commit.nextDataUrl = nextDataUrl;
        commit.resolved = true;
        flushResolvedMaskCommits();
      })
      .catch((error) => {
        console.error(error);
        commit.nextDataUrl = commit.previousDataUrl;
        commit.resolved = true;
        flushResolvedMaskCommits();
      });
  };

  const commitChange = () => {
    const canvas = canvasRef.current;
    if (!canvas || !changedRef.current) {
      return;
    }

    const previousDataUrl = undoDataUrlRef.current;
    undoDataUrlRef.current = undefined;
    changedRef.current = false;
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
    if (selection) {
      restoreMaskIslandSelection(canvas, selection);
    }
    undoDataUrlRef.current = undefined;
    drawingRef.current = false;
    lastPointRef.current = null;
    markCanvasCommitted(previousDataUrl);
  };

  const commitAutoEraseSelection = (canvas: HTMLCanvasElement) => {
    const selection = autoEraseSelectionRef.current;
    if (!selection) {
      return;
    }

    autoEraseSelectionRef.current = null;
    if (!eraseSelectedMaskIslands(canvas, selection)) {
      const previousDataUrl = undoDataUrlRef.current;
      restoreMaskIslandSelection(canvas, selection);
      undoDataUrlRef.current = undefined;
      drawingRef.current = false;
      lastPointRef.current = null;
      markCanvasCommitted(previousDataUrl);
      return;
    }

    const previousDataUrl = undoDataUrlRef.current;
    undoDataUrlRef.current = undefined;
    drawingRef.current = false;
    lastPointRef.current = null;
    commitMaskCanvasSnapshot(canvas, previousDataUrl);
  };

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
          if (autoEraseEnabled) {
            const selection = createMaskIslandSelection(event.currentTarget);
            if (!selection) {
              return;
            }
            event.currentTarget.setPointerCapture(event.pointerId);
            undoDataUrlRef.current = readCommittedCanvasState()?.dataUrl ?? dataUrl;
            startEditSession();
            autoEraseSelectionRef.current = selection;
            markCanvasEdited();
            drawingRef.current = true;
            lastPointRef.current = point;
            updateAutoEraseSelection(event.currentTarget, point);
            return;
          }

          event.currentTarget.setPointerCapture(event.pointerId);
          undoDataUrlRef.current = readCommittedCanvasState()?.dataUrl ?? dataUrl;
          startEditSession();
          markCanvasEdited();
          drawingRef.current = true;
          lastPointRef.current = point;
          drawSegment(point, point);
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
          if (!pointerEnabled) {
            return;
          }
          event.preventDefault();
          event.stopPropagation();
          const point = resolvePoint(event);
          if (!drawingRef.current || !lastPointRef.current) {
            return;
          }
          if (autoEraseEnabled) {
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
            event.currentTarget.releasePointerCapture(event.pointerId);
            selectionDragRef.current = null;
            setPreviewSelectionRect(null);
            onSelectionChange(rect.width >= 2 && rect.height >= 2 ? rect : null);
            return;
          }
          if (!pointerEnabled || !drawingRef.current) {
            return;
          }
          event.preventDefault();
          event.stopPropagation();
          const point = resolvePoint(event);
          event.currentTarget.releasePointerCapture(event.pointerId);
          if (autoEraseEnabled) {
            updateAutoEraseSelection(event.currentTarget, lastPointRef.current ?? point, point);
            commitAutoEraseSelection(event.currentTarget);
            endEditSession();
            return;
          }
          drawingRef.current = false;
          lastPointRef.current = null;
          commitChange();
          endEditSession();
        }}
        onPointerCancel={(event) => {
          if (selectionEnabled && selectionDragRef.current) {
            event.currentTarget.releasePointerCapture(event.pointerId);
            selectionDragRef.current = null;
            setPreviewSelectionRect(null);
            return;
          }
          if (!pointerEnabled || !drawingRef.current) {
            return;
          }
          event.currentTarget.releasePointerCapture(event.pointerId);
          if (autoEraseEnabled) {
            cancelAutoEraseSelection(event.currentTarget);
            endEditSession();
            return;
          }
          drawingRef.current = false;
          lastPointRef.current = null;
          commitChange();
          endEditSession();
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
