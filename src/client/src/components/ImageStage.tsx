import React from "react";
import type { ImageRect, MangaPage, TranslationBlock } from "../../../shared/types";
import type { ViewportSize } from "../lib/overlayLayout";
import { resolveCanvasPoint, resolveSelectionRect, type DrawPoint } from "../lib/inpaintLayerCanvas";
import { useImageStageView } from "../hooks/useImageStageView";
import type { InpaintTool } from "./InpaintLayerCanvas";
import { ImageStageLayers, type ImageStageActiveLayer, type ImageStageLayerOpacity, type ImageStageLayerVisibility } from "./ImageStageLayers";
import type { InpaintResultTool } from "./InpaintResultCanvas";

type RangeSelectionDragState = {
  pointerId: number;
  start: DrawPoint;
};

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
  onBlockTextUpdate: (block: TranslationBlock, translatedText: string) => void;
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
  onBlockPointerDown,
  onBlockTextUpdate
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
  const rangeSelectionDragRef = React.useRef<RangeSelectionDragState | null>(null);
  const [rangeSelectionPreviewRect, setRangeSelectionPreviewRect] = React.useState<ImageRect | null>(null);
  const rangeSelectionActive = rangeToolActive && !rangeSelectionDisabled && !temporaryPanActive;

  React.useEffect(() => {
    if (rangeSelectionActive) {
      return;
    }
    rangeSelectionDragRef.current = null;
    setRangeSelectionPreviewRect(null);
  }, [rangeSelectionActive]);

  const resolveRangeSelectionPoint = React.useCallback((event: React.PointerEvent<HTMLElement>): DrawPoint | null => {
    const stage = stageRef.current;
    if (!stage) {
      return null;
    }
    return resolveCanvasPoint(event.clientX, event.clientY, stage.getBoundingClientRect(), pageSize);
  }, [pageSize, stageRef]);

  const updateRangeSelectionPreview = React.useCallback((start: DrawPoint, current: DrawPoint) => {
    setRangeSelectionPreviewRect(resolveSelectionRect(start, current, pageSize));
  }, [pageSize]);

  const finishRangeSelection = React.useCallback((current: DrawPoint) => {
    const drag = rangeSelectionDragRef.current;
    if (!drag) {
      return;
    }
    const rect = resolveSelectionRect(drag.start, current, pageSize);
    rangeSelectionDragRef.current = null;
    setRangeSelectionPreviewRect(null);
    onInpaintSelectionChange(rect.width >= 2 && rect.height >= 2 ? rect : null);
  }, [onInpaintSelectionChange, pageSize]);

  return (
    <div
      ref={wrapRef}
      className={`stage-wrap${panning || temporaryPanActive ? " panning" : ""}`}
      onPointerMove={handleStagePointerMove}
      onPointerUp={handleStagePointerUp}
      onPointerCancel={handleStagePointerCancel}
      onPointerDown={handleStagePointerDown}
    >
      <div
        ref={stageRef}
        data-testid="image-stage"
        className={`image-stage${panning || temporaryPanActive ? " panning" : ""}`}
        style={stageStyle}
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
          rangeSelectionPreviewRect={rangeSelectionPreviewRect}
          inpaintTool={inpaintTool}
          layerOpacity={layerOpacity}
          layerVisibility={layerVisibility}
          page={page}
          pageSize={pageSize}
          rangeToolActive={rangeToolActive}
          selectedBlockId={selectedBlockId}
          stageSize={stageSize}
          temporaryPanActive={temporaryPanActive}
          onBlockPointerDown={onBlockPointerDown}
          onBlockTextUpdate={onBlockTextUpdate}
          onInpaintLayerChange={onInpaintLayerChange}
          onInpaintResultLayerChange={onInpaintResultLayerChange}
          onInpaintSelectionChange={onInpaintSelectionChange}
        />
      </div>
      {rangeSelectionActive ? (
        <div
          className="stage-range-hit-area"
          aria-label="범위 선택"
          onPointerDown={(event) => {
            if (event.button !== 0) {
              return;
            }
            const point = resolveRangeSelectionPoint(event);
            if (!point) {
              return;
            }
            event.preventDefault();
            event.stopPropagation();
            event.currentTarget.setPointerCapture(event.pointerId);
            rangeSelectionDragRef.current = { pointerId: event.pointerId, start: point };
            updateRangeSelectionPreview(point, point);
          }}
          onPointerMove={(event) => {
            const drag = rangeSelectionDragRef.current;
            if (!drag || drag.pointerId !== event.pointerId) {
              return;
            }
            const point = resolveRangeSelectionPoint(event);
            if (!point) {
              return;
            }
            event.preventDefault();
            event.stopPropagation();
            updateRangeSelectionPreview(drag.start, point);
          }}
          onPointerUp={(event) => {
            const drag = rangeSelectionDragRef.current;
            if (!drag || drag.pointerId !== event.pointerId) {
              return;
            }
            const point = resolveRangeSelectionPoint(event);
            event.preventDefault();
            event.stopPropagation();
            event.currentTarget.releasePointerCapture(event.pointerId);
            if (point) {
              finishRangeSelection(point);
            } else {
              rangeSelectionDragRef.current = null;
              setRangeSelectionPreviewRect(null);
            }
          }}
          onPointerCancel={(event) => {
            const drag = rangeSelectionDragRef.current;
            if (!drag || drag.pointerId !== event.pointerId) {
              return;
            }
            event.preventDefault();
            event.stopPropagation();
            event.currentTarget.releasePointerCapture(event.pointerId);
            rangeSelectionDragRef.current = null;
            setRangeSelectionPreviewRect(null);
          }}
        />
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
  );
}
