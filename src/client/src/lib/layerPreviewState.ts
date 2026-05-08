import {
  DEFAULT_LAYER_OPACITY,
  type ActiveLayer,
  type LayerOpacity,
  type LayerVisibility
} from "./layerState";

const FINAL_OUTPUT_LAYER_VISIBILITY: LayerVisibility = {
  image: true,
  inpaint: true,
  inpaintResult: true,
  inpaintMask: false,
  overlay: true
};

type ResolveStageLayerPreviewStateOptions = {
  activeLayer: ActiveLayer;
  forceFinalOutputPreviewActive?: boolean;
  layerOpacity: LayerOpacity;
  layerVisibility: LayerVisibility;
  overlayOpacityEditMode: boolean;
  temporaryPanActive: boolean;
};

export type StageLayerPreviewState = {
  finalOutputPreviewActive: boolean;
  inpaintResultComposite: boolean;
  layerOpacity: LayerOpacity;
  layerVisibility: LayerVisibility;
};

export function resolveStageLayerPreviewState({
  activeLayer,
  forceFinalOutputPreviewActive = false,
  layerOpacity,
  layerVisibility,
  overlayOpacityEditMode,
  temporaryPanActive
}: ResolveStageLayerPreviewStateOptions): StageLayerPreviewState {
  const finalOutputPreviewActive = temporaryPanActive || forceFinalOutputPreviewActive;
  if (finalOutputPreviewActive) {
    return {
      finalOutputPreviewActive: true,
      inpaintResultComposite: true,
      layerOpacity: DEFAULT_LAYER_OPACITY,
      layerVisibility: FINAL_OUTPUT_LAYER_VISIBILITY
    };
  }

  return {
    finalOutputPreviewActive: false,
    inpaintResultComposite: activeLayer !== "inpaintResult",
    layerOpacity: {
      ...layerOpacity,
      overlay: overlayOpacityEditMode ? 1 : layerOpacity.overlay
    },
    layerVisibility
  };
}
