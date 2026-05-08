import type { ViewportSize } from "./overlayLayout";

export const MAX_STAGE_WIDTH_PX = 1040;
export const MIN_STAGE_VIEW_SCALE = 0.1;
export const MAX_STAGE_VIEW_SCALE = 10;
export const STAGE_DRAG_ZOOM_PIXELS_PER_DOUBLE = 240;

type StageFitOptions = {
  maxWidth?: number;
  viewScale?: number | null;
};

type PanOffset = {
  x: number;
  y: number;
};

type StageZoomAnchorPanOptions = {
  anchorClientX: number;
  anchorClientY: number;
  centerClientX: number;
  centerClientY: number;
  contentX: number;
  contentY: number;
  nextScale: number;
  startScale: number;
};

export function resolveStageFitSize(
  pageSize: ViewportSize,
  bounds: ViewportSize,
  options: StageFitOptions = {}
): ViewportSize {
  const pageWidth = Math.max(1, pageSize.width);
  const pageHeight = Math.max(1, pageSize.height);
  const boundedWidth = Math.max(1, bounds.width);
  const boundedHeight = Math.max(1, bounds.height);
  const maxWidth = options.maxWidth ?? MAX_STAGE_WIDTH_PX;
  const scale =
    typeof options.viewScale === "number"
      ? clampStageViewScale(options.viewScale)
      : Math.min(boundedWidth / pageWidth, boundedHeight / pageHeight, maxWidth / pageWidth, 1);

  return {
    width: pageWidth * scale,
    height: pageHeight * scale
  };
}

export function clampStageViewScale(scale: number): number {
  if (!Number.isFinite(scale)) {
    return 1;
  }

  return Math.min(MAX_STAGE_VIEW_SCALE, Math.max(MIN_STAGE_VIEW_SCALE, scale));
}

export function resolveStageDragZoomScale(
  startScale: number,
  deltaX: number,
  pixelsPerDouble = STAGE_DRAG_ZOOM_PIXELS_PER_DOUBLE
): number {
  const safeStartScale = Number.isFinite(startScale) ? startScale : 1;
  const safeDeltaX = Number.isFinite(deltaX) ? deltaX : 0;
  const safePixelsPerDouble =
    Number.isFinite(pixelsPerDouble) && pixelsPerDouble > 0
      ? pixelsPerDouble
      : STAGE_DRAG_ZOOM_PIXELS_PER_DOUBLE;

  return clampStageViewScale(safeStartScale * Math.pow(2, safeDeltaX / safePixelsPerDouble));
}

export function resolveStageZoomAnchorPanOffset({
  anchorClientX,
  anchorClientY,
  centerClientX,
  centerClientY,
  contentX,
  contentY,
  nextScale,
  startScale
}: StageZoomAnchorPanOptions): PanOffset {
  const safeStartScale = Number.isFinite(startScale) && startScale > 0 ? startScale : 1;
  const safeNextScale = Number.isFinite(nextScale) && nextScale > 0 ? nextScale : safeStartScale;
  const scaleRatio = safeNextScale / safeStartScale;

  return {
    x: anchorClientX - centerClientX - contentX * scaleRatio,
    y: anchorClientY - centerClientY - contentY * scaleRatio
  };
}
