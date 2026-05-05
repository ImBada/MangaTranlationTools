import React, { useRef, useState } from "react";
import type { ImageRect } from "../../../shared/types";
import { useCanvasImageSync } from "../hooks/useCanvasImageSync";
import {
  blendChannel,
  brushMaskAlpha,
  clamp01,
  createBrushGradient,
  isCanvasBlank,
  parseHexColor,
  resolveBrushBounds,
  resolveSelectionRect,
  rgbaToCss,
  sampleBlur,
  sampleSharpen,
  type DrawPoint
} from "../lib/inpaintResultCanvas";
import type { InpaintLayerChangeOptions } from "../lib/inpaintLayerChange";

export type InpaintResultTool = "select" | "brush" | "eraser" | "blur" | "sharpen" | "smudge";

type InpaintResultCanvasProps = {
  dataUrl?: string;
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
  onSelectionChange: (rect: ImageRect | null) => void;
};

type SelectionDragState = {
  start: DrawPoint;
  current: DrawPoint;
};

export function InpaintResultCanvas({
  dataUrl,
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
  onSelectionChange
}: InpaintResultCanvasProps): React.JSX.Element {
  const {
    canvasRef,
    drawingRef,
    markCanvasCommitted,
    markCanvasEdited
  } = useCanvasImageSync({
    dataUrl,
    loadErrorMessage: "인페인트 결과 레이어를 불러오지 못했습니다.",
    pageSize,
    willReadFrequently: true
  });
  const changedRef = useRef(false);
  const lastPointRef = useRef<DrawPoint | null>(null);
  const smudgePatchRef = useRef<ImageData | null>(null);
  const selectionDragRef = useRef<SelectionDragState | null>(null);
  const undoDataUrlRef = useRef<string | undefined>(undefined);
  const [previewSelectionRect, setPreviewSelectionRect] = useState<ImageRect | null>(null);
  const [cursorPoint, setCursorPoint] = useState<DrawPoint | null>(null);

  const pointerEnabled = !disabled && tool !== "select";
  const selectionEnabled = !disabled && tool === "select";
  const activeSelectionRect = previewSelectionRect ?? selectionRect;
  const cursorSize = Math.max(1, brushSize);

  const drawSegment = (from: DrawPoint, to: DrawPoint) => {
    if (tool === "smudge") {
      drawSmudge(to);
      return;
    }

    const distance = Math.hypot(to.x - from.x, to.y - from.y);
    const step = Math.max(1, brushSize / (tool === "brush" || tool === "eraser" ? 4 : 2));
    const steps = Math.max(1, Math.ceil(distance / step));
    for (let index = 0; index <= steps; index += 1) {
      const t = index / steps;
      const point = {
        x: from.x + (to.x - from.x) * t,
        y: from.y + (to.y - from.y) * t
      };
      if (tool === "brush" || tool === "eraser") {
        stampPaint(point);
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

    const radius = Math.max(0.5, brushSize / 2);
    const color = parseHexColor(brushColor);
    const hardness = clamp01(brushHardness);
    const innerRadius = radius * hardness;
    const gradient = context.createRadialGradient(point.x, point.y, 0, point.x, point.y, radius);
    const solidStop = Math.min(1, hardness);

    if (tool === "eraser") {
      gradient.addColorStop(0, "rgba(0, 0, 0, 1)");
      gradient.addColorStop(solidStop, "rgba(0, 0, 0, 1)");
      gradient.addColorStop(1, "rgba(0, 0, 0, 0)");
    } else {
      gradient.addColorStop(0, rgbaToCss(color, 1));
      gradient.addColorStop(solidStop, rgbaToCss(color, 1));
      gradient.addColorStop(1, rgbaToCss(color, innerRadius >= radius ? 1 : 0));
    }

    context.save();
    context.globalCompositeOperation = tool === "eraser" ? "destination-out" : "source-over";
    context.fillStyle = gradient;
    context.beginPath();
    context.arc(point.x, point.y, radius, 0, Math.PI * 2);
    context.fill();
    context.restore();
    changedRef.current = true;
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

  const commitChange = () => {
    const canvas = canvasRef.current;
    if (!canvas || !changedRef.current) {
      return;
    }

    const previousDataUrl = undoDataUrlRef.current;
    const nextDataUrl = isCanvasBlank(canvas) ? undefined : canvas.toDataURL("image/png");
    undoDataUrlRef.current = undefined;
    changedRef.current = false;
    markCanvasCommitted(nextDataUrl);
    onChange(nextDataUrl, { previousDataUrl });
  };

  return (
    <>
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
          setCursorPoint(point);
          undoDataUrlRef.current = isCanvasBlank(event.currentTarget) ? undefined : event.currentTarget.toDataURL("image/png");
          markCanvasEdited();
          drawingRef.current = true;
          lastPointRef.current = point;
          smudgePatchRef.current = tool === "smudge" ? captureSmudgePatch(point) : null;
          if (tool !== "smudge") {
            drawSegment(point, point);
          }
        }}
        onPointerEnter={(event) => {
          if (!pointerEnabled && !selectionEnabled) {
            return;
          }
          setCursorPoint(resolvePoint(event));
        }}
        onPointerMove={(event) => {
          if (selectionEnabled) {
            event.preventDefault();
            event.stopPropagation();
            const point = resolvePoint(event);
            setCursorPoint(point);
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
          setCursorPoint(point);
          if (!drawingRef.current || !lastPointRef.current) {
            return;
          }
          drawSegment(lastPointRef.current, point);
          lastPointRef.current = point;
        }}
        onPointerLeave={() => {
          if (!drawingRef.current) {
            setCursorPoint(null);
          }
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
          setCursorPoint(null);
          commitChange();
        }}
      />
      {pointerEnabled && cursorPoint ? (
        <div
          className={`inpaint-brush-cursor ${tool}`}
          style={{
            left: `${(cursorPoint.x / Math.max(1, pageSize.width)) * 100}%`,
            top: `${(cursorPoint.y / Math.max(1, pageSize.height)) * 100}%`,
            width: `${(cursorSize / Math.max(1, pageSize.width)) * 100}%`,
            height: `${(cursorSize / Math.max(1, pageSize.height)) * 100}%`
          }}
        />
      ) : null}
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
