import React, { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  AppSettings,
  BBox,
  ChapterSnapshot,
  ImageRect,
  ImportPreviewResult,
  JobState,
  LamaRuntimeStatus,
  LibraryIndex,
  MangaPage,
  SystemFont,
  TranslationBlock
} from "../../shared/types";
import {
  applyEditableBlockBbox,
  clampBbox,
  clampRotationDeg,
  clampTextPaddingPx,
  enforceRenderDirection,
  offsetBlockBboxes,
  resolveBlockRotationDeg,
  resolveBlockRenderBbox,
  resolveEditableBlockBbox
} from "../../shared/geometry";
import { CompactNumberControl } from "./components/controls/CompactNumberControl";
import { EditorPanel } from "./components/EditorPanel";
import { FontFamilyPicker, buildFontFamilyOptions } from "./components/font/FontFamilyPicker";
import { FontOutlineControls } from "./components/font/FontOutlineControls";
import { FontPresetLinkIcon } from "./components/font/FontPresetLinkIcon";
import { ImageStage } from "./components/ImageStage";
import type { InpaintTool } from "./components/InpaintLayerCanvas";
import type { InpaintResultTool } from "./components/InpaintResultCanvas";
import { InpaintToolButton } from "./components/inpaint/InpaintToolButton";
import { ImportModal, type ImportModalSubmit } from "./components/ImportModal";
import { LayerControl } from "./components/layers/LayerControl";
import { LibraryTree } from "./components/LibraryTree";
import { PageList } from "./components/PageList";
import { RenameModal } from "./components/RenameModal";
import { EmptyPythonInstallHelp, LamaStatusPill } from "./components/runtime/RuntimeStatus";
import { SettingsModal } from "./components/SettingsModal";
import { useStageSize } from "./hooks/useStageSize";
import {
  applyFontPresetPatchToBlock,
  buildFontPresetLinkPatch,
  clearFontPresetLinkFields,
  createFontPreset,
  DEFAULT_FONT_PRESET,
  isBlockFontPresetValueLinked,
  type BlockFontPatch,
  type FontPresetPatch,
  type LinkableFontPresetKey
} from "./lib/fontPresets";
import { DEFAULT_OVERLAY_FONT_FAMILY } from "./lib/overlayLayout";
import { markChapterPagesRunning, mergeLiveChapterPreservingDirtyCompletedPages, resolveSelectionAfterChapterSync } from "./lib/chapterSync";
import {
  GLOBAL_UNDO_HISTORY_LIMIT,
  TRANSLATION_UNDO_COALESCE_MS,
  TRANSLATION_UNDO_LIMIT,
  type GlobalUndoHistoryEntry,
  type GlobalUndoKind
} from "./lib/editorUndoHistory";
import { formatJobEventLine, formatJobLabel, resolveProgressSnapshot, summarizeWarnings } from "./lib/jobProgress";
import { isPlatformUndoShortcut, resolveGlobalUndoAction, type GlobalUndoAction } from "./lib/globalUndo";
import {
  INPAINT_TOOL_SHORTCUTS,
  isPointerToolShortcut,
  isRangeToolShortcut,
  isZoomToolShortcut,
  resolveInpaintToolShortcut
} from "./lib/editorShortcuts";
import {
  angleBetweenPointsDeg,
  cloneTranslationBlock,
  createInpaintMaskUndoSnapshot,
  createTranslationUndoSnapshot,
  isEditableTarget,
  normalizeChapterTranslatedText,
  reorderByTarget,
  type InpaintMaskUndoSnapshot,
  type TranslationUndoSnapshot
} from "./lib/editorUtils";
import {
  FORCE_INCOMPLETE_LAMA_NOTICE,
  LAMA_TEST_INSTALL_GUIDE,
  LAMA_TEST_PLATFORM_OPTIONS,
  type LamaNoticePlatform
} from "./lib/lamaRuntimeNotice";
import {
  clearImageDataUrlRect,
  drawBlocksOnInpaintMask,
  fillImageDataUrlRect,
  maskDataUrlForSelection,
  mergePartialInpaintResult
} from "./lib/inpaintMaskImages";
import {
  clampInpaintMaskBrushSize,
  clampInpaintResultBrushSize,
  DEFAULT_INPAINT_SETTINGS,
  INPAINT_MASK_BRUSH_SIZE_MAX,
  INPAINT_MASK_BRUSH_SIZE_MIN,
  INPAINT_RESULT_BRUSH_SIZE_MAX,
  INPAINT_RESULT_BRUSH_SIZE_MIN
} from "./lib/inpaintToolSettings";
import { renderPageToPngDataUrl } from "./lib/pageRender";
import { resolveAdjacentPageId, resolveKeyboardPageNavigation } from "./lib/pageNavigation";
import { rangeProgressStyle } from "./lib/rangeProgressStyle";
import { clampStageViewScale } from "./lib/stageFit";
import {
  LAYER_FOCUS_OPACITY,
  type ActiveLayer,
  type LayerOpacity,
  type LayerVisibility
} from "./lib/layerState";
import "./styles.css";

const EMPTY_JOB: JobState = {
  id: "idle",
  kind: "gemma-analysis",
  status: "idle",
  progressText: "대기 중"
};

type DragMode = "move" | "resize" | "rotate";

type DragState = {
  mode: DragMode;
  blockId: string;
  startX: number;
  startY: number;
  startBbox: BBox;
  startRotationDeg: number;
  startAngleDeg: number;
  centerX: number;
  centerY: number;
  undoRecorded: boolean;
};

type PendingInpaintMaskSave = {
  chapterId: string;
  pageId: string;
  dataUrl: string | undefined;
};

type PendingInpaintResultSave = {
  chapterId: string;
  pageId: string;
  dataUrl: string | undefined;
};

const STAGE_ZOOM_STEP = 1.2;

type RenameTarget =
  | {
      kind: "work";
      id: string;
      title: string;
    }
  | {
      kind: "chapter";
      id: string;
      title: string;
    };

type StageZoomDirection = "in" | "out";

export default function App(): React.JSX.Element {
  const [library, setLibrary] = useState<LibraryIndex>({ workOrder: [], works: [] });
  const [currentChapter, setCurrentChapter] = useState<ChapterSnapshot | null>(null);
  const [selectedPageId, setSelectedPageId] = useState<string | null>(null);
  const [selectedBlockId, setSelectedBlockId] = useState<string | null>(null);
  const [jobState, setJobState] = useState<JobState>(EMPTY_JOB);
  const [statusLines, setStatusLines] = useState<string[]>([]);
  const [statusToastLine, setStatusToastLine] = useState<string | null>(null);
  const [importPreview, setImportPreview] = useState<ImportPreviewResult | null>(null);
  const [importBusy, setImportBusy] = useState(false);
  const [renderBusy, setRenderBusy] = useState(false);
  const [inpaintBusy, setInpaintBusy] = useState(false);
  const [renameTarget, setRenameTarget] = useState<RenameTarget | null>(null);
  const [renameBusy, setRenameBusy] = useState(false);
  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsBusy, setSettingsBusy] = useState(false);
  const [lamaStatus, setLamaStatus] = useState<LamaRuntimeStatus | null>(null);
  const [lamaActionBusy, setLamaActionBusy] = useState(false);
  const [lamaActionMessage, setLamaActionMessage] = useState<string | null>(null);
  const [lamaNoticePlatform, setLamaNoticePlatform] = useState<LamaNoticePlatform>("win32");
  const [stageViewScale, setStageViewScale] = useState<number | null>(null);
  const [stageViewResetKey, setStageViewResetKey] = useState(0);
  const [zoomToolActive, setZoomToolActive] = useState(false);
  const [rangeToolActive, setRangeToolActive] = useState(false);
  const [dirty, setDirty] = useState(false);
  const [undoVersion, setUndoVersion] = useState(0);
  const [saveFlash, setSaveFlash] = useState(false);
  const [inpaintTool, setInpaintTool] = useState<InpaintTool>("select");
  const [inpaintSelectionRect, setInpaintSelectionRect] = useState<ImageRect | null>(null);
  const [inpaintBrushSize, setInpaintBrushSize] = useState(28);
  const [inpaintResultTool, setInpaintResultTool] = useState<InpaintResultTool>("select");
  const [inpaintResultBrushSize, setInpaintResultBrushSize] = useState(28);
  const [inpaintResultBrushColor, setInpaintResultBrushColor] = useState("#ffffff");
  const [inpaintResultBrushHardness, setInpaintResultBrushHardness] = useState(0.85);
  const [inpaintResultToolStrength, setInpaintResultToolStrength] = useState(0.55);
  const [layerVisibility, setLayerVisibility] = useState<LayerVisibility>({
    image: true,
    inpaint: true,
    inpaintResult: true,
    inpaintMask: true,
    overlay: true
  });
  const [layerOpacity, setLayerOpacity] = useState<LayerOpacity>({
    image: 1,
    inpaint: 1,
    inpaintResult: 1,
    inpaintMask: 0.75,
    overlay: 1
  });
  const [overlayOpacityEditMode, setOverlayOpacityEditMode] = useState(false);
  const [fontPresetName, setFontPresetName] = useState("");
  const [editingFontPresetId, setEditingFontPresetId] = useState<string | null>(null);
  const [systemFonts, setSystemFonts] = useState<SystemFont[]>([]);
  const [focusModeEnabled, setFocusModeEnabled] = useState(true);
  const [statusWidgetOpen, setStatusWidgetOpen] = useState(false);
  const [activeLayer, setActiveLayer] = useState<ActiveLayer>("output");
  const [temporaryPanActive, setTemporaryPanActive] = useState(false);
  const [libraryWidgetOpen, setLibraryWidgetOpen] = useState(false);
  const workspacePanelRef = useRef<HTMLElement | null>(null);
  const libraryAnchorRef = useRef<HTMLDivElement | null>(null);
  const stageRef = useRef<HTMLDivElement | null>(null);
  const imageRef = useRef<HTMLImageElement | null>(null);
  const imageImportInputRef = useRef<HTMLInputElement | null>(null);
  const folderImportInputRef = useRef<HTMLInputElement | null>(null);
  const zipImportInputRef = useRef<HTMLInputElement | null>(null);
  const batchImportInputRef = useRef<HTMLInputElement | null>(null);
  const dragRef = useRef<DragState | null>(null);
  const saveTimerRef = useRef<number | null>(null);
  const saveFlashTimerRef = useRef<number | null>(null);
  const statusToastTimerRef = useRef<number | null>(null);
  const dirtyVersionRef = useRef(0);
  const dirtyPageIdsRef = useRef<Set<string>>(new Set());
  const currentChapterRef = useRef<ChapterSnapshot | null>(null);
  const selectedPageIdRef = useRef<string | null>(null);
  const selectedBlockIdRef = useRef<string | null>(null);
  const editingFontPresetIdRef = useRef<string | null>(null);
  const globalUndoHistoryRef = useRef<GlobalUndoHistoryEntry[]>([]);
  const translationUndoStackRef = useRef<TranslationUndoSnapshot[]>([]);
  const inpaintUndoStackRef = useRef<Map<string, InpaintMaskUndoSnapshot[]>>(new Map());
  const inpaintResultUndoStackRef = useRef<Map<string, (string | undefined)[]>>(new Map());
  const inpaintMaskSaveTimerRef = useRef<number | null>(null);
  const inpaintMaskSaveStateRef = useRef<PendingInpaintMaskSave | null>(null);
  const inpaintMaskSavingRef = useRef(false);
  const inpaintResultSaveTimerRef = useRef<number | null>(null);
  const inpaintResultSaveStateRef = useRef<PendingInpaintResultSave | null>(null);
  const inpaintResultSavingRef = useRef(false);
  const temporaryPanHeldRef = useRef(false);

  const selectedPage = useMemo(
    () => currentChapter?.pages.find((page) => page.id === selectedPageId) ?? currentChapter?.pages[0] ?? null,
    [currentChapter?.pages, selectedPageId]
  );
  const selectedBlock = selectedPage?.blocks.find((block) => block.id === selectedBlockId) ?? null;
  const fontPresets = currentChapter?.fontPresets ?? [];
  const selectedFontPreset = selectedBlock?.fontPresetId
    ? fontPresets.find((preset) => preset.id === selectedBlock.fontPresetId) ?? null
    : null;
  const editingFontPreset = editingFontPresetId
    ? fontPresets.find((preset) => preset.id === editingFontPresetId) ?? null
    : null;
  const selectedBlockFontControls = selectedBlock && selectedFontPreset
    ? applyFontPresetPatchToBlock(selectedBlock, selectedFontPreset)
    : selectedBlock;
  const fontControlValues = selectedBlockFontControls ?? editingFontPreset;
  const selectedBlockFontPresetLinks = selectedBlock
    ? {
        fontSizePx: isBlockFontPresetValueLinked(selectedBlock, "fontSizePx"),
        lineHeight: isBlockFontPresetValueLinked(selectedBlock, "lineHeight"),
        outlineColor: isBlockFontPresetValueLinked(selectedBlock, "outlineColor"),
        outlineWidthPx: isBlockFontPresetValueLinked(selectedBlock, "outlineWidthPx"),
        secondaryOutlineColor: isBlockFontPresetValueLinked(selectedBlock, "secondaryOutlineColor"),
        secondaryOutlineWidthPx: isBlockFontPresetValueLinked(selectedBlock, "secondaryOutlineWidthPx"),
        autoFitText: isBlockFontPresetValueLinked(selectedBlock, "autoFitText"),
        textColor: isBlockFontPresetValueLinked(selectedBlock, "textColor"),
        screentoneFillEnabled: isBlockFontPresetValueLinked(selectedBlock, "screentoneFillEnabled"),
        screentoneFillIntensity: isBlockFontPresetValueLinked(selectedBlock, "screentoneFillIntensity"),
        screentoneFillDensity: isBlockFontPresetValueLinked(selectedBlock, "screentoneFillDensity"),
        screentoneFillAntialias: isBlockFontPresetValueLinked(selectedBlock, "screentoneFillAntialias")
      }
    : null;
  const fontFamilyOptions = useMemo(
    () => buildFontFamilyOptions(systemFonts, fontControlValues?.fontFamily),
    [fontControlValues?.fontFamily, systemFonts]
  );
  const jobActive = ["starting", "running", "cancelling"].includes(jobState.status);
  const libraryChapterCount = useMemo(() => library.works.reduce((total, work) => total + work.chapters.length, 0), [library.works]);
  const selectedPageEditLocked = Boolean(jobActive && selectedPage && selectedPage.analysisStatus !== "completed");
  const canUndoTranslation = undoVersion >= 0 && currentChapter
    ? translationUndoStackRef.current.some((snapshot) => snapshot.chapterId === currentChapter.id) && !selectedPageEditLocked
    : false;
  const selectedPageSize = useMemo(
    () => (selectedPage ? { width: selectedPage.width, height: selectedPage.height } : null),
    [selectedPage?.height, selectedPage?.width]
  );
  const stageSize = useStageSize(imageRef, selectedPageSize);
  const currentStageScale = selectedPage && stageSize
    ? stageSize.width / Math.max(1, selectedPage.width)
    : (stageViewScale ?? 1);
  const stageZoomLabel = `${Math.round(currentStageScale * 100)}%`;
  const progressSnapshot = useMemo(() => resolveProgressSnapshot(jobState), [jobState]);
  const showProgressBar = jobState.status !== "idle" && !!progressSnapshot;
  const saveStatusTone = saveFlash ? "saved" : dirty ? "unsaved" : "synced";
  const saveStatusLabel = saveFlash ? "저장 완료" : dirty ? "저장되지 않은 변경 있음" : "최신 상태";
  const statusWidgetTone = `${jobState.status} ${saveStatusTone}`;
  const modalOpen = Boolean(importPreview || renameTarget || settingsOpen);
  const undoShortcutPlatform = useMemo(() => (typeof navigator === "undefined" ? "" : navigator.platform), []);
  const layerToolActive = activeLayer === "overlay" || activeLayer === "inpaintMask" || activeLayer === "inpaintResult";
  const selectedPageInpaintNotice =
    selectedPage?.inpaintStatus === "running"
      ? { tone: "running", title: "인페인트 중", message: selectedPage.name }
      : selectedPage?.inpaintStatus === "failed"
        ? { tone: "failed", title: "인페인트 실패", message: selectedPage.name }
        : null;
  const statusIndicatorLabel = jobActive ? jobState.progressText : saveStatusLabel;
  const showNotificationDock = Boolean(selectedPageInpaintNotice || statusToastLine || statusWidgetOpen);
  const overlayBackgroundOpacity = selectedPage?.blocks[0]?.opacity ?? 1;
  const displayedLamaStatus = FORCE_INCOMPLETE_LAMA_NOTICE && lamaStatus
    ? {
        ...lamaStatus,
        pythonAvailable: false,
        pythonInstallCommand: LAMA_TEST_INSTALL_GUIDE[lamaNoticePlatform].command,
        pythonInstallHelp: LAMA_TEST_INSTALL_GUIDE[lamaNoticePlatform].help,
        runtimeReady: false,
        runtimePreparing: false,
        modelExists: false,
        modelDownloading: false
      }
    : lamaStatus;
  const showLamaEmptyNotice = Boolean(
    displayedLamaStatus && !(displayedLamaStatus.pythonAvailable && displayedLamaStatus.runtimeReady && displayedLamaStatus.modelExists)
  );

  const zoomStage = useCallback((factor: number) => {
    setStageViewScale((current) => clampStageViewScale((current ?? currentStageScale) * factor));
  }, [currentStageScale]);
  const handleZoomToolClick = useCallback((direction: StageZoomDirection) => {
    zoomStage(direction === "out" ? 1 / STAGE_ZOOM_STEP : STAGE_ZOOM_STEP);
  }, [zoomStage]);
  const fitStageToWorkspace = useCallback(() => {
    setStageViewScale(null);
    setStageViewResetKey((current) => current + 1);
  }, []);
  const stageLayerOpacity = useMemo(
    () => temporaryPanActive
      ? { image: 1, inpaint: 1, inpaintResult: 1, inpaintMask: 0, overlay: 1 }
      : { ...layerOpacity, overlay: overlayOpacityEditMode ? 1 : layerOpacity.overlay },
    [layerOpacity, overlayOpacityEditMode, temporaryPanActive]
  );
  const stageLayerVisibility = useMemo(
    () => temporaryPanActive
      ? { image: true, inpaint: true, inpaintResult: true, inpaintMask: true, overlay: true }
      : layerVisibility,
    [layerVisibility, temporaryPanActive]
  );
  const selectLayer = useCallback((nextLayer: ActiveLayer) => {
    setActiveLayer(nextLayer);
    if (!focusModeEnabled) {
      return;
    }
    setLayerOpacity((current) => ({
      ...current,
      ...LAYER_FOCUS_OPACITY[nextLayer]
    }));
  }, [focusModeEnabled]);

  const selectSharedInpaintTool = useCallback((tool: InpaintTool) => {
    setZoomToolActive(false);
    setRangeToolActive(tool === "select");
    setInpaintTool(tool);
    setInpaintResultTool(tool);
  }, []);

  const selectPointerTool = useCallback(() => {
    setZoomToolActive(false);
    setRangeToolActive(false);
  }, []);

  const selectRangeTool = useCallback(() => {
    setZoomToolActive(false);
    setRangeToolActive(true);
    setInpaintTool("select");
    setInpaintResultTool("select");
  }, []);

  const selectZoomTool = useCallback(() => {
    setRangeToolActive(false);
    setZoomToolActive(true);
  }, []);

  const selectInpaintResultEditTool = useCallback((tool: Exclude<InpaintResultTool, "select">) => {
    setZoomToolActive(false);
    setRangeToolActive(false);
    setInpaintResultTool(tool);
  }, []);

  const refreshLibrary = useCallback(async () => {
    const next = await window.mangaApi.getLibrary();
    setLibrary(next);
  }, []);

  const refreshSettings = useCallback(async () => {
    const next = await window.mangaApi.getSettings();
    setSettings(next);
    return next;
  }, []);

  const refreshLamaStatus = useCallback(async () => {
    const next = await window.mangaApi.getLamaRuntimeStatus();
    setLamaStatus(next);
    return next;
  }, []);

  React.useEffect(() => {
    void refreshLibrary();
  }, [refreshLibrary]);

  React.useEffect(() => {
    void refreshSettings().catch((error) => {
      console.error(error);
    });
  }, [refreshSettings]);

  React.useEffect(() => {
    void refreshLamaStatus().catch((error) => {
      console.error(error);
    });
  }, [refreshLamaStatus]);

  React.useEffect(() => {
    if (!lamaStatus?.runtimePreparing && !lamaStatus?.modelDownloading) {
      return;
    }
    const interval = window.setInterval(() => {
      void refreshLamaStatus().catch((error) => {
        console.error(error);
      });
    }, 2500);
    return () => window.clearInterval(interval);
  }, [lamaStatus?.modelDownloading, lamaStatus?.runtimePreparing, refreshLamaStatus]);

  React.useEffect(() => {
    void window.mangaApi
      .getSystemFonts()
      .then(setSystemFonts)
      .catch((error) => {
        console.error(error);
      });
  }, []);

  React.useEffect(() => {
    currentChapterRef.current = currentChapter;
  }, [currentChapter]);

  React.useEffect(() => {
    selectedPageIdRef.current = selectedPageId;
  }, [selectedPageId]);

  useEffect(() => {
    setInpaintSelectionRect(null);
  }, [selectedPageId]);

  React.useEffect(() => {
    selectedBlockIdRef.current = selectedBlockId;
  }, [selectedBlockId]);

  React.useEffect(() => {
    editingFontPresetIdRef.current = editingFontPresetId;
  }, [editingFontPresetId]);

  const mergeLiveChapter = useCallback((chapter: ChapterSnapshot) => {
    const current = currentChapterRef.current;
    if (current && current.id !== chapter.id) {
      return;
    }

    const mergeResult = mergeLiveChapterPreservingDirtyCompletedPages(chapter, current, dirtyPageIdsRef.current);
    dirtyPageIdsRef.current = new Set(mergeResult.preservedDirtyPageIds);
    currentChapterRef.current = mergeResult.chapter;

    setCurrentChapter((currentChapter) => {
      if (currentChapter && currentChapter.id !== mergeResult.chapter.id) {
        return currentChapter;
      }
      return mergeResult.chapter;
    });

    const selection = resolveSelectionAfterChapterSync(mergeResult.chapter, selectedPageIdRef.current, selectedBlockIdRef.current);
    setSelectedPageId(selection.selectedPageId);
    setSelectedBlockId(selection.selectedBlockId);
    setDirty(mergeResult.preservedDirtyPageIds.length > 0);
  }, []);

  const appendStatusLine = useCallback((line: string) => {
    const next = line.trim();
    if (!next) {
      return;
    }
    setStatusToastLine(next);
    if (statusToastTimerRef.current) {
      window.clearTimeout(statusToastTimerRef.current);
    }
    statusToastTimerRef.current = window.setTimeout(() => {
      setStatusToastLine(null);
      statusToastTimerRef.current = null;
    }, 4000);
    setStatusLines((lines) => {
      if (lines[0] === next) {
        return lines;
      }
      return [next, ...lines].slice(0, 16);
    });
  }, []);

  const signalSaveComplete = useCallback(() => {
    if (saveFlashTimerRef.current) {
      window.clearTimeout(saveFlashTimerRef.current);
    }
    setSaveFlash(true);
    saveFlashTimerRef.current = window.setTimeout(() => {
      saveFlashTimerRef.current = null;
      setSaveFlash(false);
    }, 1200);
  }, []);

  React.useEffect(() => {
    return () => {
      if (saveFlashTimerRef.current) {
        window.clearTimeout(saveFlashTimerRef.current);
      }
      if (statusToastTimerRef.current) {
        window.clearTimeout(statusToastTimerRef.current);
      }
    };
  }, []);

  React.useEffect(() => {
    const unsubscribe = window.mangaApi.onJobEvent((event) => {
      const friendlyText = formatJobLabel(event);
      setJobState((current) => ({
        id: event.id,
        kind: event.kind,
        status: event.status,
        progressText: friendlyText,
        detail: event.detail ?? current.detail,
        phase: event.phase ?? current.phase,
        progressCurrent: event.progressCurrent ?? current.progressCurrent,
        progressTotal: event.progressTotal ?? current.progressTotal,
        pageIndex: event.pageIndex ?? current.pageIndex,
        pageTotal: event.pageTotal ?? current.pageTotal,
        attempt: event.attempt ?? current.attempt,
        attemptTotal: event.attemptTotal ?? current.attemptTotal
      }));
      appendStatusLine(formatJobEventLine(event));

      if (event.phase === "page_done" || event.phase === "page_skipped") {
        const chapterId = currentChapterRef.current?.id;
        if (!chapterId) {
          return;
        }

        void window.mangaApi
          .openChapter(chapterId)
          .then((chapter) => {
            if (currentChapterRef.current?.id === chapter.id) {
              mergeLiveChapter(chapter);
            }
          })
          .then(() => refreshLibrary())
          .catch((error) => {
            console.error(error);
          });
      }
    });
    return unsubscribe;
  }, [appendStatusLine, mergeLiveChapter, refreshLibrary]);

  React.useEffect(() => {
    if (!dirty || !currentChapter) {
      return;
    }

    const version = dirtyVersionRef.current;
    const dirtyPageIds = [...dirtyPageIdsRef.current];
    saveTimerRef.current = window.setTimeout(async () => {
      try {
        const normalized = normalizeChapterTranslatedText(currentChapter);
        const chapterToSave = normalized.chapter;
        const pageIdsToSave = [...new Set([...dirtyPageIds, ...normalized.dirtyPageIds])];
        await window.mangaApi.saveChapter(chapterToSave, pageIdsToSave.length > 0 ? pageIdsToSave : undefined);
        if (dirtyVersionRef.current === version) {
          if (chapterToSave !== currentChapter) {
            currentChapterRef.current = chapterToSave;
            setCurrentChapter((current) => (current?.id === chapterToSave.id ? chapterToSave : current));
          }
          dirtyPageIdsRef.current.clear();
          setDirty(false);
          signalSaveComplete();
        }
      } catch (error) {
        console.error(error);
      } finally {
        saveTimerRef.current = null;
      }
    }, 400);

    return () => {
      if (saveTimerRef.current) {
        window.clearTimeout(saveTimerRef.current);
      }
    };
  }, [currentChapter, dirty, signalSaveComplete]);

  const pushStatus = useCallback(
    (line: string) => {
      void window.mangaApi.writeLog("info", "UI status", { line });
      appendStatusLine(line);
    },
    [appendStatusLine]
  );

  const markDirty = useCallback((pageId?: string) => {
    dirtyVersionRef.current += 1;
    if (pageId) {
      dirtyPageIdsRef.current = new Set([...dirtyPageIdsRef.current, pageId]);
    }
    setDirty(true);
  }, []);

  const recordGlobalUndoEntry = useCallback((entry: GlobalUndoHistoryEntry) => {
    globalUndoHistoryRef.current = [...globalUndoHistoryRef.current, entry].slice(-GLOBAL_UNDO_HISTORY_LIMIT);
    setUndoVersion((current) => current + 1);
  }, []);

  const consumeGlobalUndoEntry = useCallback((kind: GlobalUndoKind, pageId?: string) => {
    const next = [...globalUndoHistoryRef.current];
    for (let index = next.length - 1; index >= 0; index -= 1) {
      const entry = next[index];
      if (entry?.kind === kind && entry.pageId === pageId) {
        next.splice(index, 1);
        globalUndoHistoryRef.current = next;
        setUndoVersion((current) => current + 1);
        return;
      }
    }
  }, []);

  const clearUndoStacks = useCallback(() => {
    globalUndoHistoryRef.current = [];
    translationUndoStackRef.current = [];
    inpaintUndoStackRef.current.clear();
    inpaintResultUndoStackRef.current.clear();
    setUndoVersion((current) => current + 1);
  }, []);

  const recordTranslationUndoSnapshot = useCallback((label: string) => {
    const chapter = currentChapterRef.current;
    if (!chapter) {
      return false;
    }

    const now = Date.now();
    const selectedPageId = selectedPageIdRef.current;
    const selectedBlockId = selectedBlockIdRef.current;
    const editingFontPresetId = editingFontPresetIdRef.current;
    const lastSnapshot = translationUndoStackRef.current.at(-1);
    if (
      lastSnapshot &&
      lastSnapshot.chapterId === chapter.id &&
      lastSnapshot.label === label &&
      lastSnapshot.selectedPageId === selectedPageId &&
      lastSnapshot.selectedBlockId === selectedBlockId &&
      lastSnapshot.editingFontPresetId === editingFontPresetId &&
      now - lastSnapshot.createdAtMs <= TRANSLATION_UNDO_COALESCE_MS
    ) {
      lastSnapshot.createdAtMs = now;
      return true;
    }

    translationUndoStackRef.current = [
      ...translationUndoStackRef.current,
      createTranslationUndoSnapshot(
        chapter,
        label,
        now,
        selectedPageId,
        selectedBlockId,
        editingFontPresetId
      )
    ].slice(-TRANSLATION_UNDO_LIMIT);
    recordGlobalUndoEntry({ kind: "translation", chapterId: chapter.id });
    return true;
  }, [recordGlobalUndoEntry]);

  const saveNow = useCallback(async () => {
    if (!currentChapter) {
      return;
    }
    if (saveTimerRef.current) {
      window.clearTimeout(saveTimerRef.current);
      saveTimerRef.current = null;
    }
    const dirtyPageIds = [...dirtyPageIdsRef.current];
    const normalized = normalizeChapterTranslatedText(currentChapter);
    const chapterToSave = normalized.chapter;
    const pageIdsToSave = [...new Set([...dirtyPageIds, ...normalized.dirtyPageIds])];
    await window.mangaApi.saveChapter(chapterToSave, pageIdsToSave.length > 0 ? pageIdsToSave : undefined);
    if (chapterToSave !== currentChapter) {
      currentChapterRef.current = chapterToSave;
      setCurrentChapter((current) => (current?.id === chapterToSave.id ? chapterToSave : current));
    }
    dirtyPageIdsRef.current.clear();
    setDirty(false);
    signalSaveComplete();
  }, [currentChapter, signalSaveComplete]);

  const flushInpaintMaskSave = useCallback(async () => {
    if (inpaintMaskSavingRef.current) {
      return;
    }

    const pending = inpaintMaskSaveStateRef.current;
    if (!pending) {
      return;
    }

    inpaintMaskSaveStateRef.current = null;
    inpaintMaskSavingRef.current = true;
    try {
      if (dirty && currentChapterRef.current?.id === pending.chapterId) {
        await saveNow();
      }
      const result = await window.mangaApi.saveInpaintMask({
        chapterId: pending.chapterId,
        pageId: pending.pageId,
        maskDataUrl: pending.dataUrl
      });

      if (!inpaintMaskSaveStateRef.current) {
        mergeLiveChapter(result.chapter);
        signalSaveComplete();
        void refreshLibrary();
      }
    } catch (error) {
      console.error(error);
      pushStatus(error instanceof Error ? error.message : "인페인트 마스크 저장에 실패했습니다.");
    } finally {
      inpaintMaskSavingRef.current = false;
      if (inpaintMaskSaveStateRef.current) {
        void flushInpaintMaskSave();
      }
    }
  }, [dirty, mergeLiveChapter, pushStatus, refreshLibrary, saveNow, signalSaveComplete]);

  const scheduleInpaintMaskSave = useCallback((pending: PendingInpaintMaskSave) => {
    inpaintMaskSaveStateRef.current = pending;
    if (inpaintMaskSaveTimerRef.current) {
      window.clearTimeout(inpaintMaskSaveTimerRef.current);
    }
    inpaintMaskSaveTimerRef.current = window.setTimeout(() => {
      inpaintMaskSaveTimerRef.current = null;
      void flushInpaintMaskSave();
    }, 250);
  }, [flushInpaintMaskSave]);

  const flushInpaintResultSave = useCallback(async () => {
    if (inpaintResultSavingRef.current) {
      return;
    }

    const pending = inpaintResultSaveStateRef.current;
    if (!pending) {
      return;
    }

    inpaintResultSaveStateRef.current = null;
    inpaintResultSavingRef.current = true;
    try {
      if (dirty && currentChapterRef.current?.id === pending.chapterId) {
        await saveNow();
      }
      const result = await window.mangaApi.saveInpaintResultLayer({
        chapterId: pending.chapterId,
        pageId: pending.pageId,
        resultDataUrl: pending.dataUrl
      });

      if (!inpaintResultSaveStateRef.current) {
        mergeLiveChapter(result.chapter);
        signalSaveComplete();
        void refreshLibrary();
      }
    } catch (error) {
      console.error(error);
      pushStatus(error instanceof Error ? error.message : "인페인트 결과 레이어 저장에 실패했습니다.");
    } finally {
      inpaintResultSavingRef.current = false;
      if (inpaintResultSaveStateRef.current) {
        void flushInpaintResultSave();
      }
    }
  }, [dirty, mergeLiveChapter, pushStatus, refreshLibrary, saveNow, signalSaveComplete]);

  const scheduleInpaintResultSave = useCallback((pending: PendingInpaintResultSave) => {
    inpaintResultSaveStateRef.current = pending;
    if (inpaintResultSaveTimerRef.current) {
      window.clearTimeout(inpaintResultSaveTimerRef.current);
    }
    inpaintResultSaveTimerRef.current = window.setTimeout(() => {
      inpaintResultSaveTimerRef.current = null;
      void flushInpaintResultSave();
    }, 250);
  }, [flushInpaintResultSave]);

  const updatePageInpaintStatus = useCallback((pageId: string, status: MangaPage["inpaintStatus"]) => {
    setCurrentChapter((current) => {
      if (!current) {
        return current;
      }
      const next = {
        ...current,
        pages: current.pages.map((page) =>
          page.id === pageId
            ? {
                ...page,
                inpaintStatus: status,
                updatedAt: new Date().toISOString()
              }
            : page
        )
      };
      currentChapterRef.current = next;
      return next;
    });
  }, []);

  const clearCurrentChapter = useCallback(() => {
    if (saveTimerRef.current) {
      window.clearTimeout(saveTimerRef.current);
      saveTimerRef.current = null;
    }
    if (inpaintMaskSaveTimerRef.current) {
      window.clearTimeout(inpaintMaskSaveTimerRef.current);
      inpaintMaskSaveTimerRef.current = null;
    }
    if (inpaintResultSaveTimerRef.current) {
      window.clearTimeout(inpaintResultSaveTimerRef.current);
      inpaintResultSaveTimerRef.current = null;
    }
    setCurrentChapter(null);
    currentChapterRef.current = null;
    setSelectedPageId(null);
    setSelectedBlockId(null);
    setEditingFontPresetId(null);
    clearUndoStacks();
    dirtyPageIdsRef.current.clear();
    setDirty(false);
  }, [clearUndoStacks]);

  const openChapter = useCallback(
    async (chapterId: string) => {
      if (dirty) {
        await saveNow();
      }
      const chapter = await window.mangaApi.openChapter(chapterId);
      dirtyPageIdsRef.current.clear();
      clearUndoStacks();
      currentChapterRef.current = chapter;
      setCurrentChapter(chapter);
      setSelectedPageId(chapter.pages[0]?.id ?? null);
      setSelectedBlockId(null);
      setDirty(false);
    },
    [clearUndoStacks, dirty, saveNow]
  );

  const applyChapter = useCallback((chapter: ChapterSnapshot | undefined, fallbackStatus?: string) => {
    if (!chapter) {
      return;
    }
    dirtyPageIdsRef.current.clear();
    clearUndoStacks();
    currentChapterRef.current = chapter;
    setCurrentChapter(chapter);
    setSelectedPageId((current) => (chapter.pages.some((page) => page.id === current) ? current : chapter.pages[0]?.id ?? null));
    setSelectedBlockId(null);
    setDirty(false);
    if (fallbackStatus) {
      pushStatus(fallbackStatus);
    }
  }, [clearUndoStacks, pushStatus]);

  const selectPageForReading = useCallback((pageId: string | null) => {
    if (!pageId) {
      return;
    }
    selectedPageIdRef.current = pageId;
    selectedBlockIdRef.current = null;
    setSelectedPageId(pageId);
    setSelectedBlockId(null);
  }, []);

  const openImportPreview = useCallback(async (mode: "images" | "folder" | "zip" | "zip-folder", files: File[]) => {
    const preview =
      mode === "images"
        ? await window.mangaApi.previewImagesImport(files)
        : mode === "folder"
          ? await window.mangaApi.previewFolderImport(files)
          : mode === "zip"
            ? await window.mangaApi.previewZipImport(files)
            : await window.mangaApi.previewZipFolderImport(files);
    if (!preview) {
      return;
    }
    setImportPreview(preview);
  }, []);

  const selectImportFiles = useCallback((mode: "images" | "folder" | "zip" | "zip-folder") => {
    const input =
      mode === "images"
        ? imageImportInputRef.current
        : mode === "folder"
          ? folderImportInputRef.current
          : mode === "zip"
            ? zipImportInputRef.current
            : batchImportInputRef.current;
    input?.click();
  }, []);

  const handleImportInputChange = useCallback(
    async (mode: "images" | "folder" | "zip" | "zip-folder", event: React.ChangeEvent<HTMLInputElement>) => {
      const files = Array.from(event.target.files ?? []);
      event.target.value = "";
      await openImportPreview(mode, files);
    },
    [openImportPreview]
  );

  const runAnalysis = useCallback(
    async (runMode: "pending" | "all" | "single-page", pageId?: string) => {
      if (!currentChapter || jobActive) {
        return;
      }

      await saveNow();
      setStatusLines([]);
      setStatusToastLine(null);
      if (statusToastTimerRef.current) {
        window.clearTimeout(statusToastTimerRef.current);
        statusToastTimerRef.current = null;
      }
      setJobState({
        id: "pending",
        kind: "gemma-analysis",
        status: "starting",
        progressText: "모델 준비 중",
        phase: "booting"
      });
      setCurrentChapter((chapter) => (chapter ? markChapterPagesRunning(chapter, runMode, pageId) : chapter));

      const result = await window.mangaApi.startAnalysis({ chapterId: currentChapter.id, runMode, pageId });
      if (result.chapter) {
        applyChapter(result.chapter);
      }
      await refreshLibrary();

      if (result.status === "completed") {
        const warningSummary = summarizeWarnings(result.warnings ?? []);
        if (warningSummary) {
          pushStatus(warningSummary);
        }
        return;
      }

      if (result.status === "failed" && result.error) {
        pushStatus(result.error);
      }
    },
    [applyChapter, currentChapter, jobActive, pushStatus, refreshLibrary, saveNow]
  );

  const renderSelectedPage = useCallback(async () => {
    const chapter = currentChapterRef.current;
    const pageId = selectedPageIdRef.current;
    if (!chapter || !pageId || renderBusy) {
      return;
    }

    setRenderBusy(true);
    try {
      if (dirty) {
        await saveNow();
      }
      // saveNow 이후 chapter가 서버/다른 동기화로 교체될 수 있으므로
      // 현재 ref에서 최신 페이지를 다시 조회한다
      const page = currentChapterRef.current?.pages.find((p) => p.id === pageId) ?? null;
      if (!page) {
        return;
      }
      const dataUrl = await renderPageToPngDataUrl(page, {
        layerVisibility: {
          image: true,
          inpaint: true,
          inpaintResult: true,
          inpaintMask: false,
          overlay: true
        },
        layerOpacity: {
          image: 1,
          inpaint: 1,
          inpaintResult: 1,
          inpaintMask: 1,
          overlay: 1
        },
        activeLayer: "output"
      });
      const result = await window.mangaApi.renderPage({
        chapterId: currentChapterRef.current!.id,
        pageId,
        dataUrl
      });
      signalSaveComplete();
      pushStatus(`페이지 렌더 저장: ${result.outputPath}`);
    } catch (error) {
      console.error(error);
      pushStatus(error instanceof Error ? error.message : "페이지 렌더에 실패했습니다.");
    } finally {
      setRenderBusy(false);
    }
  }, [dirty, pushStatus, renderBusy, saveNow, signalSaveComplete]);

  const runInpaintForPage = useCallback(async (page: MangaPage, maskDataUrl: string, statusMessage = "인페인트 결과를 저장했습니다.") => {
    if (!currentChapter || inpaintBusy) {
      return;
    }

    setInpaintBusy(true);
    try {
      if (dirty) {
        await saveNow();
      }
      updatePageInpaintStatus(page.id, "running");
      const result = await window.mangaApi.inpaintPage({
        chapterId: currentChapter.id,
        pageId: page.id,
        sourceDataUrl: page.dataUrl,
        maskDataUrl,
        settings: DEFAULT_INPAINT_SETTINGS
      });
      applyChapter(result.chapter);
      signalSaveComplete();
      void refreshLibrary();
      pushStatus(result.engine === "local-fill-fallback" ? "로컬 인페인트 결과를 저장했습니다." : statusMessage);
    } catch (error) {
      console.error(error);
      updatePageInpaintStatus(page.id, "failed");
      pushStatus(error instanceof Error ? error.message : "인페인트 실행에 실패했습니다.");
    } finally {
      setInpaintBusy(false);
    }
  }, [applyChapter, currentChapter, dirty, inpaintBusy, pushStatus, refreshLibrary, saveNow, signalSaveComplete, updatePageInpaintStatus]);

  const submitImport = useCallback(
    async ({ target, selections }: ImportModalSubmit) => {
      if (!importPreview) {
        return;
      }

      setImportBusy(true);
      try {
        const result = await window.mangaApi.createImport({
          preview: importPreview,
          target,
          selections
        });
        await refreshLibrary();
        applyChapter(result.openedChapter, `${result.chapterIds.length}개 화를 보관함에 추가했습니다.`);
        setImportPreview(null);

        if (importPreview.mode === "batch") {
          for (const chapterId of result.chapterIds) {
            await openChapter(chapterId);
            const runResult = await window.mangaApi.startAnalysis({ chapterId, runMode: "pending" });
            if (runResult.chapter) {
              applyChapter(runResult.chapter);
            }
            await refreshLibrary();
            if (runResult.status !== "completed") {
              break;
            }
          }
        }
      } finally {
        setImportBusy(false);
      }
    },
    [applyChapter, importPreview, openChapter, refreshLibrary]
  );

  const updateCurrentChapter = useCallback((pageId: string | undefined, updater: (chapter: ChapterSnapshot) => ChapterSnapshot) => {
    setCurrentChapter((current) => {
      if (!current) {
        return current;
      }
      const next = updater(current);
      currentChapterRef.current = next;
      markDirty(pageId);
      return next;
    });
  }, [markDirty]);

  const undoTranslationEdit = useCallback(() => {
    if (selectedPageEditLocked) {
      return;
    }

    const chapterId = currentChapterRef.current?.id;
    if (!chapterId) {
      return;
    }

    const stack = [...translationUndoStackRef.current];
    let snapshotIndex = -1;
    for (let index = stack.length - 1; index >= 0; index -= 1) {
      if (stack[index]?.chapterId === chapterId) {
        snapshotIndex = index;
        break;
      }
    }
    if (snapshotIndex < 0) {
      return;
    }

    const snapshot = stack[snapshotIndex];
    stack.splice(snapshotIndex, 1);
    translationUndoStackRef.current = stack;
    consumeGlobalUndoEntry("translation");

    const pageSnapshots = new Map(snapshot.pages.map((page) => [page.pageId, page]));
    const updatedAt = new Date().toISOString();
    setCurrentChapter((current) => {
      if (!current || current.id !== snapshot.chapterId) {
        return current;
      }

      const next = {
        ...current,
        updatedAt,
        fontPresets: snapshot.fontPresets?.map((preset) => ({ ...preset })),
        pages: current.pages.map((page) => {
          const pageSnapshot = pageSnapshots.get(page.id);
          return pageSnapshot
            ? {
                ...page,
                updatedAt,
                blocks: pageSnapshot.blocks.map(cloneTranslationBlock)
              }
            : page;
        })
      };
      currentChapterRef.current = next;
      markDirty(undefined);
      return next;
    });
    setSelectedPageId(snapshot.selectedPageId);
    setSelectedBlockId(snapshot.selectedBlockId);
    setEditingFontPresetId(snapshot.editingFontPresetId);
  }, [consumeGlobalUndoEntry, markDirty, selectedPageEditLocked]);

  const removePage = useCallback(
    async (pageId: string) => {
      if (!currentChapter) {
        return;
      }
      const page = currentChapter.pages.find((candidate) => candidate.id === pageId);
      if (!page) {
        return;
      }
      const confirmed = await window.mangaApi.confirm(
        "페이지 삭제",
        "정말 삭제하시겠습니까?",
        "이 페이지와 해당 번역 결과가 보관함에서 삭제됩니다."
      );
      if (!confirmed) {
        return;
      }

      const previousOrder = currentChapter.pages.map((candidate) => candidate.id);
      const nextChapter = await window.mangaApi.deletePage(currentChapter.id, pageId);
      applyChapter(nextChapter);
      const currentIndex = previousOrder.indexOf(pageId);
      const nextId = previousOrder[currentIndex + 1] ?? previousOrder[currentIndex - 1] ?? null;
      setSelectedPageId(nextId && nextChapter.pages.some((candidate) => candidate.id === nextId) ? nextId : nextChapter.pages[0]?.id ?? null);
      pushStatus(`${page.name} 페이지를 삭제했습니다.`);
      await refreshLibrary();
    },
    [applyChapter, currentChapter, pushStatus, refreshLibrary]
  );

  const retranslatePage = useCallback(
    async (pageId: string) => {
      const page = currentChapter?.pages.find((candidate) => candidate.id === pageId);
      if (!page || !currentChapter) {
        return;
      }
      const confirmed = await window.mangaApi.confirm(
        "페이지 재번역",
        "정말 재번역 하시겠습니까?",
        "기존 번역 결과와 수정 내용이 이 페이지에서 덮어써집니다."
      );
      if (!confirmed) {
        return;
      }
      await runAnalysis("single-page", pageId);
    },
    [currentChapter, runAnalysis]
  );

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

  const updateSelectedBlock = (patch: Partial<TranslationBlock>, options: { recordUndo?: boolean; undoLabel?: string } = {}) => {
    if (!selectedPage || !selectedBlock || selectedPageEditLocked) {
      return;
    }

    if (options.recordUndo !== false) {
      recordTranslationUndoSnapshot(options.undoLabel ?? "번역 블록 변경");
    }

    updateCurrentChapter(selectedPage.id, (current) => ({
      ...current,
      pages: current.pages.map((page) =>
        page.id !== selectedPage.id
          ? page
          : {
              ...page,
              updatedAt: new Date().toISOString(),
              blocks: page.blocks.map((block) => {
                if (block.id !== selectedBlock.id) {
                  return block;
                }

                const nextType = patch.type ?? block.type;
                return {
                  ...block,
                  ...patch,
                  type: nextType,
                  renderDirection: enforceRenderDirection(nextType, patch.renderDirection ?? block.renderDirection),
                  rotationDeg: patch.rotationDeg !== undefined ? clampRotationDeg(patch.rotationDeg) : block.rotationDeg,
                  textPaddingPx:
                    patch.textPaddingPx !== undefined
                      ? clampTextPaddingPx(patch.textPaddingPx)
                      : Object.prototype.hasOwnProperty.call(patch, "textPaddingPx")
                        ? undefined
                        : block.textPaddingPx,
                  bbox: patch.bbox ? clampBbox(patch.bbox) : block.bbox,
                  renderBbox: patch.renderBbox ? clampBbox(patch.renderBbox) : block.renderBbox
                };
              })
            }
      )
    }));
  };

  const updateAssignedFontPreset = (presetId: string, patch: FontPresetPatch, options: { recordUndo?: boolean; undoLabel?: string } = {}) => {
    if (!currentChapter || selectedPageEditLocked) {
      return;
    }

    if (options.recordUndo !== false) {
      recordTranslationUndoSnapshot(options.undoLabel ?? "폰트 설정 변경");
    }

    updateCurrentChapter(undefined, (current) => ({
      ...current,
      fontPresets: (current.fontPresets ?? []).map((preset) => (preset.id === presetId ? { ...preset, ...patch } : preset)),
      pages: current.pages.map((page) => ({
        ...page,
        updatedAt: page.blocks.some((block) => block.fontPresetId === presetId) ? new Date().toISOString() : page.updatedAt,
        blocks: page.blocks.map((block) => (block.fontPresetId === presetId ? applyFontPresetPatchToBlock(block, patch) : block))
      }))
    }));
  };

  const updateSelectedBlockFontSetting = (patch: BlockFontPatch) => {
    if ("textAlign" in patch) {
      if (patch.textAlign) {
        updateSelectedBlock({ textAlign: patch.textAlign });
      }
      return;
    }
    if (selectedBlock?.fontPresetId) {
      const presetPatch: FontPresetPatch = {};
      const blockPatch: Partial<TranslationBlock> = {};
      for (const key of Object.keys(patch) as (keyof FontPresetPatch)[]) {
        const value = patch[key];
        if (value === undefined) {
          continue;
        }
        if (key === "fontFamily" || isBlockFontPresetValueLinked(selectedBlock, key)) {
          Object.assign(presetPatch, { [key]: value });
        } else {
          Object.assign(blockPatch, { [key]: value });
        }
      }
      if (Object.keys(blockPatch).length > 0) {
        recordTranslationUndoSnapshot("폰트 설정 변경");
        updateSelectedBlock(blockPatch, { recordUndo: false });
        if (Object.keys(presetPatch).length > 0) {
          updateAssignedFontPreset(selectedBlock.fontPresetId, presetPatch, { recordUndo: false });
        }
        return;
      }
      if (Object.keys(presetPatch).length > 0) {
        updateAssignedFontPreset(selectedBlock.fontPresetId, presetPatch);
      }
      return;
    }
    if (!selectedBlock && editingFontPreset) {
      updateAssignedFontPreset(editingFontPreset.id, patch);
      return;
    }
    updateSelectedBlock(patch);
  };

  const toggleSelectedBlockFontPresetLink = (key: LinkableFontPresetKey) => {
    if (!selectedBlock || !selectedFontPreset) {
      return;
    }

    const nextLinked = !isBlockFontPresetValueLinked(selectedBlock, key);
    updateSelectedBlock({
      ...buildFontPresetLinkPatch(key, nextLinked),
      ...(nextLinked ? { [key]: selectedFontPreset[key] } : {})
    });
  };

  const renderFontPresetLinkButton = (key: LinkableFontPresetKey, label: string) => {
    if (!selectedBlock?.fontPresetId || !selectedFontPreset || !selectedBlockFontPresetLinks) {
      return null;
    }

    const linked = selectedBlockFontPresetLinks[key];
    return (
      <button
        type="button"
        className={`font-preset-link-toggle ${linked ? "linked" : "unlinked"}`}
        disabled={selectedPageEditLocked}
        onClick={(event) => {
          event.preventDefault();
          event.stopPropagation();
          toggleSelectedBlockFontPresetLink(key);
        }}
        aria-label={`${label} 프리셋 ${linked ? "연결 해제" : "연결"}`}
        title={`${label} 프리셋 ${linked ? "연결 해제" : "연결"}`}
      >
        <FontPresetLinkIcon linked={linked} />
      </button>
    );
  };

  const createFontPresetFromSelectedBlock = () => {
    if (!currentChapter || selectedPageEditLocked) {
      return;
    }

    const presetName = fontPresetName.trim() || `프리셋 ${(currentChapter?.fontPresets?.length ?? 0) + 1}`;
    const preset = createFontPreset(presetName, selectedBlock ?? DEFAULT_FONT_PRESET);
    recordTranslationUndoSnapshot("폰트 프리셋 생성");
    updateCurrentChapter(selectedPage?.id, (current) => ({
      ...current,
      fontPresets: [...(current.fontPresets ?? []), preset],
      pages: current.pages.map((page) =>
        selectedPage && selectedBlock && page.id === selectedPage.id
          ? {
              ...page,
              updatedAt: new Date().toISOString(),
              blocks: page.blocks.map((block) =>
                block.id === selectedBlock.id
                  ? {
                      ...applyFontPresetPatchToBlock(block, preset, { forceLinkedValues: true }),
                      fontPresetId: preset.id,
                      fontSizeLinkedToPreset: true,
                      lineHeightLinkedToPreset: true,
                      outlineColorLinkedToPreset: true,
                      outlineWidthLinkedToPreset: true,
                      secondaryOutlineColorLinkedToPreset: true,
                      secondaryOutlineWidthLinkedToPreset: true,
                      autoFitTextLinkedToPreset: true,
                      textColorLinkedToPreset: true,
                      screentoneFillEnabledLinkedToPreset: true,
                      screentoneFillIntensityLinkedToPreset: true,
                      screentoneFillDensityLinkedToPreset: true,
                      screentoneFillAntialiasLinkedToPreset: true
                    }
                  : block
              )
            }
          : page
      )
    }));
    setEditingFontPresetId(preset.id);
    setFontPresetName("");
  };

  const selectFontPreset = (presetId: string) => {
    if (selectedPageEditLocked) {
      return;
    }
    const preset = fontPresets.find((candidate) => candidate.id === presetId);
    if (!preset) {
      return;
    }
    setEditingFontPresetId(presetId);
    if (!selectedPage || !selectedBlock) {
      return;
    }
    updateSelectedBlock({
      ...applyFontPresetPatchToBlock(selectedBlock, preset, { forceLinkedValues: true }),
      fontPresetId: preset.id,
      fontSizeLinkedToPreset: true,
      lineHeightLinkedToPreset: true,
      outlineColorLinkedToPreset: true,
      outlineWidthLinkedToPreset: true,
      secondaryOutlineColorLinkedToPreset: true,
      secondaryOutlineWidthLinkedToPreset: true,
      autoFitTextLinkedToPreset: true,
      textColorLinkedToPreset: true,
      screentoneFillEnabledLinkedToPreset: true,
      screentoneFillIntensityLinkedToPreset: true,
      screentoneFillDensityLinkedToPreset: true,
      screentoneFillAntialiasLinkedToPreset: true
    });
  };

  const clearSelectedBlockFontPreset = () => {
    if (!selectedBlock) {
      return;
    }
    updateSelectedBlock({
      fontPresetId: undefined,
      fontSizeLinkedToPreset: undefined,
      lineHeightLinkedToPreset: undefined,
      outlineColorLinkedToPreset: undefined,
      outlineWidthLinkedToPreset: undefined,
      secondaryOutlineColorLinkedToPreset: undefined,
      secondaryOutlineWidthLinkedToPreset: undefined,
      autoFitTextLinkedToPreset: undefined,
      textColorLinkedToPreset: undefined,
      screentoneFillEnabledLinkedToPreset: undefined,
      screentoneFillIntensityLinkedToPreset: undefined,
      screentoneFillDensityLinkedToPreset: undefined,
      screentoneFillAntialiasLinkedToPreset: undefined
    });
  };

  const deleteFontPreset = (presetId: string) => {
    if (selectedPageEditLocked) {
      return;
    }
    recordTranslationUndoSnapshot("폰트 프리셋 삭제");
    updateCurrentChapter(undefined, (current) => ({
      ...current,
      fontPresets: (current.fontPresets ?? []).filter((preset) => preset.id !== presetId),
      pages: current.pages.map((page) => ({
        ...page,
        blocks: page.blocks.map((block) => {
          if (block.fontPresetId !== presetId) {
            return block;
          }
          const { fontPresetId: _fontPresetId, ...rest } = block;
          return clearFontPresetLinkFields(rest);
        })
      }))
    }));
    setEditingFontPresetId((current) => (current === presetId ? null : current));
  };

  const deleteSelectedBlock = useCallback(() => {
    if (!selectedPage || !selectedBlock || selectedPageEditLocked) {
      return;
    }
    recordTranslationUndoSnapshot("번역 블록 삭제");
    updateCurrentChapter(selectedPage.id, (current) => ({
      ...current,
      pages: current.pages.map((page) =>
        page.id === selectedPage.id
          ? {
              ...page,
              updatedAt: new Date().toISOString(),
              blocks: page.blocks.filter((block) => block.id !== selectedBlock.id)
            }
          : page
      )
    }));
    setSelectedBlockId(null);
  }, [recordTranslationUndoSnapshot, selectedBlock, selectedPage, selectedPageEditLocked, updateCurrentChapter]);

  const updateSelectedPageInpaintMask = useCallback((dataUrl: string | undefined, options: { persist?: boolean; recordUndo?: boolean } = {}) => {
    if (!currentChapter || !selectedPage || selectedPageEditLocked) {
      return;
    }

    const previousDataUrl = selectedPage.inpaintMaskDataUrl ?? selectedPage.inpaintLayerDataUrl;
    if (options.recordUndo !== false && previousDataUrl !== dataUrl) {
      const stack = inpaintUndoStackRef.current.get(selectedPage.id) ?? [];
      stack.push(createInpaintMaskUndoSnapshot(selectedPage));
      inpaintUndoStackRef.current.set(selectedPage.id, stack.slice(-30));
      recordGlobalUndoEntry({ kind: "inpaint-mask", chapterId: currentChapter.id, pageId: selectedPage.id });
    }

    const updatedAt = new Date().toISOString();
    setCurrentChapter((current) => {
      if (!current) {
        return current;
      }
      const next = {
        ...current,
        updatedAt,
        pages: current.pages.map((page) =>
          page.id === selectedPage.id
            ? {
                ...page,
                updatedAt,
                inpaintMaskPath: dataUrl ? page.inpaintMaskPath : undefined,
                inpaintResultPath: dataUrl ? page.inpaintResultPath : undefined,
                inpaintMaskDataUrl: dataUrl,
                inpaintResultDataUrl: dataUrl ? page.inpaintResultDataUrl : undefined,
                inpaintStatus: "idle" as const
              }
            : page
        )
      };
      currentChapterRef.current = next;
      return next;
    });

    if (options.persist !== false) {
      scheduleInpaintMaskSave({
        chapterId: currentChapter.id,
        pageId: selectedPage.id,
        dataUrl
      });
    }
  }, [currentChapter, recordGlobalUndoEntry, scheduleInpaintMaskSave, selectedPage, selectedPageEditLocked]);

  const updateSelectedPageInpaintResult = useCallback((dataUrl: string | undefined, options: { persist?: boolean; recordUndo?: boolean } = {}) => {
    if (!currentChapter || !selectedPage || selectedPageEditLocked) {
      return;
    }

    const previousDataUrl = selectedPage.inpaintResultDataUrl;
    if (options.recordUndo !== false && previousDataUrl !== dataUrl) {
      const stack = inpaintResultUndoStackRef.current.get(selectedPage.id) ?? [];
      stack.push(previousDataUrl);
      inpaintResultUndoStackRef.current.set(selectedPage.id, stack.slice(-30));
      recordGlobalUndoEntry({ kind: "inpaint-result", chapterId: currentChapter.id, pageId: selectedPage.id });
    }

    const updatedAt = new Date().toISOString();
    setCurrentChapter((current) => {
      if (!current) {
        return current;
      }
      const next = {
        ...current,
        updatedAt,
        pages: current.pages.map((page) =>
          page.id === selectedPage.id
            ? {
                ...page,
                updatedAt,
                inpaintResultPath: dataUrl ? page.inpaintResultPath : undefined,
                inpaintResultDataUrl: dataUrl,
                inpaintStatus: dataUrl ? "completed" as const : "idle" as const
              }
            : page
        )
      };
      currentChapterRef.current = next;
      return next;
    });

    if (options.persist !== false) {
      scheduleInpaintResultSave({
        chapterId: currentChapter.id,
        pageId: selectedPage.id,
        dataUrl
      });
    }
  }, [currentChapter, recordGlobalUndoEntry, scheduleInpaintResultSave, selectedPage, selectedPageEditLocked]);

  const restorePageInpaintMaskSnapshot = useCallback((pageId: string, snapshot: InpaintMaskUndoSnapshot) => {
    const chapter = currentChapterRef.current;
    if (!chapter || selectedPageEditLocked) {
      return;
    }

    const updatedAt = new Date().toISOString();
    setCurrentChapter((current) => {
      if (!current || current.id !== chapter.id) {
        return current;
      }
      const next = {
        ...current,
        updatedAt,
        pages: current.pages.map((page) =>
          page.id === pageId
            ? {
                ...page,
                updatedAt,
                inpaintMaskPath: snapshot.inpaintMaskPath,
                inpaintResultPath: snapshot.inpaintResultPath,
                inpaintMaskDataUrl: snapshot.inpaintMaskDataUrl,
                inpaintResultDataUrl: snapshot.inpaintResultDataUrl,
                inpaintStatus: snapshot.inpaintStatus ?? (snapshot.inpaintResultDataUrl ? "completed" as const : "idle" as const)
              }
            : page
        )
      };
      currentChapterRef.current = next;
      return next;
    });

    scheduleInpaintMaskSave({
      chapterId: chapter.id,
      pageId,
      dataUrl: snapshot.inpaintMaskDataUrl
    });
    window.setTimeout(() => {
      scheduleInpaintResultSave({
        chapterId: chapter.id,
        pageId,
        dataUrl: snapshot.inpaintResultDataUrl
      });
    }, 300);
  }, [scheduleInpaintMaskSave, scheduleInpaintResultSave, selectedPageEditLocked]);

  const undoPageInpaint = useCallback((pageId: string) => {
    if (selectedPageEditLocked) {
      return;
    }

    const stack = inpaintUndoStackRef.current.get(pageId) ?? [];
    if (stack.length === 0) {
      return;
    }
    const previousSnapshot = stack.pop();
    if (!previousSnapshot) {
      return;
    }
    inpaintUndoStackRef.current.set(pageId, stack);
    consumeGlobalUndoEntry("inpaint-mask", pageId);
    restorePageInpaintMaskSnapshot(pageId, previousSnapshot);
  }, [consumeGlobalUndoEntry, restorePageInpaintMaskSnapshot, selectedPageEditLocked]);

  const undoPageInpaintResult = useCallback((pageId: string) => {
    if (selectedPageEditLocked) {
      return;
    }

    const stack = inpaintResultUndoStackRef.current.get(pageId) ?? [];
    if (stack.length === 0) {
      return;
    }
    const previousDataUrl = stack.pop();
    inpaintResultUndoStackRef.current.set(pageId, stack);
    consumeGlobalUndoEntry("inpaint-result", pageId);

    const chapter = currentChapterRef.current;
    if (!chapter) {
      return;
    }
    const updatedAt = new Date().toISOString();
    setCurrentChapter((current) => {
      if (!current || current.id !== chapter.id) {
        return current;
      }
      const next = {
        ...current,
        updatedAt,
        pages: current.pages.map((page) =>
          page.id === pageId
            ? {
                ...page,
                updatedAt,
                inpaintResultPath: previousDataUrl ? page.inpaintResultPath : undefined,
                inpaintResultDataUrl: previousDataUrl,
                inpaintStatus: previousDataUrl ? "completed" as const : "idle" as const
              }
            : page
        )
      };
      currentChapterRef.current = next;
      return next;
    });

    scheduleInpaintResultSave({
      chapterId: chapter.id,
      pageId,
      dataUrl: previousDataUrl
    });
  }, [consumeGlobalUndoEntry, scheduleInpaintResultSave, selectedPageEditLocked]);

  const globalUndoActions = useMemo<GlobalUndoAction[]>(() => {
    const chapterId = currentChapter?.id;
    if (!chapterId || selectedPageEditLocked) {
      return [];
    }

    return [...globalUndoHistoryRef.current].reverse().map((entry, index): GlobalUndoAction => {
      const id = `${entry.kind}-${entry.pageId ?? "chapter"}-${index}`;
      if (entry.chapterId !== chapterId) {
        return { id, label: "다른 화 되돌리기", canUndo: false, run: () => undefined };
      }

      if (entry.kind === "translation") {
        return {
          id,
          label: "번역 블록 되돌리기",
          canUndo: canUndoTranslation,
          run: undoTranslationEdit
        };
      }

      if (entry.kind === "inpaint-mask" && entry.pageId) {
        return {
          id,
          label: "인페인트 마스크 되돌리기",
          canUndo: (inpaintUndoStackRef.current.get(entry.pageId)?.length ?? 0) > 0,
          run: () => undoPageInpaint(entry.pageId!)
        };
      }

      if (entry.kind === "inpaint-result" && entry.pageId) {
        return {
          id,
          label: "인페인트 결과 되돌리기",
          canUndo: (inpaintResultUndoStackRef.current.get(entry.pageId)?.length ?? 0) > 0,
          run: () => undoPageInpaintResult(entry.pageId!)
        };
      }

      return { id, label: "되돌리기", canUndo: false, run: () => undefined };
    });
  }, [
    canUndoTranslation,
    currentChapter?.id,
    selectedPageEditLocked,
    undoPageInpaint,
    undoPageInpaintResult,
    undoTranslationEdit,
    undoVersion
  ]);

  const applyInpaintSelectedBlock = useCallback(async () => {
    if (!selectedPage || !selectedBlock || selectedPageEditLocked || inpaintBusy) {
      return;
    }

    const maskDataUrl = await drawBlocksOnInpaintMask(selectedPage, [selectedBlock]);
    updateSelectedPageInpaintMask(maskDataUrl, { persist: false });
    await runInpaintForPage({ ...selectedPage, inpaintMaskDataUrl: maskDataUrl }, maskDataUrl, "선택 블록 인페인트 결과를 저장했습니다.");
  }, [inpaintBusy, runInpaintForPage, selectedBlock, selectedPage, selectedPageEditLocked, updateSelectedPageInpaintMask]);

  const applyInpaintAllBlocks = useCallback(async () => {
    if (!selectedPage || selectedPage.blocks.length === 0 || selectedPageEditLocked || inpaintBusy) {
      return;
    }

    const maskDataUrl = await drawBlocksOnInpaintMask(selectedPage, selectedPage.blocks);
    updateSelectedPageInpaintMask(maskDataUrl, { persist: false });
    await runInpaintForPage({ ...selectedPage, inpaintMaskDataUrl: maskDataUrl }, maskDataUrl, "전체 블록 인페인트 결과를 저장했습니다.");
  }, [inpaintBusy, runInpaintForPage, selectedPage, selectedPageEditLocked, updateSelectedPageInpaintMask]);

  const rerunInpaintWithCurrentMask = useCallback(async () => {
    if (selectedPageEditLocked || inpaintBusy) {
      return;
    }

    const pageId = selectedPageIdRef.current;
    const page = pageId ? currentChapterRef.current?.pages.find((candidate) => candidate.id === pageId) : null;
    if (!page) {
      return;
    }

    const maskDataUrl = page.inpaintMaskDataUrl ?? page.inpaintLayerDataUrl;
    if (!maskDataUrl) {
      pushStatus("다시 인페인트할 마스크가 없습니다.");
      return;
    }

    await runInpaintForPage(page, maskDataUrl, "현재 마스크 기준으로 인페인트 결과를 다시 저장했습니다.");
  }, [inpaintBusy, pushStatus, runInpaintForPage, selectedPageEditLocked]);

  const rerunInpaintForSelection = useCallback(async () => {
    if (!currentChapter || selectedPageEditLocked || inpaintBusy) {
      return;
    }

    const pageId = selectedPageIdRef.current;
    const page = pageId ? currentChapterRef.current?.pages.find((candidate) => candidate.id === pageId) : null;
    if (!page) {
      return;
    }

    const maskDataUrl = page.inpaintMaskDataUrl ?? page.inpaintLayerDataUrl;
    if (!maskDataUrl) {
      pushStatus("다시 인페인트할 마스크가 없습니다.");
      return;
    }
    if (!inpaintSelectionRect) {
      pushStatus("부분 인페인트할 범위를 먼저 선택하세요.");
      return;
    }

    setInpaintBusy(true);
    try {
      if (dirty) {
        await saveNow();
      }
      const selectionMaskDataUrl = await maskDataUrlForSelection(maskDataUrl, page.width, page.height, inpaintSelectionRect);
      if (!selectionMaskDataUrl) {
        pushStatus("선택 범위 안에 마스크 픽셀이 없습니다.");
        return;
      }

      updatePageInpaintStatus(page.id, "running");
      const result = await window.mangaApi.inpaintPage({
        chapterId: currentChapter.id,
        pageId: page.id,
        sourceDataUrl: page.dataUrl,
        maskDataUrl: selectionMaskDataUrl,
        settings: DEFAULT_INPAINT_SETTINGS,
        persistResult: false
      });
      const mergedResultDataUrl = await mergePartialInpaintResult(
        page.inpaintResultDataUrl,
        result.resultDataUrl,
        selectionMaskDataUrl,
        page.width,
        page.height
      );
      updateSelectedPageInpaintResult(mergedResultDataUrl, { persist: false });
      const saved = await window.mangaApi.saveInpaintResultLayer({
        chapterId: currentChapter.id,
        pageId: page.id,
        resultDataUrl: mergedResultDataUrl
      });
      applyChapter(saved.chapter);
      signalSaveComplete();
      void refreshLibrary();
      pushStatus(result.engine === "local-fill-fallback" ? "선택 범위 로컬 인페인트 결과를 저장했습니다." : "선택 범위만 다시 인페인트했습니다.");
    } catch (error) {
      console.error(error);
      updatePageInpaintStatus(page.id, "failed");
      pushStatus(error instanceof Error ? error.message : "부분 인페인트 실행에 실패했습니다.");
    } finally {
      setInpaintBusy(false);
    }
  }, [
    applyChapter,
    currentChapter,
    dirty,
    inpaintBusy,
    inpaintSelectionRect,
    pushStatus,
    refreshLibrary,
    saveNow,
    selectedPageEditLocked,
    signalSaveComplete,
    updatePageInpaintStatus,
    updateSelectedPageInpaintResult
  ]);

  const clearSelectedInpaintSelection = useCallback(async () => {
    if (selectedPageEditLocked || inpaintBusy || !inpaintSelectionRect) {
      return false;
    }

    const pageId = selectedPageIdRef.current;
    const page = pageId ? currentChapterRef.current?.pages.find((candidate) => candidate.id === pageId) : null;
    if (!page) {
      return false;
    }

    if (activeLayer === "inpaintMask" && rangeToolActive) {
      const maskDataUrl = page.inpaintMaskDataUrl ?? page.inpaintLayerDataUrl;
      if (!maskDataUrl) {
        return false;
      }
      const nextDataUrl = await clearImageDataUrlRect(maskDataUrl, page.width, page.height, inpaintSelectionRect);
      updateSelectedPageInpaintMask(nextDataUrl);
      pushStatus("선택 범위의 인페인트 마스크를 지웠습니다.");
      return true;
    }

    if (activeLayer === "inpaintResult" && rangeToolActive) {
      if (!page.inpaintResultDataUrl) {
        return false;
      }
      const nextDataUrl = await clearImageDataUrlRect(page.inpaintResultDataUrl, page.width, page.height, inpaintSelectionRect);
      updateSelectedPageInpaintResult(nextDataUrl);
      pushStatus("선택 범위의 인페인트 결과를 지웠습니다.");
      return true;
    }

    return false;
  }, [
    activeLayer,
    inpaintBusy,
    inpaintSelectionRect,
    pushStatus,
    rangeToolActive,
    selectedPageEditLocked,
    updateSelectedPageInpaintMask,
    updateSelectedPageInpaintResult
  ]);

  const fillSelectedInpaintSelection = useCallback(async () => {
    if (selectedPageEditLocked || inpaintBusy || !inpaintSelectionRect) {
      return;
    }

    const pageId = selectedPageIdRef.current;
    const page = pageId ? currentChapterRef.current?.pages.find((candidate) => candidate.id === pageId) : null;
    if (!page) {
      return;
    }

    if (activeLayer === "inpaintMask" && rangeToolActive) {
      const nextDataUrl = await fillImageDataUrlRect({
        dataUrl: page.inpaintMaskDataUrl ?? page.inpaintLayerDataUrl,
        width: page.width,
        height: page.height,
        rect: inpaintSelectionRect,
        fillStyle: "#ffffff"
      });
      updateSelectedPageInpaintMask(nextDataUrl);
      pushStatus("선택 범위를 인페인트 마스크로 채웠습니다.");
      return;
    }

    if (activeLayer === "inpaintResult" && rangeToolActive) {
      const nextDataUrl = await fillImageDataUrlRect({
        dataUrl: page.inpaintResultDataUrl,
        width: page.width,
        height: page.height,
        rect: inpaintSelectionRect,
        fillStyle: inpaintResultBrushColor
      });
      updateSelectedPageInpaintResult(nextDataUrl);
      pushStatus("선택 범위를 인페인트 결과 색상으로 채웠습니다.");
    }
  }, [
    activeLayer,
    inpaintBusy,
    inpaintResultBrushColor,
    inpaintSelectionRect,
    pushStatus,
    rangeToolActive,
    selectedPageEditLocked,
    updateSelectedPageInpaintMask,
    updateSelectedPageInpaintResult
  ]);

  const updateSelectedPageBlockOpacity = useCallback((opacity: number) => {
    if (!selectedPage || selectedPageEditLocked) {
      return;
    }

    recordTranslationUndoSnapshot("블록 투명도 변경");
    updateCurrentChapter(selectedPage.id, (current) => ({
      ...current,
      pages: current.pages.map((page) =>
        page.id === selectedPage.id
          ? {
              ...page,
              updatedAt: new Date().toISOString(),
              blocks: page.blocks.map((block) => ({ ...block, opacity }))
            }
          : page
      )
    }));
  }, [recordTranslationUndoSnapshot, selectedPage, selectedPageEditLocked, updateCurrentChapter]);

  const duplicateSelectedBlock = () => {
    if (!selectedPage || !selectedBlock || selectedPageEditLocked) {
      return;
    }
    const copy = {
      ...offsetBlockBboxes(selectedBlock, 16, 16),
      id: `${selectedBlock.id}-copy-${Date.now()}`
    };
    recordTranslationUndoSnapshot("번역 블록 복제");
    updateCurrentChapter(selectedPage.id, (current) => ({
      ...current,
      pages: current.pages.map((page) =>
        page.id === selectedPage.id
          ? {
              ...page,
              updatedAt: new Date().toISOString(),
              blocks: [...page.blocks, copy]
            }
          : page
      )
    }));
    setSelectedBlockId(copy.id);
  };

  const createEmptyBlock = () => {
    if (!selectedPage || selectedPageEditLocked || inpaintBusy) {
      return;
    }
    const sourcePreset = editingFontPreset ?? DEFAULT_FONT_PRESET;
    const blockId = `${selectedPage.id}-block-manual-${Date.now()}`;
    const block: TranslationBlock = {
      id: blockId,
      type: "speech",
      bbox: { x: 350, y: 420, w: 300, h: 140 },
      bboxSpace: "normalized_1000",
      sourceText: "",
      translatedText: "",
      confidence: 1,
      sourceDirection: "vertical",
      renderDirection: "horizontal",
      fontPresetId: editingFontPreset?.id,
      fontSizeLinkedToPreset: editingFontPreset ? true : undefined,
      lineHeightLinkedToPreset: editingFontPreset ? true : undefined,
      outlineColorLinkedToPreset: editingFontPreset ? true : undefined,
      outlineWidthLinkedToPreset: editingFontPreset ? true : undefined,
      secondaryOutlineColorLinkedToPreset: editingFontPreset ? true : undefined,
      secondaryOutlineWidthLinkedToPreset: editingFontPreset ? true : undefined,
      autoFitTextLinkedToPreset: editingFontPreset ? true : undefined,
      textColorLinkedToPreset: editingFontPreset ? true : undefined,
      screentoneFillEnabledLinkedToPreset: editingFontPreset ? true : undefined,
      screentoneFillIntensityLinkedToPreset: editingFontPreset ? true : undefined,
      screentoneFillDensityLinkedToPreset: editingFontPreset ? true : undefined,
      screentoneFillAntialiasLinkedToPreset: editingFontPreset ? true : undefined,
      fontFamily: sourcePreset.fontFamily,
      fontSizePx: sourcePreset.fontSizePx,
      lineHeight: sourcePreset.lineHeight,
      outlineColor: sourcePreset.outlineColor,
      outlineWidthPx: sourcePreset.outlineWidthPx,
      secondaryOutlineColor: sourcePreset.secondaryOutlineColor,
      secondaryOutlineWidthPx: sourcePreset.secondaryOutlineWidthPx,
      autoFitText: sourcePreset.autoFitText,
      textAlign: "center",
      textColor: sourcePreset.textColor ?? "#111111",
      screentoneFillEnabled: sourcePreset.screentoneFillEnabled,
      screentoneFillIntensity: sourcePreset.screentoneFillIntensity,
      screentoneFillDensity: sourcePreset.screentoneFillDensity,
      screentoneFillAntialias: sourcePreset.screentoneFillAntialias,
      backgroundColor: "#fffdf5",
      opacity: 0.88
    };

    recordTranslationUndoSnapshot("번역 블록 생성");
    updateCurrentChapter(selectedPage.id, (current) => ({
      ...current,
      pages: current.pages.map((page) =>
        page.id === selectedPage.id
          ? {
              ...page,
              updatedAt: new Date().toISOString(),
              blocks: [...page.blocks, block]
            }
          : page
      )
    }));
    setLayerVisibility((current) => ({ ...current, overlay: true }));
    selectLayer("overlay");
    setSelectedBlockId(blockId);
  };

  const onBlockPointerDown = (event: React.PointerEvent, block: TranslationBlock, mode: DragMode) => {
    if (!stageRef.current || selectedPageEditLocked || activeLayer !== "overlay") {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    setSelectedBlockId(block.id);
    const target = resolveEditableBlockBbox(block);
    const stageRect = stageRef.current.getBoundingClientRect();
    const centerX = stageRect.left + ((target.bbox.x + target.bbox.w / 2) / 1000) * stageRect.width;
    const centerY = stageRect.top + ((target.bbox.y + target.bbox.h / 2) / 1000) * stageRect.height;
    dragRef.current = {
      mode,
      blockId: block.id,
      startX: event.clientX,
      startY: event.clientY,
      startBbox: target.bbox,
      startRotationDeg: resolveBlockRotationDeg(block),
      startAngleDeg: angleBetweenPointsDeg(centerX, centerY, event.clientX, event.clientY),
      centerX,
      centerY,
      undoRecorded: false
    };
    stageRef.current.setPointerCapture(event.pointerId);
  };

  const onStagePointerMove = (event: React.PointerEvent) => {
    const drag = dragRef.current;
    const page = selectedPage;
    const stage = stageRef.current;
    if (!drag || !page || !stage || !currentChapter || selectedPageEditLocked) {
      return;
    }
    const rect = stage.getBoundingClientRect();
    const dx = ((event.clientX - drag.startX) / Math.max(1, rect.width)) * 1000;
    const dy = ((event.clientY - drag.startY) / Math.max(1, rect.height)) * 1000;
    const next =
      drag.mode === "move"
        ? {
            ...drag.startBbox,
            x: drag.startBbox.x + dx,
            y: drag.startBbox.y + dy
          }
        : drag.mode === "resize"
          ? {
              ...drag.startBbox,
              w: drag.startBbox.w + dx,
              h: drag.startBbox.h + dy
            }
          : drag.startBbox;
    const nextRotationDeg =
      drag.mode === "rotate"
        ? clampRotationDeg(drag.startRotationDeg + angleBetweenPointsDeg(drag.centerX, drag.centerY, event.clientX, event.clientY) - drag.startAngleDeg)
        : null;

    if (!drag.undoRecorded) {
      recordTranslationUndoSnapshot("번역 블록 위치 변경");
      drag.undoRecorded = true;
    }

    updateCurrentChapter(page.id, (chapter) => ({
      ...chapter,
      pages: chapter.pages.map((candidate) =>
        candidate.id !== page.id
          ? candidate
          : {
              ...candidate,
              updatedAt: new Date().toISOString(),
              blocks: candidate.blocks.map((block) =>
                block.id === drag.blockId
                  ? nextRotationDeg === null
                    ? applyEditableBlockBbox(block, next)
                    : { ...block, rotationDeg: nextRotationDeg }
                  : block
              )
            }
      )
    }));
  };

  const onStagePointerUp = (event: React.PointerEvent) => {
    if (dragRef.current && stageRef.current) {
      stageRef.current.releasePointerCapture(event.pointerId);
    }
    dragRef.current = null;
  };

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

  React.useEffect(() => {
    if (!layerToolActive || modalOpen) {
      temporaryPanHeldRef.current = false;
      setTemporaryPanActive(false);
    }
  }, [layerToolActive, modalOpen]);

  React.useEffect(() => {
    const shouldHandleSpacePan = (event: KeyboardEvent) =>
      layerToolActive &&
      !modalOpen &&
      !event.altKey &&
      !event.ctrlKey &&
      !event.metaKey &&
      !isEditableTarget(event.target) &&
      (event.code === "Space" || event.key === " ");

    const onKeyDown = (event: KeyboardEvent) => {
      if (!shouldHandleSpacePan(event)) {
        return;
      }
      event.preventDefault();
      temporaryPanHeldRef.current = true;
      setTemporaryPanActive(true);
    };

    const onKeyUp = (event: KeyboardEvent) => {
      if ((event.code !== "Space" && event.key !== " ") || !temporaryPanHeldRef.current) {
        return;
      }
      event.preventDefault();
      temporaryPanHeldRef.current = false;
      setTemporaryPanActive(false);
    };

    const resetTemporaryPan = () => {
      temporaryPanHeldRef.current = false;
      setTemporaryPanActive(false);
    };

    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    window.addEventListener("blur", resetTemporaryPan);
    document.addEventListener("visibilitychange", resetTemporaryPan);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      window.removeEventListener("blur", resetTemporaryPan);
      document.removeEventListener("visibilitychange", resetTemporaryPan);
    };
  }, [layerToolActive, modalOpen]);

  React.useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const editableTarget = isEditableTarget(event.target);
      if (event.key === "Escape" && libraryWidgetOpen) {
        setLibraryWidgetOpen(false);
        return;
      }
      if (event.key === "Escape" && zoomToolActive) {
        setZoomToolActive(false);
        return;
      }
      if (event.key === "Escape" && rangeToolActive) {
        setRangeToolActive(false);
        return;
      }

      if (!modalOpen && !editableTarget && isPlatformUndoShortcut(event, undoShortcutPlatform)) {
        event.preventDefault();
        const undoAction = resolveGlobalUndoAction(globalUndoActions);
        if (undoAction) {
          undoAction.run();
        }
        return;
      }

      const layerNumberShortcut =
        !modalOpen && !editableTarget && !event.altKey && !event.ctrlKey && !event.metaKey &&
        event.key >= "1" && event.key <= "5";
      if (layerNumberShortcut) {
        event.preventDefault();
        switch (event.key) {
          case "1": selectLayer("output"); break;
          case "2": selectLayer("overlay"); break;
          case "3": selectLayer("inpaintResult"); break;
          case "4": selectLayer("inpaintMask"); break;
          case "5": selectLayer("image"); break;
          default: return;
        }
        workspacePanelRef.current?.focus();
        return;
      }

      const zoomToolShortcut =
        !modalOpen &&
        !editableTarget &&
        !event.altKey &&
        !event.ctrlKey &&
        !event.metaKey &&
        isZoomToolShortcut(event);
      if (zoomToolShortcut) {
        event.preventDefault();
        selectZoomTool();
        workspacePanelRef.current?.focus();
        return;
      }

      const pointerToolShortcut =
        !modalOpen &&
        !editableTarget &&
        !event.altKey &&
        !event.ctrlKey &&
        !event.metaKey &&
        isPointerToolShortcut(event);
      if (pointerToolShortcut) {
        event.preventDefault();
        selectPointerTool();
        workspacePanelRef.current?.focus();
        return;
      }

      const rangeToolShortcut =
        !modalOpen &&
        !editableTarget &&
        !event.altKey &&
        !event.ctrlKey &&
        !event.metaKey &&
        isRangeToolShortcut(event);
      if (rangeToolShortcut && !selectedPageEditLocked) {
        event.preventDefault();
        selectRangeTool();
        workspacePanelRef.current?.focus();
        return;
      }

      const inpaintToolShortcut = !modalOpen && !editableTarget && !event.altKey && !event.ctrlKey && !event.metaKey
        ? resolveInpaintToolShortcut(event)
        : null;
      const inpaintToolShortcutEnabled =
        !selectedPageEditLocked &&
        ((activeLayer === "inpaintMask" && layerVisibility.inpaint && layerVisibility.inpaintMask) ||
          (activeLayer === "inpaintResult" && layerVisibility.inpaint && layerVisibility.inpaintResult));
      if (inpaintToolShortcut && inpaintToolShortcutEnabled) {
        event.preventDefault();
        selectSharedInpaintTool(inpaintToolShortcut);
        return;
      }

      const selectionClearShortcut =
        (event.key === "Delete" || event.key === "Backspace") &&
        !modalOpen &&
        !editableTarget &&
        Boolean(inpaintSelectionRect) &&
        rangeToolActive &&
        (activeLayer === "inpaintMask" || activeLayer === "inpaintResult");
      if (selectionClearShortcut) {
        event.preventDefault();
        void clearSelectedInpaintSelection().then((handled) => {
          if (!handled) {
            pushStatus("선택 범위에서 지울 레이어 내용이 없습니다.");
          }
        });
        return;
      }

      const blockDeleteShortcut =
        (event.key === "Delete" || event.key === "Backspace") &&
        !modalOpen &&
        !editableTarget &&
        Boolean(selectedBlockIdRef.current);
      if (blockDeleteShortcut) {
        event.preventDefault();
        deleteSelectedBlock();
        return;
      }

      if ((event.key === "Delete" || event.key === "Backspace") && !modalOpen && !editableTarget) {
        if (inpaintSelectionRect) {
          return;
        }
      }

      const chapter = currentChapterRef.current;
      const pageIds = chapter?.pages.map((page) => page.id) ?? [];
      const activeElement = typeof document !== "undefined" ? document.activeElement : null;
      const navigation = resolveKeyboardPageNavigation({
        key: event.key,
        hasPages: pageIds.length > 0,
        modalOpen,
        editableTarget,
        centerPanelFocused: Boolean(workspacePanelRef.current && activeElement && workspacePanelRef.current.contains(activeElement))
      });

      if (!navigation) {
        return;
      }

      const nextPageId = resolveAdjacentPageId(pageIds, selectedPageIdRef.current, navigation.direction);
      if (!nextPageId) {
        return;
      }

      if (navigation.preventDefault) {
        event.preventDefault();
      }

      selectPageForReading(nextPageId);
    };

    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [
    activeLayer,
    clearSelectedInpaintSelection,
    deleteSelectedBlock,
    globalUndoActions,
    inpaintResultTool,
    inpaintSelectionRect,
    inpaintTool,
    layerVisibility,
    libraryWidgetOpen,
    modalOpen,
    pushStatus,
    rangeToolActive,
    selectLayer,
    selectPointerTool,
    selectRangeTool,
    selectSharedInpaintTool,
    selectZoomTool,
    selectPageForReading,
    selectedPageEditLocked,
    undoShortcutPlatform,
    zoomToolActive
  ]);

  const renameWork = useCallback((workId: string) => {
    const work = library.works.find((candidate) => candidate.id === workId);
    if (!work) {
      return;
    }
    setRenameTarget({ kind: "work", id: workId, title: work.title });
  }, [library.works]);

  const renameChapter = useCallback((chapterId: string) => {
    const chapter =
      library.works.flatMap((work) => work.chapters).find((candidate) => candidate.id === chapterId) ??
      (currentChapter ? { id: currentChapter.id, title: currentChapter.title } : null);
    if (!chapter) {
      return;
    }
    setRenameTarget({ kind: "chapter", id: chapterId, title: chapter.title });
  }, [currentChapter, library.works]);

  const submitRename = useCallback(async (title: string) => {
    if (!renameTarget) {
      return;
    }

    setRenameBusy(true);
    try {
      if (renameTarget.kind === "work") {
        setLibrary(await window.mangaApi.renameWork(renameTarget.id, title));
      } else {
        if (currentChapter?.id === renameTarget.id && dirty) {
          await saveNow();
        }
        setLibrary(await window.mangaApi.renameChapter(renameTarget.id, title));
        if (currentChapter?.id === renameTarget.id) {
          applyChapter(await window.mangaApi.openChapter(renameTarget.id));
        }
      }
      setRenameTarget(null);
    } finally {
      setRenameBusy(false);
    }
  }, [applyChapter, currentChapter, dirty, renameTarget, saveNow]);

  const deleteRenameTarget = useCallback(async () => {
    if (!renameTarget) {
      return;
    }

    const isCurrentChapter = currentChapter?.id === renameTarget.id;
    const isCurrentWork = renameTarget.kind === "work" && currentChapter?.workId === renameTarget.id;
    const confirmed = await window.mangaApi.confirm(
      renameTarget.kind === "work" ? "작품 삭제" : "화 삭제",
      "정말 삭제하시겠습니까?",
      renameTarget.kind === "work"
        ? `"${renameTarget.title}" 작품과 포함된 모든 화, 페이지, 번역 결과가 보관함에서 삭제됩니다.`
        : `"${renameTarget.title}" 화와 포함된 모든 페이지, 번역 결과가 보관함에서 삭제됩니다.`
    );
    if (!confirmed) {
      return;
    }

    setRenameBusy(true);
    try {
      if ((isCurrentChapter || isCurrentWork) && dirty) {
        await saveNow();
      }

      if (renameTarget.kind === "work") {
        setLibrary(await window.mangaApi.deleteWork(renameTarget.id));
        if (isCurrentWork) {
          clearCurrentChapter();
        }
        pushStatus(`${renameTarget.title} 작품을 삭제했습니다.`);
      } else {
        setLibrary(await window.mangaApi.deleteChapter(renameTarget.id));
        if (isCurrentChapter) {
          clearCurrentChapter();
        }
        pushStatus(`${renameTarget.title} 화를 삭제했습니다.`);
      }

      setRenameTarget(null);
    } catch (error) {
      console.error(error);
      pushStatus(renameTarget.kind === "work" ? "작품을 삭제하지 못했습니다." : "화를 삭제하지 못했습니다.");
    } finally {
      setRenameBusy(false);
    }
  }, [clearCurrentChapter, currentChapter?.id, currentChapter?.workId, dirty, pushStatus, renameTarget, saveNow]);

  const openSettings = useCallback(async () => {
    if (settings) {
      setSettingsOpen(true);
      return;
    }

    setSettingsBusy(true);
    try {
      await refreshSettings();
      setSettingsOpen(true);
    } catch (error) {
      console.error(error);
      pushStatus("설정을 불러오지 못했습니다.");
    } finally {
      setSettingsBusy(false);
    }
  }, [pushStatus, refreshSettings, settings]);

  const submitSettings = useCallback(async (nextSettings: AppSettings) => {
    setSettingsBusy(true);
    try {
      const saved = await window.mangaApi.saveSettings(nextSettings);
      setSettings(saved);
      setSettingsOpen(false);
      pushStatus("설정을 저장했습니다. 다음 번 번역 실행부터 적용됩니다.");
    } catch (error) {
      console.error(error);
      pushStatus("설정을 저장하지 못했습니다.");
    } finally {
      setSettingsBusy(false);
    }
  }, [pushStatus]);

  const resetSettings = useCallback(async () => {
    setSettingsBusy(true);
    try {
      const reset = await window.mangaApi.resetSettings();
      setSettings(reset);
      pushStatus("설정을 기본값으로 복원했습니다. 다음 번 번역 실행부터 적용됩니다.");
    } catch (error) {
      console.error(error);
      pushStatus("기본 설정을 복원하지 못했습니다.");
    } finally {
      setSettingsBusy(false);
    }
  }, [pushStatus]);

  const prepareLamaFromEmptyState = useCallback(async () => {
    if (FORCE_INCOMPLETE_LAMA_NOTICE) {
      setLamaActionMessage("테스트 표시 모드입니다. 실제 환경 준비는 실행하지 않습니다.");
      return;
    }
    setLamaActionBusy(true);
    setLamaActionMessage("LaMa 환경 준비를 시작합니다.");
    try {
      const next = await window.mangaApi.prepareLamaRuntime();
      setLamaStatus(next);
      setLamaActionMessage(next.pythonAvailable ? "LaMa 환경 준비 중입니다." : `Python 설치가 필요합니다: ${next.pythonInstallCommand}`);
    } catch (error) {
      console.error(error);
      setLamaActionMessage(error instanceof Error ? error.message : String(error));
    } finally {
      setLamaActionBusy(false);
    }
  }, []);

  const downloadLamaModelFromEmptyState = useCallback(async () => {
    if (FORCE_INCOMPLETE_LAMA_NOTICE) {
      setLamaActionMessage("테스트 표시 모드입니다. 실제 모델 다운로드는 실행하지 않습니다.");
      return;
    }
    setLamaActionBusy(true);
    setLamaActionMessage("LaMa 모델 다운로드를 시작합니다.");
    try {
      const next = await window.mangaApi.downloadLamaModel();
      setLamaStatus(next);
      setLamaActionMessage(next.modelExists ? "LaMa 모델이 준비되어 있습니다." : "LaMa 모델 다운로드 중입니다.");
    } catch (error) {
      console.error(error);
      setLamaActionMessage(error instanceof Error ? error.message : String(error));
    } finally {
      setLamaActionBusy(false);
    }
  }, []);

  const layerToolPanel = (
    <section className="layer-tool-panel left-tool-panel">
      <h2>{activeLayer === "overlay" ? "폰트 설정" : activeLayer === "inpaintMask" ? "마스크 도구" : activeLayer === "inpaintResult" ? "결과 레이어 도구" : "도구"}</h2>
      {activeLayer === "overlay" ? (
        <>
          {fontControlValues ? (
            <>
              <div className="compact-tool-field font-picker-field">
                <span>서체</span>
                <FontFamilyPicker
                  options={fontFamilyOptions}
                  value={fontControlValues.fontFamily ?? DEFAULT_OVERLAY_FONT_FAMILY}
                  disabled={selectedPageEditLocked}
                  onChange={(fontFamily) => updateSelectedBlockFontSetting({ fontFamily })}
                />
              </div>
              <div className="font-metrics-row font-tool-grid">
                <label className="compact-tool-field font-number-field">
                  <span>폰트 크기</span>
                  <CompactNumberControl
                    ariaLabel="폰트 크기"
                    min={8}
                    max={120}
                    step={1}
                    value={fontControlValues.fontSizePx}
                    suffix="px"
                    disabled={selectedPageEditLocked}
                    onChange={(fontSizePx) => updateSelectedBlockFontSetting({ fontSizePx })}
                  />
                  {renderFontPresetLinkButton("fontSizePx", "폰트 크기")}
                </label>
                <label className="compact-tool-field font-number-field">
                  <span>줄 간격</span>
                  <CompactNumberControl
                    ariaLabel="줄 간격"
                    min={0.8}
                    max={2}
                    step={0.05}
                    value={fontControlValues.lineHeight}
                    suffix="배"
                    disabled={selectedPageEditLocked}
                    onChange={(lineHeight) => updateSelectedBlockFontSetting({ lineHeight })}
                  />
                  {renderFontPresetLinkButton("lineHeight", "줄 간격")}
                </label>
              </div>
              <FontOutlineControls
                values={fontControlValues}
                disabled={selectedPageEditLocked}
                onChange={updateSelectedBlockFontSetting}
                renderLinkButton={renderFontPresetLinkButton}
              />
              <label className="compact-tool-field font-color-field">
                <span>글자색</span>
                <span className="color-picker-shell" style={{ backgroundColor: fontControlValues.textColor ?? "#111111" }}>
                  <input
                    type="color"
                    className="outline-color-input"
                    value={fontControlValues.textColor ?? "#111111"}
                    disabled={selectedPageEditLocked}
                    onChange={(event) => updateSelectedBlockFontSetting({ textColor: event.target.value })}
                  />
                </span>
                {renderFontPresetLinkButton("textColor", "글자색")}
              </label>
              <div className="compact-tool-field font-screentone-field">
                <label className="tool-checkbox font-checkbox-field">
                  <input
                    type="checkbox"
                    checked={fontControlValues.screentoneFillEnabled ?? false}
                    disabled={selectedPageEditLocked}
                    onChange={(event) => updateSelectedBlockFontSetting({ screentoneFillEnabled: event.target.checked })}
                  />
                  <span>스크린톤 채우기</span>
                  <button
                    type="button"
                    className={`font-inline-toggle ${fontControlValues.screentoneFillAntialias ?? true ? "active" : ""}`}
                    disabled={selectedPageEditLocked}
                    onClick={(event) => {
                      event.preventDefault();
                      updateSelectedBlockFontSetting({ screentoneFillAntialias: !(fontControlValues.screentoneFillAntialias ?? true) });
                    }}
                    aria-pressed={fontControlValues.screentoneFillAntialias ?? true}
                    title={`스크린톤 안티 ${(fontControlValues.screentoneFillAntialias ?? true) ? "끄기" : "켜기"}`}
                  >
                    안티
                  </button>
                  {renderFontPresetLinkButton("screentoneFillEnabled", "스크린톤 채우기")}
                </label>
                <div className="font-screentone-range-row">
                  <label className="compact-tool-field font-range-field">
                    <span>
                      <span>강도</span>
                      <strong>{Math.round((fontControlValues.screentoneFillIntensity ?? 0.55) * 100)}%</strong>
                    </span>
                    <input
                      type="range"
                      min={0.05}
                      max={1}
                      step={0.01}
                      value={fontControlValues.screentoneFillIntensity ?? 0.55}
                      style={rangeProgressStyle(fontControlValues.screentoneFillIntensity ?? 0.55, 0.05, 1)}
                      disabled={selectedPageEditLocked}
                      onChange={(event) => updateSelectedBlockFontSetting({ screentoneFillIntensity: Number(event.target.value) })}
                    />
                    {renderFontPresetLinkButton("screentoneFillIntensity", "스크린톤 강도")}
                  </label>
                  <label className="compact-tool-field font-range-field">
                    <span>
                      <span>밀도</span>
                      <strong>{Math.round((fontControlValues.screentoneFillDensity ?? 0.55) * 100)}%</strong>
                    </span>
                    <input
                      type="range"
                      min={0.05}
                      max={1}
                      step={0.01}
                      value={fontControlValues.screentoneFillDensity ?? 0.55}
                      style={rangeProgressStyle(fontControlValues.screentoneFillDensity ?? 0.55, 0.05, 1)}
                      disabled={selectedPageEditLocked}
                      onChange={(event) => updateSelectedBlockFontSetting({ screentoneFillDensity: Number(event.target.value) })}
                    />
                    {renderFontPresetLinkButton("screentoneFillDensity", "스크린톤 밀도")}
                  </label>
                </div>
              </div>
              {selectedBlock ? (
                <div className="compact-tool-field font-align-field">
                  <span>정렬</span>
                  <div className="text-align-control" role="group" aria-label="텍스트 정렬">
                    <button
                      className={selectedBlock.textAlign === "left" ? "active" : ""}
                      disabled={selectedPageEditLocked}
                      onClick={() => updateSelectedBlockFontSetting({ textAlign: "left" })}
                    >
                      좌
                    </button>
                    <button
                      className={selectedBlock.textAlign === "center" ? "active" : ""}
                      disabled={selectedPageEditLocked}
                      onClick={() => updateSelectedBlockFontSetting({ textAlign: "center" })}
                    >
                      중앙
                    </button>
                    <button
                      className={selectedBlock.textAlign === "right" ? "active" : ""}
                      disabled={selectedPageEditLocked}
                      onClick={() => updateSelectedBlockFontSetting({ textAlign: "right" })}
                    >
                      우
                    </button>
                  </div>
                </div>
              ) : null}
              <label className="tool-checkbox compact-tool-field font-checkbox-field">
                <input
                  type="checkbox"
                  checked={fontControlValues.autoFitText ?? true}
                  disabled={selectedPageEditLocked}
                  onChange={(event) => updateSelectedBlockFontSetting({ autoFitText: event.target.checked })}
                />
                <span>자동 맞춤</span>
                {renderFontPresetLinkButton("autoFitText", "자동 맞춤")}
              </label>
            </>
          ) : (
            <p className="muted-line">블록이나 프리셋을 선택하면 폰트값을 조정할 수 있습니다.</p>
          )}
          <div className="font-preset-panel">
            <div className="font-preset-create">
              <input
                value={fontPresetName}
                disabled={selectedPageEditLocked || !currentChapter}
                placeholder="새 프리셋 이름"
                onChange={(event) => setFontPresetName(event.target.value)}
              />
              <button type="button" disabled={selectedPageEditLocked || !currentChapter} onClick={createFontPresetFromSelectedBlock}>
                만들기
              </button>
            </div>
            <div className="font-preset-tags" aria-label="폰트 프리셋">
              {fontPresets.map((preset) => (
                <span
                  key={preset.id}
                  className={`font-preset-tag ${
                    selectedBlock?.fontPresetId === preset.id || (!selectedBlock && editingFontPresetId === preset.id) ? "active" : ""
                  }`}
                >
                  <button
                    type="button"
                    className="font-preset-tag-name"
                    disabled={selectedPageEditLocked}
                    onClick={() => selectFontPreset(preset.id)}
                    title={selectedBlock ? `${preset.name} 적용` : `${preset.name} 편집`}
                  >
                    {preset.name}
                  </button>
                  <button
                    type="button"
                    className="font-preset-tag-remove"
                    disabled={selectedPageEditLocked}
                    onClick={() => deleteFontPreset(preset.id)}
                    aria-label={`${preset.name} 삭제`}
                  >
                    ×
                  </button>
                </span>
              ))}
              {selectedBlock?.fontPresetId ? (
                <button type="button" className="font-preset-clear" disabled={selectedPageEditLocked} onClick={clearSelectedBlockFontPreset}>
                  프리셋 해제
                </button>
              ) : null}
            </div>
          </div>
        </>
      ) : activeLayer === "inpaintMask" ? (
        <>
          <div className="segmented-control tool-selector mask-tool-selector" role="group" aria-label="인페인트 도구">
            <InpaintToolButton
              active={inpaintTool === "brush"}
              icon="brush"
              label="브러시"
              shortcut={INPAINT_TOOL_SHORTCUTS.brush}
              onClick={() => selectSharedInpaintTool("brush")}
              disabled={selectedPageEditLocked || !layerVisibility.inpaint || !layerVisibility.inpaintMask}
            />
            <InpaintToolButton
              active={inpaintTool === "eraser"}
              icon="eraser"
              label="지우개"
              shortcut={INPAINT_TOOL_SHORTCUTS.eraser}
              onClick={() => selectSharedInpaintTool("eraser")}
              disabled={selectedPageEditLocked || !layerVisibility.inpaint || !layerVisibility.inpaintMask}
            />
          </div>
          <div className="result-tool-settings mask-tool-settings">
            <label className="compact-tool-field result-size-field">
              <span>브러시 크기</span>
              <CompactNumberControl
                ariaLabel="마스크 브러시 크기"
                min={INPAINT_MASK_BRUSH_SIZE_MIN}
                max={INPAINT_MASK_BRUSH_SIZE_MAX}
                step={1}
                value={inpaintBrushSize}
                suffix="px"
                disabled={selectedPageEditLocked || !layerVisibility.inpaint || !layerVisibility.inpaintMask}
                onChange={(brushSize) => setInpaintBrushSize(clampInpaintMaskBrushSize(brushSize))}
              />
            </label>
          </div>
          <div className="result-action-grid mask-action-grid">
            <button type="button" onClick={() => setInpaintSelectionRect(null)} disabled={selectedPageEditLocked || !inpaintSelectionRect}>
              선택 해제
            </button>
            <button
              type="button"
              onClick={() => void fillSelectedInpaintSelection()}
              disabled={selectedPageEditLocked || !inpaintSelectionRect || !rangeToolActive}
            >
              선택 범위 채우기
            </button>
            <button
              type="button"
              onClick={() => updateSelectedPageInpaintMask(undefined)}
              disabled={selectedPageEditLocked || !(selectedPage?.inpaintMaskDataUrl ?? selectedPage?.inpaintLayerDataUrl)}
            >
              인페인트 마스크 비우기
            </button>
            <button
              type="button"
              onClick={() => void rerunInpaintForSelection()}
              disabled={selectedPageEditLocked || inpaintBusy || !inpaintSelectionRect || !(selectedPage?.inpaintMaskDataUrl ?? selectedPage?.inpaintLayerDataUrl)}
            >
              선택 범위만 다시 인페인트
            </button>
          </div>
        </>
      ) : activeLayer === "image" ? (
        <p className="muted-line">원본 이미지 레이어에는 사용할 도구가 없습니다.</p>
      ) : activeLayer === "inpaint" ? (
        <p className="muted-line">하위 레이어를 선택해 결과를 보거나 마스크를 편집하세요.</p>
      ) : activeLayer === "inpaintResult" ? (
        <>
          <div className="segmented-control tool-selector result-tool-grid" role="group" aria-label="인페인트 결과 도구">
            <InpaintToolButton
              active={inpaintResultTool === "brush"}
              icon="brush"
              label="브러시"
              shortcut={INPAINT_TOOL_SHORTCUTS.brush}
              onClick={() => selectSharedInpaintTool("brush")}
              disabled={selectedPageEditLocked || !layerVisibility.inpaint || !layerVisibility.inpaintResult}
            />
            <InpaintToolButton
              active={inpaintResultTool === "eraser"}
              icon="eraser"
              label="지우개"
              shortcut={INPAINT_TOOL_SHORTCUTS.eraser}
              onClick={() => selectSharedInpaintTool("eraser")}
              disabled={selectedPageEditLocked || !layerVisibility.inpaint || !layerVisibility.inpaintResult}
            />
            <InpaintToolButton
              active={inpaintResultTool === "blur"}
              icon="blur"
              label="흐림"
              onClick={() => selectInpaintResultEditTool("blur")}
              disabled={selectedPageEditLocked || !layerVisibility.inpaint || !layerVisibility.inpaintResult}
            />
            <InpaintToolButton
              active={inpaintResultTool === "sharpen"}
              icon="sharpen"
              label="선명"
              onClick={() => selectInpaintResultEditTool("sharpen")}
              disabled={selectedPageEditLocked || !layerVisibility.inpaint || !layerVisibility.inpaintResult}
            />
            <InpaintToolButton
              active={inpaintResultTool === "smudge"}
              icon="smudge"
              label="뭉개기"
              onClick={() => selectInpaintResultEditTool("smudge")}
              disabled={selectedPageEditLocked || !layerVisibility.inpaint || !layerVisibility.inpaintResult}
            />
          </div>
          <div className="result-tool-settings">
            <label className="compact-tool-field result-color-field">
              <span>색상</span>
              <span className="color-picker-shell" style={{ backgroundColor: inpaintResultBrushColor }}>
                <input
                  type="color"
                  className="outline-color-input"
                  value={inpaintResultBrushColor}
                  disabled={selectedPageEditLocked || !layerVisibility.inpaint || !layerVisibility.inpaintResult || inpaintResultTool !== "brush"}
                  onChange={(event) => setInpaintResultBrushColor(event.target.value)}
                />
              </span>
            </label>
            <label className="compact-tool-field result-size-field">
              <span>크기</span>
              <CompactNumberControl
                ariaLabel="결과 브러시 크기"
                min={INPAINT_RESULT_BRUSH_SIZE_MIN}
                max={INPAINT_RESULT_BRUSH_SIZE_MAX}
                step={1}
                value={inpaintResultBrushSize}
                suffix="px"
                disabled={selectedPageEditLocked || !layerVisibility.inpaint || !layerVisibility.inpaintResult}
                onChange={(brushSize) => setInpaintResultBrushSize(clampInpaintResultBrushSize(brushSize))}
              />
            </label>
            <label className="compact-tool-field">
              <span>
                <span>가장자리</span>
                <strong>{Math.round(inpaintResultBrushHardness * 100)}%</strong>
              </span>
              <input
                type="range"
                min={0}
                max={1}
                step={0.01}
                value={inpaintResultBrushHardness}
                style={rangeProgressStyle(inpaintResultBrushHardness, 0, 1)}
                disabled={selectedPageEditLocked || !layerVisibility.inpaint || !layerVisibility.inpaintResult}
                onChange={(event) => setInpaintResultBrushHardness(Number(event.target.value))}
              />
            </label>
            <label className="compact-tool-field">
              <span>
                <span>강도</span>
                <strong>{Math.round(inpaintResultToolStrength * 100)}%</strong>
              </span>
              <input
                type="range"
                min={0.05}
                max={1}
                step={0.01}
                value={inpaintResultToolStrength}
                style={rangeProgressStyle(inpaintResultToolStrength, 0.05, 1)}
                disabled={selectedPageEditLocked || !layerVisibility.inpaint || !layerVisibility.inpaintResult || inpaintResultTool === "brush" || inpaintResultTool === "eraser"}
                onChange={(event) => setInpaintResultToolStrength(Number(event.target.value))}
              />
            </label>
          </div>
          <div className="result-action-grid">
            <button type="button" onClick={() => setInpaintSelectionRect(null)} disabled={selectedPageEditLocked || !inpaintSelectionRect}>
              선택 해제
            </button>
            <button
              type="button"
              onClick={() => void fillSelectedInpaintSelection()}
              disabled={selectedPageEditLocked || !inpaintSelectionRect || !rangeToolActive}
            >
              선택 범위 채우기
            </button>
            <button
              type="button"
              onClick={() => updateSelectedPageInpaintResult(undefined)}
              disabled={selectedPageEditLocked || !selectedPage?.inpaintResultDataUrl}
            >
              인페인트 결과 비우기
            </button>
            <button
              type="button"
              onClick={() => void rerunInpaintWithCurrentMask()}
              disabled={selectedPageEditLocked || inpaintBusy || !(selectedPage?.inpaintMaskDataUrl ?? selectedPage?.inpaintLayerDataUrl)}
            >
              마스크 유지하고 인페인트 다시하기
            </button>
            <button
              type="button"
              onClick={() => void rerunInpaintForSelection()}
              disabled={selectedPageEditLocked || inpaintBusy || !inpaintSelectionRect || !(selectedPage?.inpaintMaskDataUrl ?? selectedPage?.inpaintLayerDataUrl)}
            >
              선택 범위만 다시 인페인트
            </button>
          </div>
        </>
      ) : (
        <p className="muted-line">최종 아웃풋 레이어에는 사용할 도구가 없습니다.</p>
      )}
    </section>
  );

  const notificationDock = showNotificationDock ? (
    <aside className="notification-dock" aria-label="알림" aria-live="polite">
      {selectedPageInpaintNotice ? (
        <section className={`notification-card ${selectedPageInpaintNotice.tone}`}>
          <div className="notification-copy">
            <strong>{selectedPageInpaintNotice.title}</strong>
            <span>{selectedPageInpaintNotice.message}</span>
          </div>
        </section>
      ) : null}
      {statusToastLine ? (
        <section className={`notification-card ${statusWidgetTone}`}>
          <div className="notification-copy">
            <strong>알림</strong>
            <span>{statusToastLine}</span>
          </div>
        </section>
      ) : null}
    </aside>
  ) : null;

  const statusHistoryPanel = statusWidgetOpen ? (
    <section className={`status-history-panel ${statusWidgetTone}`}>
      <div className="notification-panel-header">
        <h2>상태 기록</h2>
        <button type="button" className="notification-panel-close" onClick={() => setStatusWidgetOpen(false)} aria-label="상태 기록 닫기">
          ×
        </button>
      </div>
      <div className={`job-pill ${jobState.status}`}>{jobState.progressText}</div>
      <div className="status-log-scroll">
        {statusLines.length ? (
          statusLines.map((line, index) => <p key={`${line}-${index}`}>{line}</p>)
        ) : (
          <p className="muted-line">아직 표시할 상태가 없습니다.</p>
        )}
      </div>
    </section>
  ) : null;

  const stageZoomOverlay = selectedPage ? (
    <div className="stage-zoom-overlay" aria-label="만화 확대/축소">
      <button type="button" onClick={() => zoomStage(1 / STAGE_ZOOM_STEP)} aria-label="축소" title="축소">
        -
      </button>
      <span className="stage-zoom-value" aria-live="polite">{stageZoomLabel}</span>
      <button type="button" onClick={() => zoomStage(STAGE_ZOOM_STEP)} aria-label="확대" title="확대">
        +
      </button>
      <button type="button" onClick={() => setStageViewScale(1)} title="원본 사이즈 보기">
        원본
      </button>
      <button type="button" onClick={fitStageToWorkspace} title="화면에 맞춤">
        맞춤
      </button>
    </div>
  ) : null;
  const stageToolOverlay = selectedPage ? (
    <div className="stage-tool-overlay" aria-label="전역 도구">
      <button
        type="button"
        className={!zoomToolActive && !rangeToolActive ? "active" : ""}
        aria-label="일반 마우스"
        aria-pressed={!zoomToolActive && !rangeToolActive}
        aria-keyshortcuts="A"
        title="일반 마우스 (A)"
        onClick={selectPointerTool}
      >
        <svg className="stage-tool-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
          <path d="M6 3l12 11-6.2 1.1L8.3 21 6 3z" />
        </svg>
        <span className="stage-tool-shortcut" aria-hidden="true">A</span>
      </button>
      <button
        type="button"
        className={rangeToolActive ? "active" : ""}
        aria-label="범위 선택"
        aria-pressed={rangeToolActive}
        aria-keyshortcuts={INPAINT_TOOL_SHORTCUTS.select}
        title={`범위 선택 (${INPAINT_TOOL_SHORTCUTS.select})`}
        onClick={selectRangeTool}
        disabled={selectedPageEditLocked}
      >
        <svg className="stage-tool-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
          <rect x="4" y="4" width="11" height="11" rx="2" strokeDasharray="2.4 2.4" />
          <path d="M13 12l6 6-3 1-1 3-6-6 4-4z" />
        </svg>
        <span className="stage-tool-shortcut" aria-hidden="true">{INPAINT_TOOL_SHORTCUTS.select}</span>
      </button>
      <button
        type="button"
        className={zoomToolActive ? "active" : ""}
        aria-label="줌 도구"
        aria-pressed={zoomToolActive}
        aria-keyshortcuts="Z"
        title="줌 도구 (Z)"
        onClick={selectZoomTool}
      >
        <svg className="stage-tool-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
          <circle cx="10.5" cy="10.5" r="5.8" />
          <path d="M15 15l5 5" />
          <path d="M10.5 7.5v6" />
          <path d="M7.5 10.5h6" />
        </svg>
        <span className="stage-tool-shortcut" aria-hidden="true">Z</span>
      </button>
    </div>
  ) : null;

  return (
    <main className={currentChapter ? "app-shell grid h-screen bg-canvas" : "app-shell no-left-rail grid h-screen bg-canvas"}>
      <input
        ref={imageImportInputRef}
        type="file"
        accept="image/png,image/jpeg,image/webp"
        multiple
        hidden
        onChange={(event) => void handleImportInputChange("images", event)}
      />
      <input
        ref={folderImportInputRef}
        type="file"
        accept="image/png,image/jpeg,image/webp"
        multiple
        hidden
        {...{ webkitdirectory: "" }}
        onChange={(event) => void handleImportInputChange("folder", event)}
      />
      <input
        ref={zipImportInputRef}
        type="file"
        accept=".zip,application/zip"
        hidden
        onChange={(event) => void handleImportInputChange("zip", event)}
      />
      <input
        ref={batchImportInputRef}
        type="file"
        accept="image/png,image/jpeg,image/webp,.zip,application/zip"
        multiple
        hidden
        {...{ webkitdirectory: "" }}
        onChange={(event) => void handleImportInputChange("zip-folder", event)}
      />
      <header className="context-bar">
        <div className="context-bar-left">
          <div className="context-library-anchor" ref={libraryAnchorRef}>
            <button
              type="button"
              className={libraryWidgetOpen ? "context-button active" : "context-button"}
              onClick={() => setLibraryWidgetOpen((current) => !current)}
              aria-expanded={libraryWidgetOpen}
              aria-controls="library-widget"
            >
              보관함
              <span className="panel-count">{libraryChapterCount}</span>
            </button>
            {libraryWidgetOpen ? (
              <div id="library-widget" className="library-widget">
                <LibraryTree
                  library={library}
                  currentChapterId={currentChapter?.id ?? null}
                  jobActive={jobActive}
                  collapsed={false}
                  onToggleCollapsed={() => setLibraryWidgetOpen(false)}
                  onOpenChapter={(chapterId) => {
                    setLibraryWidgetOpen(false);
                    void openChapter(chapterId);
                  }}
                  onRenameWork={(workId) => {
                    setLibraryWidgetOpen(false);
                    void renameWork(workId);
                  }}
                  onRenameChapter={(chapterId) => {
                    setLibraryWidgetOpen(false);
                    void renameChapter(chapterId);
                  }}
                  onReorderChapter={(workId, sourceChapterId, targetChapterId) => {
                    const work = library.works.find((candidate) => candidate.id === workId);
                    if (!work) {
                      return;
                    }
                    const nextOrder = reorderByTarget(work.chapterOrder, sourceChapterId, targetChapterId);
                    void window.mangaApi.reorderChapters(workId, nextOrder).then(setLibrary);
                  }}
                />
              </div>
            ) : null}
          </div>
          <div className="context-import-group">
            <span className="context-label">가져오기</span>
            <div className="import-actions grid grid-cols-4 gap-1.5">
              <button onClick={() => selectImportFiles("images")} disabled={jobActive}>
                이미지
              </button>
              <button onClick={() => selectImportFiles("folder")} disabled={jobActive}>
                폴더
              </button>
              <button onClick={() => selectImportFiles("zip")} disabled={jobActive}>
                압축파일
              </button>
              <button onClick={() => selectImportFiles("zip-folder")} disabled={jobActive}>
                일괄 번역
              </button>
            </div>
          </div>
        </div>
        <div className="context-bar-right">
          <div className="context-chapter-chip" title={currentChapter?.title ?? "현재 화 없음"}>
            <strong>{currentChapter?.title ?? "현재 화 없음"}</strong>
            <span>{currentChapter ? `${currentChapter.pages.length}p` : "대기"}</span>
          </div>
          <div className="context-run-group" aria-label="번역 실행">
            <div className="context-run-actions">
              <button className="primary" onClick={() => void runAnalysis("pending")} disabled={!currentChapter || jobActive}>
                계속 번역 (AI)
              </button>
              <button onClick={() => void runAnalysis("all")} disabled={!currentChapter || jobActive}>
                전체 번역 (AI)
              </button>
              <button onClick={() => void renderSelectedPage()} disabled={!currentChapter || !selectedPage || jobActive || renderBusy}>
                {renderBusy ? "출력 중" : "페이지 출력"}
              </button>
              {jobActive ? (
                <button className="danger" onClick={() => void window.mangaApi.cancelJob()}>
                  취소
                </button>
              ) : null}
            </div>
            {showProgressBar && progressSnapshot ? (
              <div className="context-progress">
                <div className="context-progress-meta">
                  <span>{jobState.progressText}</span>
                  {progressSnapshot.mode === "determinate" ? (
                    <strong>
                      {progressSnapshot.current} / {progressSnapshot.total}
                    </strong>
                  ) : (
                    <strong>준비 중</strong>
                  )}
                </div>
                <div className={`progress-track ${progressSnapshot.mode === "indeterminate" ? "indeterminate" : ""}`} aria-hidden="true">
                  <div
                    className={`progress-fill ${progressSnapshot.mode === "indeterminate" ? "indeterminate" : ""}`}
                    style={
                      progressSnapshot.mode === "determinate"
                        ? { width: `${Math.round(progressSnapshot.ratio * 100)}%` }
                        : undefined
                    }
                  />
                </div>
              </div>
            ) : null}
          </div>
          <span className={`context-status-indicator ${statusWidgetTone}`} aria-label={`상태: ${statusIndicatorLabel}`} title={statusIndicatorLabel} />
          <button className="ghost-button context-settings" onClick={() => void openSettings()} disabled={settingsBusy && !settingsOpen}>
            설정
          </button>
        </div>
      </header>

      <aside className="sidebar flex min-h-0 flex-col gap-3 overflow-hidden">
        {layerToolPanel}

        <PageList
          pages={currentChapter?.pages ?? []}
          selectedPageId={selectedPage?.id ?? null}
          jobActive={jobActive}
          onSelect={selectPageForReading}
          onRetranslate={(pageId) => void retranslatePage(pageId)}
          onRemove={(pageId) => void removePage(pageId)}
          onToggleProgress={togglePageProgress}
          onReorder={(sourcePageId, targetPageId) => {
            if (!currentChapter) {
              return;
            }
            const nextOrder = reorderByTarget(currentChapter.pageOrder, sourcePageId, targetPageId);
            void window.mangaApi.reorderPages(currentChapter.id, nextOrder).then((chapter) => {
              applyChapter(chapter);
              void refreshLibrary();
            });
          }}
        />
      </aside>

      <section
        ref={workspacePanelRef}
        className="workspace relative grid place-items-center outline-none"
        tabIndex={0}
        aria-label="읽기 영역"
        onMouseDown={() => workspacePanelRef.current?.focus()}
      >
        {stageZoomOverlay}
        {stageToolOverlay}
        {notificationDock}
        {statusHistoryPanel}
        <button
          type="button"
          className={`status-history-button ${statusWidgetTone}`}
          onClick={() => setStatusWidgetOpen((current) => !current)}
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
              inpaintDisabled={selectedPageEditLocked || inpaintBusy || activeLayer !== "inpaintMask" || !layerVisibility.inpaint || !layerVisibility.inpaintMask || inpaintTool === "select"}
              inpaintResultDisabled={selectedPageEditLocked || inpaintBusy || activeLayer !== "inpaintResult" || !layerVisibility.inpaint || !layerVisibility.inpaintResult || inpaintResultTool === "select"}
              rangeSelectionDisabled={selectedPageEditLocked || inpaintBusy || !layerVisibility.inpaint}
              temporaryPanActive={temporaryPanActive}
              inpaintSelectionRect={inpaintSelectionRect}
              onInpaintLayerChange={updateSelectedPageInpaintMask}
              onInpaintSelectionChange={setInpaintSelectionRect}
              onInpaintResultLayerChange={updateSelectedPageInpaintResult}
              onZoomToolClick={handleZoomToolClick}
              onStagePointerMove={onStagePointerMove}
              onStagePointerUp={onStagePointerUp}
              onStagePointerDown={() => {
                if (activeLayer === "overlay" && !temporaryPanActive) {
                  setSelectedBlockId(null);
                }
              }}
              onBlockPointerDown={onBlockPointerDown}
            />
          </div>
        ) : (
          <>
            {showLamaEmptyNotice && displayedLamaStatus ? (
              <div className="empty-state-stack max-w-xl">
              <section className="empty-lama-card">
                <div className="empty-lama-header">
                  <h2>LaMa 인페인트 준비</h2>
                  <button type="button" onClick={() => void refreshLamaStatus()} disabled={lamaActionBusy}>
                    새로고침
                  </button>
                </div>
                <div className="empty-lama-status-grid">
                  <LamaStatusPill label="Python" ready={displayedLamaStatus.pythonAvailable} busy={false} />
                  <LamaStatusPill label="런타임" ready={displayedLamaStatus.runtimeReady} busy={displayedLamaStatus.runtimePreparing} />
                  <LamaStatusPill label="모델" ready={displayedLamaStatus.modelExists} busy={displayedLamaStatus.modelDownloading} />
                </div>
                {FORCE_INCOMPLETE_LAMA_NOTICE ? (
                  <div className="empty-lama-test-platform" aria-label="테스트 OS">
                    {LAMA_TEST_PLATFORM_OPTIONS.map((option) => (
                      <button
                        key={option.value}
                        type="button"
                        className={lamaNoticePlatform === option.value ? "active" : ""}
                        onClick={() => setLamaNoticePlatform(option.value)}
                      >
                        {option.label}
                      </button>
                    ))}
                  </div>
                ) : null}
                {!displayedLamaStatus.pythonAvailable ? <EmptyPythonInstallHelp status={displayedLamaStatus} /> : null}
                <div className="empty-actions flex flex-wrap justify-center gap-2.5">
                  <button type="button" onClick={() => void prepareLamaFromEmptyState()} disabled={lamaActionBusy || displayedLamaStatus.runtimePreparing}>
                    {displayedLamaStatus.runtimePreparing ? "환경 준비 중" : "환경 준비"}
                  </button>
                  <button type="button" onClick={() => void downloadLamaModelFromEmptyState()} disabled={lamaActionBusy || displayedLamaStatus.modelDownloading}>
                    {displayedLamaStatus.modelDownloading ? "모델 다운로드 중" : "모델 다운로드"}
                  </button>
                </div>
                <p className="muted-line">모델 경로: {displayedLamaStatus.modelPath}</p>
                {lamaActionMessage ? <p className="muted-line">{lamaActionMessage}</p> : null}
              </section>
              </div>
            ) : (
              <div className="empty-state max-w-xl text-center">
              <h2>보관함에서 화를 열거나 새로 가져오세요.</h2>
              <p>작품과 화 단위로 저장해두고, 이어서 번역하거나 페이지별로 다시 번역할 수 있습니다.</p>
              <div className="empty-actions flex flex-wrap justify-center gap-2.5">
                <button onClick={() => selectImportFiles("images")}>이미지 열기</button>
                <button onClick={() => selectImportFiles("folder")}>폴더 열기</button>
                <button onClick={() => selectImportFiles("zip")}>압축파일 열기</button>
                <button onClick={() => selectImportFiles("zip-folder")}>작품 일괄 번역</button>
              </div>
              </div>
            )}
          </>
        )}
      </section>

      <aside className="right-rail flex min-h-0 flex-col gap-3 overflow-hidden">
        <section className="layer-panel right-rail-layer-panel">
            <div className="layer-panel-header">
              <h2>레이어</h2>
              <label className="focus-mode-toggle">
                <span>FOCUS MODE</span>
                <input
                  type="checkbox"
                  checked={focusModeEnabled}
                  onChange={(event) => setFocusModeEnabled(event.target.checked)}
                />
                <span className="focus-mode-switch" aria-hidden="true" />
              </label>
            </div>
            <LayerControl
              label="1 최종 아웃풋"
              active={activeLayer === "output"}
              visible={true}
              opacity={1}
              onSelect={() => selectLayer("output")}
              onVisibleChange={() => undefined}
              onOpacityChange={() => undefined}
              viewOnly
            />
          <LayerControl
            label="2 번역 블록"
            active={activeLayer === "overlay"}
            visible={layerVisibility.overlay}
            opacity={overlayOpacityEditMode ? overlayBackgroundOpacity : layerOpacity.overlay}
            opacityEditMode={overlayOpacityEditMode}
            opacityEditModeLabel="배경 투명도 편집"
            onSelect={() => selectLayer("overlay")}
            onVisibleChange={(visible) => setLayerVisibility((current) => ({ ...current, overlay: visible }))}
            onOpacityEditModeChange={(enabled) => {
              setOverlayOpacityEditMode(enabled);
              if (enabled) {
                setLayerOpacity((current) => ({ ...current, overlay: 1 }));
              }
            }}
            onOpacityChange={(opacity) => {
              if (overlayOpacityEditMode) {
                updateSelectedPageBlockOpacity(opacity);
                return;
              }
              setLayerOpacity((current) => ({ ...current, overlay: opacity }));
            }}
          />
          <LayerControl
            label="인페인트 레이어"
            active={activeLayer === "inpaint" || activeLayer === "inpaintResult" || activeLayer === "inpaintMask"}
            visible={layerVisibility.inpaint}
            opacity={layerOpacity.inpaint}
            onSelect={() => {
              selectLayer("inpaint");
            }}
            onVisibleChange={(visible) =>
              setLayerVisibility((current) => ({
                ...current,
                inpaint: visible,
                inpaintResult: visible ? current.inpaintResult : false,
                inpaintMask: visible ? current.inpaintMask : false
              }))
            }
            onOpacityChange={(opacity) => setLayerOpacity((current) => ({ ...current, inpaint: opacity }))}
          />
          <div className="layer-subgroup">
            <LayerControl
              label="3 인페인트 결과"
              active={activeLayer === "inpaintResult"}
              visible={layerVisibility.inpaint && layerVisibility.inpaintResult}
              opacity={layerOpacity.inpaintResult}
              onSelect={() => {
                selectLayer("inpaintResult");
              }}
              onVisibleChange={(visible) => setLayerVisibility((current) => ({ ...current, inpaint: current.inpaint || visible, inpaintResult: visible }))}
              onOpacityChange={(opacity) => setLayerOpacity((current) => ({ ...current, inpaintResult: opacity }))}
              nested
            />
            <LayerControl
              label="4 인페인트 마스크"
              active={activeLayer === "inpaintMask"}
              visible={layerVisibility.inpaint && layerVisibility.inpaintMask}
              opacity={layerOpacity.inpaintMask}
              onSelect={() => {
                selectLayer("inpaintMask");
              }}
              onVisibleChange={(visible) => setLayerVisibility((current) => ({ ...current, inpaint: current.inpaint || visible, inpaintMask: visible }))}
              onOpacityChange={(opacity) => setLayerOpacity((current) => ({ ...current, inpaintMask: opacity }))}
              nested
            />
          </div>
          <LayerControl
            label="5 원본 이미지"
            active={activeLayer === "image"}
            visible={layerVisibility.image}
            opacity={layerOpacity.image}
            onSelect={() => selectLayer("image")}
            onVisibleChange={(visible) => setLayerVisibility((current) => ({ ...current, image: visible }))}
            onOpacityChange={(opacity) => setLayerOpacity((current) => ({ ...current, image: opacity }))}
          />
        </section>

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

      {importPreview ? (
        <ImportModal library={library} preview={importPreview} busy={importBusy} onCancel={() => setImportPreview(null)} onSubmit={(payload) => void submitImport(payload)} />
      ) : null}

      {renameTarget ? (
        <RenameModal
          kind={renameTarget.kind}
          initialTitle={renameTarget.title}
          busy={renameBusy}
          onCancel={() => {
            if (!renameBusy) {
              setRenameTarget(null);
            }
          }}
          onDelete={() => void deleteRenameTarget()}
          onSubmit={(title) => void submitRename(title)}
        />
      ) : null}

      {settingsOpen && settings ? (
        <SettingsModal
          initialSettings={settings}
          busy={settingsBusy}
          jobActive={jobActive}
          onCancel={() => {
            if (!settingsBusy) {
              setSettingsOpen(false);
            }
          }}
          onReset={() => void resetSettings()}
          onSubmit={(nextSettings) => void submitSettings(nextSettings)}
        />
      ) : null}
    </main>
  );
}
