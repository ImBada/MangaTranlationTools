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

export function eraseTouchedMaskIslands(canvas: HTMLCanvasElement, point: DrawPoint, brushSize = 1): boolean {
  const context = canvas.getContext("2d");
  if (!context || canvas.width <= 0 || canvas.height <= 0) {
    return false;
  }

  const startX = Math.floor(point.x);
  const startY = Math.floor(point.y);
  if (startX < 0 || startY < 0 || startX >= canvas.width || startY >= canvas.height) {
    return false;
  }

  const imageData = context.getImageData(0, 0, canvas.width, canvas.height);
  const { data } = imageData;
  const startIndexes = resolveMaskIslandStartIndexes(data, canvas.width, canvas.height, point, brushSize);
  if (startIndexes.length === 0) {
    return false;
  }

  const queue = new Int32Array(canvas.width * canvas.height);
  let head = 0;
  let tail = 0;
  for (const startIndex of startIndexes) {
    if (!isMaskPixelActive(data, startIndex)) {
      continue;
    }
    queue[tail] = startIndex;
    tail += 1;
    clearMaskPixel(data, startIndex);
  }

  while (head < tail) {
    const index = queue[head];
    head += 1;
    const x = index % canvas.width;
    const y = Math.floor(index / canvas.width);

    if (x > 0) {
      tail = enqueueActiveMaskPixel(data, queue, tail, index - 1);
    }
    if (x + 1 < canvas.width) {
      tail = enqueueActiveMaskPixel(data, queue, tail, index + 1);
    }
    if (y > 0) {
      tail = enqueueActiveMaskPixel(data, queue, tail, index - canvas.width);
    }
    if (y + 1 < canvas.height) {
      tail = enqueueActiveMaskPixel(data, queue, tail, index + canvas.width);
    }
  }

  context.putImageData(imageData, 0, 0);
  return true;
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

function enqueueActiveMaskPixel(data: Uint8ClampedArray, queue: Int32Array, tail: number, index: number): number {
  if (!isMaskPixelActive(data, index)) {
    return tail;
  }
  clearMaskPixel(data, index);
  queue[tail] = index;
  return tail + 1;
}

function resolveMaskIslandStartIndexes(
  data: Uint8ClampedArray,
  width: number,
  height: number,
  point: DrawPoint,
  brushSize: number
): number[] {
  const radius = Math.max(0.5, brushSize / 2);
  const radiusSquared = radius * radius;
  const minX = Math.max(0, Math.floor(point.x - radius));
  const minY = Math.max(0, Math.floor(point.y - radius));
  const maxX = Math.min(width - 1, Math.ceil(point.x + radius));
  const maxY = Math.min(height - 1, Math.ceil(point.y + radius));
  const indexes: number[] = [];

  for (let y = minY; y <= maxY; y += 1) {
    for (let x = minX; x <= maxX; x += 1) {
      const dx = Math.max(Math.abs(x + 0.5 - point.x) - 0.5, 0);
      const dy = Math.max(Math.abs(y + 0.5 - point.y) - 0.5, 0);
      const distanceSquared = dx * dx + dy * dy;
      if (distanceSquared > radiusSquared) {
        continue;
      }

      const index = y * width + x;
      if (isMaskPixelActive(data, index)) {
        indexes.push(index);
      }
    }
  }

  return indexes;
}

function isMaskPixelActive(data: Uint8ClampedArray, index: number): boolean {
  return data[index * 4 + 3] > 0;
}

function clearMaskPixel(data: Uint8ClampedArray, index: number): void {
  const offset = index * 4;
  data[offset] = 0;
  data[offset + 1] = 0;
  data[offset + 2] = 0;
  data[offset + 3] = 0;
}
