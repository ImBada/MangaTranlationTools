import React, { useEffect, useRef, useState } from "react";
import type { ImageRect } from "../../../shared/types";

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
  onChange: (dataUrl: string | undefined) => void;
  onSelectionChange: (rect: ImageRect | null) => void;
};

type DrawPoint = {
  x: number;
  y: number;
};

type SelectionDragState = {
  start: DrawPoint;
  current: DrawPoint;
};

type RgbaColor = {
  r: number;
  g: number;
  b: number;
  a: number;
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
  const canvasRef = useRef<HTMLCanvasElement | null>(null);
  const drawingRef = useRef(false);
  const changedRef = useRef(false);
  const lastPointRef = useRef<DrawPoint | null>(null);
  const smudgePatchRef = useRef<ImageData | null>(null);
  const selectionDragRef = useRef<SelectionDragState | null>(null);
  const [previewSelectionRect, setPreviewSelectionRect] = useState<ImageRect | null>(null);
  const [cursorPoint, setCursorPoint] = useState<DrawPoint | null>(null);

  useEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d", { willReadFrequently: true });
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

    changedRef.current = false;
    onChange(isCanvasBlank(canvas) ? undefined : canvas.toDataURL("image/png"));
  };

  const resolveSelectionRect = (from: DrawPoint, to: DrawPoint): ImageRect => {
    const left = Math.max(0, Math.min(from.x, to.x));
    const top = Math.max(0, Math.min(from.y, to.y));
    const right = Math.min(pageSize.width, Math.max(from.x, to.x));
    const bottom = Math.min(pageSize.height, Math.max(from.y, to.y));
    return {
      x: Math.floor(left),
      y: Math.floor(top),
      width: Math.max(0, Math.ceil(right) - Math.floor(left)),
      height: Math.max(0, Math.ceil(bottom) - Math.floor(top))
    };
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
            setPreviewSelectionRect(resolveSelectionRect(point, point));
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
              setPreviewSelectionRect(resolveSelectionRect(drag.start, point));
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
            const rect = resolveSelectionRect(selectionDragRef.current.start, point);
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

function resolveBrushBounds(point: DrawPoint, radius: number, width: number, height: number): { x: number; y: number; width: number; height: number } {
  const x = Math.max(0, Math.floor(point.x - radius));
  const y = Math.max(0, Math.floor(point.y - radius));
  const right = Math.min(width, Math.ceil(point.x + radius));
  const bottom = Math.min(height, Math.ceil(point.y + radius));
  return {
    x,
    y,
    width: Math.max(0, right - x),
    height: Math.max(0, bottom - y)
  };
}

function brushMaskAlpha(x: number, y: number, center: DrawPoint, radius: number, hardness: number): number {
  const distance = Math.hypot(x - center.x, y - center.y);
  if (distance >= radius) {
    return 0;
  }
  const hardRadius = radius * hardness;
  if (distance <= hardRadius) {
    return 1;
  }
  return 1 - (distance - hardRadius) / Math.max(1, radius - hardRadius);
}

function sampleBlur(source: Uint8ClampedArray, width: number, height: number, x: number, y: number): [number, number, number] {
  let red = 0;
  let green = 0;
  let blue = 0;
  let count = 0;
  for (let dy = -1; dy <= 1; dy += 1) {
    for (let dx = -1; dx <= 1; dx += 1) {
      const sx = Math.max(0, Math.min(width - 1, x + dx));
      const sy = Math.max(0, Math.min(height - 1, y + dy));
      const offset = (sy * width + sx) * 4;
      red += source[offset];
      green += source[offset + 1];
      blue += source[offset + 2];
      count += 1;
    }
  }
  return [red / count, green / count, blue / count];
}

function sampleSharpen(source: Uint8ClampedArray, width: number, height: number, x: number, y: number): [number, number, number] {
  const center = readRgb(source, width, height, x, y);
  const left = readRgb(source, width, height, x - 1, y);
  const right = readRgb(source, width, height, x + 1, y);
  const top = readRgb(source, width, height, x, y - 1);
  const bottom = readRgb(source, width, height, x, y + 1);
  return [
    clampByte(center[0] * 5 - left[0] - right[0] - top[0] - bottom[0]),
    clampByte(center[1] * 5 - left[1] - right[1] - top[1] - bottom[1]),
    clampByte(center[2] * 5 - left[2] - right[2] - top[2] - bottom[2])
  ];
}

function readRgb(source: Uint8ClampedArray, width: number, height: number, x: number, y: number): [number, number, number] {
  const sx = Math.max(0, Math.min(width - 1, x));
  const sy = Math.max(0, Math.min(height - 1, y));
  const offset = (sy * width + sx) * 4;
  return [source[offset], source[offset + 1], source[offset + 2]];
}

function createBrushGradient(context: CanvasRenderingContext2D, radius: number, hardness: number): CanvasGradient {
  const gradient = context.createRadialGradient(radius, radius, 0, radius, radius, radius);
  const solidStop = Math.min(1, clamp01(hardness));
  gradient.addColorStop(0, "rgba(0, 0, 0, 1)");
  gradient.addColorStop(solidStop, "rgba(0, 0, 0, 1)");
  gradient.addColorStop(1, "rgba(0, 0, 0, 0)");
  return gradient;
}

function parseHexColor(value: string): RgbaColor {
  const normalized = /^#[0-9a-f]{6}$/iu.test(value) ? value.slice(1) : "ffffff";
  return {
    r: Number.parseInt(normalized.slice(0, 2), 16),
    g: Number.parseInt(normalized.slice(2, 4), 16),
    b: Number.parseInt(normalized.slice(4, 6), 16),
    a: 1
  };
}

function rgbaToCss(color: RgbaColor, alpha: number): string {
  return `rgba(${color.r}, ${color.g}, ${color.b}, ${clamp01(color.a * alpha)})`;
}

function blendChannel(source: number, target: number, amount: number): number {
  return clampByte(source + (target - source) * amount);
}

function clampByte(value: number): number {
  return Math.max(0, Math.min(255, Math.round(value)));
}

function clamp01(value: number): number {
  return Math.max(0, Math.min(1, value));
}

function isCanvasBlank(canvas: HTMLCanvasElement): boolean {
  const context = canvas.getContext("2d");
  if (!context) {
    return true;
  }

  const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
  for (let index = 3; index < pixels.length; index += 4) {
    if (pixels[index] !== 0) {
      return false;
    }
  }
  return true;
}
