import React from "react";
import type { FontPreset, ImageRect, MangaPage, TranslationBlock } from "../../../shared/types";
import type { ViewportSize } from "../lib/overlayLayout";
import { resolveCanvasPoint, resolveSelectionRect, type DrawPoint } from "../lib/inpaintLayerCanvas";
import { isEditableTarget } from "../lib/editorUtils";
import { useImageStageView } from "../hooks/useImageStageView";
import type { InpaintLayerChangeOptions } from "../lib/inpaintLayerChange";
import type { InpaintTool } from "./InpaintLayerCanvas";
import { ImageStageLayers, type ImageStageActiveLayer, type ImageStageLayerOpacity, type ImageStageLayerVisibility } from "./ImageStageLayers";
import type { InpaintResultTool } from "./InpaintResultCanvas";

type RangeSelectionDragState = {
  pointerId: number;
  start: DrawPoint;
  target: "inpaint" | "block";
};

type ImageStageProps = {
  page: MangaPage;
  favoriteFontPresets: FontPreset[];
  imageRef: React.RefObject<HTMLCanvasElement | null>;
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
  blockRangeSelectionDisabled: boolean;
  temporaryPanActive: boolean;
  inpaintSelectionRect: ImageRect | null;
  onInpaintLayerChange: (dataUrl: string | undefined, options?: InpaintLayerChangeOptions) => void;
  onInpaintSelectionChange: (rect: ImageRect | null) => void;
  onInpaintResultLayerChange: (dataUrl: string | undefined, options?: InpaintLayerChangeOptions) => void;
  onZoomToolClick: (direction: "in" | "out") => void;
  onStagePointerMove: (event: React.PointerEvent) => void;
  onStagePointerUp: (event: React.PointerEvent) => void;
  onStagePointerDown: (event: React.PointerEvent) => void;
  onBlockPointerDown: (event: React.PointerEvent, block: TranslationBlock, mode: "move" | "resize" | "rotate") => void;
  onBlockFontSizeChange: (fontSizePx: number) => void;
  onBlockAutoFitDisable: () => void;
  onSelectedBlockRangeChange: (blockId: string, rect: ImageRect) => void;
  onBlockTextUpdate: (block: TranslationBlock, translatedText: string) => void;
  onBlockTextAlignChange: (textAlign: TranslationBlock["textAlign"]) => void;
  onFavoriteFontPresetSelect: (presetId: string) => void;
};

export function ImageStage({
  page,
  favoriteFontPresets,
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
  blockRangeSelectionDisabled,
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
  onBlockFontSizeChange,
  onBlockAutoFitDisable,
  onSelectedBlockRangeChange,
  onBlockTextUpdate,
  onBlockTextAlignChange,
  onFavoriteFontPresetSelect
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
    pageSize,
    temporaryPanActive,
    viewResetKey,
    viewScale,
    zoomToolActive
  });
  const rangeSelectionDragRef = React.useRef<RangeSelectionDragState | null>(null);
  const [rangeSelectionPreviewRect, setRangeSelectionPreviewRect] = React.useState<ImageRect | null>(null);
  const [blockRangeSelectionModeActive, setBlockRangeSelectionModeActive] = React.useState(false);
  const [blockRangeSelectionDragActive, setBlockRangeSelectionDragActive] = React.useState(false);
  const selectedBlock = React.useMemo(
    () => page.blocks.find((block) => block.id === selectedBlockId) ?? null,
    [page.blocks, selectedBlockId]
  );
  const inpaintRangeSelectionActive = rangeToolActive && !rangeSelectionDisabled && !temporaryPanActive;
  const blockRangeSelectionActive =
    activeLayer === "overlay" &&
    !zoomToolActive &&
    !rangeToolActive &&
    !blockRangeSelectionDisabled &&
    !temporaryPanActive &&
    layerVisibility.overlay &&
    Boolean(selectedBlock) &&
    (blockRangeSelectionModeActive || blockRangeSelectionDragActive);
  const rangeSelectionActive = inpaintRangeSelectionActive || blockRangeSelectionActive;

  React.useEffect(() => {
    if (rangeSelectionActive) {
      return;
    }
    rangeSelectionDragRef.current = null;
    setBlockRangeSelectionDragActive(false);
    setRangeSelectionPreviewRect(null);
  }, [rangeSelectionActive]);

  React.useEffect(() => {
    const updateBlockRangeSelectionMode = (event: KeyboardEvent) => {
      setBlockRangeSelectionModeActive(event.altKey && !isEditableTarget(event.target));
    };
    const clearBlockRangeSelectionMode = () => setBlockRangeSelectionModeActive(false);

    window.addEventListener("keydown", updateBlockRangeSelectionMode);
    window.addEventListener("keyup", updateBlockRangeSelectionMode);
    window.addEventListener("blur", clearBlockRangeSelectionMode);
    document.addEventListener("visibilitychange", clearBlockRangeSelectionMode);
    return () => {
      window.removeEventListener("keydown", updateBlockRangeSelectionMode);
      window.removeEventListener("keyup", updateBlockRangeSelectionMode);
      window.removeEventListener("blur", clearBlockRangeSelectionMode);
      document.removeEventListener("visibilitychange", clearBlockRangeSelectionMode);
    };
  }, []);

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
    setBlockRangeSelectionDragActive(false);
    setRangeSelectionPreviewRect(null);
    if (rect.width < 2 || rect.height < 2) {
      if (drag.target === "inpaint") {
        onInpaintSelectionChange(null);
      }
      return;
    }
    if (drag.target === "block" && selectedBlockId) {
      onSelectedBlockRangeChange(selectedBlockId, rect);
      return;
    }
    onInpaintSelectionChange(rect);
  }, [onInpaintSelectionChange, onSelectedBlockRangeChange, pageSize, selectedBlockId]);

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
          favoriteFontPresets={favoriteFontPresets}
          layerOpacity={layerOpacity}
          layerVisibility={layerVisibility}
          page={page}
          pageSize={pageSize}
          rangeToolActive={rangeToolActive}
          selectedBlockId={selectedBlockId}
          stageSize={stageSize}
          temporaryPanActive={temporaryPanActive}
          onBlockPointerDown={onBlockPointerDown}
          onBlockFontSizeChange={onBlockFontSizeChange}
          onBlockAutoFitDisable={onBlockAutoFitDisable}
          onBlockTextUpdate={onBlockTextUpdate}
          onBlockTextAlignChange={onBlockTextAlignChange}
          onFavoriteFontPresetSelect={onFavoriteFontPresetSelect}
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
            const target = blockRangeSelectionActive ? "block" : "inpaint";
            rangeSelectionDragRef.current = { pointerId: event.pointerId, start: point, target };
            setBlockRangeSelectionDragActive(target === "block");
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
              setBlockRangeSelectionDragActive(false);
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
            setBlockRangeSelectionDragActive(false);
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
