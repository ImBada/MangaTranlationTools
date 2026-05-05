import React from "react";
import type { ChapterSnapshot } from "../../../shared/types";
import { mergeLiveChapterPreservingDirtyCompletedPages, resolveSelectionAfterChapterSync } from "../lib/chapterSync";
import { normalizeChapterTranslatedText } from "../lib/editorUtils";
import type { RecoverableFailureId } from "./useRecoverableFailures";

type ChapterSessionOptions = {
  clearPendingChapterTimers?: () => void;
  clearUndoStacks: () => void;
  pushStatus: (line: string) => void;
  reportRecoverableFailure?: (failure: { id: RecoverableFailureId; message: string; title: string }) => void;
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

type LastOpenedPageTarget = {
  chapterId: string;
  pageId: string;
};

type LastOpenedPageSaveState = {
  inFlight: boolean;
  pending: LastOpenedPageTarget | null;
  saved: LastOpenedPageTarget | null;
};

function resolveStateAction<T>(action: React.SetStateAction<T>, current: T): T {
  return typeof action === "function" ? (action as (current: T) => T)(current) : action;
}

function resolveChangedPageIds(current: ChapterSnapshot, next: ChapterSnapshot): string[] {
  return next.pages
    .filter((page, index) => page !== current.pages[index] || page.id !== current.pages[index]?.id)
    .map((page) => page.id);
}

function chapterHasPage(chapter: ChapterSnapshot, pageId: string | null): pageId is string {
  return Boolean(pageId && chapter.pages.some((page) => page.id === pageId));
}

function resolveInitialSelectedPageId(chapter: ChapterSnapshot): string | null {
  const lastOpenedPageId = chapter.lastOpenedPageId ?? null;
  return chapterHasPage(chapter, lastOpenedPageId) ? lastOpenedPageId : chapter.pages[0]?.id ?? null;
}

function isSameLastOpenedPageTarget(left: LastOpenedPageTarget | null, right: LastOpenedPageTarget | null): boolean {
  return Boolean(left && right && left.chapterId === right.chapterId && left.pageId === right.pageId);
}

export function useChapterSession({
  clearPendingChapterTimers,
  clearUndoStacks,
  pushStatus,
  reportRecoverableFailure,
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
  const dirtyAllPagesRef = React.useRef(false);
  const dirtyChapterPresetsRef = React.useRef(false);
  const currentChapterRef = React.useRef<ChapterSnapshot | null>(null);
  const selectedPageIdRef = React.useRef<string | null>(null);
  const selectedBlockIdRef = React.useRef<string | null>(null);
  const editingFontPresetIdRef = React.useRef<string | null>(null);
  const lastOpenedPageSaveRef = React.useRef<LastOpenedPageSaveState>({
    inFlight: false,
    pending: null,
    saved: null
  });

  const flushLastOpenedPageSave = React.useCallback(() => {
    const saveState = lastOpenedPageSaveRef.current;
    if (saveState.inFlight) {
      return;
    }

    const next = saveState.pending;
    if (!next) {
      return;
    }
    if (isSameLastOpenedPageTarget(saveState.saved, next)) {
      saveState.pending = null;
      return;
    }

    saveState.pending = null;
    saveState.inFlight = true;
    void window.mangaApi.saveLastOpenedPage(next.chapterId, next.pageId)
      .then(() => {
        lastOpenedPageSaveRef.current.saved = next;
      })
      .catch((error: unknown) => {
        console.error(error);
      })
      .finally(() => {
        lastOpenedPageSaveRef.current.inFlight = false;
        flushLastOpenedPageSave();
      });
  }, []);

  const persistLastOpenedPage = React.useCallback(
    (pageId: string | null) => {
      const chapter = currentChapterRef.current;
      if (!chapter || !chapterHasPage(chapter, pageId)) {
        return;
      }

      const target = {
        chapterId: chapter.id,
        pageId
      };

      const saveState = lastOpenedPageSaveRef.current;
      if (isSameLastOpenedPageTarget(saveState.saved, target) && !saveState.inFlight) {
        return;
      }
      saveState.pending = target;
      flushLastOpenedPageSave();
    },
    [flushLastOpenedPageSave]
  );

  const setCurrentChapter = React.useCallback<React.Dispatch<React.SetStateAction<ChapterSnapshot | null>>>((action) => {
    setCurrentChapterState((current) => {
      const next = resolveStateAction(action, current);
      currentChapterRef.current = next;
      return next;
    });
  }, []);

  const setSelectedPageId = React.useCallback<React.Dispatch<React.SetStateAction<string | null>>>((action) => {
    const next = resolveStateAction(action, selectedPageIdRef.current);
    selectedPageIdRef.current = next;
    setSelectedPageIdState(next);
    persistLastOpenedPage(next);
  }, [persistLastOpenedPage]);

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

  const clearDirtyTracking = React.useCallback(() => {
    dirtyPageIdsRef.current.clear();
    dirtyAllPagesRef.current = false;
    dirtyChapterPresetsRef.current = false;
  }, []);

  const markDirty = React.useCallback((pageId?: string) => {
    dirtyVersionRef.current += 1;
    if (pageId) {
      dirtyPageIdsRef.current = new Set([...dirtyPageIdsRef.current, pageId]);
    } else {
      dirtyAllPagesRef.current = true;
    }
    setDirty(true);
  }, []);

  const mergeLiveChapter = React.useCallback((chapter: ChapterSnapshot) => {
    const current = currentChapterRef.current;
    if (current && current.id !== chapter.id) {
      return;
    }

    const mergeResult = mergeLiveChapterPreservingDirtyCompletedPages(chapter, current, dirtyPageIdsRef.current, {
      preserveLocalChapterPresets: dirtyChapterPresetsRef.current
    });
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
    setDirty(mergeResult.preservedDirtyPageIds.length > 0 || dirtyChapterPresetsRef.current);
  }, []);

  React.useEffect(() => {
    if (!dirty || !currentChapter) {
      return;
    }

    const version = dirtyVersionRef.current;
    const saveAllPages = dirtyAllPagesRef.current;
    const dirtyPageIds = saveAllPages ? currentChapter.pages.map((page) => page.id) : [...dirtyPageIdsRef.current];
    saveTimerRef.current = window.setTimeout(async () => {
      try {
        const normalized = normalizeChapterTranslatedText(currentChapter);
        const chapterToSave = normalized.chapter;
        const pageIdsToSave = saveAllPages
          ? chapterToSave.pages.map((page) => page.id)
          : [...new Set([...dirtyPageIds, ...normalized.dirtyPageIds])];
        await window.mangaApi.saveChapter(chapterToSave, pageIdsToSave);
        if (dirtyVersionRef.current === version) {
          if (chapterToSave !== currentChapter) {
            currentChapterRef.current = chapterToSave;
            setCurrentChapterState((current) => (current?.id === chapterToSave.id ? chapterToSave : current));
          }
          clearDirtyTracking();
          setDirty(false);
          signalSaveComplete();
        }
      } catch (error) {
        console.error(error);
        const message = error instanceof Error ? error.message : "자동 저장에 실패했습니다.";
        pushStatus(message);
        reportRecoverableFailure?.({
          id: "chapter-save",
          title: "자동 저장 실패",
          message: "변경 내용은 화면에 남아 있습니다. 네트워크 또는 저장 위치를 확인한 뒤 다시 저장하세요."
        });
      } finally {
        saveTimerRef.current = null;
      }
    }, 400);

    return clearSaveTimer;
  }, [clearDirtyTracking, clearSaveTimer, currentChapter, dirty, pushStatus, reportRecoverableFailure, signalSaveComplete]);

  const saveNow = React.useCallback(async () => {
    if (!currentChapter) {
      return;
    }
    clearSaveTimer();
    const saveAllPages = dirtyAllPagesRef.current;
    const dirtyPageIds = saveAllPages ? currentChapter.pages.map((page) => page.id) : [...dirtyPageIdsRef.current];
    const normalized = normalizeChapterTranslatedText(currentChapter);
    const chapterToSave = normalized.chapter;
    const pageIdsToSave = saveAllPages
      ? chapterToSave.pages.map((page) => page.id)
      : [...new Set([...dirtyPageIds, ...normalized.dirtyPageIds])];
    await window.mangaApi.saveChapter(chapterToSave, pageIdsToSave);
    if (chapterToSave !== currentChapter) {
      currentChapterRef.current = chapterToSave;
      setCurrentChapterState((current) => (current?.id === chapterToSave.id ? chapterToSave : current));
    }
    clearDirtyTracking();
    setDirty(false);
    signalSaveComplete();
  }, [clearDirtyTracking, clearSaveTimer, currentChapter, signalSaveComplete]);

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
    clearDirtyTracking();
    setDirty(false);
  }, [clearDirtyTracking, clearPendingChapterTimers, clearSaveTimer, clearUndoStacks]);

  const openChapter = React.useCallback(
    async (chapterId: string) => {
      if (dirty) {
        await saveNow();
      }
      const chapter = await window.mangaApi.openChapter(chapterId);
      clearDirtyTracking();
      clearUndoStacks();
      currentChapterRef.current = chapter;
      setCurrentChapterState(chapter);
      const pageId = resolveInitialSelectedPageId(chapter);
      selectedPageIdRef.current = pageId;
      setSelectedPageIdState(pageId);
      selectedBlockIdRef.current = null;
      setSelectedBlockIdState(null);
      setDirty(false);
      if (pageId && chapter.lastOpenedPageId === pageId) {
        lastOpenedPageSaveRef.current.saved = { chapterId: chapter.id, pageId };
      }
    },
    [clearDirtyTracking, clearUndoStacks, dirty, saveNow]
  );

  const applyChapter = React.useCallback((chapter: ChapterSnapshot | undefined, fallbackStatus?: string) => {
    if (!chapter) {
      return;
    }
    clearDirtyTracking();
    clearUndoStacks();
    currentChapterRef.current = chapter;
    setCurrentChapterState(chapter);
    setSelectedPageIdState((current) => {
      const next = chapterHasPage(chapter, current) ? current : resolveInitialSelectedPageId(chapter);
      selectedPageIdRef.current = next;
      if (next && chapter.lastOpenedPageId === next) {
        lastOpenedPageSaveRef.current.saved = { chapterId: chapter.id, pageId: next };
      }
      return next;
    });
    selectedBlockIdRef.current = null;
    setSelectedBlockIdState(null);
    setDirty(false);
    if (fallbackStatus) {
      pushStatus(fallbackStatus);
    }
  }, [clearDirtyTracking, clearUndoStacks, pushStatus]);

  const selectPageForReading = React.useCallback((pageId: string | null) => {
    if (!pageId) {
      return;
    }
    const chapter = currentChapterRef.current;
    if (chapter && !chapterHasPage(chapter, pageId)) {
      return;
    }
    selectedPageIdRef.current = pageId;
    selectedBlockIdRef.current = null;
    setSelectedPageIdState(pageId);
    setSelectedBlockIdState(null);
    persistLastOpenedPage(pageId);
  }, [persistLastOpenedPage]);

  const updateCurrentChapter = React.useCallback((pageId: string | undefined, updater: (chapter: ChapterSnapshot) => ChapterSnapshot) => {
    setCurrentChapter((current) => {
      if (!current) {
        return current;
      }
      const next = updater(current);
      dirtyVersionRef.current += 1;
      if (
        next.favoriteFontPresetIds !== current.favoriteFontPresetIds ||
        next.fontPresets !== current.fontPresets ||
        next.fontSizePresets !== current.fontSizePresets
      ) {
        dirtyChapterPresetsRef.current = true;
      }
      const pageIds = pageId ? [pageId] : resolveChangedPageIds(current, next);
      if (pageIds.length > 0) {
        dirtyPageIdsRef.current = new Set([...dirtyPageIdsRef.current, ...pageIds]);
      }
      setDirty(true);
      return next;
    });
  }, [setCurrentChapter]);

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
