import React from "react";
import type { ImageRect, MangaPage, TranslationBlock } from "../../../shared/types";
import type { ViewportSize } from "../lib/overlayLayout";
import { useImageStageView } from "../hooks/useImageStageView";
import { InpaintLayerCanvas, type InpaintTool } from "./InpaintLayerCanvas";
import { InpaintResultCanvas, type InpaintResultTool } from "./InpaintResultCanvas";
import { OverlayBlock } from "./OverlayBlock";
import { OverlayRenderCanvas } from "./OverlayRenderCanvas";

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
  layerVisibility: {
    image: boolean;
    inpaint: boolean;
    inpaintResult: boolean;
    inpaintMask: boolean;
    overlay: boolean;
  };
  layerOpacity: {
    image: number;
    inpaint: number;
    inpaintResult: number;
    inpaintMask: number;
    overlay: number;
  };
  activeLayer: "output" | "image" | "inpaint" | "inpaintResult" | "inpaintMask" | "overlay";
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
  const inpaintMaskDataUrl = page.inpaintMaskDataUrl ?? page.inpaintLayerDataUrl;
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
        className={`image-stage${panning || temporaryPanActive ? " panning" : ""}`}
        style={stageStyle}
        onPointerMove={handleStagePointerMove}
        onPointerUp={handleStagePointerUp}
        onPointerCancel={handleStagePointerCancel}
        onPointerDown={handleStagePointerDown}
      >
        <img
          ref={imageRef}
          className="page-image"
          src={page.dataUrl}
          alt={page.name}
          draggable={false}
          style={{
            visibility: layerVisibility.image ? "visible" : "hidden",
            opacity: layerOpacity.image
          }}
        />
        {layerVisibility.inpaint ? (
          <div className="inpaint-layer-preview" style={{ opacity: layerOpacity.inpaint }}>
            {layerVisibility.inpaintResult && (page.inpaintResultDataUrl || (activeLayer === "inpaintResult" && !temporaryPanActive)) ? (
              <InpaintResultCanvas
                className="inpaint-result-canvas"
                dataUrl={page.inpaintResultDataUrl}
                pageSize={pageSize}
                tool={inpaintResultTool}
                brushSize={inpaintResultBrushSize}
                brushColor={inpaintResultBrushColor}
                brushHardness={inpaintResultBrushHardness}
                toolStrength={inpaintResultToolStrength}
                disabled={inpaintResultDisabled || temporaryPanActive}
                selectionRect={null}
                onChange={onInpaintResultLayerChange}
                onSelectionChange={onInpaintSelectionChange}
                style={{
                  zIndex: activeLayer === "inpaintResult" ? 3 : 1,
                  opacity: layerOpacity.inpaintResult,
                  maskImage: (activeLayer !== "inpaintResult" || temporaryPanActive) && inpaintMaskDataUrl ? `url(${inpaintMaskDataUrl})` : undefined,
                  WebkitMaskImage: (activeLayer !== "inpaintResult" || temporaryPanActive) && inpaintMaskDataUrl ? `url(${inpaintMaskDataUrl})` : undefined
                }}
              />
            ) : null}
            {layerVisibility.inpaintMask ? (
              <div
                className="inpaint-mask-layer-preview"
                style={{
                  zIndex: activeLayer === "inpaintMask" ? 3 : 2,
                  opacity: layerOpacity.inpaintMask,
                  pointerEvents: activeLayer === "inpaintMask" && !temporaryPanActive ? "auto" : "none"
                }}
              >
                <InpaintLayerCanvas
                  dataUrl={inpaintMaskDataUrl}
                  pageSize={pageSize}
                  tool={inpaintTool}
                  brushSize={inpaintBrushSize}
                  disabled={inpaintDisabled || temporaryPanActive}
                  selectionRect={null}
                  onChange={onInpaintLayerChange}
                  onSelectionChange={onInpaintSelectionChange}
                />
              </div>
            ) : null}
          </div>
        ) : null}
        {layerVisibility.overlay
          ? (
              <div
                className="overlay-layer-preview"
                style={{
                  opacity: layerOpacity.overlay,
                  pointerEvents: activeLayer === "overlay" && !temporaryPanActive ? "auto" : "none"
                }}
              >
                <OverlayRenderCanvas
                  page={page}
                  stageSize={stageSize ?? pageSize}
                  editingEnabled={activeLayer === "overlay" && !temporaryPanActive}
                />
                {page.blocks.map((block) => (
                  <OverlayBlock
                    key={block.id}
                    block={block}
                    pageSize={pageSize}
                    stageSize={stageSize ?? pageSize}
                    selected={block.id === selectedBlockId}
                    editingEnabled={activeLayer === "overlay" && !temporaryPanActive}
                    visualContentVisible={false}
                    onPointerDown={(event) => onBlockPointerDown(event, block, "move")}
                    onResizePointerDown={(event) => onBlockPointerDown(event, block, "resize")}
                    onRotatePointerDown={(event) => onBlockPointerDown(event, block, "rotate")}
                  />
                ))}
              </div>
            )
          : null}
        {rangeToolActive || inpaintSelectionRect ? (
          <div className={`stage-range-selection-layer ${rangeToolActive ? "active" : ""}`}>
            <InpaintLayerCanvas
              pageSize={pageSize}
              tool="select"
              brushSize={1}
              disabled={rangeSelectionDisabled || temporaryPanActive || !rangeToolActive}
              selectionRect={inpaintSelectionRect}
              onChange={() => undefined}
              onSelectionChange={onInpaintSelectionChange}
            />
          </div>
        ) : null}
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
