import React from "react";
import type { ActiveLayer, LayerOpacity, LayerVisibility } from "../../lib/layerState";
import { LayerControl } from "./LayerControl";

type LayerPanelProps = {
  activeLayer: ActiveLayer;
  focusModeEnabled: boolean;
  layerOpacity: LayerOpacity;
  layerVisibility: LayerVisibility;
  overlayBackgroundOpacity: number;
  overlayOpacityEditMode: boolean;
  onFocusModeChange: (enabled: boolean) => void;
  onLayerOpacityChange: React.Dispatch<React.SetStateAction<LayerOpacity>>;
  onLayerVisibilityChange: React.Dispatch<React.SetStateAction<LayerVisibility>>;
  onOverlayBlockOpacityChange: (opacity: number) => void;
  onOverlayOpacityEditModeChange: (enabled: boolean) => void;
  onSelectLayer: (layer: ActiveLayer) => void;
};

export function LayerPanel({
  activeLayer,
  focusModeEnabled,
  layerOpacity,
  layerVisibility,
  overlayBackgroundOpacity,
  overlayOpacityEditMode,
  onFocusModeChange,
  onLayerOpacityChange,
  onLayerVisibilityChange,
  onOverlayBlockOpacityChange,
  onOverlayOpacityEditModeChange,
  onSelectLayer
}: LayerPanelProps): React.JSX.Element {
  return (
    <section className="layer-panel right-rail-layer-panel">
      <div className="layer-panel-header">
        <h2>레이어</h2>
        <label className="focus-mode-toggle">
          <span>FOCUS MODE</span>
          <input
            type="checkbox"
            checked={focusModeEnabled}
            onChange={(event) => onFocusModeChange(event.target.checked)}
          />
          <span className="focus-mode-switch" aria-hidden="true" />
        </label>
      </div>
      <LayerControl
        label="1 최종 아웃풋"
        active={activeLayer === "output"}
        visible={true}
        opacity={1}
        onSelect={() => onSelectLayer("output")}
        onVisibleChange={() => undefined}
        onOpacityChange={() => undefined}
        viewOnly
      />
      <LayerControl
        label="2 번역 블록"
        active={activeLayer === "overlay"}
        visible={layerVisibility.overlay}
        opacity={overlayOpacityEditMode ? overlayBackgroundOpacity : layerOpacity.overlay}
        opacityEditMode={overlayOpacityEditMode}
        opacityEditModeLabel="배경 투명도 편집"
        onSelect={() => onSelectLayer("overlay")}
        onVisibleChange={(visible) => onLayerVisibilityChange((current) => ({ ...current, overlay: visible }))}
        onOpacityEditModeChange={(enabled) => {
          onOverlayOpacityEditModeChange(enabled);
          if (enabled) {
            onLayerOpacityChange((current) => ({ ...current, overlay: 1 }));
          }
        }}
        onOpacityChange={(opacity) => {
          if (overlayOpacityEditMode) {
            onOverlayBlockOpacityChange(opacity);
            return;
          }
          onLayerOpacityChange((current) => ({ ...current, overlay: opacity }));
        }}
      />
      <LayerControl
        label="인페인트 레이어"
        active={activeLayer === "inpaint" || activeLayer === "inpaintResult" || activeLayer === "inpaintMask"}
        visible={layerVisibility.inpaint}
        opacity={layerOpacity.inpaint}
        onSelect={() => onSelectLayer("inpaint")}
        onVisibleChange={(visible) =>
          onLayerVisibilityChange((current) => ({
            ...current,
            inpaint: visible,
            inpaintResult: visible ? current.inpaintResult : false,
            inpaintMask: visible ? current.inpaintMask : false
          }))
        }
        onOpacityChange={(opacity) => onLayerOpacityChange((current) => ({ ...current, inpaint: opacity }))}
      />
      <div className="layer-subgroup">
        <LayerControl
          label="3 인페인트 결과"
          active={activeLayer === "inpaintResult"}
          visible={layerVisibility.inpaint && layerVisibility.inpaintResult}
          opacity={layerOpacity.inpaintResult}
          onSelect={() => onSelectLayer("inpaintResult")}
          onVisibleChange={(visible) => onLayerVisibilityChange((current) => ({ ...current, inpaint: current.inpaint || visible, inpaintResult: visible }))}
          onOpacityChange={(opacity) => onLayerOpacityChange((current) => ({ ...current, inpaintResult: opacity }))}
          nested
        />
        <LayerControl
          label="4 인페인트 마스크"
          active={activeLayer === "inpaintMask"}
          visible={layerVisibility.inpaint && layerVisibility.inpaintMask}
          opacity={layerOpacity.inpaintMask}
          onSelect={() => onSelectLayer("inpaintMask")}
          onVisibleChange={(visible) => onLayerVisibilityChange((current) => ({ ...current, inpaint: current.inpaint || visible, inpaintMask: visible }))}
          onOpacityChange={(opacity) => onLayerOpacityChange((current) => ({ ...current, inpaintMask: opacity }))}
          nested
        />
      </div>
      <LayerControl
        label="5 원본 이미지"
        active={activeLayer === "image"}
        visible={layerVisibility.image}
        opacity={layerOpacity.image}
        onSelect={() => onSelectLayer("image")}
        onVisibleChange={(visible) => onLayerVisibilityChange((current) => ({ ...current, image: visible }))}
        onOpacityChange={(opacity) => onLayerOpacityChange((current) => ({ ...current, image: opacity }))}
      />
    </section>
  );
}
