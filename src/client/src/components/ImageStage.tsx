import React from "react";
import type { ImageRect, MangaPage, TranslationBlock } from "../../../shared/types";
import type { ViewportSize } from "../lib/overlayLayout";
import { useImageStageView } from "../hooks/useImageStageView";
import type { InpaintTool } from "./InpaintLayerCanvas";
import { ImageStageLayers, type ImageStageActiveLayer, type ImageStageLayerOpacity, type ImageStageLayerVisibility } from "./ImageStageLayers";
import type { InpaintResultTool } from "./InpaintResultCanvas";

type ImageStageProps = {
  page: MangaPage;
  imageRef: React.RefObject<HTMLImageElement | null>;
  stageRef: React.RefObject<HTMLDivElement | null>;
  stageSize: ViewportSize | null;
  viewScale: number | null;
  viewResetKey: number;
  zoomToolActive: boolean;
  rangeToolActive: boolean;
  selectedBlockId: string | null;
  layerVisibility: ImageStageLayerVisibility;
  layerOpacity: ImageStageLayerOpacity;
  activeLayer: ImageStageActiveLayer;
  inpaintTool: InpaintTool;
  inpaintBrushSize: number;
  inpaintResultTool: InpaintResultTool;
  inpaintResultBrushSize: number;
  inpaintResultBrushColor: string;
  inpaintResultBrushHardness: number;
  inpaintResultToolStrength: number;
  inpaintDisabled: boolean;
  inpaintResultDisabled: boolean;
  rangeSelectionDisabled: boolean;
  temporaryPanActive: boolean;
  inpaintSelectionRect: ImageRect | null;
  onInpaintLayerChange: (dataUrl: string | undefined) => void;
  onInpaintSelectionChange: (rect: ImageRect | null) => void;
  onInpaintResultLayerChange: (dataUrl: string | undefined) => void;
  onZoomToolClick: (direction: "in" | "out") => void;
  onStagePointerMove: (event: React.PointerEvent) => void;
  onStagePointerUp: (event: React.PointerEvent) => void;
  onStagePointerDown: (event: React.PointerEvent) => void;
  onBlockPointerDown: (event: React.PointerEvent, block: TranslationBlock, mode: "move" | "resize" | "rotate") => void;
};

export function ImageStage({
  page,
  imageRef,
  stageRef,
  stageSize,
  viewScale,
  viewResetKey,
  zoomToolActive,
  rangeToolActive,
  selectedBlockId,
  layerVisibility,
  layerOpacity,
  activeLayer,
  inpaintTool,
  inpaintBrushSize,
  inpaintResultTool,
  inpaintResultBrushSize,
  inpaintResultBrushColor,
  inpaintResultBrushHardness,
  inpaintResultToolStrength,
  inpaintDisabled,
  inpaintResultDisabled,
  rangeSelectionDisabled,
  temporaryPanActive,
  inpaintSelectionRect,
  onInpaintLayerChange,
  onInpaintSelectionChange,
  onInpaintResultLayerChange,
  onZoomToolClick,
  onStagePointerMove,
  onStagePointerUp,
  onStagePointerDown,
  onBlockPointerDown
}: ImageStageProps): React.JSX.Element {
  const pageSize = React.useMemo(() => ({ width: page.width, height: page.height }), [page.height, page.width]);
  const {
    clearZoomCursor,
    handleStagePointerCancel,
    handleStagePointerDown,
    handleStagePointerMove,
    handleStagePointerUp,
    panning,
    stageStyle,
    updateZoomCursor,
    wrapRef,
    zoomCursor
  } = useImageStageView({
    imageRef,
    onStagePointerDown,
    onStagePointerMove,
    onStagePointerUp,
    pageId: page.id,
    pageSize,
    temporaryPanActive,
    viewResetKey,
    viewScale,
    zoomToolActive
  });

  return (
    <div ref={wrapRef} className="stage-wrap">
      <div
        ref={stageRef}
        data-testid="image-stage"
        className={`image-stage${panning || temporaryPanActive ? " panning" : ""}`}
        style={stageStyle}
        onPointerMove={handleStagePointerMove}
        onPointerUp={handleStagePointerUp}
        onPointerCancel={handleStagePointerCancel}
        onPointerDown={handleStagePointerDown}
      >
        <ImageStageLayers
          activeLayer={activeLayer}
          imageRef={imageRef}
          inpaintBrushSize={inpaintBrushSize}
          inpaintDisabled={inpaintDisabled}
          inpaintResultBrushColor={inpaintResultBrushColor}
          inpaintResultBrushHardness={inpaintResultBrushHardness}
          inpaintResultBrushSize={inpaintResultBrushSize}
          inpaintResultDisabled={inpaintResultDisabled}
          inpaintResultTool={inpaintResultTool}
          inpaintResultToolStrength={inpaintResultToolStrength}
          inpaintSelectionRect={inpaintSelectionRect}
          inpaintTool={inpaintTool}
          layerOpacity={layerOpacity}
          layerVisibility={layerVisibility}
          page={page}
          pageSize={pageSize}
          rangeSelectionDisabled={rangeSelectionDisabled}
          rangeToolActive={rangeToolActive}
          selectedBlockId={selectedBlockId}
          stageSize={stageSize}
          temporaryPanActive={temporaryPanActive}
          onBlockPointerDown={onBlockPointerDown}
          onInpaintLayerChange={onInpaintLayerChange}
          onInpaintResultLayerChange={onInpaintResultLayerChange}
          onInpaintSelectionChange={onInpaintSelectionChange}
        />
        {zoomToolActive && !temporaryPanActive ? (
          <div
            className="stage-zoom-hit-area"
            aria-label="줌 도구"
            onPointerEnter={updateZoomCursor}
            onPointerMove={updateZoomCursor}
            onPointerLeave={clearZoomCursor}
            onPointerDown={(event) => {
              if (event.button !== 0) {
                return;
              }
              event.preventDefault();
              event.stopPropagation();
              updateZoomCursor(event);
              onZoomToolClick(event.altKey ? "out" : "in");
            }}
            onPointerUp={(event) => {
              event.preventDefault();
              event.stopPropagation();
            }}
            onPointerCancel={(event) => {
              event.preventDefault();
              event.stopPropagation();
            }}
          >
            {zoomCursor ? (
              <div
                className={`stage-zoom-cursor ${zoomCursor.altKey ? "zoom-out" : "zoom-in"}`}
                style={{
                  left: `${zoomCursor.x}px`,
                  top: `${zoomCursor.y}px`
                }}
                aria-hidden="true"
              >
                <span className="stage-zoom-cursor-mark" />
              </div>
            ) : null}
          </div>
        ) : null}
      </div>
    </div>
  );
}
