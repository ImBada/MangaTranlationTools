import React from "react";
import type { FontPreset, ImageRect, MangaPage, TranslationBlock } from "../../../shared/types";
import type { FontWeightAvailability, ViewportSize } from "../lib/overlayLayout";
import { resolveToggledTranslationBlockIds, resolveTranslationBlockIdsInSelection } from "../lib/blockSelection";
import { resolveCanvasPoint, resolveSelectionRect, type DrawPoint } from "../lib/inpaintLayerCanvas";
import { isEditableTarget } from "../lib/editorUtils";
import { useImageStageView, type StageZoomPanAnchor } from "../hooks/useImageStageView";
import type { InpaintLayerChangeOptions } from "../lib/inpaintLayerChange";
import { resolveStageDragZoomScale } from "../lib/stageFit";
import { InpaintBrushCursorOverlay } from "./InpaintBrushCursorOverlay";
import type { InpaintTool } from "./InpaintLayerCanvas";
import { ImageStageLayers, type ImageStageActiveLayer, type ImageStageLayerOpacity, type ImageStageLayerVisibility } from "./ImageStageLayers";
import type { InpaintResultTool } from "./InpaintResultCanvas";

type RangeSelectionDragState = {
  pointerId: number;
  start: DrawPoint;
  target: "inpaint" | "block" | "blocks";
  toggleBlockSelection: boolean;
};

type ZoomToolDragState = {
  panAnchor: StageZoomPanAnchor | null;
  pointerId: number;
  startScale: number;
  startX: number;
};

type ImageStageProps = {
  page: MangaPage;
  favoriteFontPresets: FontPreset[];
  fontWeightAvailability: readonly FontWeightAvailability[];
  imageRef: React.RefObject<HTMLCanvasElement | null>;
  stageRef: React.RefObject<HTMLDivElement | null>;
  stageSize: ViewportSize | null;
  viewScale: number | null;
  viewResetKey: number;
  zoomToolActive: boolean;
  rangeToolActive: boolean;
  selectedBlockId: string | null;
  selectedBlockIds: string[];
  layerVisibility: ImageStageLayerVisibility;
  layerOpacity: ImageStageLayerOpacity;
  activeLayer: ImageStageActiveLayer;
  finalOutputPreviewActive: boolean;
  inpaintResultComposite: boolean;
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
  activeBlockDragId: string | null;
  onInpaintLayerChange: (dataUrl: string | undefined, options?: InpaintLayerChangeOptions) => void;
  onInpaintLayerEditEnd: () => void;
  onInpaintLayerEditStart: () => void;
  onInpaintResultColorPick: (color: string) => void;
  onInpaintSelectionChange: (rect: ImageRect | null) => void;
  onInpaintResultLayerChange: (dataUrl: string | undefined, options?: InpaintLayerChangeOptions) => void;
  onZoomToolDrag: (scale: number) => void;
  onStagePointerMove: (event: React.PointerEvent) => void;
  onStagePointerUp: (event: React.PointerEvent) => void;
  onStagePointerDown: (event: React.PointerEvent) => void;
  onBlockSelectionChange: (blockIds: string[]) => void;
  onBlockPointerDown: (event: React.PointerEvent, block: TranslationBlock, mode: "move" | "resize" | "rotate") => void;
  onBlockFontStyleCopy: () => void | Promise<void>;
  onBlockFontSizeChange: (fontSizePx: number) => void;
  onBlockAutoFitDisable: () => void;
  onSelectedBlockRangeChange: (blockId: string, rect: ImageRect) => void;
  onBlockTextUpdate: (block: TranslationBlock, translatedText: string) => void;
  onBlockTextSelectionSplitDuplicate: (
    block: TranslationBlock,
    translatedText: string,
    selectionStart: number,
    selectionEnd: number
  ) => boolean;
  onBlockTextAlignChange: (textAlign: TranslationBlock["textAlign"]) => void;
  onBlockInlineEditActiveChange: (active: boolean) => void;
  onFavoriteFontPresetSelect: (presetId: string) => void;
};

export function ImageStage({
  page,
  favoriteFontPresets,
  fontWeightAvailability,
  imageRef,
  stageRef,
  stageSize,
  viewScale,
  viewResetKey,
  zoomToolActive,
  rangeToolActive,
  selectedBlockId,
  selectedBlockIds,
  layerVisibility,
  layerOpacity,
  activeLayer,
  finalOutputPreviewActive,
  inpaintResultComposite,
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
  activeBlockDragId,
  onInpaintLayerChange,
  onInpaintLayerEditEnd,
  onInpaintLayerEditStart,
  onInpaintResultColorPick,
  onInpaintSelectionChange,
  onInpaintResultLayerChange,
  onZoomToolDrag,
  onStagePointerMove,
  onStagePointerUp,
  onStagePointerDown,
  onBlockSelectionChange,
  onBlockPointerDown,
  onBlockFontStyleCopy,
  onBlockFontSizeChange,
  onBlockAutoFitDisable,
  onSelectedBlockRangeChange,
  onBlockTextUpdate,
  onBlockTextSelectionSplitDuplicate,
  onBlockTextAlignChange,
  onBlockInlineEditActiveChange,
  onFavoriteFontPresetSelect
}: ImageStageProps): React.JSX.Element {
  const pageSize = React.useMemo(() => ({ width: page.width, height: page.height }), [page.height, page.width]);
  const rangeSelectionDragRef = React.useRef<RangeSelectionDragState | null>(null);
  const zoomToolDragRef = React.useRef<ZoomToolDragState | null>(null);
  const [rangeSelectionPreviewRect, setRangeSelectionPreviewRect] = React.useState<ImageRect | null>(null);
  const [blockRangeSelectionModeActive, setBlockRangeSelectionModeActive] = React.useState(false);
  const [blockRangeSelectionDragActive, setBlockRangeSelectionDragActive] = React.useState(false);
  const [inpaintStrokeActive, setInpaintStrokeActive] = React.useState(false);
  const selectedBlock = React.useMemo(
    () => page.blocks.find((block) => block.id === selectedBlockId) ?? null,
    [page.blocks, selectedBlockId]
  );
  const inpaintRangeSelectionActive = activeLayer !== "overlay" && rangeToolActive && !rangeSelectionDisabled && !temporaryPanActive;
  const blockMultiSelectionActive =
    activeLayer === "overlay" &&
    rangeToolActive &&
    !blockRangeSelectionDisabled &&
    !temporaryPanActive &&
    layerVisibility.overlay;
  const blockRangeSelectionActive =
    activeLayer === "overlay" &&
    !zoomToolActive &&
    !rangeToolActive &&
    !blockRangeSelectionDisabled &&
    !temporaryPanActive &&
    layerVisibility.overlay &&
    Boolean(selectedBlock) &&
    (blockRangeSelectionModeActive || blockRangeSelectionDragActive);
  const rangeSelectionActive = inpaintRangeSelectionActive || blockMultiSelectionActive || blockRangeSelectionActive;
  const inpaintBrushInputActive = !temporaryPanActive || inpaintStrokeActive;
  const activeInpaintBrushCursorTool = React.useMemo(() => {
    if (
      activeLayer === "inpaintMask" &&
      !inpaintDisabled &&
      inpaintTool !== "select" &&
      inpaintBrushInputActive &&
      !rangeSelectionActive &&
      !zoomToolActive
    ) {
      return {
        brushSize: inpaintBrushSize,
        tool: inpaintTool
      };
    }
    if (
      activeLayer === "inpaintResult" &&
      !inpaintResultDisabled &&
      inpaintResultTool !== "select" &&
      inpaintResultTool !== "colorPicker" &&
      inpaintBrushInputActive &&
      !rangeSelectionActive &&
      !zoomToolActive
    ) {
      return {
        brushSize: inpaintResultBrushSize,
        tool: inpaintResultTool
      };
    }
    return null;
  }, [
    activeLayer,
    inpaintBrushSize,
    inpaintBrushInputActive,
    inpaintDisabled,
    inpaintResultBrushSize,
    inpaintResultDisabled,
    inpaintResultTool,
    inpaintTool,
    rangeSelectionActive,
    zoomToolActive
  ]);
  const {
    applyStageZoomAnchor,
    beginStageZoomAnchor,
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
    stagePanDisabled: Boolean(activeInpaintBrushCursorTool),
    temporaryPanActive,
    viewResetKey,
    viewScale,
    zoomToolActive
  });
  const temporaryPanCursorActive = temporaryPanActive && !activeInpaintBrushCursorTool;

  const handleInpaintLayerEditStart = React.useCallback(() => {
    setInpaintStrokeActive(true);
    onInpaintLayerEditStart();
  }, [onInpaintLayerEditStart]);

  const handleInpaintLayerEditEnd = React.useCallback(() => {
    setInpaintStrokeActive(false);
    onInpaintLayerEditEnd();
  }, [onInpaintLayerEditEnd]);

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

  const resolveCurrentZoomScale = React.useCallback(() => {
    const stage = stageRef.current;
    const rect = stage?.getBoundingClientRect();
    if (rect?.width) {
      return rect.width / Math.max(1, page.width);
    }
    if (stageSize?.width) {
      return stageSize.width / Math.max(1, page.width);
    }
    return viewScale ?? 1;
  }, [page.width, stageRef, stageSize?.width, viewScale]);

  const updateZoomCursorFromDrag = React.useCallback((event: React.PointerEvent<HTMLElement>, drag: ZoomToolDragState | null) => {
    updateZoomCursor(event, drag ? event.clientX < drag.startX : false);
  }, [updateZoomCursor]);

  const clearZoomCursorIfOutside = React.useCallback((event: React.PointerEvent<HTMLElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    if (
      event.clientX < rect.left ||
      event.clientX > rect.right ||
      event.clientY < rect.top ||
      event.clientY > rect.bottom
    ) {
      clearZoomCursor();
    } else {
      updateZoomCursorFromDrag(event, null);
    }
  }, [clearZoomCursor, updateZoomCursorFromDrag]);

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
      } else if (drag.target === "blocks" && !drag.toggleBlockSelection) {
        onBlockSelectionChange([]);
      }
      return;
    }
    if (drag.target === "blocks") {
      const blockIds = resolveTranslationBlockIdsInSelection(page, rect);
      onBlockSelectionChange(
        drag.toggleBlockSelection
          ? resolveToggledTranslationBlockIds(selectedBlockId, selectedBlockIds, blockIds)
          : blockIds
      );
      return;
    }
    if (drag.target === "block" && selectedBlockId) {
      onSelectedBlockRangeChange(selectedBlockId, rect);
      return;
    }
    onInpaintSelectionChange(rect);
  }, [
    onBlockSelectionChange,
    onInpaintSelectionChange,
    onSelectedBlockRangeChange,
    page,
    pageSize,
    selectedBlockId,
    selectedBlockIds
  ]);

  return (
    <div
      ref={wrapRef}
      className={`stage-wrap${panning || temporaryPanCursorActive ? " panning" : ""}${activeInpaintBrushCursorTool ? " inpaint-cursor-active" : ""}`}
      onPointerMove={handleStagePointerMove}
      onPointerUp={handleStagePointerUp}
      onPointerCancel={handleStagePointerCancel}
      onPointerDown={handleStagePointerDown}
    >
      <div
        ref={stageRef}
        data-testid="image-stage"
        className={`image-stage${panning || temporaryPanCursorActive ? " panning" : ""}`}
        style={stageStyle}
      >
        <ImageStageLayers
          activeLayer={activeLayer}
          finalOutputPreviewActive={finalOutputPreviewActive}
          inpaintResultComposite={inpaintResultComposite}
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
          inpaintStrokeActive={inpaintStrokeActive}
          favoriteFontPresets={favoriteFontPresets}
          fontWeightAvailability={fontWeightAvailability}
          layerOpacity={layerOpacity}
          layerVisibility={layerVisibility}
          page={page}
          pageSize={pageSize}
          rangeToolActive={rangeToolActive}
          blockRangeSelectionActive={blockRangeSelectionActive}
          selectedBlockId={selectedBlockId}
          selectedBlockIds={selectedBlockIds}
          stageSize={stageSize}
          temporaryPanActive={temporaryPanActive}
          activeBlockDragId={activeBlockDragId}
          onBlockPointerDown={onBlockPointerDown}
          onBlockFontStyleCopy={onBlockFontStyleCopy}
          onBlockFontSizeChange={onBlockFontSizeChange}
          onBlockAutoFitDisable={onBlockAutoFitDisable}
          onBlockTextUpdate={onBlockTextUpdate}
          onBlockTextSelectionSplitDuplicate={onBlockTextSelectionSplitDuplicate}
          onBlockTextAlignChange={onBlockTextAlignChange}
          onBlockInlineEditActiveChange={onBlockInlineEditActiveChange}
          onFavoriteFontPresetSelect={onFavoriteFontPresetSelect}
          onInpaintLayerChange={onInpaintLayerChange}
          onInpaintLayerEditEnd={handleInpaintLayerEditEnd}
          onInpaintLayerEditStart={handleInpaintLayerEditStart}
          onInpaintResultColorPick={onInpaintResultColorPick}
          onInpaintResultLayerChange={onInpaintResultLayerChange}
          onInpaintSelectionChange={onInpaintSelectionChange}
        />
      </div>
      {activeInpaintBrushCursorTool ? (
        <InpaintBrushCursorOverlay
          brushSize={activeInpaintBrushCursorTool.brushSize}
          pageSize={pageSize}
          stageRef={stageRef}
          tool={activeInpaintBrushCursorTool.tool}
          wrapRef={wrapRef}
        />
      ) : null}
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
            const target = blockMultiSelectionActive ? "blocks" : blockRangeSelectionActive ? "block" : "inpaint";
            rangeSelectionDragRef.current = {
              pointerId: event.pointerId,
              start: point,
              target,
              toggleBlockSelection: target === "blocks" && event.shiftKey
            };
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
          onPointerEnter={(event) => updateZoomCursorFromDrag(event, zoomToolDragRef.current)}
          onPointerMove={(event) => {
            const drag = zoomToolDragRef.current;
            if (!drag || drag.pointerId !== event.pointerId) {
              updateZoomCursorFromDrag(event, null);
              return;
            }
            event.preventDefault();
            event.stopPropagation();
            updateZoomCursorFromDrag(event, drag);
            const nextScale = resolveStageDragZoomScale(drag.startScale, event.clientX - drag.startX);
            applyStageZoomAnchor(drag.panAnchor, nextScale);
            onZoomToolDrag(nextScale);
          }}
          onPointerLeave={() => {
            if (!zoomToolDragRef.current) {
              clearZoomCursor();
            }
          }}
          onPointerDown={(event) => {
            if (event.button !== 0) {
              return;
            }
            event.preventDefault();
            event.stopPropagation();
            event.currentTarget.setPointerCapture(event.pointerId);
            const startScale = resolveCurrentZoomScale();
            zoomToolDragRef.current = {
              panAnchor: beginStageZoomAnchor(event.clientX, event.clientY, startScale),
              pointerId: event.pointerId,
              startScale,
              startX: event.clientX
            };
            updateZoomCursorFromDrag(event, zoomToolDragRef.current);
          }}
          onPointerUp={(event) => {
            const drag = zoomToolDragRef.current;
            if (!drag || drag.pointerId !== event.pointerId) {
              event.preventDefault();
              event.stopPropagation();
              return;
            }
            event.preventDefault();
            event.stopPropagation();
            event.currentTarget.releasePointerCapture(event.pointerId);
            zoomToolDragRef.current = null;
            clearZoomCursorIfOutside(event);
          }}
          onPointerCancel={(event) => {
            const drag = zoomToolDragRef.current;
            event.preventDefault();
            event.stopPropagation();
            if (drag?.pointerId === event.pointerId) {
              event.currentTarget.releasePointerCapture(event.pointerId);
              zoomToolDragRef.current = null;
            }
            clearZoomCursorIfOutside(event);
          }}
        >
          {zoomCursor ? (
            <div
              className={`stage-zoom-cursor ${zoomCursor.zoomOut ? "zoom-out" : "zoom-in"}`}
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
