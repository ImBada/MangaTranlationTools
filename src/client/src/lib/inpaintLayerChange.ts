export type InpaintLayerChangeOptions = {
  intermediateLayerUndoSnapshots?: {
    maskDataUrl: string | undefined;
    resultDataUrl: string | undefined;
  }[];
  intermediateUndoDataUrls?: (string | undefined)[];
  maskDataUrl?: string;
  persist?: boolean;
  previousMaskDataUrl?: string;
  previousDataUrl?: string;
  recordUndo?: boolean;
};
