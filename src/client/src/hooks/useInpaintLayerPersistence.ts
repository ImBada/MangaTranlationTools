import React from "react";
import type { ChapterSnapshot, MangaPage } from "../../../shared/types";
import { createInpaintMaskUndoSnapshot, type InpaintMaskUndoSnapshot } from "../lib/editorUtils";
import type { GlobalUndoHistoryEntry, GlobalUndoKind } from "../lib/editorUndoHistory";
import type { InpaintLayerChangeOptions } from "../lib/inpaintLayerChange";
import { useInpaintLayerSaveQueue } from "./useInpaintLayerSaveQueue";
import type { RecoverableFailureId } from "./useRecoverableFailures";

type UseInpaintLayerPersistenceOptions = {
  clearRecoverableFailure?: (id: RecoverableFailureId) => void;
  consumeGlobalUndoEntry: (kind: GlobalUndoKind, pageId?: string) => void;
  currentChapter: ChapterSnapshot | null;
  currentChapterRef: React.RefObject<ChapterSnapshot | null>;
  dirty: boolean;
  mergeLiveChapter: (chapter: ChapterSnapshot) => void;
  pushStatus: (line: string) => void;
  recordGlobalUndoEntry: (entry: GlobalUndoHistoryEntry) => void;
  refreshLibrary: () => Promise<void>;
  reportRecoverableFailure?: (failure: { id: RecoverableFailureId; message: string; title: string }) => void;
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
  recordInpaintMaskUndoSnapshot: (page: MangaPage, overrides?: Partial<InpaintMaskUndoSnapshot>) => void;
  undoPageInpaint: (pageId: string) => void;
  undoPageInpaintResult: (pageId: string) => void;
  updatePageInpaintStatus: (pageId: string, status: MangaPage["inpaintStatus"]) => void;
  updateSelectedPageInpaintMask: (dataUrl: string | undefined, options?: InpaintLayerChangeOptions) => void;
  updateSelectedPageInpaintResult: (dataUrl: string | undefined, options?: InpaintLayerChangeOptions) => void;
};

export function useInpaintLayerPersistence({
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
}: UseInpaintLayerPersistenceOptions): UseInpaintLayerPersistenceState {
  const inpaintUndoStackRef = React.useRef<Map<string, InpaintMaskUndoSnapshot[]>>(new Map());
  const inpaintResultUndoStackRef = React.useRef<Map<string, (string | undefined)[]>>(new Map());
  const {
    clearPendingInpaintSaves,
    flushInpaintMaskSave,
    flushInpaintResultSave,
    scheduleInpaintMaskSave,
    scheduleInpaintResultSave
  } = useInpaintLayerSaveQueue({
    clearRecoverableFailure,
    currentChapterRef,
    dirty,
    mergeLiveChapter,
    pushStatus,
    refreshLibrary,
    reportRecoverableFailure,
    saveNow,
    signalSaveComplete
  });

  const clearInpaintUndoStacks = React.useCallback(() => {
    inpaintUndoStackRef.current.clear();
    inpaintResultUndoStackRef.current.clear();
  }, []);

  const recordInpaintMaskUndoSnapshot = React.useCallback((page: MangaPage, overrides?: Partial<InpaintMaskUndoSnapshot>) => {
    if (!currentChapter) {
      return;
    }
    const stack = inpaintUndoStackRef.current.get(page.id) ?? [];
    stack.push(createInpaintMaskUndoSnapshot(page, overrides));
    inpaintUndoStackRef.current.set(page.id, stack.slice(-30));
    recordGlobalUndoEntry({ kind: "inpaint-mask", chapterId: currentChapter.id, pageId: page.id });
  }, [currentChapter, recordGlobalUndoEntry]);

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

  const updateSelectedPageInpaintMask = React.useCallback((dataUrl: string | undefined, options: InpaintLayerChangeOptions = {}) => {
    if (!currentChapter || !selectedPage || selectedPageEditLocked) {
      return;
    }

    const previousDataUrl = "previousDataUrl" in options ? options.previousDataUrl : selectedPage.inpaintMaskDataUrl ?? selectedPage.inpaintLayerDataUrl;
    if (options.recordUndo !== false && previousDataUrl !== dataUrl) {
      recordInpaintMaskUndoSnapshot(selectedPage, { inpaintMaskDataUrl: previousDataUrl });
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

  const updateSelectedPageInpaintResult = React.useCallback((dataUrl: string | undefined, options: InpaintLayerChangeOptions = {}) => {
    if (!currentChapter || !selectedPage || selectedPageEditLocked) {
      return;
    }

    const previousDataUrl = "previousDataUrl" in options ? options.previousDataUrl : selectedPage.inpaintResultDataUrl;
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
