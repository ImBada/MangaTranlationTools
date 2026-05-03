import React from "react";
import type { ImageRect, MangaPage, TranslationBlock } from "../../../shared/types";
import type { ViewportSize } from "../lib/overlayLayout";
import { InpaintLayerCanvas, type InpaintTool } from "./InpaintLayerCanvas";
import { InpaintResultCanvas, type InpaintResultTool } from "./InpaintResultCanvas";
import { OverlayBlock } from "./OverlayBlock";
import { OverlayRenderCanvas } from "./OverlayRenderCanvas";

export type ImageStageActiveLayer = "output" | "image" | "inpaint" | "inpaintResult" | "inpaintMask" | "overlay";

export type ImageStageLayerVisibility = {
  image: boolean;
  inpaint: boolean;
  inpaintResult: boolean;
  inpaintMask: boolean;
  overlay: boolean;
};

export type ImageStageLayerOpacity = {
  image: number;
  inpaint: number;
  inpaintResult: number;
  inpaintMask: number;
  overlay: number;
};

type ImageStageLayersProps = {
  activeLayer: ImageStageActiveLayer;
  imageRef: React.RefObject<HTMLImageElement | null>;
  inpaintBrushSize: number;
  inpaintDisabled: boolean;
  inpaintResultBrushColor: string;
  inpaintResultBrushHardness: number;
  inpaintResultBrushSize: number;
  inpaintResultDisabled: boolean;
  inpaintResultTool: InpaintResultTool;
  inpaintResultToolStrength: number;
  inpaintSelectionRect: ImageRect | null;
  inpaintTool: InpaintTool;
  layerOpacity: ImageStageLayerOpacity;
  layerVisibility: ImageStageLayerVisibility;
  page: MangaPage;
  pageSize: ViewportSize;
  rangeSelectionDisabled: boolean;
  rangeToolActive: boolean;
  selectedBlockId: string | null;
  stageSize: ViewportSize | null;
  temporaryPanActive: boolean;
  onBlockPointerDown: (event: React.PointerEvent, block: TranslationBlock, mode: "move" | "resize" | "rotate") => void;
  onInpaintLayerChange: (dataUrl: string | undefined) => void;
  onInpaintResultLayerChange: (dataUrl: string | undefined) => void;
  onInpaintSelectionChange: (rect: ImageRect | null) => void;
};

export function ImageStageLayers({
  activeLayer,
  imageRef,
  inpaintBrushSize,
  inpaintDisabled,
  inpaintResultBrushColor,
  inpaintResultBrushHardness,
  inpaintResultBrushSize,
  inpaintResultDisabled,
  inpaintResultTool,
  inpaintResultToolStrength,
  inpaintSelectionRect,
  inpaintTool,
  layerOpacity,
  layerVisibility,
  page,
  pageSize,
  rangeSelectionDisabled,
  rangeToolActive,
  selectedBlockId,
  stageSize,
  temporaryPanActive,
  onBlockPointerDown,
  onInpaintLayerChange,
  onInpaintResultLayerChange,
  onInpaintSelectionChange
}: ImageStageLayersProps): React.JSX.Element {
  const inpaintMaskDataUrl = page.inpaintMaskDataUrl ?? page.inpaintLayerDataUrl;
  const resolvedStageSize = stageSize ?? pageSize;

  return (
    <>
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
                stageSize={resolvedStageSize}
                editingEnabled={activeLayer === "overlay" && !temporaryPanActive}
              />
              {page.blocks.map((block) => (
                <OverlayBlock
                  key={block.id}
                  block={block}
                  pageSize={pageSize}
                  stageSize={resolvedStageSize}
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
    </>
  );
}
