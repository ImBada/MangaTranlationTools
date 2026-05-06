import React from "react";
import type { ChapterSnapshot, ImageRect, MangaPage, TranslationBlock } from "../../../shared/types";
import type { InpaintTool } from "../components/InpaintLayerCanvas";
import type { InpaintResultTool } from "../components/InpaintResultCanvas";
import type { GlobalUndoHistoryEntry, GlobalUndoKind } from "../lib/editorUndoHistory";
import type { InpaintLayerChangeOptions } from "../lib/inpaintLayerChange";
import type { ActiveLayer } from "../lib/layerState";
import type { FontWeightAvailability } from "../lib/overlayLayout";
import { useInpaintLayerPersistence } from "./useInpaintLayerPersistence";
import { useInpaintPsdActions } from "./useInpaintPsdActions";
import { useInpaintRunActions } from "./useInpaintRunActions";
import { useInpaintSelectionActions } from "./useInpaintSelectionActions";
import type { RecoverableFailureId } from "./useRecoverableFailures";

type UseInpaintActionsOptions = {
  activeLayer: ActiveLayer;
  applyChapter: (chapter: ChapterSnapshot | undefined, fallbackStatus?: string) => void;
  clearRecoverableFailure?: (id: RecoverableFailureId) => void;
  consumeGlobalUndoEntry: (kind: GlobalUndoKind, pageId?: string) => void;
  currentChapter: ChapterSnapshot | null;
  currentChapterId: string | null;
  currentChapterRef: React.RefObject<ChapterSnapshot | null>;
  dirty: boolean;
  fontWeightAvailability: readonly FontWeightAvailability[];
  mergeLiveChapter: (chapter: ChapterSnapshot) => void;
  pushStatus: (line: string) => void;
  rangeToolActive: boolean;
  recordGlobalUndoEntry: (entry: GlobalUndoHistoryEntry) => void;
  refreshLibrary: () => Promise<void>;
  reportRecoverableFailure?: (failure: { id: RecoverableFailureId; message: string; title: string }) => void;
  saveNow: () => Promise<void>;
  selectedBlock: TranslationBlock | null;
  selectedBlocks: TranslationBlock[];
  selectedPage: MangaPage | null;
  selectedPageCurrentId: string | null;
  selectedPageEditLocked: boolean;
  selectedPageIdRef: React.RefObject<string | null>;
  setCurrentChapter: React.Dispatch<React.SetStateAction<ChapterSnapshot | null>>;
  showInpaintLayers: () => void;
  signalSaveComplete: () => void;
};

type UseInpaintActionsState = {
  applyInpaintAllBlocks: () => Promise<void>;
  applyInpaintAllPages: () => Promise<void>;
  applyInpaintSelectedBlock: () => Promise<void>;
  canUndoInpaintMask: (pageId: string) => boolean;
  canUndoInpaintResult: (pageId: string) => boolean;
  clearInpaintUndoStacks: () => void;
  clearPendingInpaintSaves: () => void;
  clearSelectedInpaintSelection: () => Promise<boolean>;
  downloadLastImportedInpaintPsd: () => Promise<void>;
  exportSelectedPageInpaintPsd: () => Promise<void>;
  fillSelectedInpaintSelection: () => Promise<void>;
  flushInpaintMaskSave: () => Promise<void>;
  flushInpaintResultSave: () => Promise<void>;
  handleInpaintPsdInputChange: (event: React.ChangeEvent<HTMLInputElement>) => Promise<void>;
  inpaintBrushSize: number;
  inpaintBusy: boolean;
  inpaintPsdBusy: boolean;
  inpaintPsdInputRef: React.RefObject<HTMLInputElement | null>;
  inpaintResultBrushColor: string;
  inpaintResultBrushHardness: number;
  inpaintResultBrushSize: number;
  inpaintResultTool: InpaintResultTool;
  inpaintResultToolStrength: number;
  inpaintSelectionRect: ImageRect | null;
  inpaintTool: InpaintTool;
  lastImportedInpaintPsdAt: string | null;
  lastImportedInpaintPsdLabel: string | null;
  rerunInpaintForSelection: () => Promise<void>;
  rerunInpaintWithCurrentMask: () => Promise<void>;
  selectInpaintPsdFile: () => void;
  setInpaintBrushSize: React.Dispatch<React.SetStateAction<number>>;
  setInpaintResultBrushColor: React.Dispatch<React.SetStateAction<string>>;
  setInpaintResultBrushHardness: React.Dispatch<React.SetStateAction<number>>;
  setInpaintResultBrushSize: React.Dispatch<React.SetStateAction<number>>;
  setInpaintResultTool: React.Dispatch<React.SetStateAction<InpaintResultTool>>;
  setInpaintResultToolStrength: React.Dispatch<React.SetStateAction<number>>;
  setInpaintSelectionRect: React.Dispatch<React.SetStateAction<ImageRect | null>>;
  setInpaintTool: React.Dispatch<React.SetStateAction<InpaintTool>>;
  undoPageInpaint: (pageId: string) => void;
  undoPageInpaintResult: (pageId: string) => void;
  updateSelectedPageInpaintMask: (dataUrl: string | undefined, options?: InpaintLayerChangeOptions) => void;
  updateSelectedPageInpaintResult: (dataUrl: string | undefined, options?: InpaintLayerChangeOptions) => void;
};

export function useInpaintActions({
  activeLayer,
  applyChapter,
  clearRecoverableFailure,
  consumeGlobalUndoEntry,
  currentChapter,
  currentChapterId,
  currentChapterRef,
  dirty,
  fontWeightAvailability,
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
}: UseInpaintActionsOptions): UseInpaintActionsState {
  const [inpaintTool, setInpaintTool] = React.useState<InpaintTool>("select");
  const [inpaintSelectionRect, setInpaintSelectionRect] = React.useState<ImageRect | null>(null);
  const [inpaintBrushSize, setInpaintBrushSize] = React.useState(28);
  const [inpaintResultTool, setInpaintResultTool] = React.useState<InpaintResultTool>("select");
  const [inpaintResultBrushSize, setInpaintResultBrushSize] = React.useState(28);
  const [inpaintResultBrushColor, setInpaintResultBrushColor] = React.useState("#ffffff");
  const [inpaintResultBrushHardness, setInpaintResultBrushHardness] = React.useState(0.85);
  const [inpaintResultToolStrength, setInpaintResultToolStrength] = React.useState(0.55);

  React.useEffect(() => {
    setInpaintSelectionRect(null);
  }, [selectedPageCurrentId]);

  const {
    canUndoInpaintMask,
    canUndoInpaintResult,
    clearInpaintUndoStacks,
    clearPendingInpaintSaves,
    flushInpaintMaskSave,
    flushInpaintResultSave,
    recordInpaintMaskUndoSnapshot,
    undoPageInpaint,
    undoPageInpaintResult,
    updatePageInpaintStatus,
    updateSelectedPageInpaintMask,
    updateSelectedPageInpaintResult
  } = useInpaintLayerPersistence({
    clearRecoverableFailure,
    consumeGlobalUndoEntry,
    currentChapter,
    currentChapterRef,
    dirty,
    mergeLiveChapter,
    pushStatus,
    recordGlobalUndoEntry,
    refreshLibrary,
    reportRecoverableFailure,
    saveNow,
    selectedPage,
    selectedPageEditLocked,
    setCurrentChapter,
    signalSaveComplete
  });

  const {
    downloadLastImportedInpaintPsd,
    exportSelectedPageInpaintPsd,
    handleInpaintPsdInputChange,
    inpaintPsdBusy,
    inpaintPsdInputRef,
    lastImportedInpaintPsdAt,
    lastImportedInpaintPsdLabel,
    selectInpaintPsdFile
  } = useInpaintPsdActions({
    clearPendingInpaintSaves,
    currentChapter,
    currentChapterId,
    dirty,
    flushInpaintMaskSave,
    flushInpaintResultSave,
    fontWeightAvailability,
    mergeLiveChapter,
    pushStatus,
    recordInpaintMaskUndoSnapshot,
    refreshLibrary,
    saveNow,
    selectedPage,
    selectedPageCurrentId,
    selectedPageEditLocked,
    showInpaintLayers,
    signalSaveComplete
  });

  const {
    applyInpaintAllBlocks,
    applyInpaintAllPages,
    applyInpaintSelectedBlock,
    inpaintBusy,
    rerunInpaintForSelection,
    rerunInpaintWithCurrentMask
  } = useInpaintRunActions({
    applyChapter,
    clearRecoverableFailure,
    currentChapter,
    currentChapterRef,
    dirty,
    inpaintSelectionRect,
    pushStatus,
    reportRecoverableFailure,
    refreshLibrary,
    saveNow,
    selectedBlock,
    selectedBlocks,
    selectedPage,
    selectedPageEditLocked,
    selectedPageIdRef,
    signalSaveComplete,
    updatePageInpaintStatus,
    updateSelectedPageInpaintMask,
    updateSelectedPageInpaintResult
  });

  const {
    clearSelectedInpaintSelection,
    fillSelectedInpaintSelection
  } = useInpaintSelectionActions({
    activeLayer,
    currentChapterRef,
    inpaintBusy,
    inpaintResultBrushColor,
    inpaintSelectionRect,
    pushStatus,
    rangeToolActive,
    selectedPageEditLocked,
    selectedPageIdRef,
    updateSelectedPageInpaintMask,
    updateSelectedPageInpaintResult
  });

  return {
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
  };
}
