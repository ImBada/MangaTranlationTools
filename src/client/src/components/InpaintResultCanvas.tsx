import React, { useRef, useState } from "react";
import type { ImageRect } from "../../../shared/types";
import { useCanvasImageSync, type CanvasImageSyncState } from "../hooks/useCanvasImageSync";
import { encodeCanvasSnapshotDataUrl } from "../lib/canvasSnapshotEncoding";
import {
  blendChannel,
  brushMaskAlpha,
  clamp01,
  createBrushGradient,
  parseHexColor,
  resolveBrushBounds,
  resolveSelectionRect,
  rgbaToCss,
  sampleBlur,
  sampleSharpen,
  type DrawPoint
} from "../lib/inpaintResultCanvas";
import type { InpaintLayerChangeOptions } from "../lib/inpaintLayerChange";

export type InpaintResultTool = "select" | "brush" | "smartBrush" | "eraser" | "blur" | "sharpen" | "smudge";

type InpaintResultCanvasProps = {
  dataUrl?: string;
  maskDataUrl?: string;
  pageSize: {
    width: number;
    height: number;
  };
  tool: InpaintResultTool;
  brushSize: number;
  brushColor: string;
  brushHardness: number;
  toolStrength: number;
  disabled: boolean;
  className?: string;
  style?: React.CSSProperties;
  selectionRect: ImageRect | null;
  onChange: (dataUrl: string | undefined, options?: InpaintLayerChangeOptions) => void;
  onEditEnd?: () => void;
  onEditStart?: () => void;
  onSelectionChange: (rect: ImageRect | null) => void;
};

type SelectionDragState = {
  start: DrawPoint;
  current: DrawPoint;
};

type PendingResultCanvasCommit = {
  capturesMaskDataUrl: boolean;
  nextDataUrl: string | undefined;
  nextMaskDataUrl: string | undefined;
  onChange: InpaintResultCanvasProps["onChange"];
  previousDataUrl: string | undefined;
  previousMaskDataUrl: string | undefined;
  resolved: boolean;
  smartMaskSourceState: CanvasImageSyncState | undefined;
  sourceState: CanvasImageSyncState;
};

type IntermediateResultSnapshot = {
  capturesMaskDataUrl: boolean;
  maskDataUrl: string | undefined;
  resultDataUrl: string | undefined;
};

function resolveIntermediateLayerUndoSnapshots(
  snapshots: IntermediateResultSnapshot[],
  initialMaskDataUrl: string | undefined
): {
  maskDataUrl: string | undefined;
  resultDataUrl: string | undefined;
}[] {
  let maskDataUrl = initialMaskDataUrl;
  return snapshots.map((snapshot) => {
    if (snapshot.capturesMaskDataUrl) {
      maskDataUrl = snapshot.maskDataUrl;
    }
    return {
      maskDataUrl,
      resultDataUrl: snapshot.resultDataUrl
    };
  });
}

export function InpaintResultCanvas({
  dataUrl,
  maskDataUrl,
  pageSize,
  tool,
  brushSize,
  brushColor,
  brushHardness,
  toolStrength,
  disabled,
  className,
  style,
  selectionRect,
  onChange,
  onEditEnd,
  onEditStart,
  onSelectionChange
}: InpaintResultCanvasProps): React.JSX.Element {
  const {
    canvasRef,
    drawingRef,
    markCanvasCommitted,
    markCanvasEdited,
    readCanvasSourceState,
    readCommittedCanvasState
  } = useCanvasImageSync({
    dataUrl,
    loadErrorMessage: "인페인트 결과 레이어를 불러오지 못했습니다.",
    pageSize,
    willReadFrequently: true
  });
  const {
    canvasRef: smartMaskCanvasRef,
    drawingRef: smartMaskDrawingRef,
    markCanvasCommitted: markSmartMaskCommitted,
    markCanvasEdited: markSmartMaskEdited,
    readCanvasSourceState: readSmartMaskCanvasSourceState,
    readCommittedCanvasState: readCommittedSmartMaskCanvasState
  } = useCanvasImageSync({
    dataUrl: maskDataUrl,
    loadErrorMessage: "인페인트 마스크를 불러오지 못했습니다.",
    pageSize,
    willReadFrequently: true
  });
  const changedRef = useRef(false);
  const lastPointRef = useRef<DrawPoint | null>(null);
  const undoMaskDataUrlRef = useRef<string | undefined>(undefined);
  const smudgePatchRef = useRef<ImageData | null>(null);
  const selectionDragRef = useRef<SelectionDragState | null>(null);
  const strokeForcesVisiblePixelsRef = useRef(false);
  const undoDataUrlRef = useRef<string | undefined>(undefined);
  const editSessionActiveRef = useRef(false);
  const pendingCommitsRef = useRef<PendingResultCanvasCommit[]>([]);
  const intermediateSnapshotsRef = useRef<IntermediateResultSnapshot[]>([]);
  const [previewSelectionRect, setPreviewSelectionRect] = useState<ImageRect | null>(null);

  const pointerEnabled = !disabled && tool !== "select";
  const selectionEnabled = !disabled && tool === "select";
  const activeSelectionRect = previewSelectionRect ?? selectionRect;

  const drawSegment = (from: DrawPoint, to: DrawPoint) => {
    if (tool === "smudge") {
      drawSmudge(to);
      return;
    }

    const distance = Math.hypot(to.x - from.x, to.y - from.y);
    const step = Math.max(1, brushSize / (tool === "brush" || tool === "smartBrush" || tool === "eraser" ? 4 : 2));
    const steps = Math.max(1, Math.ceil(distance / step));
    for (let index = 0; index <= steps; index += 1) {
      const t = index / steps;
      const point = {
        x: from.x + (to.x - from.x) * t,
        y: from.y + (to.y - from.y) * t
      };
      if (tool === "brush" || tool === "smartBrush" || tool === "eraser") {
        stampPaint(point);
        if (tool === "smartBrush") {
          stampSmartMaskPatch(point);
        }
      } else if (tool === "blur" || tool === "sharpen") {
        stampFilter(point, tool);
      }
    }
  };

  const stampPaint = (point: DrawPoint) => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d");
    if (!canvas || !context) {
      return;
    }

    const color = parseHexColor(brushColor);
    const centerColor = tool === "eraser" ? "rgba(0, 0, 0, 1)" : rgbaToCss(color, 1);
    const edgeColor = tool === "eraser" ? "rgba(0, 0, 0, 0)" : rgbaToCss(color, edgeBrushAlpha());
    const { gradient, radius } = createStampGradient(context, point, centerColor, edgeColor);

    fillBrushStamp(context, point, radius, gradient, tool === "eraser" ? "destination-out" : "source-over");
    changedRef.current = true;
    strokeForcesVisiblePixelsRef.current = strokeForcesVisiblePixelsRef.current || tool !== "eraser";
  };

  const stampSmartMaskPatch = (point: DrawPoint) => {
    const canvas = smartMaskCanvasRef.current;
    const context = canvas?.getContext("2d");
    if (!canvas || !context) {
      return;
    }

    const { gradient, radius } = createStampGradient(
      context,
      point,
      "rgba(255, 255, 255, 1)",
      `rgba(255, 255, 255, ${edgeBrushAlpha()})`
    );
    fillBrushStamp(context, point, radius, gradient, "source-over");
  };

  const createStampGradient = (
    context: CanvasRenderingContext2D,
    point: DrawPoint,
    centerColor: string,
    edgeColor: string
  ): { gradient: CanvasGradient; radius: number } => {
    const radius = Math.max(0.5, brushSize / 2);
    const hardness = clamp01(brushHardness);
    const gradient = context.createRadialGradient(point.x, point.y, 0, point.x, point.y, radius);
    gradient.addColorStop(0, centerColor);
    gradient.addColorStop(Math.min(1, hardness), centerColor);
    gradient.addColorStop(1, edgeColor);
    return { gradient, radius };
  };

  const fillBrushStamp = (
    context: CanvasRenderingContext2D,
    point: DrawPoint,
    radius: number,
    fillStyle: CanvasGradient,
    compositeOperation: GlobalCompositeOperation
  ) => {
    context.save();
    context.globalCompositeOperation = compositeOperation;
    context.fillStyle = fillStyle;
    context.beginPath();
    context.arc(point.x, point.y, radius, 0, Math.PI * 2);
    context.fill();
    context.restore();
  };

  const edgeBrushAlpha = (): number => (clamp01(brushHardness) >= 1 ? 1 : 0);

  const stampFilter = (point: DrawPoint, filterTool: "blur" | "sharpen") => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d", { willReadFrequently: true });
    if (!canvas || !context) {
      return;
    }

    const radius = Math.max(1, Math.round(brushSize / 2));
    const bounds = resolveBrushBounds(point, radius, canvas.width, canvas.height);
    if (bounds.width <= 0 || bounds.height <= 0) {
      return;
    }

    const imageData = context.getImageData(bounds.x, bounds.y, bounds.width, bounds.height);
    const source = new Uint8ClampedArray(imageData.data);
    const strength = clamp01(toolStrength);
    const hardness = clamp01(brushHardness);

    for (let y = 0; y < bounds.height; y += 1) {
      for (let x = 0; x < bounds.width; x += 1) {
        const absoluteX = bounds.x + x;
        const absoluteY = bounds.y + y;
        const maskAlpha = brushMaskAlpha(absoluteX, absoluteY, point, radius, hardness) * strength;
        if (maskAlpha <= 0) {
          continue;
        }

        const targetOffset = (y * bounds.width + x) * 4;
        const filtered = filterTool === "blur"
          ? sampleBlur(source, bounds.width, bounds.height, x, y)
          : sampleSharpen(source, bounds.width, bounds.height, x, y);

        imageData.data[targetOffset] = blendChannel(source[targetOffset], filtered[0], maskAlpha);
        imageData.data[targetOffset + 1] = blendChannel(source[targetOffset + 1], filtered[1], maskAlpha);
        imageData.data[targetOffset + 2] = blendChannel(source[targetOffset + 2], filtered[2], maskAlpha);
      }
    }

    context.putImageData(imageData, bounds.x, bounds.y);
    changedRef.current = true;
  };

  const drawSmudge = (point: DrawPoint) => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d", { willReadFrequently: true });
    const patch = smudgePatchRef.current;
    if (!canvas || !context || !patch) {
      return;
    }

    const radius = Math.max(1, Math.round(brushSize / 2));
    const patchCanvas = document.createElement("canvas");
    patchCanvas.width = patch.width;
    patchCanvas.height = patch.height;
    const patchContext = patchCanvas.getContext("2d");
    if (!patchContext) {
      return;
    }
    patchContext.putImageData(patch, 0, 0);
    patchContext.globalCompositeOperation = "destination-in";
    patchContext.fillStyle = createBrushGradient(patchContext, radius, brushHardness);
    patchContext.fillRect(0, 0, patch.width, patch.height);

    context.save();
    context.globalAlpha = clamp01(toolStrength);
    context.drawImage(patchCanvas, point.x - radius, point.y - radius);
    context.restore();
    changedRef.current = true;
    smudgePatchRef.current = captureSmudgePatch(point);
  };

  const captureSmudgePatch = (point: DrawPoint): ImageData | null => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d", { willReadFrequently: true });
    if (!canvas || !context) {
      return null;
    }
    const radius = Math.max(1, Math.round(brushSize / 2));
    const patchCanvas = document.createElement("canvas");
    patchCanvas.width = radius * 2;
    patchCanvas.height = radius * 2;
    const patchContext = patchCanvas.getContext("2d", { willReadFrequently: true });
    if (!patchContext) {
      return null;
    }
    patchContext.drawImage(canvas, point.x - radius, point.y - radius, radius * 2, radius * 2, 0, 0, radius * 2, radius * 2);
    return patchContext.getImageData(0, 0, radius * 2, radius * 2);
  };

  const resolvePoint = (event: React.PointerEvent<HTMLCanvasElement>): DrawPoint => {
    const rect = event.currentTarget.getBoundingClientRect();
    return {
      x: ((event.clientX - rect.left) / Math.max(1, rect.width)) * pageSize.width,
      y: ((event.clientY - rect.top) / Math.max(1, rect.height)) * pageSize.height
    };
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

  const appendIntermediateResultSnapshot = (commit: PendingResultCanvasCommit) => {
    const snapshots = intermediateSnapshotsRef.current;
    const lastSnapshot = snapshots[snapshots.length - 1];
    if (
      !lastSnapshot ||
      lastSnapshot.resultDataUrl !== commit.nextDataUrl ||
      lastSnapshot.capturesMaskDataUrl !== commit.capturesMaskDataUrl ||
      (commit.capturesMaskDataUrl && lastSnapshot.maskDataUrl !== commit.nextMaskDataUrl)
    ) {
      snapshots.push({
        capturesMaskDataUrl: commit.capturesMaskDataUrl,
        maskDataUrl: commit.nextMaskDataUrl,
        resultDataUrl: commit.nextDataUrl
      });
    }
  };

  const flushResolvedResultCommits = React.useCallback(() => {
    const queue = pendingCommitsRef.current;
    while (queue.length > 0 && queue[0].resolved) {
      const commit = queue[0];
      queue.shift();
      if (queue.length > 0) {
        appendIntermediateResultSnapshot(commit);
        continue;
      }

      const committed = markCanvasCommitted(commit.nextDataUrl, commit.sourceState);
      const intermediateSnapshots = [...intermediateSnapshotsRef.current];
      intermediateSnapshotsRef.current = [];
      if (commit.capturesMaskDataUrl) {
        if (committed) {
          smartMaskDrawingRef.current = false;
          markSmartMaskCommitted(commit.nextMaskDataUrl, commit.smartMaskSourceState);
        }
        commit.onChange(commit.nextDataUrl, {
          previousDataUrl: commit.previousDataUrl,
          previousMaskDataUrl: commit.previousMaskDataUrl,
          maskDataUrl: commit.nextMaskDataUrl,
          intermediateLayerUndoSnapshots: resolveIntermediateLayerUndoSnapshots(
            intermediateSnapshots,
            commit.previousMaskDataUrl
          )
        });
        continue;
      }

      commit.onChange(commit.nextDataUrl, {
        previousDataUrl: commit.previousDataUrl,
        intermediateUndoDataUrls: intermediateSnapshots.map((snapshot) => snapshot.resultDataUrl)
      });
    }
  }, [markCanvasCommitted, markSmartMaskCommitted]);

  const commitChange = () => {
    const canvas = canvasRef.current;
    if (!canvas || !changedRef.current) {
      return;
    }

    const previousDataUrl = undoDataUrlRef.current;
    const includeBlank = strokeForcesVisiblePixelsRef.current;
    const capturesMaskDataUrl =
      tool === "smartBrush" ||
      pendingCommitsRef.current.some((commit) => commit.capturesMaskDataUrl) ||
      intermediateSnapshotsRef.current.some((snapshot) => snapshot.capturesMaskDataUrl);
    const smartMaskSourceState = capturesMaskDataUrl ? readSmartMaskCanvasSourceState() : undefined;
    const nextDataUrlPromise = encodeCanvasSnapshotDataUrl(canvas, { mode: "image", includeBlank });
    const nextMaskDataUrlPromise = capturesMaskDataUrl
      ? resolveSmartMaskDataUrl()
      : Promise.resolve(undefined);
    const previousMaskDataUrl = capturesMaskDataUrl
      ? undoMaskDataUrlRef.current ?? readCommittedSmartMaskCanvasState()?.dataUrl ?? maskDataUrl
      : undefined;
    const commit: PendingResultCanvasCommit = {
      capturesMaskDataUrl,
      nextDataUrl: undefined,
      nextMaskDataUrl: undefined,
      onChange,
      previousDataUrl,
      previousMaskDataUrl,
      resolved: false,
      smartMaskSourceState,
      sourceState: readCanvasSourceState()
    };
    pendingCommitsRef.current.push(commit);
    undoDataUrlRef.current = undefined;
    undoMaskDataUrlRef.current = undefined;
    changedRef.current = false;
    strokeForcesVisiblePixelsRef.current = false;
    void Promise.all([nextDataUrlPromise, nextMaskDataUrlPromise])
      .then(([nextDataUrl, nextMaskDataUrl]) => {
        commit.nextDataUrl = nextDataUrl;
        commit.nextMaskDataUrl = nextMaskDataUrl;
        commit.resolved = true;
        flushResolvedResultCommits();
      })
      .catch((error) => {
        console.error(error);
        commit.nextDataUrl = commit.previousDataUrl;
        commit.nextMaskDataUrl = commit.previousMaskDataUrl;
        commit.resolved = true;
        flushResolvedResultCommits();
      });
  };

  const resolveSmartMaskDataUrl = (): Promise<string | undefined> => {
    const canvas = smartMaskCanvasRef.current;
    return canvas ? encodeCanvasSnapshotDataUrl(canvas, { mode: "mask" }) : Promise.resolve(undefined);
  };

  return (
    <>
      <canvas ref={smartMaskCanvasRef} aria-hidden="true" style={{ display: "none" }} />
      <canvas
        ref={canvasRef}
        className={`${className ?? ""} ${pointerEnabled ? "editing" : ""} ${selectionEnabled ? "selecting" : ""}`.trim()}
        style={style}
        aria-label="인페인트 결과 레이어"
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
          event.currentTarget.setPointerCapture(event.pointerId);
          const point = resolvePoint(event);
          undoDataUrlRef.current = readCommittedCanvasState()?.dataUrl ?? dataUrl;
          startEditSession();
          markCanvasEdited();
          drawingRef.current = true;
          lastPointRef.current = point;
          strokeForcesVisiblePixelsRef.current = false;
          if (tool === "smartBrush") {
            const committedSmartMaskState = readCommittedSmartMaskCanvasState();
            undoMaskDataUrlRef.current = committedSmartMaskState ? committedSmartMaskState.dataUrl : maskDataUrl;
            markSmartMaskEdited();
            smartMaskDrawingRef.current = true;
          }
          smudgePatchRef.current = tool === "smudge" ? captureSmudgePatch(point) : null;
          if (tool !== "smudge") {
            drawSegment(point, point);
          }
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
          event.currentTarget.releasePointerCapture(event.pointerId);
          drawingRef.current = false;
          lastPointRef.current = null;
          smudgePatchRef.current = null;
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
          drawingRef.current = false;
          lastPointRef.current = null;
          smudgePatchRef.current = null;
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
