import React from "react";
import type {
  ImageRect,
  ImportSourceKind,
  JobState,
  LamaRuntimeStatus,
  MangaPage,
  TranslationBlock
} from "../../../../shared/types";
import { INPAINT_TOOL_SHORTCUTS } from "../../lib/editorShortcuts";
import type { RecoverableFailure, RecoverableFailureId } from "../../hooks/useRecoverableFailures";
import { FORCE_INCOMPLETE_LAMA_NOTICE, type LamaNoticePlatform } from "../../lib/lamaRuntimeNotice";
import type { ActiveLayer, LayerOpacity, LayerVisibility } from "../../lib/layerState";
import type { ViewportSize } from "../../lib/overlayLayout";
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
  fitStageToWorkspace: () => void;
  handleZoomToolClick: (direction: "in" | "out") => void;
  imageRef: React.RefObject<HTMLImageElement | null>;
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
  statusWidgetOpen: boolean;
  statusWidgetTone: string;
  temporaryPanActive: boolean;
  workspacePanelRef: React.RefObject<HTMLElement | null>;
  zoomToolActive: boolean;
  onBlockPointerDown: (event: React.PointerEvent, block: TranslationBlock, mode: BlockDragMode) => void;
  onDownloadLamaModel: () => void | Promise<unknown>;
  onInpaintLayerChange: (dataUrl: string | undefined) => void;
  onInpaintResultLayerChange: (dataUrl: string | undefined) => void;
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
  onZoomInStage: () => void;
  onZoomOutStage: () => void;
};

export function WorkspacePanel({
  activeLayer,
  displayedLamaStatus,
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
  statusWidgetOpen,
  statusWidgetTone,
  temporaryPanActive,
  workspacePanelRef,
  zoomToolActive,
  onBlockPointerDown,
  onDownloadLamaModel,
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
  onZoomInStage,
  onZoomOutStage
}: WorkspacePanelProps): React.JSX.Element {
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

  return (
    <section
      ref={workspacePanelRef}
      className="workspace relative grid place-items-center outline-none"
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
          rangeShortcut={INPAINT_TOOL_SHORTCUTS.select ?? "T"}
          rangeToolActive={rangeToolActive}
          selectedPageEditLocked={selectedPageEditLocked}
          zoomToolActive={zoomToolActive}
          onSelectPointerTool={onSelectPointerTool}
          onSelectRangeTool={onSelectRangeTool}
          onSelectZoomTool={onSelectZoomTool}
        />
      ) : null}
      <NotificationDock
        inpaintNotice={selectedPageInpaintNotice}
        recoverableFailures={recoverableFailures}
        onDismissRecoverableFailure={onDismissRecoverableFailure}
        onRetryRecoverableFailure={onRetryRecoverableFailure}
        statusToastLine={statusToastLine}
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
        <div className="workspace-pane w-full max-w-[1040px]">
          <ImageStage
            page={selectedPage}
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
