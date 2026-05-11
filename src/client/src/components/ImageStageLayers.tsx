import React from "react";
import type { FontPreset, ImageRect, MangaPage, TranslationBlock } from "../../../shared/types";
import { resolveBlockRotationDeg } from "../../../shared/geometry";
import { isBlockDuplicateModifier, isBlockInlineEditShortcut } from "../lib/editorShortcuts";
import { drawImageToCanvas, loadCanvasImage, resizeCanvasToSize } from "../lib/canvasImageDrawing";
import { isEditableTarget } from "../lib/editorUtils";
import {
  getActiveTranslationBlockDragDebugId,
  isInpaintDebugLogEnabled,
  roundInpaintDebugMs,
  writeInpaintDebugLog
} from "../lib/inpaintDiagnostics";
import { resolveSelectedTranslationBlockGroup } from "../lib/blockGroups";
import { hasEnabledTranslationBlockGroupEffects } from "../lib/blockGroupEffects";
import type { InpaintLayerChangeOptions } from "../lib/inpaintLayerChange";
import { resolveBlockRectPx, type FontWeightAvailability, type PixelRect, type ViewportSize } from "../lib/overlayLayout";
import { drawOverlayBlocks, resolveBlockCanvasDirtyRect } from "../lib/pageRender";
import {
  addTranslationBlockDragPreviewOffsetListener,
  getTranslationBlockDragPreviewOffset,
  type TranslationBlockDragPreviewOffset
} from "../lib/translationBlockDragPreview";
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
type BlockDragMode = "move" | "resize" | "rotate";

const OVERLAY_REACT_DEBUG_SLOW_RENDER_MS = 16;
const OVERLAY_REACT_DEBUG_SUMMARY_INTERVAL_MS = 500;
const GROUP_DRAG_PREVIEW_EFFECT_MARGIN_PX = 328;

type ImageStageLayersProps = {
  activeBlockDragIds: string[];
  activeBlockDragMode: BlockDragMode | null;
  activeLayer: ImageStageActiveLayer;
  imageRef: React.RefObject<HTMLCanvasElement | null>;
  finalOutputPreviewActive: boolean;
  inpaintResultComposite: boolean;
  inpaintBrushSize: number;
  inpaintDisabled: boolean;
  inpaintResultBrushColor: string;
  inpaintResultBrushHardness: number;
  inpaintResultBrushSize: number;
  inpaintResultDisabled: boolean;
  inpaintResultTool: InpaintResultTool;
  inpaintResultToolStrength: number;
  inpaintSelectionRect: ImageRect | null;
  rangeSelectionPreviewRect: ImageRect | null;
  inpaintTool: InpaintTool;
  inpaintStrokeActive: boolean;
  favoriteFontPresets: FontPreset[];
  fontWeightAvailability: readonly FontWeightAvailability[];
  layerOpacity: ImageStageLayerOpacity;
  layerVisibility: ImageStageLayerVisibility;
  page: MangaPage;
  pageSize: ViewportSize;
  rangeToolActive: boolean;
  blockRangeSelectionActive: boolean;
  selectedBlockId: string | null;
  selectedBlockIds: string[];
  stageSize: ViewportSize | null;
  temporaryPanActive: boolean;
  onBlockPointerDown: (event: React.PointerEvent, block: TranslationBlock, mode: BlockDragMode) => void;
  onBlockFontStyleCopy: () => void | Promise<void>;
  onBlockFontSizeChange: (fontSizePx: number) => void;
  onBlockAutoFitDisable: () => void;
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
  onInpaintLayerEditEnd: () => void;
  onInpaintLayerEditStart: () => void;
  onInpaintLayerChange: (dataUrl: string | undefined, options?: InpaintLayerChangeOptions) => void;
  onInpaintResultColorPick: (color: string) => void;
  onInpaintResultLayerChange: (dataUrl: string | undefined, options?: InpaintLayerChangeOptions) => void;
  onInpaintSelectionChange: (rect: ImageRect | null) => void;
};

export function ImageStageLayers({
  activeBlockDragIds,
  activeBlockDragMode,
  activeLayer,
  imageRef,
  finalOutputPreviewActive,
  inpaintResultComposite,
  inpaintBrushSize,
  inpaintDisabled,
  inpaintResultBrushColor,
  inpaintResultBrushHardness,
  inpaintResultBrushSize,
  inpaintResultDisabled,
  inpaintResultTool,
  inpaintResultToolStrength,
  inpaintSelectionRect,
  rangeSelectionPreviewRect,
  inpaintTool,
  inpaintStrokeActive,
  favoriteFontPresets,
  fontWeightAvailability,
  layerOpacity,
  layerVisibility,
  page,
  pageSize,
  rangeToolActive,
  blockRangeSelectionActive,
  selectedBlockId,
  selectedBlockIds,
  stageSize,
  temporaryPanActive,
  onBlockPointerDown,
  onBlockFontStyleCopy,
  onBlockFontSizeChange,
  onBlockAutoFitDisable,
  onBlockTextUpdate,
  onBlockTextSelectionSplitDuplicate,
  onBlockTextAlignChange,
  onBlockInlineEditActiveChange,
  onFavoriteFontPresetSelect,
  onInpaintLayerEditEnd,
  onInpaintLayerEditStart,
  onInpaintLayerChange,
  onInpaintResultColorPick,
  onInpaintResultLayerChange,
  onInpaintSelectionChange
}: ImageStageLayersProps): React.JSX.Element {
  const inpaintMaskDataUrl = page.inpaintMaskDataUrl ?? page.inpaintLayerDataUrl;
  const resolvedStageSize = stageSize ?? pageSize;
  const [inlineEdit, setInlineEdit] = React.useState<{ blockId: string; draft: string } | null>(null);
  const inlineEditorRef = React.useRef<HTMLTextAreaElement | null>(null);
  const overlayRenderCanvasRef = React.useRef<HTMLCanvasElement | null>(null);
  const suppressInlineCommitBlockIdRef = React.useRef<string | null>(null);
  const overlayReactDebugStatsRef = React.useRef({
    lastSummaryAtMs: 0,
    maxRenderMs: 0,
    renderCount: 0,
    slowRenderCount: 0,
    totalRenderMs: 0
  });
  const duplicateModifierPlatform = React.useMemo(() => (typeof navigator === "undefined" ? "" : navigator.platform), []);
  const [duplicateBlockMode, setDuplicateBlockMode] = React.useState(false);
  const selectedBlockIdSet = React.useMemo(() => new Set(selectedBlockIds), [selectedBlockIds]);
  const multiBlockSelectionActive = selectedBlockIds.length > 1;
  const visibleRangeSelectionRect =
    activeLayer === "overlay" ? rangeSelectionPreviewRect : rangeSelectionPreviewRect ?? inpaintSelectionRect;
  const rangeSelectionLayerVisible =
    rangeToolActive || Boolean(rangeSelectionPreviewRect) || (activeLayer !== "overlay" && Boolean(inpaintSelectionRect));
  const inpaintResultMaskImage = inpaintResultComposite && inpaintMaskDataUrl ? `url(${inpaintMaskDataUrl})` : undefined;
  const renderInpaintResultLayer =
    layerVisibility.inpaintResult &&
    (Boolean(page.inpaintResultDataUrl) || activeLayer === "inpaintResult");
  const renderInpaintMaskLayer =
    layerVisibility.inpaintMask ||
    (activeLayer === "inpaintMask" && temporaryPanActive);
  const inpaintEditingEnabled = !temporaryPanActive || inpaintStrokeActive;
  const inpaintMaskEditingEnabled = activeLayer === "inpaintMask" && inpaintEditingEnabled;
  const overlayEditingEnabled = activeLayer === "overlay" && !temporaryPanActive;
  const debugLogEnabled = isInpaintDebugLogEnabled();
  const activeBlockDragIdSet = React.useMemo(() => new Set(activeBlockDragIds), [activeBlockDragIds]);
  const selectedBlockGroup = React.useMemo(
    () => selectedBlockIds.length > 1 ? resolveSelectedTranslationBlockGroup(page, selectedBlockIds) : null,
    [page, selectedBlockIds]
  );
  const selectedGroupBlockIdSet = React.useMemo(
    () => new Set(selectedBlockGroup?.blockIds ?? []),
    [selectedBlockGroup]
  );
  const selectedGroupBlocks = React.useMemo(() => (
    selectedBlockGroup
      ? page.blocks.filter((block) => selectedGroupBlockIdSet.has(block.id) && block.renderDirection !== "hidden")
      : []
  ), [page.blocks, selectedBlockGroup, selectedGroupBlockIdSet]);
  const selectedGroupOutlineRect = React.useMemo(
    () => resolveBlocksSelectionOutlineRect(selectedGroupBlocks, pageSize, resolvedStageSize),
    [pageSize, resolvedStageSize, selectedGroupBlocks]
  );
  const selectedGroupDragPreviewActive =
    activeBlockDragMode === "move" &&
    selectedGroupBlockIdSet.size > 0 &&
    [...selectedGroupBlockIdSet].every((blockId) => activeBlockDragIdSet.has(blockId));
  const activeDragBlocks = React.useMemo(() => {
    if (activeBlockDragIdSet.size === 0) {
      return [];
    }

    return page.blocks.filter((block) => activeBlockDragIdSet.has(block.id));
  }, [activeBlockDragIdSet, page.blocks]);

  const handleOverlayProfilerRender = React.useCallback((
    id: string,
    phase: "mount" | "update" | "nested-update",
    actualDuration: number,
    baseDuration: number,
    startTime: number,
    commitTime: number
  ) => {
    if (!debugLogEnabled) {
      return;
    }

    const stats = overlayReactDebugStatsRef.current;
    stats.renderCount += 1;
    stats.totalRenderMs += actualDuration;
    stats.maxRenderMs = Math.max(stats.maxRenderMs, actualDuration);

    const activeDragDebugId = getActiveTranslationBlockDragDebugId();
    const shouldLogSlowRender = actualDuration >= OVERLAY_REACT_DEBUG_SLOW_RENDER_MS;
    if (shouldLogSlowRender) {
      stats.slowRenderCount += 1;
    }
    const shouldWriteSlowRender = shouldLogSlowRender && (stats.slowRenderCount <= 8 || stats.slowRenderCount % 10 === 0);
    const shouldLogSummary = activeDragDebugId && commitTime - stats.lastSummaryAtMs >= OVERLAY_REACT_DEBUG_SUMMARY_INTERVAL_MS;
    if (!shouldWriteSlowRender && !shouldLogSummary) {
      return;
    }

    stats.lastSummaryAtMs = commitTime;
    writeInpaintDebugLog(shouldWriteSlowRender ? "overlay-layer:react-render-slow" : "overlay-layer:react-render-summary", {
      activeDragDebugId,
      actualDurationMs: roundInpaintDebugMs(actualDuration),
      activeLayer,
      averageRenderMs: roundInpaintDebugMs(stats.totalRenderMs / Math.max(1, stats.renderCount)),
      baseDurationMs: roundInpaintDebugMs(baseDuration),
      blockCount: page.blocks.length,
      commitOffsetMs: roundInpaintDebugMs(commitTime - startTime),
      editingEnabled: overlayEditingEnabled,
      maxRenderMs: roundInpaintDebugMs(stats.maxRenderMs),
      pageId: page.id,
      phase,
      profilerId: id,
      renderCount: stats.renderCount,
      selectedBlockCount: selectedBlockIds.length,
      selectedBlockId,
      slowRenderCount: stats.slowRenderCount
    });
  }, [activeLayer, debugLogEnabled, overlayEditingEnabled, page.blocks.length, page.id, selectedBlockId, selectedBlockIds.length]);

  const resolveDuplicateModifierState = React.useCallback((event: Pick<KeyboardEvent | PointerEvent | React.PointerEvent, "ctrlKey" | "metaKey">) => (
    isBlockDuplicateModifier(event, duplicateModifierPlatform)
  ), [duplicateModifierPlatform]);
  const startInlineEdit = React.useCallback((block: TranslationBlock) => {
    setInlineEdit({ blockId: block.id, draft: block.translatedText });
  }, []);

  const commitInlineEdit = React.useCallback(() => {
    if (!inlineEdit) {
      return;
    }
    if (suppressInlineCommitBlockIdRef.current === inlineEdit.blockId) {
      suppressInlineCommitBlockIdRef.current = null;
      setInlineEdit(null);
      return;
    }
    const block = page.blocks.find((candidate) => candidate.id === inlineEdit.blockId);
    if (block && inlineEdit.draft !== block.translatedText) {
      onBlockTextUpdate(block, inlineEdit.draft);
    }
    setInlineEdit(null);
  }, [inlineEdit, onBlockTextUpdate, page.blocks]);

  React.useEffect(() => {
    if (!inlineEdit) {
      suppressInlineCommitBlockIdRef.current = null;
    }
  }, [inlineEdit]);

  React.useEffect(() => {
    if (multiBlockSelectionActive && inlineEdit) {
      commitInlineEdit();
    }
  }, [commitInlineEdit, inlineEdit, multiBlockSelectionActive]);

  React.useEffect(() => {
    onBlockInlineEditActiveChange(Boolean(inlineEdit));
  }, [inlineEdit, onBlockInlineEditActiveChange]);

  React.useEffect(() => () => onBlockInlineEditActiveChange(false), [onBlockInlineEditActiveChange]);

  const readInlineEditSelection = React.useCallback((blockId: string) => {
    const editor = inlineEditorRef.current;
    if (!inlineEdit || inlineEdit.blockId !== blockId || !editor) {
      return null;
    }
    const selectionStart = editor.selectionStart;
    const selectionEnd = editor.selectionEnd;
    if (selectionStart === selectionEnd) {
      return null;
    }
    return {
      translatedText: editor.value,
      selectionStart,
      selectionEnd
    };
  }, [inlineEdit]);

  const handleMovePointerDown = React.useCallback((event: React.PointerEvent, block: TranslationBlock) => {
    if (resolveDuplicateModifierState(event)) {
      const selection = readInlineEditSelection(block.id);
      if (selection) {
        event.preventDefault();
        event.stopPropagation();
        suppressInlineCommitBlockIdRef.current = block.id;
        const splitApplied = onBlockTextSelectionSplitDuplicate(
          block,
          selection.translatedText,
          selection.selectionStart,
          selection.selectionEnd
        );
        if (splitApplied) {
          setInlineEdit(null);
          return;
        }
        suppressInlineCommitBlockIdRef.current = null;
      }
    }

    onBlockPointerDown(event, block, "move");
  }, [onBlockPointerDown, onBlockTextSelectionSplitDuplicate, readInlineEditSelection, resolveDuplicateModifierState]);

  React.useEffect(() => {
    if (!inlineEdit) {
      return;
    }
    if (!page.blocks.some((block) => block.id === inlineEdit.blockId)) {
      setInlineEdit(null);
      return;
    }
    if (selectedBlockId !== inlineEdit.blockId) {
      commitInlineEdit();
    }
  }, [commitInlineEdit, inlineEdit, page.blocks, selectedBlockId]);

  React.useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (
        activeLayer !== "overlay" ||
        temporaryPanActive ||
        !layerVisibility.overlay ||
        multiBlockSelectionActive ||
        inlineEdit ||
        isEditableTarget(event.target) ||
        !isBlockInlineEditShortcut(event)
      ) {
        return;
      }

      const block = page.blocks.find((candidate) => candidate.id === selectedBlockId);
      if (!block || block.renderDirection === "hidden") {
        return;
      }

      event.preventDefault();
      startInlineEdit(block);
    };

    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [activeLayer, inlineEdit, layerVisibility.overlay, multiBlockSelectionActive, page.blocks, selectedBlockId, startInlineEdit, temporaryPanActive]);

  React.useEffect(() => {
    const updateFromKeyboardEvent = (event: KeyboardEvent) => {
      setDuplicateBlockMode(resolveDuplicateModifierState(event));
    };
    const clearDuplicateMode = () => setDuplicateBlockMode(false);

    window.addEventListener("keydown", updateFromKeyboardEvent);
    window.addEventListener("keyup", updateFromKeyboardEvent);
    window.addEventListener("blur", clearDuplicateMode);
    return () => {
      window.removeEventListener("keydown", updateFromKeyboardEvent);
      window.removeEventListener("keyup", updateFromKeyboardEvent);
      window.removeEventListener("blur", clearDuplicateMode);
    };
  }, [resolveDuplicateModifierState]);

  const renderOverlayLayerContent = () => (
    <>
      <OverlayRenderCanvas
        canvasRef={overlayRenderCanvasRef}
        page={page}
        stageSize={resolvedStageSize}
        editingEnabled={overlayEditingEnabled}
        hiddenBlockIds={activeBlockDragIds}
        fontWeightAvailability={fontWeightAvailability}
      />
      {page.blocks.map((block) => (
        <OverlayBlock
          key={block.id}
          block={block}
          pageSize={pageSize}
          stageSize={resolvedStageSize}
          selected={
            !selectedGroupBlockIdSet.has(block.id) &&
            (selectedBlockIdSet.has(block.id) || (!multiBlockSelectionActive && block.id === selectedBlockId))
          }
          editingEnabled={overlayEditingEnabled}
          widgetsVisible={!blockRangeSelectionActive && !multiBlockSelectionActive}
          inlineEditDraft={inlineEdit?.blockId === block.id ? inlineEdit.draft : undefined}
          inlineEditorRef={inlineEdit?.blockId === block.id ? inlineEditorRef : undefined}
          visualContentVisible={false}
          dragPreviewHidden={activeBlockDragMode === "move" && activeBlockDragIdSet.has(block.id)}
          favoriteFontPresets={favoriteFontPresets}
          onPointerDown={(event) => handleMovePointerDown(event, block)}
          onStartInlineEdit={
            multiBlockSelectionActive
              ? undefined
              : (event) => {
                  event.preventDefault();
                  event.stopPropagation();
                  startInlineEdit(block);
                }
          }
          onInlineEditChange={(draft) => setInlineEdit({ blockId: block.id, draft })}
          onInlineEditCancel={() => setInlineEdit(null)}
          onInlineEditCommit={commitInlineEdit}
          onFontStyleCopy={onBlockFontStyleCopy}
          onFontSizeChange={onBlockFontSizeChange}
          onAutoFitDisable={onBlockAutoFitDisable}
          onTextAlignChange={onBlockTextAlignChange}
          onFavoriteFontPresetSelect={onFavoriteFontPresetSelect}
          onResizePointerDown={(event) => onBlockPointerDown(event, block, "resize")}
          onRotatePointerDown={(event) => onBlockPointerDown(event, block, "rotate")}
        />
      ))}
      {selectedGroupOutlineRect && overlayEditingEnabled ? (
        <OverlayBlockGroupSelectionOutline
          dragPreviewActive={selectedGroupDragPreviewActive}
          rect={selectedGroupOutlineRect}
        />
      ) : null}
      {activeDragBlocks.length > 0 ? (
        <OverlayBlockDragPreviewCanvas
          blocks={activeDragBlocks}
          editingEnabled={overlayEditingEnabled}
          fontWeightAvailability={fontWeightAvailability}
          page={page}
          stageSize={resolvedStageSize}
        />
      ) : null}
    </>
  );

  return (
    <>
      <SourceImageCanvas
        canvasRef={imageRef}
        className="page-image"
        dataUrl={page.dataUrl}
        pageSize={pageSize}
        label={page.name}
        style={{
          visibility: layerVisibility.image ? "visible" : "hidden",
          opacity: layerOpacity.image
        }}
      />
      {layerVisibility.inpaint ? (
        <div className="inpaint-layer-preview" style={{ opacity: layerOpacity.inpaint }}>
          {renderInpaintResultLayer ? (
            <InpaintResultCanvas
              className="inpaint-result-canvas"
              colorPickerSampleRequired={finalOutputPreviewActive}
              dataUrl={page.inpaintResultDataUrl}
              fallbackCanvasRef={imageRef}
              finalOutputOverlayCanvasRef={overlayRenderCanvasRef}
              maskDataUrl={inpaintMaskDataUrl}
              pageSize={pageSize}
              tool={inpaintResultTool}
              brushSize={inpaintResultBrushSize}
              brushColor={inpaintResultBrushColor}
              brushHardness={inpaintResultBrushHardness}
              toolStrength={inpaintResultToolStrength}
              disabled={inpaintResultDisabled || !inpaintEditingEnabled}
              selectionRect={null}
              onChange={onInpaintResultLayerChange}
              onColorPick={onInpaintResultColorPick}
              onEditEnd={onInpaintLayerEditEnd}
              onEditStart={onInpaintLayerEditStart}
              onSelectionChange={onInpaintSelectionChange}
              style={{
                zIndex: inpaintResultComposite ? 1 : 3,
                opacity: layerOpacity.inpaintResult,
                maskImage: inpaintResultMaskImage,
                WebkitMaskImage: inpaintResultMaskImage
              }}
            />
          ) : null}
          {renderInpaintMaskLayer ? (
            <div
              className="inpaint-mask-layer-preview"
              style={{
                zIndex: activeLayer === "inpaintMask" ? 3 : 2,
                opacity: layerVisibility.inpaintMask ? layerOpacity.inpaintMask : 0,
                pointerEvents: inpaintMaskEditingEnabled ? "auto" : "none"
              }}
            >
              <InpaintLayerCanvas
                dataUrl={inpaintMaskDataUrl}
                pageSize={pageSize}
                tool={inpaintTool}
                brushSize={inpaintBrushSize}
                disabled={inpaintDisabled || !inpaintEditingEnabled}
                selectionRect={null}
                onChange={onInpaintLayerChange}
                onEditEnd={onInpaintLayerEditEnd}
                onEditStart={onInpaintLayerEditStart}
                onSelectionChange={onInpaintSelectionChange}
              />
            </div>
          ) : null}
        </div>
      ) : null}
      {layerVisibility.overlay
        ? (
            <div
              className={`overlay-layer-preview${duplicateBlockMode ? " duplicate-block-mode" : ""}`}
              style={{
                opacity: layerOpacity.overlay,
                pointerEvents: overlayEditingEnabled ? "auto" : "none"
              }}
              onPointerEnter={(event) => setDuplicateBlockMode(resolveDuplicateModifierState(event))}
              onPointerMove={(event) => setDuplicateBlockMode(resolveDuplicateModifierState(event))}
              onPointerLeave={() => setDuplicateBlockMode(false)}
            >
              {debugLogEnabled ? (
                <React.Profiler id="overlay-layer" onRender={handleOverlayProfilerRender}>
                  {renderOverlayLayerContent()}
                </React.Profiler>
              ) : renderOverlayLayerContent()}
            </div>
          )
        : null}
      {rangeSelectionLayerVisible ? (
        <div className="stage-range-selection-layer">
          <InpaintLayerCanvas
            pageSize={pageSize}
            tool="select"
            brushSize={1}
            disabled={true}
            selectionRect={visibleRangeSelectionRect}
            onChange={() => undefined}
            onSelectionChange={onInpaintSelectionChange}
          />
        </div>
      ) : null}
    </>
  );
}

type SourceImageCanvasProps = {
  canvasRef: React.RefObject<HTMLCanvasElement | null>;
  className: string;
  dataUrl: string;
  label: string;
  pageSize: ViewportSize;
  style?: React.CSSProperties;
};

function SourceImageCanvas({
  canvasRef,
  className,
  dataUrl,
  label,
  pageSize,
  style
}: SourceImageCanvasProps): React.JSX.Element {
  React.useEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d", { willReadFrequently: true });
    if (!canvas || !context) {
      return;
    }

    let cancelled = false;
    resizeCanvasToSize(canvas, pageSize);
    const drawImage = async () => {
      const image = await loadCanvasImage(dataUrl, "원본 이미지를 불러오지 못했습니다.");
      if (cancelled) {
        return;
      }
      drawImageToCanvas(canvas, context, image, pageSize);
    };

    void drawImage().catch((error) => {
      if (!cancelled) {
        console.error(error);
      }
    });
    return () => {
      cancelled = true;
    };
  }, [canvasRef, dataUrl, pageSize.height, pageSize.width]);

  return <canvas ref={canvasRef} className={className} style={style} role="img" aria-label={label} />;
}

type OverlayBlockDragPreviewCanvasProps = {
  blocks: TranslationBlock[];
  editingEnabled: boolean;
  fontWeightAvailability: readonly FontWeightAvailability[];
  page: MangaPage;
  stageSize: ViewportSize;
};

function OverlayBlockDragPreviewCanvas({
  blocks,
  editingEnabled,
  fontWeightAvailability,
  page,
  stageSize
}: OverlayBlockDragPreviewCanvasProps): React.JSX.Element | null {
  const canvasRef = React.useRef<HTMLCanvasElement | null>(null);
  const dirtyRect = React.useMemo(() => resolveBlocksCanvasDirtyRect(blocks, page), [blocks, page]);
  const includedBlockIds = React.useMemo(() => new Set(blocks.map((block) => block.id)), [blocks]);
  useTranslationBlockDragPreviewTransform(canvasRef, true);

  React.useLayoutEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d", { willReadFrequently: true });
    if (!canvas || !context || !dirtyRect) {
      return;
    }

    canvas.width = dirtyRect.width;
    canvas.height = dirtyRect.height;
    context.setTransform(1, 0, 0, 1, 0, 0);
    context.clearRect(0, 0, dirtyRect.width, dirtyRect.height);
    context.setTransform(1, 0, 0, 1, -dirtyRect.x, -dirtyRect.y);
    drawOverlayBlocks(context, page, {
      renderSize: { width: page.width, height: page.height },
      editingEnabled,
      includedBlockIds,
      fontWeightAvailability
    });
  }, [dirtyRect, editingEnabled, fontWeightAvailability, includedBlockIds, page]);

  if (!dirtyRect) {
    return null;
  }

  const style: React.CSSProperties = {
    left: (dirtyRect.x / Math.max(1, page.width)) * stageSize.width,
    top: (dirtyRect.y / Math.max(1, page.height)) * stageSize.height,
    width: (dirtyRect.width / Math.max(1, page.width)) * stageSize.width,
    height: (dirtyRect.height / Math.max(1, page.height)) * stageSize.height,
    pointerEvents: "none",
    position: "absolute",
    willChange: "transform",
    zIndex: 80
  };

  return <canvas ref={canvasRef} className="overlay-block-drag-preview-canvas" style={style} aria-hidden="true" />;
}

type OverlayBlockGroupSelectionOutlineProps = {
  dragPreviewActive: boolean;
  rect: PixelRect;
};

function OverlayBlockGroupSelectionOutline({
  dragPreviewActive,
  rect
}: OverlayBlockGroupSelectionOutlineProps): React.JSX.Element {
  const outlineRef = React.useRef<HTMLDivElement | null>(null);
  useTranslationBlockDragPreviewTransform(outlineRef, dragPreviewActive);

  const style: React.CSSProperties = {
    left: rect.left,
    top: rect.top,
    width: rect.width,
    height: rect.height
  };

  return <div ref={outlineRef} className="overlay-block-group-selection-outline" style={style} aria-hidden="true" />;
}

type OverlayCanvasDirtyRect = NonNullable<ReturnType<typeof resolveBlockCanvasDirtyRect>>;

function resolveBlocksCanvasDirtyRect(
  blocks: readonly TranslationBlock[],
  page: MangaPage
): OverlayCanvasDirtyRect | null {
  let left = Number.POSITIVE_INFINITY;
  let top = Number.POSITIVE_INFINITY;
  let right = Number.NEGATIVE_INFINITY;
  let bottom = Number.NEGATIVE_INFINITY;

  for (const block of blocks) {
    const rect = resolveBlockCanvasDirtyRect(block, page);
    if (!rect) {
      continue;
    }

    left = Math.min(left, rect.x);
    top = Math.min(top, rect.y);
    right = Math.max(right, rect.x + rect.width);
    bottom = Math.max(bottom, rect.y + rect.height);
  }

  if (!Number.isFinite(left) || !Number.isFinite(top) || !Number.isFinite(right) || !Number.isFinite(bottom)) {
    return null;
  }

  const margin = page.blockGroups?.some(hasEnabledTranslationBlockGroupEffects)
    ? GROUP_DRAG_PREVIEW_EFFECT_MARGIN_PX
    : 0;
  const x = Math.max(0, Math.floor(left - margin));
  const y = Math.max(0, Math.floor(top - margin));
  const width = Math.min(page.width, Math.ceil(right + margin)) - x;
  const height = Math.min(page.height, Math.ceil(bottom + margin)) - y;
  if (width <= 0 || height <= 0) {
    return null;
  }

  return { x, y, width, height };
}

function useTranslationBlockDragPreviewTransform<T extends HTMLElement>(
  ref: React.RefObject<T | null>,
  enabled: boolean
): void {
  React.useLayoutEffect(() => {
    const element = ref.current;
    if (!element) {
      return;
    }
    if (!enabled) {
      element.style.transform = "";
      return;
    }

    const applyOffset = (offset: TranslationBlockDragPreviewOffset) => {
      element.style.transform =
        offset.offsetX !== 0 || offset.offsetY !== 0
          ? `translate3d(${offset.offsetX}px, ${offset.offsetY}px, 0)`
          : "";
    };
    applyOffset(getTranslationBlockDragPreviewOffset());
    return addTranslationBlockDragPreviewOffsetListener(applyOffset);
  }, [enabled, ref]);
}

function resolveBlocksSelectionOutlineRect(
  blocks: readonly TranslationBlock[],
  pageSize: ViewportSize,
  stageSize: ViewportSize
): PixelRect | null {
  let left = Number.POSITIVE_INFINITY;
  let top = Number.POSITIVE_INFINITY;
  let right = Number.NEGATIVE_INFINITY;
  let bottom = Number.NEGATIVE_INFINITY;

  for (const block of blocks) {
    const rect = resolveRotatedSelectionRect(resolveBlockRectPx(block, pageSize, stageSize), resolveBlockRotationDeg(block));
    left = Math.min(left, rect.left);
    top = Math.min(top, rect.top);
    right = Math.max(right, rect.left + rect.width);
    bottom = Math.max(bottom, rect.top + rect.height);
  }

  if (!Number.isFinite(left) || !Number.isFinite(top) || !Number.isFinite(right) || !Number.isFinite(bottom)) {
    return null;
  }

  return {
    left,
    top,
    width: right - left,
    height: bottom - top
  };
}

function resolveRotatedSelectionRect(rect: PixelRect, rotationDeg: number): PixelRect {
  if (rotationDeg === 0) {
    return rect;
  }

  const centerX = rect.left + rect.width / 2;
  const centerY = rect.top + rect.height / 2;
  const angleRad = (rotationDeg * Math.PI) / 180;
  const cos = Math.cos(angleRad);
  const sin = Math.sin(angleRad);
  const corners = [
    { x: rect.left, y: rect.top },
    { x: rect.left + rect.width, y: rect.top },
    { x: rect.left + rect.width, y: rect.top + rect.height },
    { x: rect.left, y: rect.top + rect.height }
  ].map((corner) => {
    const dx = corner.x - centerX;
    const dy = corner.y - centerY;
    return {
      x: centerX + dx * cos - dy * sin,
      y: centerY + dx * sin + dy * cos
    };
  });
  const left = Math.min(...corners.map((corner) => corner.x));
  const top = Math.min(...corners.map((corner) => corner.y));
  const right = Math.max(...corners.map((corner) => corner.x));
  const bottom = Math.max(...corners.map((corner) => corner.y));

  return {
    left,
    top,
    width: right - left,
    height: bottom - top
  };
}
