import type { InpaintSettings } from "../../../shared/types";

export const DEFAULT_INPAINT_SETTINGS: InpaintSettings = {
  engine: "lama",
  paddingPx: 0,
  featherPx: 0,
  tileSize: 1024,
  artifactCleanupPx: 8
};

export const INPAINT_RESULT_BRUSH_SIZE_MIN = 2;
export const INPAINT_RESULT_BRUSH_SIZE_MAX = 128;
export const INPAINT_MASK_BRUSH_SIZE_MIN = 4;
export const INPAINT_MASK_BRUSH_SIZE_MAX = 96;

export function clampInpaintResultBrushSize(value: number): number {
  if (!Number.isFinite(value)) {
    return 28;
  }
  return Math.min(INPAINT_RESULT_BRUSH_SIZE_MAX, Math.max(INPAINT_RESULT_BRUSH_SIZE_MIN, Math.round(value)));
}

export function clampInpaintMaskBrushSize(value: number): number {
  if (!Number.isFinite(value)) {
    return 28;
  }
  return Math.min(INPAINT_MASK_BRUSH_SIZE_MAX, Math.max(INPAINT_MASK_BRUSH_SIZE_MIN, Math.round(value)));
}
