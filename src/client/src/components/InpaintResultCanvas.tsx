import React, { useRef, useState } from "react";
import type { ImageRect } from "../../../shared/types";
import { useCanvasImageSync, type CanvasImageSyncState } from "../hooks/useCanvasImageSync";
import { encodeCanvasSnapshotDataUrl } from "../lib/canvasSnapshotEncoding";
import { createUndoCanvasSnapshot, isInlineDataUrl } from "../lib/inpaintCanvasUndo";
import {
  blendChannel,
  brushMaskAlpha,
  clamp01,
  clampByte,
  parseHexColor,
  resolveBrushBounds,
  resolveSelectionRect,
  sampleBlur,
  sampleSharpen,
  type DrawPoint,
  type RgbaColor
} from "../lib/inpaintResultCanvas";
import {
  createInpaintDebugId,
  summarizeCanvasSyncState,
  summarizeDataUrl,
  summarizeError,
  writeInpaintDebugLog
} from "../lib/inpaintDiagnostics";
import type { InpaintLayerChangeOptions } from "../lib/inpaintLayerChange";

export type InpaintResultTool = "select" | "brush" | "smartBrush" | "eraser" | "blur" | "sharpen" | "smudge" | "colorPicker";
type InpaintResultPaintTool = Exclude<InpaintResultTool, "select" | "colorPicker">;

type InpaintResultCanvasProps = {
  colorPickerSampleRequired?: boolean;
  dataUrl?: string;
  finalOutputOverlayCanvasRef?: React.RefObject<HTMLCanvasElement | null>;
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
  fallbackCanvasRef?: React.RefObject<HTMLCanvasElement | null>;
  style?: React.CSSProperties;
  selectionRect: ImageRect | null;
  onChange: (dataUrl: string | undefined, options?: InpaintLayerChangeOptions) => void;
  onColorPick: (color: string) => void;
  onEditEnd?: () => void;
  onEditStart?: () => void;
  onSelectionChange: (rect: ImageRect | null) => void;
};

type SelectionDragState = {
  start: DrawPoint;
  current: DrawPoint;
};

type ColorPickerPreview = {
  currentColor: string;
  point: DrawPoint;
  sampledColor: string;
};

type ColorPickerHover = {
  point: DrawPoint;
};

type PendingResultCanvasCommit = {
  capturesMaskDataUrl: boolean;
  commitId: string;
  nextDataUrl: string | undefined;
  nextMaskDataUrl: string | undefined;
  onChange: InpaintResultCanvasProps["onChange"];
  previousDataUrl: string | undefined;
  previousMaskDataUrl: string | undefined;
  previousMaskSourceDataUrl: string | undefined;
  previousResultSourceDataUrl: string | undefined;
  resolved: boolean;
  maskSourceState: CanvasImageSyncState | null;
  sourceState: CanvasImageSyncState;
  strokeTool: InpaintResultPaintTool | null;
};

type BrushStampCompositeMode = "paint" | "erase";

function isInpaintResultPaintTool(tool: InpaintResultTool): tool is InpaintResultPaintTool {
  return tool !== "select" && tool !== "colorPicker";
}

function compositeBrushPixel(data: Uint8ClampedArray, offset: number, color: RgbaColor, maskAlpha: number): void {
  const sourceAlpha = clamp01(color.a * maskAlpha);
  if (sourceAlpha <= 0) {
    return;
  }

  const targetAlpha = data[offset + 3] / 255;
  const outputAlpha = sourceAlpha + targetAlpha * (1 - sourceAlpha);
  if (outputAlpha <= 0) {
    data[offset] = 0;
    data[offset + 1] = 0;
    data[offset + 2] = 0;
    data[offset + 3] = 0;
    return;
  }

  data[offset] = clampByte((color.r * sourceAlpha + data[offset] * targetAlpha * (1 - sourceAlpha)) / outputAlpha);
  data[offset + 1] = clampByte((color.g * sourceAlpha + data[offset + 1] * targetAlpha * (1 - sourceAlpha)) / outputAlpha);
  data[offset + 2] = clampByte((color.b * sourceAlpha + data[offset + 2] * targetAlpha * (1 - sourceAlpha)) / outputAlpha);
  data[offset + 3] = clampByte(outputAlpha * 255);
}

export function InpaintResultCanvas({
  colorPickerSampleRequired = false,
  dataUrl,
  finalOutputOverlayCanvasRef,
  maskDataUrl,
  pageSize,
  tool,
  brushSize,
  brushColor,
  brushHardness,
  toolStrength,
  disabled,
  className,
  fallbackCanvasRef,
  style,
  selectionRect,
  onChange,
  onColorPick,
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
    readCanvasSourceState: readSmartMaskSourceState,
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
  const undoCanvasSnapshotRef = useRef<HTMLCanvasElement | null>(null);
  const undoSmartMaskCanvasSnapshotRef = useRef<HTMLCanvasElement | null>(null);
  const editSessionActiveRef = useRef(false);
  const activeStrokePointerIdRef = useRef<number | null>(null);
  const activeStrokeToolRef = useRef<InpaintResultPaintTool | null>(null);
  const pendingCommitsRef = useRef<PendingResultCanvasCommit[]>([]);
  const colorPickerPointerIdRef = useRef<number | null>(null);
  const [previewSelectionRect, setPreviewSelectionRect] = useState<ImageRect | null>(null);
  const [colorPickerHover, setColorPickerHover] = useState<ColorPickerHover | null>(null);
  const [colorPickerPreview, setColorPickerPreview] = useState<ColorPickerPreview | null>(null);

  const colorPickerEnabled = !disabled && tool === "colorPicker";
  const pointerEnabled = !disabled && tool !== "select" && tool !== "colorPicker";
  const selectionEnabled = !disabled && tool === "select";
  const activeSelectionRect = previewSelectionRect ?? selectionRect;

  const drawSegment = (from: DrawPoint, to: DrawPoint, strokeTool: InpaintResultPaintTool) => {
    if (strokeTool === "smudge") {
      drawSmudge(to);
      return;
    }

    const distance = Math.hypot(to.x - from.x, to.y - from.y);
    const step = Math.max(1, brushSize / (strokeTool === "brush" || strokeTool === "smartBrush" || strokeTool === "eraser" ? 4 : 2));
    const steps = Math.max(1, Math.ceil(distance / step));
    for (let index = 0; index <= steps; index += 1) {
      const t = index / steps;
      const point = {
        x: from.x + (to.x - from.x) * t,
        y: from.y + (to.y - from.y) * t
      };
      if (strokeTool === "brush" || strokeTool === "smartBrush" || strokeTool === "eraser") {
        stampPaint(point, strokeTool);
        if (strokeTool === "smartBrush") {
          stampSmartMaskPatch(point);
        }
      } else if (strokeTool === "blur" || strokeTool === "sharpen") {
        stampFilter(point, strokeTool);
      }
    }
  };

  const stampPaint = (point: DrawPoint, strokeTool: "brush" | "smartBrush" | "eraser") => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d", { willReadFrequently: true });
    if (!canvas || !context) {
      return;
    }

    const stamped = stampBrushPixels(
      context,
      canvas,
      point,
      strokeTool === "eraser" ? { r: 0, g: 0, b: 0, a: 1 } : parseHexColor(brushColor),
      strokeTool === "eraser" ? "erase" : "paint"
    );
    changedRef.current = changedRef.current || stamped;
    strokeForcesVisiblePixelsRef.current = strokeForcesVisiblePixelsRef.current || (stamped && strokeTool !== "eraser");
  };

  const stampSmartMaskPatch = (point: DrawPoint) => {
    const canvas = smartMaskCanvasRef.current;
    const context = canvas?.getContext("2d", { willReadFrequently: true });
    if (!canvas || !context) {
      return;
    }

    stampBrushPixels(
      context,
      canvas,
      point,
      { r: 255, g: 255, b: 255, a: 1 },
      "paint"
    );
  };

  const stampBrushPixels = (
    context: CanvasRenderingContext2D,
    canvas: HTMLCanvasElement,
    point: DrawPoint,
    color: RgbaColor,
    compositeMode: BrushStampCompositeMode
  ): boolean => {
    const radius = Math.max(0.5, brushSize / 2);
    const hardness = clamp01(brushHardness);
    const bounds = resolveBrushBounds(point, radius, canvas.width, canvas.height);
    if (bounds.width <= 0 || bounds.height <= 0) {
      return false;
    }

    const imageData = context.getImageData(bounds.x, bounds.y, bounds.width, bounds.height);
    let changed = false;
    for (let y = 0; y < bounds.height; y += 1) {
      for (let x = 0; x < bounds.width; x += 1) {
        const maskAlpha = brushMaskAlpha(bounds.x + x + 0.5, bounds.y + y + 0.5, point, radius, hardness);
        if (maskAlpha <= 0) {
          continue;
        }

        const offset = (y * bounds.width + x) * 4;
        if (compositeMode === "erase") {
          imageData.data[offset + 3] = clampByte(imageData.data[offset + 3] * (1 - maskAlpha));
        } else {
          compositeBrushPixel(imageData.data, offset, color, maskAlpha);
        }
        changed = true;
      }
    }

    if (changed) {
      context.putImageData(imageData, bounds.x, bounds.y);
    }
    return changed;
  };

  const maskBrushPatchAlpha = (imageData: ImageData, radius: number): void => {
    const hardness = clamp01(brushHardness);
    const center = { x: radius, y: radius };
    for (let y = 0; y < imageData.height; y += 1) {
      for (let x = 0; x < imageData.width; x += 1) {
        const offset = (y * imageData.width + x) * 4;
        const maskAlpha = brushMaskAlpha(x + 0.5, y + 0.5, center, radius, hardness);
        imageData.data[offset + 3] = clampByte(imageData.data[offset + 3] * maskAlpha);
      }
    }
  };

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
        const absoluteX = bounds.x + x + 0.5;
        const absoluteY = bounds.y + y + 0.5;
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
    const maskedPatch = patchContext.createImageData(patch.width, patch.height);
    maskedPatch.data.set(patch.data);
    maskBrushPatchAlpha(maskedPatch, radius);
    patchContext.putImageData(maskedPatch, 0, 0);

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

  const sampleCanvasColor = (point: DrawPoint): string | null => {
    if (colorPickerSampleRequired) {
      return sampleFinalOutputColor({
        imageCanvas: fallbackCanvasRef?.current,
        maskCanvas: smartMaskCanvasRef.current,
        overlayCanvas: finalOutputOverlayCanvasRef?.current,
        point,
        resultCanvas: canvasRef.current
      });
    }

    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d", { willReadFrequently: true });
    if (!canvas || !context) {
      return null;
    }

    const x = Math.min(canvas.width - 1, Math.max(0, Math.floor(point.x)));
    const y = Math.min(canvas.height - 1, Math.max(0, Math.floor(point.y)));
    const [red, green, blue, alpha] = context.getImageData(x, y, 1, 1).data;
    if (alpha === 0) {
      const fallbackColor = pickFallbackCanvasColor(fallbackCanvasRef?.current, x, y);
      if (fallbackColor) {
        return fallbackColor;
      }
    }
    return rgbToHex(red, green, blue);
  };

  const updateColorPickerPreview = (point: DrawPoint, currentColor = brushColor): string | null => {
    const sampledColor = sampleCanvasColor(point);
    if (!sampledColor) {
      return null;
    }
    setColorPickerPreview({ currentColor, point, sampledColor });
    return sampledColor;
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

  React.useEffect(() => {
    if (colorPickerEnabled) {
      return;
    }
    colorPickerPointerIdRef.current = null;
    setColorPickerHover(null);
    setColorPickerPreview(null);
  }, [colorPickerEnabled]);

  const flushResolvedResultCommits = React.useCallback(() => {
    const queue = pendingCommitsRef.current;
    writeInpaintDebugLog("inpaint-result:commit-flush-start", {
      queueLength: queue.length,
      ready: queue.filter((commit) => commit.resolved).length
    });
    while (queue.length > 0 && queue[0].resolved) {
      const commit = queue[0];
      queue.shift();

      const marked = markCanvasCommitted(commit.nextDataUrl, commit.sourceState);
      writeInpaintDebugLog("inpaint-result:commit-apply", {
        capturesMaskDataUrl: commit.capturesMaskDataUrl,
        commitId: commit.commitId,
        marked,
        nextDataUrl: summarizeDataUrl(commit.nextDataUrl),
        nextMaskDataUrl: summarizeDataUrl(commit.nextMaskDataUrl),
        previousDataUrl: summarizeDataUrl(commit.previousDataUrl),
        previousMaskDataUrl: summarizeDataUrl(commit.previousMaskDataUrl),
        previousMaskSourceDataUrl: summarizeDataUrl(commit.previousMaskSourceDataUrl),
        previousResultSourceDataUrl: summarizeDataUrl(commit.previousResultSourceDataUrl),
        queueRemaining: queue.length,
        sourceState: summarizeCanvasSyncState(commit.sourceState),
        strokeTool: commit.strokeTool
      });
      if (commit.capturesMaskDataUrl) {
        smartMaskDrawingRef.current = queue.some((queuedCommit) => queuedCommit.capturesMaskDataUrl);
        markSmartMaskCommitted(commit.nextMaskDataUrl, commit.maskSourceState ?? undefined);
        commit.onChange(commit.nextDataUrl, {
          previousDataUrl: commit.previousDataUrl,
          previousMaskDataUrl: commit.previousMaskDataUrl,
          previousMaskSourceDataUrl: commit.previousMaskSourceDataUrl,
          previousResultSourceDataUrl: commit.previousResultSourceDataUrl,
          maskDataUrl: commit.nextMaskDataUrl,
          maskDataUrlMode: "full"
        });
      } else {
        commit.onChange(commit.nextDataUrl, {
          previousDataUrl: commit.previousDataUrl,
          previousResultSourceDataUrl: commit.previousResultSourceDataUrl
        });
      }

      if (queue.length > 0) {
        queue[0].previousDataUrl = commit.nextDataUrl;
        writeInpaintDebugLog("inpaint-result:commit-carry-forward-result", {
          fromCommitId: commit.commitId,
          nextCommitId: queue[0].commitId,
          nextPreviousDataUrl: summarizeDataUrl(queue[0].previousDataUrl)
        });
        if (commit.capturesMaskDataUrl) {
          const nextMaskCommit = queue.find((queuedCommit) => queuedCommit.capturesMaskDataUrl);
          if (nextMaskCommit) {
            nextMaskCommit.previousMaskDataUrl = commit.nextMaskDataUrl;
            writeInpaintDebugLog("inpaint-result:commit-carry-forward-mask", {
              fromCommitId: commit.commitId,
              nextCommitId: nextMaskCommit.commitId,
              nextPreviousMaskDataUrl: summarizeDataUrl(nextMaskCommit.previousMaskDataUrl)
            });
          }
        }
      }
    }
  }, [markCanvasCommitted, markSmartMaskCommitted]);

  const commitChange = (strokeTool: InpaintResultPaintTool | null) => {
    const canvas = canvasRef.current;
    if (!canvas || !changedRef.current) {
      undoCanvasSnapshotRef.current = null;
      undoSmartMaskCanvasSnapshotRef.current = null;
      writeInpaintDebugLog("inpaint-result:commit-skip", {
        changed: changedRef.current,
        hasCanvas: Boolean(canvas),
        pendingQueueLength: pendingCommitsRef.current.length,
        strokeTool,
        tool
      });
      return;
    }

    const previousDataUrl = undoDataUrlRef.current;
    const includeBlank = strokeForcesVisiblePixelsRef.current;
    const capturesMaskDataUrl = strokeTool === "smartBrush";
    const commitId = createInpaintDebugId("result-commit");
    const sourceState = readCanvasSourceState();
    const smartMaskSourceState = capturesMaskDataUrl ? readSmartMaskSourceState() : null;
    const previousSnapshot = undoCanvasSnapshotRef.current;
    undoCanvasSnapshotRef.current = null;
    const previousDataUrlPromise = previousSnapshot
      ? encodeCanvasSnapshotDataUrl(previousSnapshot, { mode: "image" })
      : Promise.resolve(previousDataUrl);
    const nextDataUrlPromise = encodeCanvasSnapshotDataUrl(canvas, { mode: "image", includeBlank });
    const nextMaskDataUrlPromise = capturesMaskDataUrl
      ? resolveSmartMaskDataUrl()
      : Promise.resolve(undefined);
    const previousMaskDataUrl = capturesMaskDataUrl
      ? undoMaskDataUrlRef.current ?? readCommittedSmartMaskCanvasState()?.dataUrl ?? maskDataUrl
      : undefined;
    const previousMaskSnapshot = undoSmartMaskCanvasSnapshotRef.current;
    undoSmartMaskCanvasSnapshotRef.current = null;
    const previousMaskDataUrlPromise = previousMaskSnapshot
      ? encodeCanvasSnapshotDataUrl(previousMaskSnapshot, { mode: "mask" })
      : Promise.resolve(previousMaskDataUrl);
    const commit: PendingResultCanvasCommit = {
      capturesMaskDataUrl,
      commitId,
      nextDataUrl: undefined,
      nextMaskDataUrl: undefined,
      onChange,
      previousDataUrl,
      previousMaskDataUrl,
      previousMaskSourceDataUrl: smartMaskSourceState?.dataUrl,
      previousResultSourceDataUrl: sourceState.dataUrl,
      resolved: false,
      maskSourceState: smartMaskSourceState,
      sourceState,
      strokeTool
    };
    pendingCommitsRef.current.push(commit);
    writeInpaintDebugLog("inpaint-result:commit-enqueue", {
      capturesMaskDataUrl,
      commitId,
      includeBlank,
      previousDataUrl: summarizeDataUrl(previousDataUrl),
      previousMaskDataUrl: summarizeDataUrl(previousMaskDataUrl),
      previousMaskSourceDataUrl: summarizeDataUrl(commit.previousMaskSourceDataUrl),
      previousResultSourceDataUrl: summarizeDataUrl(commit.previousResultSourceDataUrl),
      queueLength: pendingCommitsRef.current.length,
      resultCanvasState: summarizeCanvasSyncState(readCommittedCanvasState()),
      resultSourceState: summarizeCanvasSyncState(sourceState),
      smartMaskCanvasState: summarizeCanvasSyncState(readCommittedSmartMaskCanvasState()),
      smartMaskSourceState: summarizeCanvasSyncState(smartMaskSourceState),
      strokeTool
    });
    undoDataUrlRef.current = undefined;
    undoCanvasSnapshotRef.current = null;
    undoMaskDataUrlRef.current = undefined;
    undoSmartMaskCanvasSnapshotRef.current = null;
    changedRef.current = false;
    strokeForcesVisiblePixelsRef.current = false;
    void Promise.all([previousDataUrlPromise, previousMaskDataUrlPromise, nextDataUrlPromise, nextMaskDataUrlPromise])
      .then(([resolvedPreviousDataUrl, resolvedPreviousMaskDataUrl, nextDataUrl, nextMaskDataUrl]) => {
        commit.previousDataUrl = resolvedPreviousDataUrl;
        commit.previousMaskDataUrl = resolvedPreviousMaskDataUrl;
        commit.nextDataUrl = nextDataUrl;
        commit.nextMaskDataUrl = nextMaskDataUrl;
        commit.resolved = true;
        writeInpaintDebugLog("inpaint-result:commit-encoded", {
          commitId,
          nextDataUrl: summarizeDataUrl(nextDataUrl),
          nextMaskDataUrl: summarizeDataUrl(nextMaskDataUrl),
          previousDataUrl: summarizeDataUrl(resolvedPreviousDataUrl),
          previousMaskDataUrl: summarizeDataUrl(resolvedPreviousMaskDataUrl),
          queueLength: pendingCommitsRef.current.length
        });
        flushResolvedResultCommits();
      })
      .catch((error) => {
        console.error(error);
        const fallbackDataUrl = isInlineDataUrl(previousDataUrl) ? previousDataUrl : undefined;
        const fallbackMaskDataUrl = isInlineDataUrl(previousMaskDataUrl) ? previousMaskDataUrl : undefined;
        commit.previousDataUrl = fallbackDataUrl;
        commit.previousMaskDataUrl = fallbackMaskDataUrl;
        commit.nextDataUrl = fallbackDataUrl;
        commit.nextMaskDataUrl = fallbackMaskDataUrl;
        commit.resolved = true;
        writeInpaintDebugLog("inpaint-result:commit-encode-error", {
          commitId,
          error: summarizeError(error),
          fallbackDataUrl: summarizeDataUrl(fallbackDataUrl),
          fallbackMaskDataUrl: summarizeDataUrl(fallbackMaskDataUrl),
          queueLength: pendingCommitsRef.current.length
        });
        flushResolvedResultCommits();
      });
  };

  const resolveSmartMaskDataUrl = (): Promise<string | undefined> => {
    const canvas = smartMaskCanvasRef.current;
    return canvas ? encodeCanvasSnapshotDataUrl(canvas, { mode: "mask" }) : Promise.resolve(undefined);
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

  const startResultStroke = (canvas: HTMLCanvasElement, point: DrawPoint, pointerId: number) => {
    if (!isInpaintResultPaintTool(tool)) {
      writeInpaintDebugLog("inpaint-result:stroke-start-skip", {
        point: summarizePoint(point),
        pointerId,
        reason: "not-paint-tool",
        tool
      });
      return;
    }
    const strokeTool = tool;
    captureCanvasPointer(canvas, pointerId);
    undoDataUrlRef.current = readCommittedCanvasState()?.dataUrl ?? dataUrl;
    undoCanvasSnapshotRef.current = createUndoCanvasSnapshot(canvas, undoDataUrlRef.current);
    writeInpaintDebugLog("inpaint-result:stroke-start", {
      brushHardness,
      brushSize,
      capturesMaskDataUrl: strokeTool === "smartBrush",
      point: summarizePoint(point),
      pointerId,
      previousDataUrl: summarizeDataUrl(undoDataUrlRef.current),
      previousMaskDataUrl: summarizeDataUrl(readCommittedSmartMaskCanvasState()?.dataUrl ?? maskDataUrl),
      queueLength: pendingCommitsRef.current.length,
      strokeTool,
      toolStrength
    });
    startEditSession();
    markCanvasEdited();
    drawingRef.current = true;
    activeStrokePointerIdRef.current = pointerId;
    activeStrokeToolRef.current = strokeTool;
    lastPointRef.current = point;
    strokeForcesVisiblePixelsRef.current = false;
    if (strokeTool === "smartBrush") {
      undoMaskDataUrlRef.current = readCommittedSmartMaskCanvasState()?.dataUrl ?? maskDataUrl;
      undoSmartMaskCanvasSnapshotRef.current = smartMaskCanvasRef.current
        ? createUndoCanvasSnapshot(smartMaskCanvasRef.current, undoMaskDataUrlRef.current)
        : null;
      markSmartMaskEdited();
      smartMaskDrawingRef.current = true;
    }
    smudgePatchRef.current = strokeTool === "smudge" ? captureSmudgePatch(point) : null;
    if (strokeTool !== "smudge") {
      drawSegment(point, point, strokeTool);
    }
  };

  const finishResultStroke = (pointerId: number | null = null) => {
    if (!drawingRef.current) {
      writeInpaintDebugLog("inpaint-result:stroke-finish-skip", {
        pointerId,
        reason: "not-drawing",
        tool
      });
      return;
    }
    const activePointerId = activeStrokePointerIdRef.current;
    if (activePointerId !== null && pointerId !== null && activePointerId !== pointerId) {
      writeInpaintDebugLog("inpaint-result:stroke-finish-skip", {
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
    const strokeTool = activeStrokeToolRef.current;
    drawingRef.current = false;
    activeStrokePointerIdRef.current = null;
    activeStrokeToolRef.current = null;
    lastPointRef.current = null;
    smudgePatchRef.current = null;
    writeInpaintDebugLog("inpaint-result:stroke-finish", {
      activePointerId,
      strokeTool
    });
    commitChange(strokeTool);
    endEditSession();
  };

  React.useEffect(() => {
    if (!disabled || !drawingRef.current) {
      return;
    }
    finishResultStroke(activeStrokePointerIdRef.current);
  }, [disabled, finishResultStroke]);

  React.useEffect(() => {
    const finishActivePointer = (event: PointerEvent) => {
      if (!drawingRef.current) {
        return;
      }
      const activePointerId = activeStrokePointerIdRef.current;
      if (activePointerId !== null && activePointerId !== event.pointerId) {
        return;
      }
      const canvas = canvasRef.current;
      if (canvas && event.composedPath().includes(canvas)) {
        return;
      }
      finishResultStroke(event.pointerId);
    };

    window.addEventListener("pointerup", finishActivePointer, true);
    window.addEventListener("pointercancel", finishActivePointer, true);
    return () => {
      window.removeEventListener("pointerup", finishActivePointer, true);
      window.removeEventListener("pointercancel", finishActivePointer, true);
    };
  }, [finishResultStroke]);

  return (
    <>
      <canvas ref={smartMaskCanvasRef} aria-hidden="true" style={{ display: "none" }} />
      <canvas
        ref={canvasRef}
        className={`${className ?? ""} ${pointerEnabled ? "editing" : ""} ${selectionEnabled ? "selecting" : ""} ${colorPickerEnabled ? "color-picking" : ""}`.trim()}
        style={style}
        aria-label="인페인트 결과 레이어"
        onPointerEnter={(event) => {
          if (!colorPickerEnabled || colorPickerPointerIdRef.current !== null) {
            return;
          }
          setColorPickerHover({ point: resolvePoint(event) });
        }}
        onPointerDown={(event) => {
          if (colorPickerEnabled) {
            if (event.button !== 0) {
              return;
            }
            event.preventDefault();
            event.stopPropagation();
            const point = resolvePoint(event);
            if (!updateColorPickerPreview(point)) {
              return;
            }
            captureCanvasPointer(event.currentTarget, event.pointerId);
            colorPickerPointerIdRef.current = event.pointerId;
            return;
          }
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
          startResultStroke(event.currentTarget, point, event.pointerId);
        }}
        onPointerMove={(event) => {
          if (drawingRef.current) {
            event.preventDefault();
            event.stopPropagation();
            if (disabled || (event.buttons & 1) === 0) {
              finishResultStroke(event.pointerId);
              return;
            }
            const point = resolvePoint(event);
            if (!lastPointRef.current) {
              return;
            }
            drawSegment(lastPointRef.current, point, activeStrokeToolRef.current ?? "brush");
            lastPointRef.current = point;
            return;
          }
          if (colorPickerEnabled) {
            if (colorPickerPointerIdRef.current !== event.pointerId) {
              setColorPickerHover({ point: resolvePoint(event) });
              return;
            }
            event.preventDefault();
            event.stopPropagation();
            updateColorPickerPreview(resolvePoint(event), colorPickerPreview?.currentColor ?? brushColor);
            return;
          }
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
          if (isInpaintResultPaintTool(tool) && (event.buttons & 1) !== 0) {
            startResultStroke(event.currentTarget, point, event.pointerId);
          }
        }}
        onPointerUp={(event) => {
          if (colorPickerEnabled && colorPickerPointerIdRef.current === event.pointerId) {
            event.preventDefault();
            event.stopPropagation();
            const sampledColor = updateColorPickerPreview(resolvePoint(event), colorPickerPreview?.currentColor ?? brushColor);
            if (sampledColor) {
              onColorPick(sampledColor);
            }
            releaseCanvasPointer(event.currentTarget, event.pointerId);
            colorPickerPointerIdRef.current = null;
            setColorPickerPreview(null);
            return;
          }
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
          finishResultStroke(event.pointerId);
        }}
        onPointerLeave={() => {
          if (colorPickerPointerIdRef.current === null) {
            setColorPickerHover(null);
          }
        }}
        onPointerCancel={(event) => {
          if (colorPickerEnabled && colorPickerPointerIdRef.current === event.pointerId) {
            releaseCanvasPointer(event.currentTarget, event.pointerId);
            colorPickerPointerIdRef.current = null;
            setColorPickerHover(null);
            setColorPickerPreview(null);
            return;
          }
          if (selectionEnabled && selectionDragRef.current) {
            releaseCanvasPointer(event.currentTarget, event.pointerId);
            selectionDragRef.current = null;
            setPreviewSelectionRect(null);
            return;
          }
          if (!drawingRef.current) {
            return;
          }
          finishResultStroke(event.pointerId);
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
      {colorPickerEnabled && colorPickerHover && !colorPickerPreview ? (
        <div
          className="inpaint-color-picker-cursor"
          style={{
            left: `${(colorPickerHover.point.x / Math.max(1, pageSize.width)) * 100}%`,
            top: `${(colorPickerHover.point.y / Math.max(1, pageSize.height)) * 100}%`
          }}
          aria-hidden="true"
        />
      ) : null}
      {colorPickerPreview ? (
        <div
          className="inpaint-color-picker-preview"
          style={{
            left: `${(colorPickerPreview.point.x / Math.max(1, pageSize.width)) * 100}%`,
            top: `${(colorPickerPreview.point.y / Math.max(1, pageSize.height)) * 100}%`
          }}
          aria-hidden="true"
        >
          <span
            className="inpaint-color-picker-preview-half sampled"
            style={{ backgroundColor: colorPickerPreview.sampledColor }}
          />
          <span
            className="inpaint-color-picker-preview-half current"
            style={{ backgroundColor: colorPickerPreview.currentColor }}
          />
          <span className="inpaint-color-picker-preview-crosshair" />
        </div>
      ) : null}
    </>
  );
}

function rgbToHex(red: number, green: number, blue: number): string {
  return `#${[red, green, blue]
    .map((channel) => Math.min(255, Math.max(0, channel)).toString(16).padStart(2, "0"))
    .join("")}`;
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


function pickFallbackCanvasColor(canvas: HTMLCanvasElement | null | undefined, x: number, y: number): string | null {
  const context = canvas?.getContext("2d", { willReadFrequently: true });
  if (!canvas || !context || x >= canvas.width || y >= canvas.height) {
    return null;
  }

  const [red, green, blue] = context.getImageData(x, y, 1, 1).data;
  return rgbToHex(red, green, blue);
}

function sampleFinalOutputColor({
  imageCanvas,
  maskCanvas,
  overlayCanvas,
  point,
  resultCanvas
}: {
  imageCanvas: HTMLCanvasElement | null | undefined;
  maskCanvas: HTMLCanvasElement | null | undefined;
  overlayCanvas: HTMLCanvasElement | null | undefined;
  point: DrawPoint;
  resultCanvas: HTMLCanvasElement | null | undefined;
}): string | null {
  const x = Math.floor(point.x);
  const y = Math.floor(point.y);
  const base = readCanvasRgba(imageCanvas, x, y);
  const overlay = readCanvasRgba(overlayCanvas, x, y);
  if (!base || !overlay) {
    return null;
  }

  const result = readCanvasRgba(resultCanvas, x, y);
  const mask = readCanvasRgba(maskCanvas, x, y);
  const maskedResult = result && mask
    ? { ...result, a: result.a * (mask.a / 255) * (Math.max(mask.r, mask.g, mask.b) / 255) }
    : result;
  const withResult = maskedResult ? compositeRgba(base, maskedResult) : base;
  const finalColor = compositeRgba(withResult, overlay);
  return rgbToHex(Math.round(finalColor.r), Math.round(finalColor.g), Math.round(finalColor.b));
}

function readCanvasRgba(canvas: HTMLCanvasElement | null | undefined, x: number, y: number): { r: number; g: number; b: number; a: number } | null {
  const context = canvas?.getContext("2d", { willReadFrequently: true });
  if (!canvas || !context || canvas.width <= 0 || canvas.height <= 0) {
    return null;
  }
  const sampleX = Math.min(canvas.width - 1, Math.max(0, x));
  const sampleY = Math.min(canvas.height - 1, Math.max(0, y));
  const [r, g, b, a] = context.getImageData(sampleX, sampleY, 1, 1).data;
  return { r, g, b, a };
}

function compositeRgba(
  base: { r: number; g: number; b: number; a: number },
  top: { r: number; g: number; b: number; a: number }
): { r: number; g: number; b: number; a: number } {
  const topAlpha = top.a / 255;
  const baseAlpha = base.a / 255;
  const outAlpha = topAlpha + baseAlpha * (1 - topAlpha);
  if (outAlpha <= 0) {
    return { r: 0, g: 0, b: 0, a: 0 };
  }
  return {
    r: (top.r * topAlpha + base.r * baseAlpha * (1 - topAlpha)) / outAlpha,
    g: (top.g * topAlpha + base.g * baseAlpha * (1 - topAlpha)) / outAlpha,
    b: (top.b * topAlpha + base.b * baseAlpha * (1 - topAlpha)) / outAlpha,
    a: outAlpha * 255
  };
}
