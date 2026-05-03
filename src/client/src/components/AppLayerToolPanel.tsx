import React from "react";
import { LayerToolPanel } from "./tools/LayerToolPanel";

type LayerToolPanelProps = React.ComponentProps<typeof LayerToolPanel>;

type AppLayerToolPanelProps = Omit<LayerToolPanelProps, "onClearInpaintMask" | "onClearInpaintResult" | "onInpaintSelectionClear"> & {
  onClearInpaintMaskData: () => void;
  onClearInpaintResultData: () => void;
  onClearInpaintSelectionRect: () => void;
};

export function AppLayerToolPanel({
  onClearInpaintMaskData,
  onClearInpaintResultData,
  onClearInpaintSelectionRect,
  ...props
}: AppLayerToolPanelProps): React.JSX.Element {
  return (
    <LayerToolPanel
      {...props}
      onClearInpaintMask={onClearInpaintMaskData}
      onClearInpaintResult={onClearInpaintResultData}
      onInpaintSelectionClear={onClearInpaintSelectionRect}
    />
  );
}
