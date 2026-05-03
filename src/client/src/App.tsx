import React, { useCallback, useMemo, useRef, useState } from "react";
import { AppFileInputs } from "./components/AppFileInputs";
import { AppModals } from "./components/AppModals";
import { ContextBar } from "./components/ContextBar";
import { EditorPanel } from "./components/EditorPanel";
import { LayerPanel } from "./components/layers/LayerPanel";
import { LayerToolPanel } from "./components/tools/LayerToolPanel";
import { PageList } from "./components/PageList";
import { WorkspacePanel } from "./components/workspace/WorkspacePanel";
import { useAnalysisJob } from "./hooks/useAnalysisJob";
import { useChapterSession } from "./hooks/useChapterSession";
import { useGlobalUndoHistory } from "./hooks/useGlobalUndoHistory";
import { useInpaintActions } from "./hooks/useInpaintActions";
import { useLibraryActions } from "./hooks/useLibraryActions";
import { usePageRendering } from "./hooks/usePageRendering";
import { useRecoverableFailures, type RecoverableFailureId } from "./hooks/useRecoverableFailures";
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

  const selectedPage = useMemo(
    () => currentChapter?.pages.find((page) => page.id === selectedPageId) ?? currentChapter?.pages[0] ?? null,
    [currentChapter?.pages, selectedPageId]
  );
  const currentChapterId = currentChapter?.id ?? null;
  const selectedPageCurrentId = selectedPage?.id ?? null;
  const selectedBlock = selectedPage?.blocks.find((block) => block.id === selectedBlockId) ?? null;
  const selectedPageEditLocked = Boolean(jobActive && selectedPage && selectedPage.analysisStatus !== "completed");
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
  const saveStatusTone = saveFlash ? "saved" : dirty ? "unsaved" : "synced";
  const saveStatusLabel = saveFlash ? "저장 완료" : dirty ? "저장되지 않은 변경 있음" : "최신 상태";
  const statusWidgetTone = `${jobState.status} ${recoverableFailures.length ? "failed" : saveStatusTone}`;
  const modalOpen = Boolean(importPreview || renameTarget || settingsOpen);
  const undoShortcutPlatform = useMemo(() => (typeof navigator === "undefined" ? "" : navigator.platform), []);
  const selectedPageInpaintNotice =
    selectedPage?.inpaintStatus === "running"
      ? { tone: "running", title: "인페인트 중", message: selectedPage.name }
      : selectedPage?.inpaintStatus === "failed"
        ? {
            tone: "failed",
            title: "인페인트 실패",
            message: `${selectedPage.name} - 마스크와 레이어 상태는 유지됨`,
            actionLabel: "다시 실행",
            onAction: rerunInpaintWithCurrentMask
          }
        : null;
  const statusIndicatorLabel = jobActive ? jobState.progressText : saveStatusLabel;
  const overlayBackgroundOpacity = selectedPage?.blocks[0]?.opacity ?? 1;

  React.useEffect(() => {
    void refreshLibrary();
  }, [refreshLibrary]);

  const retryRecoverableFailure = useCallback(async (id: RecoverableFailureId) => {
    try {
      let shouldClearAfterRetry = true;
      if (id === "chapter-save") {
        await saveNow();
      } else if (id === "inpaint-mask-save") {
        await flushInpaintMaskSave();
        shouldClearAfterRetry = false;
      } else if (id === "inpaint-result-save") {
        await flushInpaintResultSave();
        shouldClearAfterRetry = false;
      } else if (id === "analysis-run") {
        await retryLastAnalysis();
        shouldClearAfterRetry = false;
      } else if (id === "analysis-sync") {
        const chapterId = currentChapterRef.current?.id;
        if (chapterId) {
          mergeLiveChapter(await window.mangaApi.openChapter(chapterId));
        }
        await refreshLibrary();
      } else if (id === "inpaint-run") {
        await rerunInpaintWithCurrentMask();
        shouldClearAfterRetry = false;
      }
      if (shouldClearAfterRetry) {
        clearRecoverableFailure(id);
      }
    } catch (error) {
      console.error(error);
      pushStatus(error instanceof Error ? error.message : "재시도에 실패했습니다.");
    }
  }, [
    clearRecoverableFailure,
    currentChapterRef,
    flushInpaintMaskSave,
    flushInpaintResultSave,
    mergeLiveChapter,
    pushStatus,
    refreshLibrary,
    rerunInpaintWithCurrentMask,
    retryLastAnalysis,
    saveNow
  ]);

  const togglePageProgress = useCallback(
    (pageId: string) => {
      if (!currentChapter) {
        return;
      }
      updateCurrentChapter(pageId, (current) => ({
        ...current,
        pages: current.pages.map((page) =>
          page.id !== pageId
            ? page
            : {
                ...page,
                updatedAt: new Date().toISOString(),
                progressCompleted: !page.progressCompleted
              }
        )
      }));
    },
    [currentChapter, updateCurrentChapter]
  );

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
    <LayerToolPanel
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
      onClearInpaintMask={() => updateSelectedPageInpaintMask(undefined)}
      onClearInpaintResult={() => updateSelectedPageInpaintResult(undefined)}
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
      onInpaintSelectionClear={() => setInpaintSelectionRect(null)}
      onRerunInpaintForSelection={rerunInpaintForSelection}
      onRerunInpaintWithCurrentMask={rerunInpaintWithCurrentMask}
      onSelectFontPreset={selectFontPreset}
      onSelectInpaintPsdFile={selectInpaintPsdFile}
      onSelectInpaintResultEditTool={selectInpaintResultEditTool}
      onSelectSharedInpaintTool={selectSharedInpaintTool}
    />
  );

  return (
    <main className={currentChapter ? "app-shell grid h-screen bg-canvas" : "app-shell no-left-rail grid h-screen bg-canvas"}>
      <AppFileInputs
        batchImportInputRef={batchImportInputRef}
        folderImportInputRef={folderImportInputRef}
        imageImportInputRef={imageImportInputRef}
        inpaintPsdInputRef={inpaintPsdInputRef}
        zipImportInputRef={zipImportInputRef}
        onImportInputChange={handleImportInputChange}
        onInpaintPsdInputChange={handleInpaintPsdInputChange}
      />
      <ContextBar
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

      <aside className="sidebar flex min-h-0 flex-col gap-3 overflow-hidden">
        <PageList
          pages={currentChapter?.pages ?? []}
          selectedPageId={selectedPage?.id ?? null}
          jobActive={jobActive}
          onSelect={selectPageForReading}
          onRetranslate={(pageId) => void retranslatePage(pageId)}
          onRemove={(pageId) => void removePage(pageId)}
          onToggleProgress={togglePageProgress}
          onReorder={(sourcePageId, targetPageId) => {
            void reorderPages(sourcePageId, targetPageId);
          }}
        />
      </aside>

      <WorkspacePanel
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

      <aside className="layer-tools-rail flex min-h-0 flex-col gap-3 overflow-hidden">
        {layerToolPanel}
      </aside>

      <aside className="right-rail flex min-h-0 flex-col gap-3 overflow-hidden">
        <LayerPanel
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
        />

        <EditorPanel
          block={selectedBlock}
          fontPresetName={selectedFontPreset?.name}
          disabled={selectedPageEditLocked || inpaintBusy || !selectedPage}
          onUpdate={updateSelectedBlock}
          onCreate={createEmptyBlock}
          onDelete={deleteSelectedBlock}
          onDuplicate={duplicateSelectedBlock}
          onApplyInpaint={() => void applyInpaintSelectedBlock()}
          onApplyBatchInpaint={() => void applyInpaintAllBlocks()}
          batchInpaintDisabled={selectedPageEditLocked || inpaintBusy || !selectedPage || selectedPage.blocks.length === 0}
        />
      </aside>

      <AppModals
        importBusy={importBusy}
        importPreview={importPreview}
        jobActive={jobActive}
        library={library}
        renameBusy={renameBusy}
        renameTarget={renameTarget}
        settings={settings}
        settingsBusy={settingsBusy}
        settingsOpen={settingsOpen}
        onCancelImport={() => setImportPreview(null)}
        onCancelRename={() => setRenameTarget(null)}
        onCancelSettings={() => setSettingsOpen(false)}
        onDeleteRenameTarget={deleteRenameTarget}
        onResetSettings={resetSettings}
        onSubmitImport={submitImport}
        onSubmitRename={submitRename}
        onSubmitSettings={submitSettings}
      />
    </main>
  );
}
