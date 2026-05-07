import type { ImageRect } from "../../../shared/types";
import { isInpaintMaskPixelCovered } from "./inpaintMaskImages";

export type DrawPoint = {
  x: number;
  y: number;
};

export type SelectionDragState = {
  start: DrawPoint;
  current: DrawPoint;
};

export type MaskIslandSelection = {
  originalData: Uint8ClampedArray;
  selected: Uint8Array;
  selectedCount: number;
  width: number;
  height: number;
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

export function createMaskIslandSelection(canvas: HTMLCanvasElement): MaskIslandSelection | null {
  const context = canvas.getContext("2d");
  if (!context || canvas.width <= 0 || canvas.height <= 0) {
    return null;
  }

  const imageData = context.getImageData(0, 0, canvas.width, canvas.height);
  return {
    originalData: new Uint8ClampedArray(imageData.data),
    selected: new Uint8Array(canvas.width * canvas.height),
    selectedCount: 0,
    width: canvas.width,
    height: canvas.height
  };
}

export function selectTouchedMaskIslands(selection: MaskIslandSelection, point: DrawPoint, brushSize = 1): boolean {
  const startX = Math.floor(point.x);
  const startY = Math.floor(point.y);
  if (startX < 0 || startY < 0 || startX >= selection.width || startY >= selection.height) {
    return false;
  }

  const startIndexes = resolveMaskIslandStartIndexes(selection, point, brushSize);
  if (startIndexes.length === 0) {
    return false;
  }

  const queue = new Int32Array(selection.width * selection.height);
  let head = 0;
  let tail = 0;
  for (const startIndex of startIndexes) {
    if (!isSelectableMaskPixel(selection, startIndex)) {
      continue;
    }
    queue[tail] = startIndex;
    tail += 1;
    markMaskPixelSelected(selection, startIndex);
  }

  while (head < tail) {
    const index = queue[head];
    head += 1;
    const x = index % selection.width;
    const y = Math.floor(index / selection.width);

    if (x > 0) {
      tail = enqueueSelectableMaskPixel(selection, queue, tail, index - 1);
    }
    if (x + 1 < selection.width) {
      tail = enqueueSelectableMaskPixel(selection, queue, tail, index + 1);
    }
    if (y > 0) {
      tail = enqueueSelectableMaskPixel(selection, queue, tail, index - selection.width);
    }
    if (y + 1 < selection.height) {
      tail = enqueueSelectableMaskPixel(selection, queue, tail, index + selection.width);
    }
  }

  return true;
}

export function renderMaskIslandSelectionPreview(canvas: HTMLCanvasElement, selection: MaskIslandSelection): void {
  writeMaskSelectionData(canvas, selection, true);
}

export function restoreMaskIslandSelection(canvas: HTMLCanvasElement, selection: MaskIslandSelection): void {
  writeMaskSelectionData(canvas, selection, false);
}

export function eraseSelectedMaskIslands(canvas: HTMLCanvasElement, selection: MaskIslandSelection): boolean {
  if (selection.selectedCount <= 0) {
    return false;
  }

  writeMaskSelectionData(canvas, selection, false, true);
  return true;
}

export function isCanvasBlank(canvas: HTMLCanvasElement): boolean {
  const context = canvas.getContext("2d");
  if (!context) {
    return true;
  }

  const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
  for (let offset = 0; offset + 3 < pixels.length; offset += 4) {
    if (isInpaintMaskPixelCovered(pixels, offset)) {
      return false;
    }
  }
  return true;
}

function enqueueSelectableMaskPixel(selection: MaskIslandSelection, queue: Int32Array, tail: number, index: number): number {
  if (!isSelectableMaskPixel(selection, index)) {
    return tail;
  }
  markMaskPixelSelected(selection, index);
  queue[tail] = index;
  return tail + 1;
}

function resolveMaskIslandStartIndexes(selection: MaskIslandSelection, point: DrawPoint, brushSize: number): number[] {
  const radius = Math.max(0.5, brushSize / 2);
  const radiusSquared = radius * radius;
  const minX = Math.max(0, Math.floor(point.x - radius));
  const minY = Math.max(0, Math.floor(point.y - radius));
  const maxX = Math.min(selection.width - 1, Math.ceil(point.x + radius));
  const maxY = Math.min(selection.height - 1, Math.ceil(point.y + radius));
  const indexes: number[] = [];

  for (let y = minY; y <= maxY; y += 1) {
    for (let x = minX; x <= maxX; x += 1) {
      const dx = Math.max(Math.abs(x + 0.5 - point.x) - 0.5, 0);
      const dy = Math.max(Math.abs(y + 0.5 - point.y) - 0.5, 0);
      const distanceSquared = dx * dx + dy * dy;
      if (distanceSquared > radiusSquared) {
        continue;
      }

      const index = y * selection.width + x;
      if (isSelectableMaskPixel(selection, index)) {
        indexes.push(index);
      }
    }
  }

  return indexes;
}

function isSelectableMaskPixel(selection: MaskIslandSelection, index: number): boolean {
  return selection.selected[index] === 0 && isInpaintMaskPixelCovered(selection.originalData, index * 4);
}

function markMaskPixelSelected(selection: MaskIslandSelection, index: number): void {
  selection.selected[index] = 1;
  selection.selectedCount += 1;
}

function clearMaskPixel(data: Uint8ClampedArray, index: number): void {
  const offset = index * 4;
  data[offset] = 0;
  data[offset + 1] = 0;
  data[offset + 2] = 0;
  data[offset + 3] = 0;
}

function writeMaskSelectionData(
  canvas: HTMLCanvasElement,
  selection: MaskIslandSelection,
  previewSelected: boolean,
  clearSelected = false
): void {
  const context = canvas.getContext("2d");
  if (!context) {
    return;
  }

  const imageData = context.createImageData(selection.width, selection.height);
  imageData.data.set(selection.originalData);
  for (let index = 0; index < selection.selected.length; index += 1) {
    if (selection.selected[index] === 0) {
      continue;
    }

    if (clearSelected) {
      clearMaskPixel(imageData.data, index);
    } else if (previewSelected) {
      tintSelectedMaskPixel(imageData.data, index);
    }
  }
  context.putImageData(imageData, 0, 0);
}

function tintSelectedMaskPixel(data: Uint8ClampedArray, index: number): void {
  const offset = index * 4;
  data[offset] = 255;
  data[offset + 1] = 120;
  data[offset + 2] = 88;
  data[offset + 3] = Math.max(data[offset + 3], 180);
}
