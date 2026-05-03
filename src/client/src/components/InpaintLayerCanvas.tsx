import React, { useEffect, useRef, useState } from "react";
import type { ImageRect } from "../../../shared/types";
import {
  drawMaskSegment,
  isCanvasBlank,
  resolveCanvasPoint,
  resolveSelectionRect,
  type DrawPoint,
  type SelectionDragState
} from "../lib/inpaintLayerCanvas";

export type InpaintTool = "select" | "brush" | "eraser";

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
  onChange: (dataUrl: string | undefined) => void;
  onSelectionChange: (rect: ImageRect | null) => void;
};

export function InpaintLayerCanvas({
  dataUrl,
  pageSize,
  tool,
  brushSize,
  disabled,
  selectionRect,
  onChange,
  onSelectionChange
}: InpaintLayerCanvasProps): React.JSX.Element {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const drawingRef = useRef(false);
  const changedRef = useRef(false);
  const lastPointRef = useRef<DrawPoint | null>(null);
  const selectionDragRef = useRef<SelectionDragState | null>(null);
  const [previewSelectionRect, setPreviewSelectionRect] = useState<ImageRect | null>(null);
  const [cursorPoint, setCursorPoint] = useState<DrawPoint | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d");
    if (!canvas || !context) {
      return;
    }

    canvas.width = pageSize.width;
    canvas.height = pageSize.height;
    context.clearRect(0, 0, canvas.width, canvas.height);

    if (!dataUrl) {
      return;
    }

    const image = new Image();
    image.onload = () => {
      context.clearRect(0, 0, canvas.width, canvas.height);
      context.drawImage(image, 0, 0, canvas.width, canvas.height);
    };
    image.src = dataUrl;
  }, [dataUrl, pageSize.height, pageSize.width]);

  const pointerEnabled = !disabled && tool !== "select";
  const selectionEnabled = !disabled && tool === "select";
  const activeSelectionRect = previewSelectionRect ?? selectionRect;
  const cursorSize = Math.max(1, brushSize);

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

  const commitChange = () => {
    const canvas = canvasRef.current;
    if (!canvas || !changedRef.current) {
      return;
    }

    changedRef.current = false;
    onChange(isCanvasBlank(canvas) ? undefined : canvas.toDataURL("image/png"));
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
          event.currentTarget.setPointerCapture(event.pointerId);
          const point = resolvePoint(event);
          setCursorPoint(point);
          drawingRef.current = true;
          lastPointRef.current = point;
          drawSegment(point, point);
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
          const point = resolvePoint(event);
          setCursorPoint(point);
          event.currentTarget.releasePointerCapture(event.pointerId);
          drawingRef.current = false;
          lastPointRef.current = null;
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
