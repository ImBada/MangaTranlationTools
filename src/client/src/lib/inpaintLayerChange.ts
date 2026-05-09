export type InpaintLayerChangeOptions = {
  intermediateUndoDataUrls?: (string | undefined)[];
  maskDataUrl?: string;
  maskDataUrlMode?: "full" | "patch";
  persist?: boolean;
  previousMaskDataUrl?: string;
  previousMaskSourceDataUrl?: string;
  previousDataUrl?: string;
  previousResultSourceDataUrl?: string;
  recordUndo?: boolean;
};
