import type { ViewportSize } from "./overlayLayout";

export const MAX_STAGE_WIDTH_PX = 1040;
export const MIN_STAGE_VIEW_SCALE = 0.1;
export const MAX_STAGE_VIEW_SCALE = 2;

type StageFitOptions = {
  maxWidth?: number;
  viewScale?: number | null;
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
