import React from "react";
import type { ChapterSnapshot, MangaPage } from "../../../shared/types";
import { createInpaintMaskUndoSnapshot, type InpaintMaskUndoSnapshot } from "../lib/editorUtils";
import type { GlobalUndoHistoryEntry, GlobalUndoKind } from "../lib/editorUndoHistory";

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

type UseInpaintLayerPersistenceOptions = {
  consumeGlobalUndoEntry: (kind: GlobalUndoKind, pageId?: string) => void;
  currentChapter: ChapterSnapshot | null;
  currentChapterRef: React.RefObject<ChapterSnapshot | null>;
  dirty: boolean;
  mergeLiveChapter: (chapter: ChapterSnapshot) => void;
  pushStatus: (line: string) => void;
  recordGlobalUndoEntry: (entry: GlobalUndoHistoryEntry) => void;
  refreshLibrary: () => Promise<void>;
  saveNow: () => Promise<void>;
  selectedPage: MangaPage | null;
  selectedPageEditLocked: boolean;
  setCurrentChapter: React.Dispatch<React.SetStateAction<ChapterSnapshot | null>>;
  signalSaveComplete: () => void;
};

type UseInpaintLayerPersistenceState = {
  canUndoInpaintMask: (pageId: string) => boolean;
  canUndoInpaintResult: (pageId: string) => boolean;
  clearInpaintUndoStacks: () => void;
  clearPendingInpaintSaves: () => void;
  flushInpaintMaskSave: () => Promise<void>;
  flushInpaintResultSave: () => Promise<void>;
  recordInpaintMaskUndoSnapshot: (page: MangaPage) => void;
  undoPageInpaint: (pageId: string) => void;
  undoPageInpaintResult: (pageId: string) => void;
  updatePageInpaintStatus: (pageId: string, status: MangaPage["inpaintStatus"]) => void;
  updateSelectedPageInpaintMask: (dataUrl: string | undefined, options?: { persist?: boolean; recordUndo?: boolean }) => void;
  updateSelectedPageInpaintResult: (dataUrl: string | undefined, options?: { persist?: boolean; recordUndo?: boolean }) => void;
};

export function useInpaintLayerPersistence({
  consumeGlobalUndoEntry,
  currentChapter,
  currentChapterRef,
  dirty,
  mergeLiveChapter,
  pushStatus,
  recordGlobalUndoEntry,
  refreshLibrary,
  saveNow,
  selectedPage,
  selectedPageEditLocked,
  setCurrentChapter,
  signalSaveComplete
}: UseInpaintLayerPersistenceOptions): UseInpaintLayerPersistenceState {
  const inpaintUndoStackRef = React.useRef<Map<string, InpaintMaskUndoSnapshot[]>>(new Map());
  const inpaintResultUndoStackRef = React.useRef<Map<string, (string | undefined)[]>>(new Map());
  const inpaintMaskSaveTimerRef = React.useRef<number | null>(null);
  const inpaintMaskSaveStateRef = React.useRef<PendingInpaintMaskSave | null>(null);
  const inpaintMaskSavingRef = React.useRef(false);
  const inpaintResultSaveTimerRef = React.useRef<number | null>(null);
  const inpaintResultSaveStateRef = React.useRef<PendingInpaintResultSave | null>(null);
  const inpaintResultSavingRef = React.useRef(false);

  const clearInpaintUndoStacks = React.useCallback(() => {
    inpaintUndoStackRef.current.clear();
    inpaintResultUndoStackRef.current.clear();
  }, []);

  const recordInpaintMaskUndoSnapshot = React.useCallback((page: MangaPage) => {
    if (!currentChapter) {
      return;
    }
    const stack = inpaintUndoStackRef.current.get(page.id) ?? [];
    stack.push(createInpaintMaskUndoSnapshot(page));
    inpaintUndoStackRef.current.set(page.id, stack.slice(-30));
    recordGlobalUndoEntry({ kind: "inpaint-mask", chapterId: currentChapter.id, pageId: page.id });
  }, [currentChapter, recordGlobalUndoEntry]);

  const clearPendingInpaintSaves = React.useCallback(() => {
    if (inpaintMaskSaveTimerRef.current) {
      window.clearTimeout(inpaintMaskSaveTimerRef.current);
      inpaintMaskSaveTimerRef.current = null;
    }
    if (inpaintResultSaveTimerRef.current) {
      window.clearTimeout(inpaintResultSaveTimerRef.current);
      inpaintResultSaveTimerRef.current = null;
    }
  }, []);

  const updatePageInpaintStatus = React.useCallback((pageId: string, status: MangaPage["inpaintStatus"]) => {
    setCurrentChapter((current) => {
      if (!current) {
        return current;
      }
      return {
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
    });
  }, [setCurrentChapter]);

  const flushInpaintMaskSave = React.useCallback(async () => {
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
  }, [currentChapterRef, dirty, mergeLiveChapter, pushStatus, refreshLibrary, saveNow, signalSaveComplete]);

  const scheduleInpaintMaskSave = React.useCallback((pending: PendingInpaintMaskSave) => {
    inpaintMaskSaveStateRef.current = pending;
    if (inpaintMaskSaveTimerRef.current) {
      window.clearTimeout(inpaintMaskSaveTimerRef.current);
    }
    inpaintMaskSaveTimerRef.current = window.setTimeout(() => {
      inpaintMaskSaveTimerRef.current = null;
      void flushInpaintMaskSave();
    }, 250);
  }, [flushInpaintMaskSave]);

  const flushInpaintResultSave = React.useCallback(async () => {
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
  }, [currentChapterRef, dirty, mergeLiveChapter, pushStatus, refreshLibrary, saveNow, signalSaveComplete]);

  const scheduleInpaintResultSave = React.useCallback((pending: PendingInpaintResultSave) => {
    inpaintResultSaveStateRef.current = pending;
    if (inpaintResultSaveTimerRef.current) {
      window.clearTimeout(inpaintResultSaveTimerRef.current);
    }
    inpaintResultSaveTimerRef.current = window.setTimeout(() => {
      inpaintResultSaveTimerRef.current = null;
      void flushInpaintResultSave();
    }, 250);
  }, [flushInpaintResultSave]);

  const updateSelectedPageInpaintMask = React.useCallback((dataUrl: string | undefined, options: { persist?: boolean; recordUndo?: boolean } = {}) => {
    if (!currentChapter || !selectedPage || selectedPageEditLocked) {
      return;
    }

    const previousDataUrl = selectedPage.inpaintMaskDataUrl ?? selectedPage.inpaintLayerDataUrl;
    if (options.recordUndo !== false && previousDataUrl !== dataUrl) {
      recordInpaintMaskUndoSnapshot(selectedPage);
    }

    const updatedAt = new Date().toISOString();
    setCurrentChapter((current) => {
      if (!current) {
        return current;
      }
      return {
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
    });

    if (options.persist !== false) {
      scheduleInpaintMaskSave({
        chapterId: currentChapter.id,
        pageId: selectedPage.id,
        dataUrl
      });
    }
  }, [currentChapter, recordInpaintMaskUndoSnapshot, scheduleInpaintMaskSave, selectedPage, selectedPageEditLocked, setCurrentChapter]);

  const updateSelectedPageInpaintResult = React.useCallback((dataUrl: string | undefined, options: { persist?: boolean; recordUndo?: boolean } = {}) => {
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
      return {
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
    });

    if (options.persist !== false) {
      scheduleInpaintResultSave({
        chapterId: currentChapter.id,
        pageId: selectedPage.id,
        dataUrl
      });
    }
  }, [currentChapter, recordGlobalUndoEntry, scheduleInpaintResultSave, selectedPage, selectedPageEditLocked, setCurrentChapter]);

  const restorePageInpaintMaskSnapshot = React.useCallback((pageId: string, snapshot: InpaintMaskUndoSnapshot) => {
    const chapter = currentChapterRef.current;
    if (!chapter || selectedPageEditLocked) {
      return;
    }

    const updatedAt = new Date().toISOString();
    setCurrentChapter((current) => {
      if (!current || current.id !== chapter.id) {
        return current;
      }
      return {
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
  }, [currentChapterRef, scheduleInpaintMaskSave, scheduleInpaintResultSave, selectedPageEditLocked, setCurrentChapter]);

  const undoPageInpaint = React.useCallback((pageId: string) => {
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

  const undoPageInpaintResult = React.useCallback((pageId: string) => {
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
      return {
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
    });

    scheduleInpaintResultSave({
      chapterId: chapter.id,
      pageId,
      dataUrl: previousDataUrl
    });
  }, [consumeGlobalUndoEntry, currentChapterRef, scheduleInpaintResultSave, selectedPageEditLocked, setCurrentChapter]);

  return {
    canUndoInpaintMask: (pageId) => (inpaintUndoStackRef.current.get(pageId)?.length ?? 0) > 0,
    canUndoInpaintResult: (pageId) => (inpaintResultUndoStackRef.current.get(pageId)?.length ?? 0) > 0,
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
  };
}
