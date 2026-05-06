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
import { isMacLikePlatform } from "../../lib/globalUndo";
import type { FontWeightAvailability, ViewportSize } from "../../lib/overlayLayout";
import { ImageStage } from "../ImageStage";
import type { InpaintTool } from "../InpaintLayerCanvas";
import type { InpaintResultTool } from "../InpaintResultCanvas";
import { WorkspaceEmptyState } from "./WorkspaceEmptyState";
import {
  NotificationDock,
  type PageInpaintNotice,
  StageToolOverlay,
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
  handleZoomToolClick: (direction: "in" | "out") => void;
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
  rangeToolActive: boolean;
  recoverableFailures: RecoverableFailure[];
  selectedBlockId: string | null;
  selectedPage: MangaPage | null;
  selectedPageEditLocked: boolean;
  selectedPageInpaintNotice: PageInpaintNotice | null;
  showLamaEmptyNotice: boolean;
  showOriginalStageSize: () => void;
  stageLayerOpacity: LayerOpacity;
  stageLayerVisibility: LayerVisibility;
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
  onInpaintResultLayerChange: (dataUrl: string | undefined, options?: InpaintLayerChangeOptions) => void;
  onInpaintSelectionChange: (rect: ImageRect | null) => void;
  onPrepareLama: () => void | Promise<unknown>;
  onRefreshLamaStatus: () => void | Promise<unknown>;
  onDismissRecoverableFailure: (id: RecoverableFailureId) => void;
  onRetryRecoverableFailure: (id: RecoverableFailureId) => void | Promise<void>;
  onSelectBlock: React.Dispatch<React.SetStateAction<string | null>>;
  onSelectImportFiles: (mode: ImportSourceKind) => void;
  onSelectPointerTool: () => void;
  onSelectRangeTool: () => void;
  onSelectZoomTool: () => void;
  onSetLamaNoticePlatform: (platform: LamaNoticePlatform) => void;
  onSetStatusWidgetOpen: React.Dispatch<React.SetStateAction<boolean>>;
  onStagePointerMove: (event: React.PointerEvent) => void;
  onStagePointerUp: (event: React.PointerEvent) => void;
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
  handleZoomToolClick,
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
  layerVisibility,
  rangeToolActive,
  recoverableFailures,
  selectedBlockId,
  selectedPage,
  selectedPageEditLocked,
  selectedPageInpaintNotice,
  showLamaEmptyNotice,
  showOriginalStageSize,
  stageLayerOpacity,
  stageLayerVisibility,
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
  onInpaintResultLayerChange,
  onInpaintSelectionChange,
  onPrepareLama,
  onRefreshLamaStatus,
  onDismissRecoverableFailure,
  onRetryRecoverableFailure,
  onSelectBlock,
  onSelectImportFiles,
  onSelectPointerTool,
  onSelectRangeTool,
  onSelectZoomTool,
  onSetLamaNoticePlatform,
  onSetStatusWidgetOpen,
  onStagePointerMove,
  onStagePointerUp,
  onSelectedBlockRangeChange,
  onBlockTextAlignChange,
  onZoomInStage,
  onZoomOutStage
}: WorkspacePanelProps): React.JSX.Element {
  const [blockInlineEditActive, setBlockInlineEditActive] = React.useState(false);
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
  const selectedBlock = selectedPage?.blocks.find((block) => block.id === selectedBlockId) ?? null;
  const blockInlineEditShortcutVisible =
    activeLayer === "overlay" &&
    !temporaryPanActive &&
    layerVisibility.overlay &&
    Boolean(selectedBlock && selectedBlock.renderDirection !== "hidden");

  return (
    <section
      ref={workspacePanelRef}
      className={`workspace relative grid place-items-center outline-none${selectedPage ? " has-stage-find-replace" : ""}`}
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
          rangeShortcut={INPAINT_TOOL_SHORTCUTS.select ?? "T"}
          rangeToolActive={rangeToolActive}
          selectedPageEditLocked={selectedPageEditLocked}
          zoomToolActive={zoomToolActive}
          onSelectPointerTool={onSelectPointerTool}
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
            layerVisibility={stageLayerVisibility}
            layerOpacity={stageLayerOpacity}
            activeLayer={activeLayer}
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
            onInpaintLayerChange={onInpaintLayerChange}
            onInpaintSelectionChange={onInpaintSelectionChange}
            onInpaintResultLayerChange={onInpaintResultLayerChange}
            onZoomToolClick={handleZoomToolClick}
            onStagePointerMove={onStagePointerMove}
            onStagePointerUp={onStagePointerUp}
            onStagePointerDown={() => {
              if (activeLayer === "overlay" && !temporaryPanActive) {
                onSelectBlock(null);
              }
            }}
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
