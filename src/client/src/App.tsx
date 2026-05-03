import React, { useCallback, useMemo, useRef, useState } from "react";
import { AppContextBarSlot } from "./components/AppContextBarSlot";
import { AppFileInputs } from "./components/AppFileInputs";
import { AppLayerToolPanel } from "./components/AppLayerToolPanel";
import { AppLayout } from "./components/AppLayout";
import { AppModalsSlot } from "./components/AppModalsSlot";
import { AppPageListSlot } from "./components/AppPageListSlot";
import { AppRightRail } from "./components/AppRightRail";
import { AppWorkspaceSlot } from "./components/AppWorkspaceSlot";
import { useAnalysisJob } from "./hooks/useAnalysisJob";
import { useAppActions } from "./hooks/useAppActions";
import { useAppStatusState } from "./hooks/useAppStatusState";
import { useAppWorkspaceState } from "./hooks/useAppWorkspaceState";
import { useChapterSession } from "./hooks/useChapterSession";
import { useGlobalUndoHistory } from "./hooks/useGlobalUndoHistory";
import { useInpaintActions } from "./hooks/useInpaintActions";
import { useLibraryActions } from "./hooks/useLibraryActions";
import { usePageRendering } from "./hooks/usePageRendering";
import { useRecoverableFailures } from "./hooks/useRecoverableFailures";
import { useRuntimeSettings } from "./hooks/useRuntimeSettings";
import { useStageInteraction } from "./hooks/useStageInteraction";
import { useStatusFeedback } from "./hooks/useStatusFeedback";
import { useTranslationEditing } from "./hooks/useTranslationEditing";
import { useWorkspaceShortcuts } from "./hooks/useWorkspaceShortcuts";
import { useWorkspaceToolState } from "./hooks/useWorkspaceToolState";
import "./styles.css";

export default function App(): React.JSX.Element {
  const [libraryWidgetOpen, setLibraryWidgetOpen] = useState(false);
  const workspacePanelRef = useRef<HTMLElement | null>(null);
  const libraryAnchorRef = useRef<HTMLDivElement | null>(null);
  const clearPendingInpaintSavesRef = useRef<() => void>(() => undefined);
  const {
    activeLayer,
    focusModeEnabled,
    layerOpacity,
    layerVisibility,
    overlayOpacityEditMode,
    rangeToolActive,
    registerInpaintToolSetters,
    selectInpaintResultEditTool,
    selectLayer,
    selectPointerTool,
    selectRangeTool,
    selectSharedInpaintTool,
    selectZoomTool,
    setFocusModeEnabled,
    setLayerOpacity,
    setLayerVisibility,
    setOverlayOpacityEditMode,
    setRangeToolActive,
    setTemporaryPanActive,
    setZoomToolActive,
    showInpaintLayers,
    showOverlayLayer,
    stageLayerOpacity,
    stageLayerVisibility,
    temporaryPanActive,
    temporaryPanHeldRef,
    temporaryPanShortcutEnabled,
    zoomToolActive
  } = useWorkspaceToolState();
  const {
    clearUndoStacks,
    consumeGlobalUndoEntry,
    recordGlobalUndoEntry,
    registerInpaintUndoClearer,
    registerTranslationUndoClearer,
    resolveGlobalUndoActions,
    undoVersion
  } = useGlobalUndoHistory();
  const clearPendingChapterTimers = useCallback(() => {
    clearPendingInpaintSavesRef.current();
  }, []);
  const {
    appendStatusLine,
    pushStatus,
    resetStatusLog,
    saveFlash,
    signalSaveComplete,
    statusLines,
    statusToastLine,
    statusWidgetOpen,
    setStatusWidgetOpen
  } = useStatusFeedback();
  const {
    clearRecoverableFailure,
    recoverableFailures,
    reportRecoverableFailure
  } = useRecoverableFailures();
  const {
    applyChapter,
    clearCurrentChapter,
    currentChapter,
    currentChapterRef,
    dirty,
    editingFontPresetId,
    editingFontPresetIdRef,
    markDirty,
    mergeLiveChapter,
    openChapter,
    saveNow,
    selectPageForReading,
    selectedBlockId,
    selectedBlockIdRef,
    selectedPageId,
    selectedPageIdRef,
    setCurrentChapter,
    setEditingFontPresetId,
    setSelectedBlockId,
    setSelectedPageId,
    updateCurrentChapter
  } = useChapterSession({
    clearPendingChapterTimers,
    clearUndoStacks,
    pushStatus,
    reportRecoverableFailure,
    signalSaveComplete
  });
  const {
    displayedLamaStatus,
    downloadLamaModelFromEmptyState,
    lamaActionBusy,
    lamaActionMessage,
    lamaNoticePlatform,
    openSettings,
    prepareLamaFromEmptyState,
    refreshLamaStatus,
    resetSettings,
    settings,
    settingsBusy,
    settingsOpen,
    setLamaNoticePlatform,
    setSettingsOpen,
    showLamaEmptyNotice,
    submitSettings,
    systemFonts
  } = useRuntimeSettings({ pushStatus });
  const {
    batchImportInputRef,
    deleteRenameTarget,
    folderImportInputRef,
    handleImportInputChange,
    imageImportInputRef,
    importBusy,
    importPreview,
    library,
    refreshLibrary,
    removePage,
    renameBusy,
    renameChapter,
    renameTarget,
    renameWork,
    reorderChapters,
    reorderPages,
    selectImportFiles,
    setImportPreview,
    setRenameTarget,
    submitImport,
    submitRename,
    zipImportInputRef
  } = useLibraryActions({
    applyChapter,
    clearCurrentChapter,
    currentChapter,
    dirty,
    openChapter,
    pushStatus,
    saveNow,
    setSelectedPageId
  });
  const {
    renderAllPages,
    renderBusy,
    renderProgress,
    renderSelectedPage
  } = usePageRendering({
    currentChapterRef,
    dirty,
    pushStatus,
    saveNow,
    selectedPageIdRef,
    signalSaveComplete
  });
  const {
    jobActive,
    jobState,
    progressSnapshot,
    retryLastAnalysis,
    retranslatePage,
    runAnalysis,
    showProgressBar
  } = useAnalysisJob({
    appendStatusLine,
    applyChapter,
    clearRecoverableFailure,
    currentChapter,
    currentChapterRef,
    mergeLiveChapter,
    pushStatus,
    refreshLibrary,
    reportRecoverableFailure,
    resetStatusLog,
    saveNow,
    setCurrentChapter
  });

  const {
    currentChapterId,
    overlayBackgroundOpacity,
    selectedBlock,
    selectedPage,
    selectedPageCurrentId,
    selectedPageEditLocked
  } = useAppWorkspaceState({
    currentChapter,
    jobActive,
    selectedBlockId,
    selectedPageId
  });
  const {
    applyInpaintAllBlocks,
    applyInpaintAllPages,
    applyInpaintSelectedBlock,
    canUndoInpaintMask,
    canUndoInpaintResult,
    clearInpaintUndoStacks,
    clearPendingInpaintSaves,
    clearSelectedInpaintSelection,
    downloadLastImportedInpaintPsd,
    exportSelectedPageInpaintPsd,
    fillSelectedInpaintSelection,
    flushInpaintMaskSave,
    flushInpaintResultSave,
    handleInpaintPsdInputChange,
    inpaintBrushSize,
    inpaintBusy,
    inpaintPsdBusy,
    inpaintPsdInputRef,
    inpaintResultBrushColor,
    inpaintResultBrushHardness,
    inpaintResultBrushSize,
    inpaintResultTool,
    inpaintResultToolStrength,
    inpaintSelectionRect,
    inpaintTool,
    lastImportedInpaintPsdAt,
    lastImportedInpaintPsdLabel,
    rerunInpaintForSelection,
    rerunInpaintWithCurrentMask,
    selectInpaintPsdFile,
    setInpaintBrushSize,
    setInpaintResultBrushColor,
    setInpaintResultBrushHardness,
    setInpaintResultBrushSize,
    setInpaintResultTool,
    setInpaintResultToolStrength,
    setInpaintSelectionRect,
    setInpaintTool,
    undoPageInpaint,
    undoPageInpaintResult,
    updateSelectedPageInpaintMask,
    updateSelectedPageInpaintResult
  } = useInpaintActions({
    activeLayer,
    applyChapter,
    clearRecoverableFailure,
    consumeGlobalUndoEntry,
    currentChapter,
    currentChapterId,
    currentChapterRef,
    dirty,
    mergeLiveChapter,
    pushStatus,
    rangeToolActive,
    recordGlobalUndoEntry,
    refreshLibrary,
    reportRecoverableFailure,
    saveNow,
    selectedBlock,
    selectedPage,
    selectedPageCurrentId,
    selectedPageEditLocked,
    selectedPageIdRef,
    setCurrentChapter,
    showInpaintLayers,
    signalSaveComplete
  });
  registerInpaintToolSetters({ setInpaintResultTool, setInpaintTool });
  registerInpaintUndoClearer(clearInpaintUndoStacks);
  clearPendingInpaintSavesRef.current = clearPendingInpaintSaves;
  const {
    canUndoTranslation,
    clearSelectedBlockFontPreset,
    clearTranslationUndoStack,
    copySelectedBlockToClipboard,
    createEmptyBlock,
    createFontPresetFromSelectedBlock,
    deleteFontPreset,
    deleteSelectedBlock,
    duplicateSelectedBlock,
    fontControlValues,
    fontFamilyOptions,
    fontPresetName,
    fontPresets,
    pasteTranslationBlockFromClipboard,
    recordTranslationUndoSnapshot,
    renderFontPresetLinkButton,
    selectFontPreset,
    selectedFontPreset,
    setFontPresetName,
    undoTranslationEdit,
    updateSelectedBlock,
    updateSelectedBlockFontSetting,
    updateSelectedPageBlockOpacity
  } = useTranslationEditing({
    consumeGlobalUndoEntry,
    currentChapter,
    currentChapterRef,
    editingFontPresetId,
    editingFontPresetIdRef,
    inpaintBusy,
    markDirty,
    pushStatus,
    recordGlobalUndoEntry,
    selectLayer,
    selectedBlock,
    selectedBlockIdRef,
    selectedPage,
    selectedPageEditLocked,
    selectedPageIdRef,
    setCurrentChapter,
    setEditingFontPresetId,
    setSelectedBlockId,
    setSelectedPageId,
    showOverlayLayer,
    systemFonts,
    undoVersion,
    updateCurrentChapter
  });
  registerTranslationUndoClearer(clearTranslationUndoStack);
  const {
    fitStageToWorkspace,
    handleZoomToolClick,
    imageRef,
    onBlockPointerDown,
    onStagePointerMove,
    onStagePointerUp,
    stageRef,
    stageSize,
    stageViewResetKey,
    stageViewScale,
    stageZoomLabel,
    showOriginalStageSize,
    zoomInStage,
    zoomOutStage
  } = useStageInteraction({
    activeLayer,
    currentChapter,
    recordTranslationUndoSnapshot,
    selectedPage,
    selectedPageEditLocked,
    setSelectedBlockId,
    updateCurrentChapter
  });
  const modalOpen = Boolean(importPreview || renameTarget || settingsOpen);
  const undoShortcutPlatform = useMemo(() => (typeof navigator === "undefined" ? "" : navigator.platform), []);
  const {
    selectedPageInpaintNotice,
    statusIndicatorLabel,
    statusWidgetTone
  } = useAppStatusState({
    dirty,
    jobActive,
    jobState,
    recoverableFailures,
    rerunInpaintWithCurrentMask,
    saveFlash,
    selectedPage
  });

  React.useEffect(() => {
    void refreshLibrary();
  }, [refreshLibrary]);

  const { retryRecoverableFailure, togglePageProgress } = useAppActions({
    clearRecoverableFailure,
    currentChapter,
    currentChapterRef,
    flushInpaintMaskSave,
    flushInpaintResultSave,
    mergeLiveChapter,
    pushStatus,
    refreshLibrary,
    rerunInpaintWithCurrentMask,
    retryLastAnalysis,
    saveNow,
    updateCurrentChapter
  });

  const globalUndoActions = useMemo(() => resolveGlobalUndoActions({
    canUndoInpaintMask,
    canUndoInpaintResult,
    canUndoTranslation,
    currentChapterId: currentChapter?.id,
    selectedPageEditLocked,
    undoPageInpaint,
    undoPageInpaintResult,
    undoTranslationEdit
  }), [
    canUndoInpaintMask,
    canUndoInpaintResult,
    canUndoTranslation,
    currentChapter?.id,
    resolveGlobalUndoActions,
    selectedPageEditLocked,
    undoPageInpaint,
    undoPageInpaintResult,
    undoTranslationEdit,
    undoVersion
  ]);

  React.useEffect(() => {
    if (!libraryWidgetOpen) {
      return;
    }

    const onPointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (target instanceof Node && libraryAnchorRef.current?.contains(target)) {
        return;
      }
      setLibraryWidgetOpen(false);
    };

    window.addEventListener("pointerdown", onPointerDown);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
    };
  }, [libraryWidgetOpen]);

  useWorkspaceShortcuts({
    activeLayer,
    clearSelectedInpaintSelection,
    copySelectedBlockToClipboard,
    currentChapterRef,
    deleteSelectedBlock,
    globalUndoActions,
    inpaintSelectionRect,
    layerVisibility,
    libraryWidgetOpen,
    modalOpen,
    pasteTranslationBlockFromClipboard,
    pushStatus,
    rangeToolActive,
    selectLayer,
    selectPageForReading,
    selectPointerTool,
    selectRangeTool,
    selectSharedInpaintTool,
    selectZoomTool,
    selectedBlockIdRef,
    selectedPageEditLocked,
    selectedPageIdRef,
    setLibraryWidgetOpen,
    setRangeToolActive,
    setTemporaryPanActive,
    setZoomToolActive,
    temporaryPanHeldRef,
    temporaryPanShortcutEnabled,
    undoShortcutPlatform,
    workspacePanelRef,
    zoomToolActive
  });

  const layerToolPanel = (
    <AppLayerToolPanel
      activeLayer={activeLayer}
      currentChapter={currentChapter}
      editingFontPresetId={editingFontPresetId}
      fontControlValues={fontControlValues}
      fontFamilyOptions={fontFamilyOptions}
      fontPresetName={fontPresetName}
      fontPresets={fontPresets}
      inpaintBrushSize={inpaintBrushSize}
      inpaintBusy={inpaintBusy}
      inpaintPsdBusy={inpaintPsdBusy}
      inpaintResultBrushColor={inpaintResultBrushColor}
      inpaintResultBrushHardness={inpaintResultBrushHardness}
      inpaintResultBrushSize={inpaintResultBrushSize}
      inpaintResultTool={inpaintResultTool}
      inpaintResultToolStrength={inpaintResultToolStrength}
      inpaintSelectionRect={inpaintSelectionRect}
      inpaintTool={inpaintTool}
      lastImportedInpaintPsdAt={lastImportedInpaintPsdAt}
      lastImportedInpaintPsdLabel={lastImportedInpaintPsdLabel}
      layerVisibility={layerVisibility}
      rangeToolActive={rangeToolActive}
      renderFontPresetLinkButton={renderFontPresetLinkButton}
      selectedBlock={selectedBlock}
      selectedPage={selectedPage}
      selectedPageEditLocked={selectedPageEditLocked}
      onClearInpaintMaskData={() => updateSelectedPageInpaintMask(undefined)}
      onClearInpaintResultData={() => updateSelectedPageInpaintResult(undefined)}
      onClearSelectedBlockFontPreset={clearSelectedBlockFontPreset}
      onCreateFontPreset={createFontPresetFromSelectedBlock}
      onDeleteFontPreset={deleteFontPreset}
      onDownloadLastImportedInpaintPsd={downloadLastImportedInpaintPsd}
      onExportInpaintPsd={exportSelectedPageInpaintPsd}
      onFillSelectedInpaintSelection={fillSelectedInpaintSelection}
      onFontPresetNameChange={setFontPresetName}
      onFontSettingChange={updateSelectedBlockFontSetting}
      onInpaintBrushSizeChange={setInpaintBrushSize}
      onInpaintResultBrushColorChange={setInpaintResultBrushColor}
      onInpaintResultBrushHardnessChange={setInpaintResultBrushHardness}
      onInpaintResultBrushSizeChange={setInpaintResultBrushSize}
      onInpaintResultToolStrengthChange={setInpaintResultToolStrength}
      onClearInpaintSelectionRect={() => setInpaintSelectionRect(null)}
      onRerunInpaintForSelection={rerunInpaintForSelection}
      onRerunInpaintWithCurrentMask={rerunInpaintWithCurrentMask}
      onSelectFontPreset={selectFontPreset}
      onSelectInpaintPsdFile={selectInpaintPsdFile}
      onSelectInpaintResultEditTool={selectInpaintResultEditTool}
      onSelectSharedInpaintTool={selectSharedInpaintTool}
    />
  );

  return (
    <AppLayout
      currentChapterPresent={Boolean(currentChapter)}
      fileInputs={
        <AppFileInputs
          batchImportInputRef={batchImportInputRef}
          folderImportInputRef={folderImportInputRef}
          imageImportInputRef={imageImportInputRef}
          inpaintPsdInputRef={inpaintPsdInputRef}
          zipImportInputRef={zipImportInputRef}
          onImportInputChange={handleImportInputChange}
          onInpaintPsdInputChange={handleInpaintPsdInputChange}
        />
      }
      contextBar={
        <AppContextBarSlot
          currentChapter={currentChapter}
          currentChapterId={currentChapterId}
          jobActive={jobActive}
          jobState={jobState}
          library={library}
          libraryAnchorRef={libraryAnchorRef}
          libraryWidgetOpen={libraryWidgetOpen}
          progressSnapshot={progressSnapshot}
          renderBusy={renderBusy}
          renderProgress={renderProgress}
          selectedPage={selectedPage}
          settingsBusy={settingsBusy}
          settingsOpen={settingsOpen}
          showProgressBar={showProgressBar}
          statusIndicatorLabel={statusIndicatorLabel}
          statusWidgetTone={statusWidgetTone}
          inpaintBusy={inpaintBusy}
          onApplyInpaintAllPages={applyInpaintAllPages}
          onOpenChapter={openChapter}
          onOpenSettings={openSettings}
          onRenameChapter={renameChapter}
          onRenameWork={renameWork}
          onRenderAllPages={renderAllPages}
          onRenderSelectedPage={renderSelectedPage}
          onReorderChapter={reorderChapters}
          onRunAnalysis={runAnalysis}
          onSelectImportFiles={selectImportFiles}
          onSetLibraryWidgetOpen={setLibraryWidgetOpen}
        />
      }
      pageList={
        <AppPageListSlot
          pages={currentChapter?.pages ?? []}
          selectedPageId={selectedPage?.id ?? null}
          jobActive={jobActive}
          onSelectPage={selectPageForReading}
          onRetranslatePage={retranslatePage}
          onRemovePage={removePage}
          onToggleProgress={togglePageProgress}
          onReorderPages={reorderPages}
        />
      }
      workspace={
        <AppWorkspaceSlot
          activeLayer={activeLayer}
          displayedLamaStatus={displayedLamaStatus}
          fitStageToWorkspace={fitStageToWorkspace}
          handleZoomToolClick={handleZoomToolClick}
          imageRef={imageRef}
          inpaintBrushSize={inpaintBrushSize}
          inpaintBusy={inpaintBusy}
          inpaintResultBrushColor={inpaintResultBrushColor}
          inpaintResultBrushHardness={inpaintResultBrushHardness}
          inpaintResultBrushSize={inpaintResultBrushSize}
          inpaintResultTool={inpaintResultTool}
          inpaintResultToolStrength={inpaintResultToolStrength}
          inpaintSelectionRect={inpaintSelectionRect}
          inpaintTool={inpaintTool}
          jobState={jobState}
          lamaActionBusy={lamaActionBusy}
          lamaActionMessage={lamaActionMessage}
          lamaNoticePlatform={lamaNoticePlatform}
          layerVisibility={layerVisibility}
          rangeToolActive={rangeToolActive}
          recoverableFailures={recoverableFailures}
          selectedBlockId={selectedBlockId}
          selectedPage={selectedPage}
          selectedPageEditLocked={selectedPageEditLocked}
          selectedPageInpaintNotice={selectedPageInpaintNotice}
          showLamaEmptyNotice={showLamaEmptyNotice}
          showOriginalStageSize={showOriginalStageSize}
          stageLayerOpacity={stageLayerOpacity}
          stageLayerVisibility={stageLayerVisibility}
          stageRef={stageRef}
          stageSize={stageSize}
          stageViewResetKey={stageViewResetKey}
          stageViewScale={stageViewScale}
          stageZoomLabel={stageZoomLabel}
          statusLines={statusLines}
          statusToastLine={statusToastLine}
          statusWidgetOpen={statusWidgetOpen}
          statusWidgetTone={statusWidgetTone}
          temporaryPanActive={temporaryPanActive}
          workspacePanelRef={workspacePanelRef}
          zoomToolActive={zoomToolActive}
          onBlockPointerDown={onBlockPointerDown}
          onDownloadLamaModel={downloadLamaModelFromEmptyState}
          onInpaintLayerChange={updateSelectedPageInpaintMask}
          onInpaintResultLayerChange={updateSelectedPageInpaintResult}
          onInpaintSelectionChange={setInpaintSelectionRect}
          onPrepareLama={prepareLamaFromEmptyState}
          onRefreshLamaStatus={refreshLamaStatus}
          onDismissRecoverableFailure={clearRecoverableFailure}
          onRetryRecoverableFailure={retryRecoverableFailure}
          onSelectBlock={setSelectedBlockId}
          onSelectImportFiles={selectImportFiles}
          onSelectPointerTool={selectPointerTool}
          onSelectRangeTool={selectRangeTool}
          onSelectZoomTool={selectZoomTool}
          onSetLamaNoticePlatform={setLamaNoticePlatform}
          onSetStatusWidgetOpen={setStatusWidgetOpen}
          onStagePointerMove={onStagePointerMove}
          onStagePointerUp={onStagePointerUp}
          onZoomInStage={zoomInStage}
          onZoomOutStage={zoomOutStage}
        />
      }
      layerTools={layerToolPanel}
      rightRail={
        <AppRightRail
          activeLayer={activeLayer}
          focusModeEnabled={focusModeEnabled}
          layerOpacity={layerOpacity}
          layerVisibility={layerVisibility}
          overlayBackgroundOpacity={overlayBackgroundOpacity}
          overlayOpacityEditMode={overlayOpacityEditMode}
          onFocusModeChange={setFocusModeEnabled}
          onLayerOpacityChange={setLayerOpacity}
          onLayerVisibilityChange={setLayerVisibility}
          onOverlayBlockOpacityChange={updateSelectedPageBlockOpacity}
          onOverlayOpacityEditModeChange={setOverlayOpacityEditMode}
          onSelectLayer={selectLayer}
          block={selectedBlock}
          fontPresetName={selectedFontPreset?.name}
          inpaintBusy={inpaintBusy}
          selectedPage={selectedPage}
          selectedPageEditLocked={selectedPageEditLocked}
          onUpdate={updateSelectedBlock}
          onCreate={createEmptyBlock}
          onDelete={deleteSelectedBlock}
          onDuplicate={duplicateSelectedBlock}
          onApplyInpaint={applyInpaintSelectedBlock}
          onApplyBatchInpaint={applyInpaintAllBlocks}
        />
      }
      modals={
        <AppModalsSlot
          importBusy={importBusy}
          importPreview={importPreview}
          jobActive={jobActive}
          library={library}
          renameBusy={renameBusy}
          renameTarget={renameTarget}
          settings={settings}
          settingsBusy={settingsBusy}
          settingsOpen={settingsOpen}
          onCloseImport={() => setImportPreview(null)}
          onCloseRename={() => setRenameTarget(null)}
          onCloseSettings={() => setSettingsOpen(false)}
          onDeleteRenameTarget={deleteRenameTarget}
          onResetSettings={resetSettings}
          onSubmitImport={submitImport}
          onSubmitRename={submitRename}
          onSubmitSettings={submitSettings}
        />
      }
    />
  );
}
