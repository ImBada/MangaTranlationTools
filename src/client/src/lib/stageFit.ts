import type { ViewportSize } from "./overlayLayout";

export const MAX_STAGE_WIDTH_PX = 1040;
export const MIN_STAGE_VIEW_SCALE = 0.1;
export const MAX_STAGE_VIEW_SCALE = 2;
export const STAGE_PAN_EDGE_MARGIN_PX = 80;

type StageFitOptions = {
  maxWidth?: number;
  viewScale?: number | null;
};

export type StagePanRange = {
  minX: number;
  maxX: number;
  minY: number;
  maxY: number;
};

type RectBounds = {
  left: number;
  top: number;
  right: number;
  bottom: number;
  width: number;
  height: number;
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

export function resolveStagePanBounds(stageSize: ViewportSize, clipSize: ViewportSize): ViewportSize {
  const stageWidth = Math.max(1, stageSize.width);
  const stageHeight = Math.max(1, stageSize.height);
  const clipWidth = Math.max(1, clipSize.width);
  const clipHeight = Math.max(1, clipSize.height);

  return {
    width: stageWidth > clipWidth ? stageWidth / 2 : 0,
    height: stageHeight > clipHeight ? stageHeight / 2 : 0
  };
}

export function resolveStagePanRange(
  stageRect: RectBounds,
  clipRect: RectBounds,
  edgeMarginPx = STAGE_PAN_EDGE_MARGIN_PX
): StagePanRange {
  const margin = Math.max(0, edgeMarginPx);
  const minX = stageRect.width > clipRect.width ? clipRect.right - stageRect.right - margin : -margin;
  const maxX = stageRect.width > clipRect.width ? clipRect.left - stageRect.left + margin : margin;
  const minY = stageRect.height > clipRect.height ? clipRect.bottom - stageRect.bottom - margin : -margin;
  const maxY = stageRect.height > clipRect.height ? clipRect.top - stageRect.top + margin : margin;

  return { minX, maxX, minY, maxY };
}
