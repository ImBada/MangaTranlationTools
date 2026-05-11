import React from "react";
import type {
  ImageRect,
  FontPreset,
  ImportSourceKind,
  JobState,
  LamaRuntimeStatus,
  MangaPage,
  TranslationBlock
} from "../../../../shared/types";
import { BLOCK_INLINE_EDIT_SHORTCUT, INPAINT_TOOL_SHORTCUTS } from "../../lib/editorShortcuts";
import type { RecoverableFailure, RecoverableFailureId } from "../../hooks/useRecoverableFailures";
import type { StatusToastTone } from "../../hooks/useStatusFeedback";
import type { InpaintLayerChangeOptions } from "../../lib/inpaintLayerChange";
import { FORCE_INCOMPLETE_LAMA_NOTICE, type LamaNoticePlatform } from "../../lib/lamaRuntimeNotice";
import type { ActiveLayer, LayerOpacity, LayerVisibility } from "../../lib/layerState";
import { resolveStageLayerPreviewState } from "../../lib/layerPreviewState";
import { isMacLikePlatform } from "../../lib/globalUndo";
import { isExistingTranslationBlockGroupSelection } from "../../lib/blockGroups";
import type { FontWeightAvailability, ViewportSize } from "../../lib/overlayLayout";
import { ImageStage } from "../ImageStage";
import type { InpaintTool } from "../InpaintLayerCanvas";
import type { InpaintResultTool } from "../InpaintResultCanvas";
import { WorkspaceEmptyState } from "./WorkspaceEmptyState";
import {
  NotificationDock,
  type PageInpaintNotice,
  StageToolOverlay,
  StageTextBlockList,
  StageZoomOverlay,
  StatusHistoryPanel
} from "./WorkspaceOverlays";

type BlockDragMode = "move" | "resize" | "rotate";

type WorkspacePanelProps = {
  activeLayer: ActiveLayer;
  displayedLamaStatus: LamaRuntimeStatus | null;
  favoriteFontPresets: FontPreset[];
  fontWeightAvailability: readonly FontWeightAvailability[];
  fitStageToWorkspace: () => void;
  handleZoomToolDrag: (scale: number) => void;
  imageRef: React.RefObject<HTMLCanvasElement | null>;
  inpaintBrushSize: number;
  inpaintBusy: boolean;
  inpaintResultBrushColor: string;
  inpaintResultBrushHardness: number;
  inpaintResultBrushSize: number;
  inpaintResultTool: InpaintResultTool;
  inpaintResultToolStrength: number;
  inpaintSelectionRect: ImageRect | null;
  inpaintTool: InpaintTool;
  jobState: JobState;
  lamaActionBusy: boolean;
  lamaActionMessage: string | null;
  lamaNoticePlatform: LamaNoticePlatform;
  layerVisibility: LayerVisibility;
  layerOpacity: LayerOpacity;
  overlayOpacityEditMode: boolean;
  rangeToolActive: boolean;
  recoverableFailures: RecoverableFailure[];
  selectedBlockId: string | null;
  selectedBlockIds: string[];
  selectedPage: MangaPage | null;
  selectedPageEditLocked: boolean;
  selectedPageInpaintNotice: PageInpaintNotice | null;
  showLamaEmptyNotice: boolean;
  showOriginalStageSize: () => void;
  stageRef: React.RefObject<HTMLDivElement | null>;
  stageSize: ViewportSize | null;
  stageViewResetKey: number;
  stageViewScale: number | null;
  stageZoomLabel: string;
  statusLines: string[];
  statusToastLine: string | null;
  statusToastTone: StatusToastTone;
  statusWidgetOpen: boolean;
  statusWidgetTone: string;
  temporaryPanActive: boolean;
  workspacePanelRef: React.RefObject<HTMLElement | null>;
  zoomToolActive: boolean;
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
  onFavoriteFontPresetSelect: (presetId: string) => void;
  onDownloadLamaModel: () => void | Promise<unknown>;
  onOpenFindReplace: () => void;
  onInpaintLayerChange: (dataUrl: string | undefined, options?: InpaintLayerChangeOptions) => void;
  onInpaintLayerEditEnd: () => void;
  onInpaintLayerEditStart: () => void;
  onInpaintResultColorPick: (color: string) => void;
  onInpaintResultLayerChange: (dataUrl: string | undefined, options?: InpaintLayerChangeOptions) => void;
  onInpaintSelectionChange: (rect: ImageRect | null) => void;
  onPrepareLama: () => void | Promise<unknown>;
  onRefreshLamaStatus: () => void | Promise<unknown>;
  onDismissRecoverableFailure: (id: RecoverableFailureId) => void;
  onRetryRecoverableFailure: (id: RecoverableFailureId) => void | Promise<void>;
  onSelectBlock: React.Dispatch<React.SetStateAction<string | null>>;
  onBlockGroupOrderChange: (groupId: string, blockIds: string[]) => void;
  onBlockOrderChange: (blockIds: string[]) => void;
  onGroupSelectedBlocks: () => void;
  onUngroupSelectedBlocks: () => void;
  onSelectImportFiles: (mode: ImportSourceKind) => void;
  onSelectInpaintResultTool: (tool: Exclude<InpaintResultTool, "select">) => void;
  onSelectPointerTool: () => void;
  onSelectRangeTool: () => void;
  onSelectZoomTool: () => void;
  onSetLamaNoticePlatform: (platform: LamaNoticePlatform) => void;
  onSetStatusWidgetOpen: React.Dispatch<React.SetStateAction<boolean>>;
  activeBlockDragIds: string[];
  activeBlockDragMode: BlockDragMode | null;
  onStagePointerMove: (event: React.PointerEvent) => void;
  onStagePointerUp: (event: React.PointerEvent) => void;
  onBlockSelectionChange: (blockIds: string[]) => void;
  onSelectedBlockRangeChange: (blockId: string, rect: ImageRect) => void;
  onBlockTextAlignChange: (textAlign: TranslationBlock["textAlign"]) => void;
  onZoomInStage: () => void;
  onZoomOutStage: () => void;
};

export function WorkspacePanel({
  activeLayer,
  displayedLamaStatus,
  favoriteFontPresets,
  fontWeightAvailability,
  fitStageToWorkspace,
  handleZoomToolDrag,
  imageRef,
  inpaintBrushSize,
  inpaintBusy,
  inpaintResultBrushColor,
  inpaintResultBrushHardness,
  inpaintResultBrushSize,
  inpaintResultTool,
  inpaintResultToolStrength,
  inpaintSelectionRect,
  inpaintTool,
  jobState,
  lamaActionBusy,
  lamaActionMessage,
  lamaNoticePlatform,
  layerOpacity,
  layerVisibility,
  overlayOpacityEditMode,
  rangeToolActive,
  recoverableFailures,
  selectedBlockId,
  selectedBlockIds,
  selectedPage,
  selectedPageEditLocked,
  selectedPageInpaintNotice,
  showLamaEmptyNotice,
  showOriginalStageSize,
  stageRef,
  stageSize,
  stageViewResetKey,
  stageViewScale,
  stageZoomLabel,
  statusLines,
  statusToastLine,
  statusToastTone,
  statusWidgetOpen,
  statusWidgetTone,
  temporaryPanActive,
  workspacePanelRef,
  zoomToolActive,
  onBlockPointerDown,
  onBlockFontStyleCopy,
  onBlockFontSizeChange,
  onBlockAutoFitDisable,
  onBlockTextUpdate,
  onBlockTextSelectionSplitDuplicate,
  onFavoriteFontPresetSelect,
  onDownloadLamaModel,
  onOpenFindReplace,
  onInpaintLayerChange,
  onInpaintLayerEditEnd,
  onInpaintLayerEditStart,
  onInpaintResultColorPick,
  onInpaintResultLayerChange,
  onInpaintSelectionChange,
  onPrepareLama,
  onRefreshLamaStatus,
  onDismissRecoverableFailure,
  onRetryRecoverableFailure,
  onSelectBlock,
  onBlockGroupOrderChange,
  onBlockOrderChange,
  onGroupSelectedBlocks,
  onUngroupSelectedBlocks,
  onSelectImportFiles,
  onSelectInpaintResultTool,
  onSelectPointerTool,
  onSelectRangeTool,
  onSelectZoomTool,
  onSetLamaNoticePlatform,
  onSetStatusWidgetOpen,
  activeBlockDragIds,
  activeBlockDragMode,
  onStagePointerMove,
  onStagePointerUp,
  onBlockSelectionChange,
  onSelectedBlockRangeChange,
  onBlockTextAlignChange,
  onZoomInStage,
  onZoomOutStage
}: WorkspacePanelProps): React.JSX.Element {
  const [blockInlineEditActive, setBlockInlineEditActive] = React.useState(false);
  const [textBlockListCollapsed, setTextBlockListCollapsed] = React.useState(false);
  const findReplaceShortcutLabel = React.useMemo(() => (
    isMacLikePlatform(typeof navigator === "undefined" ? "" : navigator.platform) ? "⌘F" : "CtrlF"
  ), []);
  const inpaintDisabled =
    selectedPageEditLocked ||
    inpaintBusy ||
    activeLayer !== "inpaintMask" ||
    !layerVisibility.inpaint ||
    !layerVisibility.inpaintMask ||
    inpaintTool === "select";
  const inpaintResultDisabled =
    selectedPageEditLocked ||
    inpaintBusy ||
    activeLayer !== "inpaintResult" ||
    !layerVisibility.inpaint ||
    !layerVisibility.inpaintResult ||
    inpaintResultTool === "select";
  const rangeSelectionDisabled = selectedPageEditLocked || inpaintBusy || !layerVisibility.inpaint;
  const pointerToolActive =
    !zoomToolActive &&
    !rangeToolActive &&
    (activeLayer === "inpaintMask"
      ? inpaintTool === "select"
      : activeLayer === "inpaintResult"
        ? inpaintResultTool === "select"
        : true);
  const colorPickerFinalOutputPreviewActive = activeLayer === "inpaintResult" && inpaintResultTool === "colorPicker";
  const stageLayerPreview = React.useMemo(() => resolveStageLayerPreviewState({
    activeLayer,
    forceFinalOutputPreviewActive: colorPickerFinalOutputPreviewActive,
    layerOpacity,
    layerVisibility,
    overlayOpacityEditMode,
    temporaryPanActive
  }), [
    activeLayer,
    colorPickerFinalOutputPreviewActive,
    layerOpacity,
    layerVisibility,
    overlayOpacityEditMode,
    temporaryPanActive
  ]);
  const selectedBlock = selectedPage?.blocks.find((block) => block.id === selectedBlockId) ?? null;
  const multiBlockSelectionActive = selectedBlockIds.length > 1;
  const textBlockGroupAction = selectedPage && multiBlockSelectionActive
    ? isExistingTranslationBlockGroupSelection(selectedPage, selectedBlockIds)
      ? "ungroup"
      : "group"
    : null;
  const blockInlineEditShortcutVisible =
    activeLayer === "overlay" &&
    !temporaryPanActive &&
    layerVisibility.overlay &&
    !multiBlockSelectionActive &&
    Boolean(selectedBlock && selectedBlock.renderDirection !== "hidden");

  return (
    <section
      ref={workspacePanelRef}
      className={`workspace relative grid place-items-center outline-none${selectedPage ? ` has-stage-block-list${textBlockListCollapsed ? " has-stage-block-list-collapsed" : ""}${textBlockGroupAction ? " has-stage-block-list-actions" : ""} has-layer-glow layer-${activeLayer}` : ""}`}
      tabIndex={0}
      aria-label="읽기 영역"
      onMouseDown={() => workspacePanelRef.current?.focus()}
    >
      {selectedPage ? (
        <StageZoomOverlay
          label={stageZoomLabel}
          onFit={fitStageToWorkspace}
          onOriginalSize={showOriginalStageSize}
          onZoomIn={onZoomInStage}
          onZoomOut={onZoomOutStage}
        />
      ) : null}
      {selectedPage ? (
        <StageToolOverlay
          activeLayer={activeLayer}
          blockInlineEditShortcut={BLOCK_INLINE_EDIT_SHORTCUT}
          blockInlineEditShortcutActive={blockInlineEditActive && blockInlineEditShortcutVisible}
          blockInlineEditShortcutVisible={blockInlineEditShortcutVisible}
          colorPickerActive={colorPickerFinalOutputPreviewActive}
          colorPickerShortcut={INPAINT_TOOL_SHORTCUTS.colorPicker ?? "I"}
          colorPickerVisible={activeLayer === "inpaintResult" && layerVisibility.inpaint && layerVisibility.inpaintResult}
          pointerToolActive={pointerToolActive}
          rangeShortcut={INPAINT_TOOL_SHORTCUTS.select ?? "T"}
          rangeToolActive={rangeToolActive}
          selectedPageEditLocked={selectedPageEditLocked}
          zoomToolActive={zoomToolActive}
          onSelectPointerTool={onSelectPointerTool}
          onSelectResultColorPicker={() => onSelectInpaintResultTool("colorPicker")}
          onSelectRangeTool={onSelectRangeTool}
          onSelectZoomTool={onSelectZoomTool}
        />
      ) : null}
      {selectedPage ? (
        <button
          type="button"
          className="stage-find-replace-button"
          aria-label="찾아바꾸기"
          aria-keyshortcuts="Control+F Meta+F"
          title="찾아바꾸기 (Ctrl/Cmd+F)"
          onClick={onOpenFindReplace}
        >
          <svg className="stage-find-replace-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
            <circle cx="9.5" cy="9.5" r="4.8" />
            <path d="M13 13l3.7 3.7" />
            <path d="M16.2 5.8h2.3c1.1 0 2 .9 2 2v2.7" />
            <path d="M18.1 3.9 16.2 5.8l1.9 1.9" />
            <path d="M20.5 17.2h-2.3c-1.1 0-2-.9-2-2v-2.7" />
            <path d="M18.6 19.1l1.9-1.9-1.9-1.9" />
          </svg>
          <span className="stage-tool-shortcut" aria-hidden="true">{findReplaceShortcutLabel}</span>
        </button>
      ) : null}
      {selectedPage ? (
        <StageTextBlockList
          collapsed={textBlockListCollapsed}
          groupAction={textBlockGroupAction}
          groupDisabled={selectedPageEditLocked}
          page={selectedPage}
          selectedBlockId={selectedBlockId}
          selectedBlockIds={selectedBlockIds}
          selectedPageEditLocked={selectedPageEditLocked}
          onSelectBlock={onSelectBlock}
          onBlockSelectionChange={onBlockSelectionChange}
          onBlockGroupOrderChange={onBlockGroupOrderChange}
          onBlockOrderChange={onBlockOrderChange}
          onGroupSelectedBlocks={onGroupSelectedBlocks}
          onUngroupSelectedBlocks={onUngroupSelectedBlocks}
          onToggleCollapsed={() => setTextBlockListCollapsed((current) => !current)}
        />
      ) : null}
      <NotificationDock
        inpaintNotice={selectedPageInpaintNotice}
        recoverableFailures={recoverableFailures}
        onDismissRecoverableFailure={onDismissRecoverableFailure}
        onRetryRecoverableFailure={onRetryRecoverableFailure}
        statusToastLine={statusToastLine}
        statusToastTone={statusToastTone}
        statusWidgetTone={statusWidgetTone}
      />
      {statusWidgetOpen ? (
        <StatusHistoryPanel
          jobState={jobState}
          statusLines={statusLines}
          statusWidgetTone={statusWidgetTone}
          onClose={() => onSetStatusWidgetOpen(false)}
        />
      ) : null}
      <button
        type="button"
        className={`status-history-button ${statusWidgetTone}`}
        onClick={() => onSetStatusWidgetOpen((current) => !current)}
        aria-expanded={statusWidgetOpen}
        aria-label={`상태 기록 ${statusWidgetOpen ? "닫기" : "열기"}`}
        title="상태 기록"
      >
        기록
      </button>
      {selectedPage ? (
        <div className="workspace-pane w-full">
          <ImageStage
            page={selectedPage}
            favoriteFontPresets={favoriteFontPresets}
            fontWeightAvailability={fontWeightAvailability}
            imageRef={imageRef}
            stageRef={stageRef}
            stageSize={stageSize}
            viewScale={stageViewScale}
            viewResetKey={stageViewResetKey}
            zoomToolActive={zoomToolActive}
            rangeToolActive={rangeToolActive}
            selectedBlockId={selectedBlockId}
            selectedBlockIds={selectedBlockIds}
            layerOpacity={stageLayerPreview.layerOpacity}
            layerVisibility={stageLayerPreview.layerVisibility}
            activeLayer={activeLayer}
            finalOutputPreviewActive={stageLayerPreview.finalOutputPreviewActive}
            inpaintResultComposite={stageLayerPreview.inpaintResultComposite}
            inpaintTool={inpaintTool}
            inpaintBrushSize={inpaintBrushSize}
            inpaintResultTool={inpaintResultTool}
            inpaintResultBrushSize={inpaintResultBrushSize}
            inpaintResultBrushColor={inpaintResultBrushColor}
            inpaintResultBrushHardness={inpaintResultBrushHardness}
            inpaintResultToolStrength={inpaintResultToolStrength}
            inpaintDisabled={inpaintDisabled}
            inpaintResultDisabled={inpaintResultDisabled}
            rangeSelectionDisabled={rangeSelectionDisabled}
            blockRangeSelectionDisabled={selectedPageEditLocked}
            temporaryPanActive={temporaryPanActive}
            inpaintSelectionRect={inpaintSelectionRect}
            activeBlockDragIds={activeBlockDragIds}
            activeBlockDragMode={activeBlockDragMode}
            onInpaintLayerChange={onInpaintLayerChange}
            onInpaintLayerEditEnd={onInpaintLayerEditEnd}
            onInpaintLayerEditStart={onInpaintLayerEditStart}
            onInpaintResultColorPick={onInpaintResultColorPick}
            onInpaintSelectionChange={onInpaintSelectionChange}
            onInpaintResultLayerChange={onInpaintResultLayerChange}
            onZoomToolDrag={handleZoomToolDrag}
            onStagePointerMove={onStagePointerMove}
            onStagePointerUp={onStagePointerUp}
            onStagePointerDown={() => {
              if (activeLayer === "overlay" && !temporaryPanActive) {
                onSelectBlock(null);
              }
            }}
            onBlockSelectionChange={onBlockSelectionChange}
            onBlockPointerDown={onBlockPointerDown}
            onBlockFontStyleCopy={onBlockFontStyleCopy}
            onBlockFontSizeChange={onBlockFontSizeChange}
            onBlockAutoFitDisable={onBlockAutoFitDisable}
            onSelectedBlockRangeChange={onSelectedBlockRangeChange}
            onBlockTextUpdate={onBlockTextUpdate}
            onBlockTextSelectionSplitDuplicate={onBlockTextSelectionSplitDuplicate}
            onBlockTextAlignChange={onBlockTextAlignChange}
            onBlockInlineEditActiveChange={setBlockInlineEditActive}
            onFavoriteFontPresetSelect={onFavoriteFontPresetSelect}
          />
        </div>
      ) : (
        <WorkspaceEmptyState
          lamaActionBusy={lamaActionBusy}
          lamaActionMessage={lamaActionMessage}
          lamaNoticePlatform={lamaNoticePlatform}
          lamaStatus={displayedLamaStatus}
          showLamaEmptyNotice={showLamaEmptyNotice}
          showTestPlatformSelector={FORCE_INCOMPLETE_LAMA_NOTICE}
          onDownloadLamaModel={onDownloadLamaModel}
          onPrepareLama={onPrepareLama}
          onRefreshLamaStatus={onRefreshLamaStatus}
          onSelectImportFiles={onSelectImportFiles}
          onSetLamaNoticePlatform={onSetLamaNoticePlatform}
        />
      )}
    </section>
  );
}
