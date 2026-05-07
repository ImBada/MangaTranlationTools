import React from "react";
import type { ChapterSnapshot, MangaPage } from "../../../shared/types";
import { createInpaintMaskUndoSnapshot, type InpaintMaskUndoSnapshot } from "../lib/editorUtils";
import type { GlobalUndoHistoryEntry, GlobalUndoKind } from "../lib/editorUndoHistory";
import type { InpaintLayerChangeOptions } from "../lib/inpaintLayerChange";
import { mergePartialInpaintMask } from "../lib/inpaintMaskImages";
import { useInpaintLayerSaveQueue, type PendingInpaintLayerSave } from "./useInpaintLayerSaveQueue";
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
  beginInpaintLayerInteraction: () => void;
  canUndoInpaintMask: (pageId: string) => boolean;
  canUndoInpaintResult: (pageId: string) => boolean;
  clearInpaintUndoStacks: () => void;
  clearPendingInpaintSaveTimers: () => void;
  clearPendingInpaintSaves: () => void;
  endInpaintLayerInteraction: () => void;
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
  const inpaintLayerCommitRevisionRef = React.useRef(0);
  const nextInpaintLayerCommitRevision = React.useCallback(() => {
    inpaintLayerCommitRevisionRef.current += 1;
    return inpaintLayerCommitRevisionRef.current;
  }, []);
  const isInpaintLayerSaveCurrent = React.useCallback((commitRevision: number | undefined) => {
    return commitRevision === undefined || inpaintLayerCommitRevisionRef.current === commitRevision;
  }, []);
  const resolveLatestInpaintLayerSave = React.useCallback((chapterId: string, pageId: string): PendingInpaintLayerSave | null => {
    const chapter = currentChapterRef.current;
    if (!chapter || chapter.id !== chapterId) {
      return null;
    }
    const page = chapter.pages.find((candidate) => candidate.id === pageId);
    if (!page) {
      return null;
    }
    return {
      kind: "layers",
      chapterId,
      commitRevision: inpaintLayerCommitRevisionRef.current,
      pageId,
      maskDataUrl: page.inpaintMaskDataUrl ?? page.inpaintLayerDataUrl,
      resultDataUrl: page.inpaintResultDataUrl
    };
  }, [currentChapterRef]);
  const {
    beginInpaintLayerInteraction,
    clearPendingInpaintSaveTimers,
    clearPendingInpaintSaves,
    endInpaintLayerInteraction,
    flushInpaintMaskSave,
    flushInpaintResultSave,
    scheduleInpaintLayersSave,
    scheduleInpaintMaskSave,
    scheduleInpaintResultSave
  } = useInpaintLayerSaveQueue({
    clearRecoverableFailure,
    currentChapterRef,
    dirty,
    isInpaintLayerSaveCurrent,
    mergeLiveChapter,
    pushStatus,
    refreshLibrary,
    reportRecoverableFailure,
    resolveLatestInpaintLayerSave,
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

    const chapterId = currentChapter.id;
    const targetPage = selectedPage;
    const previousDataUrl = "previousDataUrl" in options ? options.previousDataUrl : selectedPage.inpaintMaskDataUrl ?? selectedPage.inpaintLayerDataUrl;
    const livePage = resolveOpenChapterPage(currentChapterRef.current, chapterId, targetPage.id);
    if (livePage === null || (livePage && resolvePageMaskDataUrl(livePage) !== previousDataUrl)) {
      return;
    }
    const applyLocalUpdate = livePage !== undefined;
    const commitRevision = applyLocalUpdate ? nextInpaintLayerCommitRevision() : undefined;

    if (livePage) {
      if (options.recordUndo !== false) {
        const undoDataUrls = resolveInpaintUndoDataUrlSequence(previousDataUrl, options.intermediateUndoDataUrls, dataUrl);
        for (const undoDataUrl of undoDataUrls) {
          recordInpaintMaskUndoSnapshot(livePage, { inpaintMaskDataUrl: undoDataUrl });
        }
      }

      const updatedAt = new Date().toISOString();
      setCurrentChapter((current) => {
        if (!current || current.id !== chapterId) {
          return current;
        }
        return {
          ...current,
          updatedAt,
          pages: current.pages.map((page) =>
            page.id === targetPage.id
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
    }

    if (options.persist !== false) {
      scheduleInpaintMaskSave({
        chapterId,
        commitRevision,
        pageId: targetPage.id,
        dataUrl
      });
    }
  }, [currentChapter, currentChapterRef, nextInpaintLayerCommitRevision, recordInpaintMaskUndoSnapshot, scheduleInpaintMaskSave, selectedPage, selectedPageEditLocked, setCurrentChapter]);

  const commitSelectedPageInpaintResult = React.useCallback((dataUrl: string | undefined, options: InpaintLayerChangeOptions = {}) => {
    if (!currentChapter || !selectedPage || selectedPageEditLocked) {
      return;
    }

    const chapterId = currentChapter.id;
    const targetPage = selectedPage;
    const previousDataUrl = "previousDataUrl" in options ? options.previousDataUrl : selectedPage.inpaintResultDataUrl;
    const livePage = resolveOpenChapterPage(currentChapterRef.current, chapterId, targetPage.id);
    if (livePage === null || (livePage && livePage.inpaintResultDataUrl !== previousDataUrl)) {
      return;
    }
    const applyLocalUpdate = livePage !== undefined;
    const commitRevision = applyLocalUpdate ? nextInpaintLayerCommitRevision() : undefined;

    if (livePage) {
      if (options.recordUndo !== false) {
        const undoDataUrls = resolveInpaintUndoDataUrlSequence(previousDataUrl, options.intermediateUndoDataUrls, dataUrl);
        const stack = inpaintResultUndoStackRef.current.get(targetPage.id) ?? [];
        for (const undoDataUrl of undoDataUrls) {
          stack.push(undoDataUrl);
          recordGlobalUndoEntry({ kind: "inpaint-result", chapterId, pageId: targetPage.id });
        }
        inpaintResultUndoStackRef.current.set(targetPage.id, stack.slice(-30));
      }

      const updatedAt = new Date().toISOString();
      setCurrentChapter((current) => {
        if (!current || current.id !== chapterId) {
          return current;
        }
        return {
          ...current,
          updatedAt,
          pages: current.pages.map((page) =>
            page.id === targetPage.id
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
    }

    if (options.persist !== false) {
      scheduleInpaintResultSave({
        chapterId,
        commitRevision,
        pageId: targetPage.id,
        dataUrl
      });
    }
  }, [currentChapter, currentChapterRef, nextInpaintLayerCommitRevision, recordGlobalUndoEntry, scheduleInpaintResultSave, selectedPage, selectedPageEditLocked, setCurrentChapter]);

  const commitSelectedPageInpaintLayers = React.useCallback(({
    chapterId,
    commitRevision,
    resultDataUrl,
    nextMaskDataUrl,
    targetPage,
    previousMaskDataUrl,
    previousResultDataUrl,
    intermediateLayerUndoSnapshots,
    applyLocalUpdate,
    recordUndo,
    persist
  }: {
    chapterId: string;
    commitRevision: number | undefined;
    resultDataUrl: string | undefined;
    nextMaskDataUrl: string | undefined;
    targetPage: MangaPage;
    previousMaskDataUrl: string | undefined;
    previousResultDataUrl: string | undefined;
    intermediateLayerUndoSnapshots?: {
      maskDataUrl: string | undefined;
      resultDataUrl: string | undefined;
    }[];
    applyLocalUpdate: boolean;
    recordUndo: boolean;
    persist: boolean;
  }) => {
    if (applyLocalUpdate) {
      if (recordUndo) {
        const undoSnapshots = resolveInpaintLayerUndoSnapshotSequence(
          {
            maskDataUrl: previousMaskDataUrl,
            resultDataUrl: previousResultDataUrl
          },
          intermediateLayerUndoSnapshots,
          {
            maskDataUrl: nextMaskDataUrl,
            resultDataUrl
          }
        );
        for (const undoSnapshot of undoSnapshots) {
          recordInpaintMaskUndoSnapshot(targetPage, {
            inpaintMaskDataUrl: undoSnapshot.maskDataUrl,
            inpaintResultDataUrl: undoSnapshot.resultDataUrl,
            inpaintStatus: undoSnapshot.resultDataUrl ? "completed" as const : "idle" as const
          });
        }
      }

      const updatedAt = new Date().toISOString();
      setCurrentChapter((current) => {
        if (!current || current.id !== chapterId) {
          return current;
        }
        return {
          ...current,
          updatedAt,
          pages: current.pages.map((page) =>
            page.id === targetPage.id
              ? {
                  ...page,
                  updatedAt,
                  inpaintMaskPath: nextMaskDataUrl ? page.inpaintMaskPath : undefined,
                  inpaintResultPath: resultDataUrl ? page.inpaintResultPath : undefined,
                  inpaintMaskDataUrl: nextMaskDataUrl,
                  inpaintResultDataUrl: resultDataUrl,
                  inpaintStatus: resultDataUrl ? "completed" as const : "idle" as const
                }
              : page
          )
        };
      });
    }

    if (persist) {
      scheduleInpaintLayersSave({
        chapterId,
        commitRevision,
        pageId: targetPage.id,
        maskDataUrl: nextMaskDataUrl,
        resultDataUrl
      });
    }
  }, [recordInpaintMaskUndoSnapshot, scheduleInpaintLayersSave, setCurrentChapter]);

  const updateSelectedPageInpaintResult = React.useCallback((dataUrl: string | undefined, options: InpaintLayerChangeOptions = {}) => {
    if (!("maskDataUrl" in options)) {
      commitSelectedPageInpaintResult(dataUrl, options);
      return;
    }
    if (!currentChapter || !selectedPage || selectedPageEditLocked) {
      return;
    }

    const targetPage = selectedPage;
    const chapterId = currentChapter.id;
    const previousMaskDataUrl =
      ("previousMaskDataUrl" in options ? options.previousMaskDataUrl : undefined) ??
      selectedPage.inpaintMaskDataUrl ??
      selectedPage.inpaintLayerDataUrl;
    const previousResultDataUrl = "previousDataUrl" in options ? options.previousDataUrl : selectedPage.inpaintResultDataUrl;
    const patchMaskDataUrl = options.maskDataUrl;
    const livePage = resolveOpenChapterPage(currentChapterRef.current, chapterId, targetPage.id);
    if (
      livePage === null ||
      (livePage && (
        resolvePageMaskDataUrl(livePage) !== previousMaskDataUrl ||
        livePage.inpaintResultDataUrl !== previousResultDataUrl
      ))
    ) {
      return;
    }
    const applyLocalUpdate = livePage !== undefined;
    const commitRevision = applyLocalUpdate ? nextInpaintLayerCommitRevision() : undefined;

    void (patchMaskDataUrl
      ? mergePartialInpaintMask(previousMaskDataUrl, patchMaskDataUrl, targetPage.width, targetPage.height)
      : Promise.resolve(previousMaskDataUrl)
    ).then((nextMaskDataUrl) => {
      if (commitRevision !== undefined && inpaintLayerCommitRevisionRef.current !== commitRevision) {
        return;
      }
      const currentLivePage = resolveOpenChapterPage(currentChapterRef.current, chapterId, targetPage.id);
      if (
        currentLivePage === null ||
        (currentLivePage && (
          resolvePageMaskDataUrl(currentLivePage) !== previousMaskDataUrl ||
          currentLivePage.inpaintResultDataUrl !== previousResultDataUrl
        ))
      ) {
        return;
      }
      const applyCurrentLocalUpdate = applyLocalUpdate && Boolean(currentLivePage);

      commitSelectedPageInpaintLayers({
        chapterId,
        commitRevision: applyCurrentLocalUpdate ? commitRevision : undefined,
        resultDataUrl: dataUrl,
        nextMaskDataUrl,
        targetPage: currentLivePage ?? targetPage,
        previousMaskDataUrl,
        previousResultDataUrl,
        intermediateLayerUndoSnapshots: options.intermediateLayerUndoSnapshots,
        applyLocalUpdate: applyCurrentLocalUpdate,
        recordUndo: options.recordUndo !== false,
        persist: options.persist !== false
      });
    }).catch((error) => {
      console.error(error);
      if (commitRevision !== undefined && inpaintLayerCommitRevisionRef.current !== commitRevision) {
        return;
      }
      const currentLivePage = resolveOpenChapterPage(currentChapterRef.current, chapterId, targetPage.id);
      if (
        currentLivePage === null ||
        (currentLivePage && (
          resolvePageMaskDataUrl(currentLivePage) !== previousMaskDataUrl ||
          currentLivePage.inpaintResultDataUrl !== previousResultDataUrl
        ))
      ) {
        return;
      }
      const applyCurrentLocalUpdate = applyLocalUpdate && Boolean(currentLivePage);
      commitSelectedPageInpaintLayers({
        chapterId,
        commitRevision: applyCurrentLocalUpdate ? commitRevision : undefined,
        resultDataUrl: dataUrl,
        nextMaskDataUrl: previousMaskDataUrl,
        targetPage: currentLivePage ?? targetPage,
        previousMaskDataUrl,
        previousResultDataUrl,
        intermediateLayerUndoSnapshots: options.intermediateLayerUndoSnapshots,
        applyLocalUpdate: applyCurrentLocalUpdate,
        recordUndo: options.recordUndo !== false,
        persist: options.persist !== false
      });
    });
  }, [commitSelectedPageInpaintLayers, commitSelectedPageInpaintResult, currentChapter, currentChapterRef, nextInpaintLayerCommitRevision, selectedPage, selectedPageEditLocked]);

  const restorePageInpaintMaskSnapshot = React.useCallback((pageId: string, snapshot: InpaintMaskUndoSnapshot) => {
    const chapter = currentChapterRef.current;
    if (!chapter || selectedPageEditLocked) {
      return;
    }

    const commitRevision = nextInpaintLayerCommitRevision();
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

    scheduleInpaintLayersSave({
      chapterId: chapter.id,
      commitRevision,
      pageId,
      maskDataUrl: snapshot.inpaintMaskDataUrl,
      resultDataUrl: snapshot.inpaintResultDataUrl
    });
  }, [currentChapterRef, nextInpaintLayerCommitRevision, scheduleInpaintLayersSave, selectedPageEditLocked, setCurrentChapter]);

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
    const commitRevision = nextInpaintLayerCommitRevision();
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
      commitRevision,
      pageId,
      dataUrl: previousDataUrl
    });
  }, [consumeGlobalUndoEntry, currentChapterRef, nextInpaintLayerCommitRevision, scheduleInpaintResultSave, selectedPageEditLocked, setCurrentChapter]);

  return {
    beginInpaintLayerInteraction,
    canUndoInpaintMask: (pageId) => (inpaintUndoStackRef.current.get(pageId)?.length ?? 0) > 0,
    canUndoInpaintResult: (pageId) => (inpaintResultUndoStackRef.current.get(pageId)?.length ?? 0) > 0,
    clearInpaintUndoStacks,
    clearPendingInpaintSaveTimers,
    clearPendingInpaintSaves,
    endInpaintLayerInteraction,
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

function resolveOpenChapterPage(
  chapter: ChapterSnapshot | null,
  chapterId: string,
  pageId: string
): MangaPage | null | undefined {
  if (!chapter || chapter.id !== chapterId) {
    return undefined;
  }
  return chapter.pages.find((page) => page.id === pageId) ?? null;
}

function resolvePageMaskDataUrl(page: MangaPage): string | undefined {
  return page.inpaintMaskDataUrl ?? page.inpaintLayerDataUrl;
}

export function resolveInpaintUndoDataUrlSequence(
  previousDataUrl: string | undefined,
  intermediateDataUrls: (string | undefined)[] | undefined,
  nextDataUrl: string | undefined
): (string | undefined)[] {
  const sequence = [previousDataUrl, ...(intermediateDataUrls ?? [])];
  const undoDataUrls: (string | undefined)[] = [];
  for (const dataUrl of sequence) {
    if (
      dataUrl === nextDataUrl ||
      (undoDataUrls.length > 0 && dataUrl === undoDataUrls[undoDataUrls.length - 1])
    ) {
      continue;
    }
    undoDataUrls.push(dataUrl);
  }
  return undoDataUrls;
}

export function resolveInpaintLayerUndoSnapshotSequence(
  previousSnapshot: {
    maskDataUrl: string | undefined;
    resultDataUrl: string | undefined;
  },
  intermediateSnapshots: {
    maskDataUrl: string | undefined;
    resultDataUrl: string | undefined;
  }[] | undefined,
  nextSnapshot: {
    maskDataUrl: string | undefined;
    resultDataUrl: string | undefined;
  }
): {
  maskDataUrl: string | undefined;
  resultDataUrl: string | undefined;
}[] {
  const sequence = [previousSnapshot, ...(intermediateSnapshots ?? [])];
  const undoSnapshots: {
    maskDataUrl: string | undefined;
    resultDataUrl: string | undefined;
  }[] = [];
  for (const snapshot of sequence) {
    const lastSnapshot = undoSnapshots[undoSnapshots.length - 1];
    if (
      isInpaintLayerUndoSnapshotEqual(snapshot, nextSnapshot) ||
      (lastSnapshot && isInpaintLayerUndoSnapshotEqual(snapshot, lastSnapshot))
    ) {
      continue;
    }
    undoSnapshots.push(snapshot);
  }
  return undoSnapshots;
}

function isInpaintLayerUndoSnapshotEqual(
  left: {
    maskDataUrl: string | undefined;
    resultDataUrl: string | undefined;
  },
  right: {
    maskDataUrl: string | undefined;
    resultDataUrl: string | undefined;
  }
): boolean {
  return left.maskDataUrl === right.maskDataUrl && left.resultDataUrl === right.resultDataUrl;
}
