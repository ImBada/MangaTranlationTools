import React from "react";
import type { ImageRect, MangaPage, TranslationBlock } from "../../../shared/types";
import type { ViewportSize } from "../lib/overlayLayout";
import { InpaintLayerCanvas, type InpaintTool } from "./InpaintLayerCanvas";
import { InpaintResultCanvas, type InpaintResultTool } from "./InpaintResultCanvas";
import { OverlayBlock } from "./OverlayBlock";
import { OverlayRenderCanvas } from "./OverlayRenderCanvas";

type ImageStageProps = {
  page: MangaPage;
  imageRef: React.RefObject<HTMLImageElement | null>;
  stageRef: React.RefObject<HTMLDivElement | null>;
  stageSize: ViewportSize | null;
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
  inpaintSelectionRect: ImageRect | null;
  onInpaintLayerChange: (dataUrl: string | undefined) => void;
  onInpaintSelectionChange: (rect: ImageRect | null) => void;
  onInpaintResultLayerChange: (dataUrl: string | undefined) => void;
  onStagePointerMove: (event: React.PointerEvent) => void;
  onStagePointerUp: (event: React.PointerEvent) => void;
  onStagePointerDown: () => void;
  onBlockPointerDown: (event: React.PointerEvent, block: TranslationBlock, mode: "move" | "resize" | "rotate") => void;
};

export function ImageStage({
  page,
  imageRef,
  stageRef,
  stageSize,
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
  inpaintSelectionRect,
  onInpaintLayerChange,
  onInpaintSelectionChange,
  onInpaintResultLayerChange,
  onStagePointerMove,
  onStagePointerUp,
  onStagePointerDown,
  onBlockPointerDown
}: ImageStageProps): React.JSX.Element {
  const inpaintMaskDataUrl = page.inpaintMaskDataUrl ?? page.inpaintLayerDataUrl;

  return (
    <div className="stage-wrap">
      <div
        ref={stageRef}
        className="image-stage"
        onPointerMove={onStagePointerMove}
        onPointerUp={onStagePointerUp}
        onPointerCancel={onStagePointerUp}
        onPointerDown={onStagePointerDown}
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
            {layerVisibility.inpaintResult && (page.inpaintResultDataUrl || activeLayer === "inpaintResult") ? (
              <InpaintResultCanvas
                className="inpaint-result-canvas"
                dataUrl={page.inpaintResultDataUrl}
                pageSize={{ width: page.width, height: page.height }}
                tool={inpaintResultTool}
                brushSize={inpaintResultBrushSize}
                brushColor={inpaintResultBrushColor}
                brushHardness={inpaintResultBrushHardness}
                toolStrength={inpaintResultToolStrength}
                disabled={inpaintResultDisabled}
                selectionRect={inpaintSelectionRect}
                onChange={onInpaintResultLayerChange}
                onSelectionChange={onInpaintSelectionChange}
                style={{
                  zIndex: activeLayer === "inpaintResult" ? 3 : 1,
                  opacity: layerOpacity.inpaintResult,
                  maskImage: activeLayer !== "inpaintResult" && inpaintMaskDataUrl ? `url(${inpaintMaskDataUrl})` : undefined,
                  WebkitMaskImage: activeLayer !== "inpaintResult" && inpaintMaskDataUrl ? `url(${inpaintMaskDataUrl})` : undefined
                }}
              />
            ) : null}
            {layerVisibility.inpaintMask ? (
              <div
                className="inpaint-mask-layer-preview"
                style={{
                  zIndex: activeLayer === "inpaintMask" ? 3 : 2,
                  opacity: layerOpacity.inpaintMask,
                  pointerEvents: activeLayer === "inpaintMask" ? "auto" : "none"
                }}
              >
                <InpaintLayerCanvas
                  dataUrl={inpaintMaskDataUrl}
                  pageSize={{ width: page.width, height: page.height }}
                  tool={inpaintTool}
                  brushSize={inpaintBrushSize}
                  disabled={inpaintDisabled}
                  selectionRect={inpaintSelectionRect}
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
                  pointerEvents: activeLayer === "overlay" ? "auto" : "none"
                }}
              >
                <OverlayRenderCanvas
                  page={page}
                  stageSize={stageSize ?? { width: page.width, height: page.height }}
                  editingEnabled={activeLayer === "overlay"}
                />
                {page.blocks.map((block) => (
                  <OverlayBlock
                    key={block.id}
                    block={block}
                    pageSize={{ width: page.width, height: page.height }}
                    stageSize={stageSize ?? { width: page.width, height: page.height }}
                    selected={block.id === selectedBlockId}
                    editingEnabled={activeLayer === "overlay"}
                    visualContentVisible={false}
                    onPointerDown={(event) => onBlockPointerDown(event, block, "move")}
                    onResizePointerDown={(event) => onBlockPointerDown(event, block, "resize")}
                    onRotatePointerDown={(event) => onBlockPointerDown(event, block, "rotate")}
                  />
                ))}
              </div>
            )
          : null}
      </div>
    </div>
  );
}
