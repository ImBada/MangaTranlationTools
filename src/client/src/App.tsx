import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { TranslationBlock } from "../../shared/types";
import { AppContextBarSlot } from "./components/AppContextBarSlot";
import { AppFileInputs } from "./components/AppFileInputs";
import { AppLayerToolPanel } from "./components/AppLayerToolPanel";
import { AppLayout } from "./components/AppLayout";
import { AppModalsSlot } from "./components/AppModalsSlot";
import { AppPageListSlot } from "./components/AppPageListSlot";
import { AppRightRail } from "./components/AppRightRail";
import { AppWorkspaceSlot } from "./components/AppWorkspaceSlot";
import { FindReplaceModal } from "./components/FindReplaceModal";
import { useAnalysisJob } from "./hooks/useAnalysisJob";
import { useAppActions } from "./hooks/useAppActions";
import { useAppStatusState } from "./hooks/useAppStatusState";
import { useAppWorkspaceState } from "./hooks/useAppWorkspaceState";
import { useChapterSession } from "./hooks/useChapterSession";
import { useGlobalUndoHistory } from "./hooks/useGlobalUndoHistory";
import { useFindReplaceEditing } from "./hooks/useFindReplaceEditing";
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
import { resolveSelectedTranslationBlocks, resolveShiftSelectedTranslationBlockIds } from "./lib/blockSelection";
import type { ActiveLayer } from "./lib/layerState";
import "./styles.css";

export default function App(): React.JSX.Element {
  const [libraryWidgetOpen, setLibraryWidgetOpen] = useState(false);
  const workspacePanelRef = useRef<HTMLElement | null>(null);
  const libraryAnchorRef = useRef<HTMLDivElement | null>(null);
  const clearPendingInpaintSavesRef = useRef<() => void>(() => undefined);
  const restoreOverlaySelectionFrameRef = useRef<number | null>(null);
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
    statusToastTone,
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
  const [selectedBlockIds, setSelectedBlockIds] = useState<string[]>([]);
  const setSingleSelectedBlockId = useCallback<React.Dispatch<React.SetStateAction<string | null>>>((action) => {
    setSelectedBlockIds([]);
    setSelectedBlockId(action);
  }, [setSelectedBlockId]);
  const setSelectedBlockGroupIds = useCallback((blockIds: string[]) => {
    const uniqueBlockIds = Array.from(new Set(blockIds));
    setSelectedBlockIds(uniqueBlockIds.length > 1 ? uniqueBlockIds : []);
    setSelectedBlockId(uniqueBlockIds[0] ?? null);
  }, [setSelectedBlockId]);
  const updateBlockSelectionWithShiftClick = useCallback((blockId: string) => {
    const nextBlockIds = resolveShiftSelectedTranslationBlockIds(selectedBlockId, selectedBlockIds, blockId);
    if (!nextBlockIds) {
      return false;
    }
    setSelectedBlockGroupIds(nextBlockIds);
    return true;
  }, [selectedBlockId, selectedBlockIds, setSelectedBlockGroupIds]);
  const lastOverlaySelectedBlockIdRef = useRef<string | null>(selectedBlockId);

  useEffect(() => {
    if (activeLayer === "overlay") {
      lastOverlaySelectedBlockIdRef.current = selectedBlockId;
    }
  }, [activeLayer, selectedBlockId]);

  const selectLayerWithBlockSelection = useCallback((nextLayer: ActiveLayer) => {
    if (restoreOverlaySelectionFrameRef.current !== null) {
      window.cancelAnimationFrame(restoreOverlaySelectionFrameRef.current);
      restoreOverlaySelectionFrameRef.current = null;
    }

    if (nextLayer === "overlay") {
      const restoredBlockId = lastOverlaySelectedBlockIdRef.current;
      const currentPage =
        currentChapterRef.current?.pages.find((page) => page.id === selectedPageIdRef.current) ??
        currentChapterRef.current?.pages[0] ??
        null;
      selectLayer(nextLayer);
      restoreOverlaySelectionFrameRef.current = window.requestAnimationFrame(() => {
        restoreOverlaySelectionFrameRef.current = null;
        setSelectedBlockIds([]);
        setSelectedBlockId(
          restoredBlockId && currentPage?.blocks.some((block) => block.id === restoredBlockId)
            ? restoredBlockId
            : null
        );
      });
      return;
    }

    if (activeLayer === "overlay") {
      lastOverlaySelectedBlockIdRef.current = selectedBlockIdRef.current;
    }
    selectLayer(nextLayer);
    setSelectedBlockIds([]);
    setSelectedBlockId(null);
  }, [activeLayer, currentChapterRef, selectLayer, selectedBlockIdRef, selectedPageIdRef, setSelectedBlockId]);

  useEffect(() => () => {
    if (restoreOverlaySelectionFrameRef.current !== null) {
      window.cancelAnimationFrame(restoreOverlaySelectionFrameRef.current);
    }
  }, []);
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
    fontWeightAvailability: systemFonts,
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
  const selectedBlocks = useMemo(
    () => resolveSelectedTranslationBlocks(selectedPage, selectedBlockIds),
    [selectedBlockIds, selectedPage]
  );
  const multiSelectedBlockCount = selectedBlocks.length > 1 ? selectedBlocks.length : 0;

  useEffect(() => {
    if (selectedBlockIds.length === 0) {
      return;
    }
    if (!selectedBlockId || !selectedPage) {
      setSelectedBlockIds([]);
      return;
    }

    const validBlockIds = new Set(selectedPage.blocks.map((block) => block.id));
    const nextBlockIds = selectedBlockIds.filter((blockId) => validBlockIds.has(blockId));
    if (nextBlockIds.length < 2 || !nextBlockIds.includes(selectedBlockId)) {
      setSelectedBlockIds([]);
      return;
    }
    if (nextBlockIds.length !== selectedBlockIds.length) {
      setSelectedBlockIds(nextBlockIds);
    }
  }, [selectedBlockId, selectedBlockIds, selectedPage]);
  const {
    applyInpaintAllBlocks,
    applyInpaintAllPages,
    applyInpaintSelectedBlock,
    beginInpaintLayerInteraction,
    canUndoInpaintMask,
    canUndoInpaintResult,
    clearInpaintUndoStacks,
    clearPendingInpaintSaves,
    clearSelectedInpaintSelection,
    downloadLastImportedInpaintPsd,
    endInpaintLayerInteraction,
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
    fontWeightAvailability: systemFonts,
    mergeLiveChapter,
    pushStatus,
    rangeToolActive,
    recordGlobalUndoEntry,
    refreshLibrary,
    reportRecoverableFailure,
    saveNow,
    selectedBlock,
    selectedBlocks,
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
    activeFontSizePresetId,
    canUndoTranslation,
    clearSelectedBlockFontPreset,
    clearTranslationUndoStack,
    copySelectedBlockFontStyleToClipboard,
    copySelectedBlockToClipboard,
    createEmptyBlock,
    createFontPresetFromSelectedBlock,
    createFontPresetListBackup,
    createFontSizePresetFromCurrentFontSize,
    deleteFontPresetBackup,
    deleteFontPreset,
    deleteFontSizePreset,
    deleteSelectedBlock,
    duplicateBlock,
    duplicateBlockTextSelection,
    duplicateSelectedBlock,
    favoriteFontPresetIds,
    favoriteFontPresets,
    fontControlValues,
    fontFamilyOptions,
    fontPresetName,
    fontPresets,
    fontSizePresets,
    listFontPresetBackups,
    pasteSelectedBlockFontStyleFromClipboard,
    pasteTranslationBlockFromClipboard,
    recordTranslationUndoSnapshot,
    renderFontPresetLinkButton,
    renderFontPresetLinkGroupButton,
    renameFontPreset,
    restoreFontPresetListBackup,
    selectFontSizePreset,
    selectFontPreset,
    selectedFontPreset,
    setFontPresetName,
    toggleFavoriteFontPreset,
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
    selectedBlockIds,
    selectedBlockIdRef,
    selectedPage,
    selectedPageEditLocked,
    selectedPageIdRef,
    setCurrentChapter,
    setEditingFontPresetId,
    setSelectedBlockId: setSingleSelectedBlockId,
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
    onSelectedBlockRangeChange,
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
    duplicateBlock,
    recordTranslationUndoSnapshot,
    selectedPage,
    selectedPageEditLocked,
    setSelectedBlockId: setSingleSelectedBlockId,
    updateCurrentChapter
  });
  const updateInlineBlockText = useCallback((block: TranslationBlock, translatedText: string) => {
    if (!selectedPage || selectedPageEditLocked) {
      return;
    }

    recordTranslationUndoSnapshot("번역 텍스트 변경");
    setSingleSelectedBlockId(block.id);
    updateCurrentChapter(selectedPage.id, (chapter) => ({
      ...chapter,
      pages: chapter.pages.map((page) =>
        page.id !== selectedPage.id
          ? page
          : {
              ...page,
              updatedAt: new Date().toISOString(),
              blocks: page.blocks.map((candidate) =>
                candidate.id === block.id ? { ...candidate, translatedText } : candidate
              )
            }
      )
    }));
  }, [recordTranslationUndoSnapshot, selectedPage, selectedPageEditLocked, setSingleSelectedBlockId, updateCurrentChapter]);
  const updateSelectedBlockIndividualFontSize = useCallback((fontSizePx: number) => {
    updateSelectedBlock(
      { fontSizePx, fontSizeLinkedToPreset: false },
      { undoLabel: "폰트 크기 개별 변경" }
    );
  }, [updateSelectedBlock]);
  const disableSelectedBlockAutoFit = useCallback(() => {
    updateSelectedBlock(
      { autoFitText: false, autoFitTextLinkedToPreset: false },
      { undoLabel: "자동 맞춤 개별 해제" }
    );
  }, [updateSelectedBlock]);

  const {
    findReplaceOpen,
    focusFindReplaceMatch,
    openFindReplace,
    replaceAllMatches,
    replaceSingleMatch,
    setFindReplaceOpen
  } = useFindReplaceEditing({
    currentChapter,
    jobActive,
    pushStatus,
    recordTranslationUndoSnapshot,
    selectLayer,
    setSelectedBlockId: setSingleSelectedBlockId,
    setSelectedPageId,
    updateCurrentChapter
  });

  const modalOpen = Boolean(importPreview || renameTarget || settingsOpen || findReplaceOpen);
  const closeFindReplace = useCallback(() => setFindReplaceOpen(false), [setFindReplaceOpen]);
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
    oneHandMode: settings?.oneHandMode ?? false,
    pasteSelectedBlockFontStyleFromClipboard,
    pasteTranslationBlockFromClipboard,
    pushStatus,
    rangeToolActive,
    openFindReplace,
    selectLayer: selectLayerWithBlockSelection,
    selectInpaintResultEditTool,
    selectPageForReading,
    selectPointerTool,
    selectRangeTool,
    selectSharedInpaintTool,
    selectZoomTool,
    selectedBlockIdRef,
    selectedBlockCount: multiSelectedBlockCount,
    selectedPageEditLocked,
    selectedPageIdRef,
    setLibraryWidgetOpen,
    setRangeToolActive,
    setTemporaryPanActive,
    setZoomToolActive,
    temporaryPanHeldRef,
    temporaryPanShortcutEnabled,
    toggleSelectedPageProgress: togglePageProgress,
    undoShortcutPlatform,
    workspacePanelRef,
    zoomToolActive
  });

  const layerToolPanel = (
    <AppLayerToolPanel
      activeLayer={activeLayer}
      currentChapter={currentChapter}
      editingFontPresetId={editingFontPresetId}
      activeFontSizePresetId={activeFontSizePresetId}
      fontControlValues={multiSelectedBlockCount > 1 ? null : fontControlValues}
      fontFamilyOptions={fontFamilyOptions}
      fontPresetName={fontPresetName}
      favoriteFontPresetIds={favoriteFontPresetIds}
      fontPresets={fontPresets}
      fontSizePresets={fontSizePresets}
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
      renderFontPresetLinkGroupButton={renderFontPresetLinkGroupButton}
      selectedBlock={multiSelectedBlockCount > 1 ? null : selectedBlock}
      selectedPage={selectedPage}
      selectedPageEditLocked={selectedPageEditLocked}
      onClearEditingFontPreset={() => setEditingFontPresetId(null)}
      onClearInpaintMaskData={() => updateSelectedPageInpaintMask(undefined)}
      onClearInpaintResultData={() => updateSelectedPageInpaintResult(undefined)}
      onClearSelectedBlockFontPreset={clearSelectedBlockFontPreset}
      onCreateFontPreset={createFontPresetFromSelectedBlock}
      onCreateFontPresetListBackup={createFontPresetListBackup}
      onCreateFontSizePreset={createFontSizePresetFromCurrentFontSize}
      onDeleteFontPresetBackup={deleteFontPresetBackup}
      onDeleteFontPreset={deleteFontPreset}
      onDeleteFontSizePreset={deleteFontSizePreset}
      onDownloadLastImportedInpaintPsd={downloadLastImportedInpaintPsd}
      onExportInpaintPsd={exportSelectedPageInpaintPsd}
      onFillSelectedInpaintSelection={fillSelectedInpaintSelection}
      onFontPresetNameChange={setFontPresetName}
      onFontPresetRename={renameFontPreset}
      onFavoriteFontPresetToggle={toggleFavoriteFontPreset}
      onFontSettingChange={updateSelectedBlockFontSetting}
      onInpaintBrushSizeChange={setInpaintBrushSize}
      onInpaintResultBrushColorChange={setInpaintResultBrushColor}
      onInpaintResultBrushHardnessChange={setInpaintResultBrushHardness}
      onInpaintResultBrushSizeChange={setInpaintResultBrushSize}
      onInpaintResultToolStrengthChange={setInpaintResultToolStrength}
      onClearInpaintSelectionRect={() => setInpaintSelectionRect(null)}
      onRerunInpaintForSelection={rerunInpaintForSelection}
      onRerunInpaintWithCurrentMask={rerunInpaintWithCurrentMask}
      onListFontPresetBackups={listFontPresetBackups}
      onRestoreFontPresetListBackup={restoreFontPresetListBackup}
      onSelectFontPreset={selectFontPreset}
      onSelectFontSizePreset={selectFontSizePreset}
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
          favoriteFontPresets={favoriteFontPresets}
          fontWeightAvailability={systemFonts}
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
          layerOpacity={layerOpacity}
          layerVisibility={layerVisibility}
          overlayOpacityEditMode={overlayOpacityEditMode}
          rangeToolActive={rangeToolActive}
          recoverableFailures={recoverableFailures}
          selectedBlockId={selectedBlockId}
          selectedBlockIds={selectedBlockIds}
          selectedPage={selectedPage}
          selectedPageEditLocked={selectedPageEditLocked}
          selectedPageInpaintNotice={selectedPageInpaintNotice}
          showLamaEmptyNotice={showLamaEmptyNotice}
          showOriginalStageSize={showOriginalStageSize}
          stageRef={stageRef}
          stageSize={stageSize}
          stageViewResetKey={stageViewResetKey}
          stageViewScale={stageViewScale}
          stageZoomLabel={stageZoomLabel}
          statusLines={statusLines}
          statusToastLine={statusToastLine}
          statusToastTone={statusToastTone}
          statusWidgetOpen={statusWidgetOpen}
          statusWidgetTone={statusWidgetTone}
          temporaryPanActive={temporaryPanActive}
          workspacePanelRef={workspacePanelRef}
          zoomToolActive={zoomToolActive}
          onBlockPointerDown={(event, block, mode) => {
            const shiftMultiSelect =
              mode === "move" &&
              event.button === 0 &&
              event.shiftKey &&
              !event.altKey &&
              !event.ctrlKey &&
              !event.metaKey &&
              activeLayer === "overlay" &&
              !rangeToolActive &&
              !zoomToolActive &&
              !temporaryPanActive &&
              updateBlockSelectionWithShiftClick(block.id);
            if (shiftMultiSelect) {
              event.preventDefault();
              event.stopPropagation();
              return;
            }
            setSelectedBlockIds([]);
            onBlockPointerDown(event, block, mode);
          }}
          onBlockFontStyleCopy={copySelectedBlockFontStyleToClipboard}
          onBlockFontSizeChange={updateSelectedBlockIndividualFontSize}
          onBlockAutoFitDisable={disableSelectedBlockAutoFit}
          onSelectedBlockRangeChange={onSelectedBlockRangeChange}
          onBlockTextUpdate={updateInlineBlockText}
          onBlockTextSelectionSplitDuplicate={duplicateBlockTextSelection}
          onBlockTextAlignChange={(textAlign) => updateSelectedBlockFontSetting({ textAlign })}
          onFavoriteFontPresetSelect={selectFontPreset}
          onOpenFindReplace={openFindReplace}
          onDownloadLamaModel={downloadLamaModelFromEmptyState}
          onInpaintLayerChange={updateSelectedPageInpaintMask}
          onInpaintLayerEditEnd={endInpaintLayerInteraction}
          onInpaintLayerEditStart={beginInpaintLayerInteraction}
          onInpaintResultColorPick={setInpaintResultBrushColor}
          onInpaintResultLayerChange={updateSelectedPageInpaintResult}
          onInpaintSelectionChange={setInpaintSelectionRect}
          onPrepareLama={prepareLamaFromEmptyState}
          onRefreshLamaStatus={refreshLamaStatus}
          onDismissRecoverableFailure={clearRecoverableFailure}
          onRetryRecoverableFailure={retryRecoverableFailure}
          onSelectBlock={setSingleSelectedBlockId}
          onBlockSelectionChange={setSelectedBlockGroupIds}
          onSelectImportFiles={selectImportFiles}
          onSelectInpaintResultTool={selectInpaintResultEditTool}
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
          onSelectLayer={selectLayerWithBlockSelection}
          block={multiSelectedBlockCount > 1 ? null : selectedBlock}
          selectedBlockCount={multiSelectedBlockCount}
          fontPresetName={multiSelectedBlockCount > 1 ? undefined : selectedFontPreset?.name}
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
        <>
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
          {findReplaceOpen && currentChapter ? (
            <FindReplaceModal
              pages={currentChapter.pages}
              replaceDisabled={jobActive}
              onCancel={closeFindReplace}
              onFocusMatch={focusFindReplaceMatch}
              onReplaceAll={replaceAllMatches}
              onReplaceOne={replaceSingleMatch}
            />
          ) : null}
        </>
      }
    />
  );
}
