export type LayerVisibility = {
  image: boolean;
  inpaint: boolean;
  inpaintResult: boolean;
  inpaintMask: boolean;
  overlay: boolean;
};

export type LayerOpacity = {
  image: number;
  inpaint: number;
  inpaintResult: number;
  inpaintMask: number;
  overlay: number;
};

export type ActiveLayer = "output" | "image" | "inpaint" | "inpaintResult" | "inpaintMask" | "overlay";

export const DEFAULT_LAYER_OPACITY: LayerOpacity = {
  image: 1,
  inpaint: 1,
  inpaintResult: 1,
  inpaintMask: 0,
  overlay: 1
};

export const LAYER_FOCUS_OPACITY: Record<ActiveLayer, Partial<LayerOpacity>> = {
  output: DEFAULT_LAYER_OPACITY,
  image: {
    image: 1,
    inpaint: 0,
    inpaintResult: 0,
    inpaintMask: 0,
    overlay: 0
  },
  inpaint: {
    image: 0.5,
    inpaint: 1,
    inpaintResult: 1,
    inpaintMask: 0
  },
  inpaintResult: {
    image: 0.5,
    inpaint: 1,
    inpaintResult: 1,
    inpaintMask: 0,
    overlay: 0.5
  },
  inpaintMask: {
    image: 0.5,
    inpaint: 1,
    inpaintResult: 0,
    inpaintMask: 1,
    overlay: 0.5
  },
  overlay: {
    image: 1,
    inpaint: 0.2,
    inpaintResult: 0.2,
    inpaintMask: 0,
    overlay: 1
  }
};
