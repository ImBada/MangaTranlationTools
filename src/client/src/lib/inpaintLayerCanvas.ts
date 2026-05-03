import type { ImageRect } from "../../../shared/types";

export type DrawPoint = {
  x: number;
  y: number;
};

export type SelectionDragState = {
  start: DrawPoint;
  current: DrawPoint;
};

export function resolveCanvasPoint(
  clientX: number,
  clientY: number,
  rect: DOMRect,
  pageSize: { width: number; height: number }
): DrawPoint {
  return {
    x: ((clientX - rect.left) / Math.max(1, rect.width)) * pageSize.width,
    y: ((clientY - rect.top) / Math.max(1, rect.height)) * pageSize.height
  };
}

export function resolveSelectionRect(
  from: DrawPoint,
  to: DrawPoint,
  pageSize: { width: number; height: number }
): ImageRect {
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
}

export function drawMaskSegment(
  context: CanvasRenderingContext2D,
  from: DrawPoint,
  to: DrawPoint,
  brushSize: number,
  erasing: boolean
): void {
  context.save();
  context.lineCap = "round";
  context.lineJoin = "round";
  context.lineWidth = Math.max(1, brushSize);
  context.globalCompositeOperation = erasing ? "destination-out" : "source-over";
  context.strokeStyle = "#ffffff";
  context.beginPath();
  context.moveTo(from.x, from.y);
  context.lineTo(to.x, to.y);
  context.stroke();
  context.restore();
}

export function isCanvasBlank(canvas: HTMLCanvasElement): boolean {
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
