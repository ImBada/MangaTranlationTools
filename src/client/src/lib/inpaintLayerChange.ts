export type InpaintLayerChangeOptions = {
  intermediateLayerUndoSnapshots?: {
    maskDataUrl: string | undefined;
    resultDataUrl: string | undefined;
  }[];
  intermediateUndoDataUrls?: (string | undefined)[];
  maskDataUrl?: string;
  maskDataUrlMode?: "full" | "patch";
  persist?: boolean;
  previousMaskDataUrl?: string;
  previousDataUrl?: string;
  recordUndo?: boolean;
};
