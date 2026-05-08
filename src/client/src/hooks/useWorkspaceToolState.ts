import React from "react";
import type { InpaintTool } from "../components/InpaintLayerCanvas";
import type { InpaintResultTool } from "../components/InpaintResultCanvas";
import {
  DEFAULT_LAYER_OPACITY,
  LAYER_FOCUS_OPACITY,
  type ActiveLayer,
  type LayerOpacity,
  type LayerVisibility
} from "../lib/layerState";

type InpaintToolSetters = {
  setInpaintResultTool: React.Dispatch<React.SetStateAction<InpaintResultTool>>;
  setInpaintTool: React.Dispatch<React.SetStateAction<InpaintTool>>;
};

type UseWorkspaceToolStateState = {
  activeLayer: ActiveLayer;
  focusModeEnabled: boolean;
  layerOpacity: LayerOpacity;
  layerToolActive: boolean;
  layerVisibility: LayerVisibility;
  overlayOpacityEditMode: boolean;
  rangeToolActive: boolean;
  registerInpaintToolSetters: (setters: InpaintToolSetters) => void;
  selectInpaintResultEditTool: (tool: Exclude<InpaintResultTool, "select">) => void;
  selectLayer: (nextLayer: ActiveLayer) => void;
  selectPointerTool: () => void;
  selectRangeTool: () => void;
  selectSharedInpaintTool: (tool: InpaintTool) => void;
  selectZoomTool: () => void;
  setFocusModeEnabled: React.Dispatch<React.SetStateAction<boolean>>;
  setLayerOpacity: React.Dispatch<React.SetStateAction<LayerOpacity>>;
  setLayerVisibility: React.Dispatch<React.SetStateAction<LayerVisibility>>;
  setOverlayOpacityEditMode: React.Dispatch<React.SetStateAction<boolean>>;
  setRangeToolActive: React.Dispatch<React.SetStateAction<boolean>>;
  setTemporaryPanActive: React.Dispatch<React.SetStateAction<boolean>>;
  setZoomToolActive: React.Dispatch<React.SetStateAction<boolean>>;
  showInpaintLayers: () => void;
  showOverlayLayer: () => void;
  stageLayerOpacity: LayerOpacity;
  stageLayerVisibility: LayerVisibility;
  temporaryPanActive: boolean;
  temporaryPanHeldRef: React.MutableRefObject<boolean>;
  temporaryPanShortcutEnabled: boolean;
  zoomToolActive: boolean;
};

export function useWorkspaceToolState(): UseWorkspaceToolStateState {
  const [zoomToolActive, setZoomToolActive] = React.useState(false);
  const [rangeToolActive, setRangeToolActive] = React.useState(false);
  const [layerVisibility, setLayerVisibility] = React.useState<LayerVisibility>({
    image: true,
    inpaint: true,
    inpaintResult: true,
    inpaintMask: true,
    overlay: true
  });
  const [layerOpacity, setLayerOpacity] = React.useState<LayerOpacity>(DEFAULT_LAYER_OPACITY);
  const [overlayOpacityEditMode, setOverlayOpacityEditMode] = React.useState(false);
  const [focusModeEnabled, setFocusModeEnabled] = React.useState(true);
  const [activeLayer, setActiveLayer] = React.useState<ActiveLayer>("output");
  const [temporaryPanActive, setTemporaryPanActive] = React.useState(false);
  const temporaryPanHeldRef = React.useRef(false);
  const inpaintToolSettersRef = React.useRef<InpaintToolSetters | null>(null);

  const registerInpaintToolSetters = React.useCallback((setters: InpaintToolSetters) => {
    inpaintToolSettersRef.current = setters;
  }, []);

  const showOverlayLayer = React.useCallback(() => {
    setLayerVisibility((current) => ({ ...current, overlay: true }));
  }, []);

  const showInpaintLayers = React.useCallback(() => {
    setLayerVisibility((current) => ({ ...current, inpaint: true, inpaintResult: true, inpaintMask: true }));
  }, []);

  const selectLayer = React.useCallback((nextLayer: ActiveLayer) => {
    setActiveLayer(nextLayer);
    if (!focusModeEnabled) {
      return;
    }
    setLayerOpacity((current) => ({
      ...current,
      ...LAYER_FOCUS_OPACITY[nextLayer]
    }));
  }, [focusModeEnabled]);

  const layerToolActive =
    activeLayer === "image" ||
    activeLayer === "overlay" ||
    activeLayer === "inpaint" ||
    activeLayer === "inpaintMask" ||
    activeLayer === "inpaintResult";
  const temporaryPanShortcutEnabled = layerToolActive || zoomToolActive;

  const stageLayerOpacity = React.useMemo(
    () => temporaryPanActive
      ? DEFAULT_LAYER_OPACITY
      : { ...layerOpacity, overlay: overlayOpacityEditMode ? 1 : layerOpacity.overlay },
    [layerOpacity, overlayOpacityEditMode, temporaryPanActive]
  );

  const stageLayerVisibility = React.useMemo(
    () => temporaryPanActive
      ? { image: true, inpaint: true, inpaintResult: true, inpaintMask: true, overlay: true }
      : layerVisibility,
    [layerVisibility, temporaryPanActive]
  );

  const selectSharedInpaintTool = React.useCallback((tool: InpaintTool) => {
    setZoomToolActive(false);
    setRangeToolActive(tool === "select");
    inpaintToolSettersRef.current?.setInpaintTool(tool);
    if (tool !== "autoEraser") {
      inpaintToolSettersRef.current?.setInpaintResultTool(tool);
    }
  }, []);

  const selectPointerTool = React.useCallback(() => {
    setZoomToolActive(false);
    setRangeToolActive(false);
    inpaintToolSettersRef.current?.setInpaintTool("select");
    inpaintToolSettersRef.current?.setInpaintResultTool("select");
  }, []);

  const selectRangeTool = React.useCallback(() => {
    setZoomToolActive(false);
    setRangeToolActive(true);
    inpaintToolSettersRef.current?.setInpaintTool("select");
    inpaintToolSettersRef.current?.setInpaintResultTool("select");
  }, []);

  const selectZoomTool = React.useCallback(() => {
    setRangeToolActive(false);
    setZoomToolActive(true);
  }, []);

  const selectInpaintResultEditTool = React.useCallback((tool: Exclude<InpaintResultTool, "select">) => {
    setZoomToolActive(false);
    setRangeToolActive(false);
    inpaintToolSettersRef.current?.setInpaintResultTool(tool);
  }, []);

  return {
    activeLayer,
    focusModeEnabled,
    layerOpacity,
    layerToolActive,
    layerVisibility,
    overlayOpacityEditMode,
    rangeToolActive,
    registerInpaintToolSetters,
    selectInpaintResultEditTool,
    selectLayer,
    selectPointerTool,
    selectRangeTool,
    selectSharedInpaintTool,
    selectZoomTool,
    setFocusModeEnabled,
    setLayerOpacity,
    setLayerVisibility,
    setOverlayOpacityEditMode,
    setRangeToolActive,
    setTemporaryPanActive,
    setZoomToolActive,
    showInpaintLayers,
    showOverlayLayer,
    stageLayerOpacity,
    stageLayerVisibility,
    temporaryPanActive,
    temporaryPanHeldRef,
    temporaryPanShortcutEnabled,
    zoomToolActive
  };
}
