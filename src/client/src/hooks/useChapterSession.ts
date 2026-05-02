import React from "react";
import type { ChapterSnapshot } from "../../../shared/types";
import { mergeLiveChapterPreservingDirtyCompletedPages, resolveSelectionAfterChapterSync } from "../lib/chapterSync";
import { normalizeChapterTranslatedText } from "../lib/editorUtils";

type ChapterSessionOptions = {
  clearPendingChapterTimers?: () => void;
  clearUndoStacks: () => void;
  pushStatus: (line: string) => void;
  signalSaveComplete: () => void;
};

type ChapterSessionState = {
  applyChapter: (chapter: ChapterSnapshot | undefined, fallbackStatus?: string) => void;
  clearCurrentChapter: () => void;
  currentChapter: ChapterSnapshot | null;
  currentChapterRef: React.RefObject<ChapterSnapshot | null>;
  dirty: boolean;
  editingFontPresetId: string | null;
  editingFontPresetIdRef: React.RefObject<string | null>;
  markDirty: (pageId?: string) => void;
  mergeLiveChapter: (chapter: ChapterSnapshot) => void;
  openChapter: (chapterId: string) => Promise<void>;
  saveNow: () => Promise<void>;
  selectPageForReading: (pageId: string | null) => void;
  selectedBlockId: string | null;
  selectedBlockIdRef: React.RefObject<string | null>;
  selectedPageId: string | null;
  selectedPageIdRef: React.RefObject<string | null>;
  setCurrentChapter: React.Dispatch<React.SetStateAction<ChapterSnapshot | null>>;
  setEditingFontPresetId: React.Dispatch<React.SetStateAction<string | null>>;
  setSelectedBlockId: React.Dispatch<React.SetStateAction<string | null>>;
  setSelectedPageId: React.Dispatch<React.SetStateAction<string | null>>;
  updateCurrentChapter: (pageId: string | undefined, updater: (chapter: ChapterSnapshot) => ChapterSnapshot) => void;
};

function resolveStateAction<T>(action: React.SetStateAction<T>, current: T): T {
  return typeof action === "function" ? (action as (current: T) => T)(current) : action;
}

export function useChapterSession({
  clearPendingChapterTimers,
  clearUndoStacks,
  pushStatus,
  signalSaveComplete
}: ChapterSessionOptions): ChapterSessionState {
  const [currentChapter, setCurrentChapterState] = React.useState<ChapterSnapshot | null>(null);
  const [selectedPageId, setSelectedPageIdState] = React.useState<string | null>(null);
  const [selectedBlockId, setSelectedBlockIdState] = React.useState<string | null>(null);
  const [editingFontPresetId, setEditingFontPresetIdState] = React.useState<string | null>(null);
  const [dirty, setDirty] = React.useState(false);
  const saveTimerRef = React.useRef<number | null>(null);
  const dirtyVersionRef = React.useRef(0);
  const dirtyPageIdsRef = React.useRef<Set<string>>(new Set());
  const currentChapterRef = React.useRef<ChapterSnapshot | null>(null);
  const selectedPageIdRef = React.useRef<string | null>(null);
  const selectedBlockIdRef = React.useRef<string | null>(null);
  const editingFontPresetIdRef = React.useRef<string | null>(null);

  const setCurrentChapter = React.useCallback<React.Dispatch<React.SetStateAction<ChapterSnapshot | null>>>((action) => {
    setCurrentChapterState((current) => {
      const next = resolveStateAction(action, current);
      currentChapterRef.current = next;
      return next;
    });
  }, []);

  const setSelectedPageId = React.useCallback<React.Dispatch<React.SetStateAction<string | null>>>((action) => {
    setSelectedPageIdState((current) => {
      const next = resolveStateAction(action, current);
      selectedPageIdRef.current = next;
      return next;
    });
  }, []);

  const setSelectedBlockId = React.useCallback<React.Dispatch<React.SetStateAction<string | null>>>((action) => {
    setSelectedBlockIdState((current) => {
      const next = resolveStateAction(action, current);
      selectedBlockIdRef.current = next;
      return next;
    });
  }, []);

  const setEditingFontPresetId = React.useCallback<React.Dispatch<React.SetStateAction<string | null>>>((action) => {
    setEditingFontPresetIdState((current) => {
      const next = resolveStateAction(action, current);
      editingFontPresetIdRef.current = next;
      return next;
    });
  }, []);

  const clearSaveTimer = React.useCallback(() => {
    if (saveTimerRef.current) {
      window.clearTimeout(saveTimerRef.current);
      saveTimerRef.current = null;
    }
  }, []);

  const markDirty = React.useCallback((pageId?: string) => {
    dirtyVersionRef.current += 1;
    if (pageId) {
      dirtyPageIdsRef.current = new Set([...dirtyPageIdsRef.current, pageId]);
    }
    setDirty(true);
  }, []);

  const mergeLiveChapter = React.useCallback((chapter: ChapterSnapshot) => {
    const current = currentChapterRef.current;
    if (current && current.id !== chapter.id) {
      return;
    }

    const mergeResult = mergeLiveChapterPreservingDirtyCompletedPages(chapter, current, dirtyPageIdsRef.current);
    dirtyPageIdsRef.current = new Set(mergeResult.preservedDirtyPageIds);
    currentChapterRef.current = mergeResult.chapter;

    setCurrentChapterState((currentChapter) => {
      if (currentChapter && currentChapter.id !== mergeResult.chapter.id) {
        return currentChapter;
      }
      return mergeResult.chapter;
    });

    const selection = resolveSelectionAfterChapterSync(mergeResult.chapter, selectedPageIdRef.current, selectedBlockIdRef.current);
    selectedPageIdRef.current = selection.selectedPageId;
    selectedBlockIdRef.current = selection.selectedBlockId;
    setSelectedPageIdState(selection.selectedPageId);
    setSelectedBlockIdState(selection.selectedBlockId);
    setDirty(mergeResult.preservedDirtyPageIds.length > 0);
  }, []);

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
            setCurrentChapterState((current) => (current?.id === chapterToSave.id ? chapterToSave : current));
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

    return clearSaveTimer;
  }, [clearSaveTimer, currentChapter, dirty, signalSaveComplete]);

  const saveNow = React.useCallback(async () => {
    if (!currentChapter) {
      return;
    }
    clearSaveTimer();
    const dirtyPageIds = [...dirtyPageIdsRef.current];
    const normalized = normalizeChapterTranslatedText(currentChapter);
    const chapterToSave = normalized.chapter;
    const pageIdsToSave = [...new Set([...dirtyPageIds, ...normalized.dirtyPageIds])];
    await window.mangaApi.saveChapter(chapterToSave, pageIdsToSave.length > 0 ? pageIdsToSave : undefined);
    if (chapterToSave !== currentChapter) {
      currentChapterRef.current = chapterToSave;
      setCurrentChapterState((current) => (current?.id === chapterToSave.id ? chapterToSave : current));
    }
    dirtyPageIdsRef.current.clear();
    setDirty(false);
    signalSaveComplete();
  }, [clearSaveTimer, currentChapter, signalSaveComplete]);

  const clearCurrentChapter = React.useCallback(() => {
    clearSaveTimer();
    clearPendingChapterTimers?.();
    setCurrentChapterState(null);
    currentChapterRef.current = null;
    setSelectedPageIdState(null);
    selectedPageIdRef.current = null;
    setSelectedBlockIdState(null);
    selectedBlockIdRef.current = null;
    setEditingFontPresetIdState(null);
    editingFontPresetIdRef.current = null;
    clearUndoStacks();
    dirtyPageIdsRef.current.clear();
    setDirty(false);
  }, [clearPendingChapterTimers, clearSaveTimer, clearUndoStacks]);

  const openChapter = React.useCallback(
    async (chapterId: string) => {
      if (dirty) {
        await saveNow();
      }
      const chapter = await window.mangaApi.openChapter(chapterId);
      dirtyPageIdsRef.current.clear();
      clearUndoStacks();
      currentChapterRef.current = chapter;
      setCurrentChapterState(chapter);
      const pageId = chapter.pages[0]?.id ?? null;
      selectedPageIdRef.current = pageId;
      setSelectedPageIdState(pageId);
      selectedBlockIdRef.current = null;
      setSelectedBlockIdState(null);
      setDirty(false);
    },
    [clearUndoStacks, dirty, saveNow]
  );

  const applyChapter = React.useCallback((chapter: ChapterSnapshot | undefined, fallbackStatus?: string) => {
    if (!chapter) {
      return;
    }
    dirtyPageIdsRef.current.clear();
    clearUndoStacks();
    currentChapterRef.current = chapter;
    setCurrentChapterState(chapter);
    setSelectedPageIdState((current) => {
      const next = chapter.pages.some((page) => page.id === current) ? current : chapter.pages[0]?.id ?? null;
      selectedPageIdRef.current = next;
      return next;
    });
    selectedBlockIdRef.current = null;
    setSelectedBlockIdState(null);
    setDirty(false);
    if (fallbackStatus) {
      pushStatus(fallbackStatus);
    }
  }, [clearUndoStacks, pushStatus]);

  const selectPageForReading = React.useCallback((pageId: string | null) => {
    if (!pageId) {
      return;
    }
    selectedPageIdRef.current = pageId;
    selectedBlockIdRef.current = null;
    setSelectedPageIdState(pageId);
    setSelectedBlockIdState(null);
  }, []);

  const updateCurrentChapter = React.useCallback((pageId: string | undefined, updater: (chapter: ChapterSnapshot) => ChapterSnapshot) => {
    setCurrentChapter((current) => {
      if (!current) {
        return current;
      }
      const next = updater(current);
      markDirty(pageId);
      return next;
    });
  }, [markDirty, setCurrentChapter]);

  return {
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
  };
}
