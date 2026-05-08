import { describe, expect, it } from "vitest";
import { DEFAULT_LAYER_OPACITY, type LayerOpacity, type LayerVisibility } from "../src/client/src/lib/layerState";
import { resolveStageLayerPreviewState } from "../src/client/src/lib/layerPreviewState";

const layerOpacity: LayerOpacity = {
  image: 0.2,
  inpaint: 0.3,
  inpaintResult: 0.4,
  inpaintMask: 0.5,
  overlay: 0.6
};

const layerVisibility: LayerVisibility = {
  image: false,
  inpaint: false,
  inpaintResult: false,
  inpaintMask: true,
  overlay: false
};

describe("layer preview state", () => {
  it("keeps editable inpaint result above the overlay until final preview is active", () => {
    const preview = resolveStageLayerPreviewState({
      activeLayer: "inpaintResult",
      layerOpacity,
      layerVisibility,
      overlayOpacityEditMode: false,
      temporaryPanActive: false
    });

    expect(preview).toEqual({
      finalOutputPreviewActive: false,
      inpaintResultComposite: false,
      layerOpacity,
      layerVisibility
    });
  });

  it("applies overlay opacity edit mode in normal stage rendering", () => {
    const preview = resolveStageLayerPreviewState({
      activeLayer: "overlay",
      layerOpacity,
      layerVisibility,
      overlayOpacityEditMode: true,
      temporaryPanActive: false
    });

    expect(preview.layerOpacity).toEqual({
      ...layerOpacity,
      overlay: 1
    });
    expect(preview.inpaintResultComposite).toBe(true);
  });

  it("uses one final-output preview state for temporary pan regardless of active layer", () => {
    const imagePreview = resolveStageLayerPreviewState({
      activeLayer: "image",
      layerOpacity,
      layerVisibility,
      overlayOpacityEditMode: true,
      temporaryPanActive: true
    });
    const resultPreview = resolveStageLayerPreviewState({
      activeLayer: "inpaintResult",
      layerOpacity,
      layerVisibility,
      overlayOpacityEditMode: false,
      temporaryPanActive: true
    });

    expect(imagePreview).toEqual(resultPreview);
    expect(imagePreview).toEqual({
      finalOutputPreviewActive: true,
      inpaintResultComposite: true,
      layerOpacity: DEFAULT_LAYER_OPACITY,
      layerVisibility: {
        image: true,
        inpaint: true,
        inpaintResult: true,
        inpaintMask: false,
        overlay: true
      }
    });
  });

  it("uses the same final-output preview state for explicit preview tools", () => {
    const preview = resolveStageLayerPreviewState({
      activeLayer: "inpaintResult",
      forceFinalOutputPreviewActive: true,
      layerOpacity,
      layerVisibility,
      overlayOpacityEditMode: false,
      temporaryPanActive: false
    });

    expect(preview.finalOutputPreviewActive).toBe(true);
    expect(preview.inpaintResultComposite).toBe(true);
    expect(preview.layerOpacity).toEqual(DEFAULT_LAYER_OPACITY);
  });
});
