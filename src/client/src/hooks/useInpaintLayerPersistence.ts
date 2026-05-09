import React from "react";
import type { ChapterSnapshot, MangaPage } from "../../../shared/types";
import { createInpaintMaskUndoSnapshot, type InpaintMaskUndoSnapshot } from "../lib/editorUtils";
import type { GlobalUndoHistoryEntry, GlobalUndoKind } from "../lib/editorUndoHistory";
import type { InpaintLayerChangeOptions } from "../lib/inpaintLayerChange";
import { summarizeDataUrl, summarizeError, writeInpaintDebugLog } from "../lib/inpaintDiagnostics";
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
      writeInpaintDebugLog("inpaint-undo:record-mask-skip", {
        pageId: page.id,
        reason: "missing-current-chapter"
      });
      return;
    }
    const stack = inpaintUndoStackRef.current.get(page.id) ?? [];
    const snapshot = createInpaintMaskUndoSnapshot(page, overrides);
    stack.push(snapshot);
    inpaintUndoStackRef.current.set(page.id, stack.slice(-30));
    recordGlobalUndoEntry({ kind: "inpaint-mask", chapterId: currentChapter.id, pageId: page.id });
    writeInpaintDebugLog("inpaint-undo:record-mask", {
      chapterId: currentChapter.id,
      pageId: page.id,
      snapshot: summarizeInpaintMaskUndoSnapshot(snapshot),
      stackSize: inpaintUndoStackRef.current.get(page.id)?.length ?? 0
    });
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
    const previousSourceDataUrl = options.previousMaskSourceDataUrl;
    const livePage = resolveOpenChapterPage(currentChapterRef.current, chapterId, targetPage.id);
    const liveMaskDataUrl = livePage ? resolvePageMaskDataUrl(livePage) : undefined;
    if (livePage === null || (livePage && !isExpectedPreviousInpaintMask(liveMaskDataUrl, previousDataUrl, previousSourceDataUrl))) {
      writeInpaintDebugLog("inpaint-mask:state-skip", {
        reason: livePage === null ? "page-not-found" : "previous-mask-mismatch",
        chapterId,
        pageId: targetPage.id,
        incomingDataUrl: summarizeDataUrl(dataUrl),
        liveMaskDataUrl: summarizeDataUrl(liveMaskDataUrl),
        previousDataUrl: summarizeDataUrl(previousDataUrl),
        previousSourceDataUrl: summarizeDataUrl(previousSourceDataUrl),
        selectedPageMaskDataUrl: summarizeDataUrl(selectedPage.inpaintMaskDataUrl ?? selectedPage.inpaintLayerDataUrl)
      });
      return;
    }
    const applyLocalUpdate = livePage !== undefined;
    const commitRevision = applyLocalUpdate ? nextInpaintLayerCommitRevision() : undefined;
    writeInpaintDebugLog("inpaint-mask:state-update", {
      chapterId,
      pageId: targetPage.id,
      applyLocalUpdate,
      commitRevision,
      incomingDataUrl: summarizeDataUrl(dataUrl),
      intermediateUndoCount: options.intermediateUndoDataUrls?.length ?? 0,
      persist: options.persist !== false,
      previousDataUrl: summarizeDataUrl(previousDataUrl),
      previousSourceDataUrl: summarizeDataUrl(previousSourceDataUrl),
      recordUndo: options.recordUndo !== false
    });

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
      writeInpaintDebugLog("inpaint-mask:save-scheduled", {
        chapterId,
        commitRevision,
        dataUrl: summarizeDataUrl(dataUrl),
        pageId: targetPage.id
      });
    }
  }, [currentChapter, currentChapterRef, nextInpaintLayerCommitRevision, recordInpaintMaskUndoSnapshot, scheduleInpaintMaskSave, selectedPage, selectedPageEditLocked, setCurrentChapter]);

  const commitSelectedPageInpaintResult = React.useCallback((dataUrl: string | undefined, options: InpaintLayerChangeOptions = {}) => {
    if (!currentChapter || !selectedPage || selectedPageEditLocked) {
      writeInpaintDebugLog("inpaint-result:state-skip", {
        hasCurrentChapter: Boolean(currentChapter),
        hasSelectedPage: Boolean(selectedPage),
        reason: selectedPageEditLocked ? "edit-locked" : "missing-state"
      });
      return;
    }

    const chapterId = currentChapter.id;
    const targetPage = selectedPage;
    const previousDataUrl = "previousDataUrl" in options ? options.previousDataUrl : selectedPage.inpaintResultDataUrl;
    const previousSourceDataUrl = options.previousResultSourceDataUrl;
    const livePage = resolveOpenChapterPage(currentChapterRef.current, chapterId, targetPage.id);
    if (livePage === null || (livePage && !isExpectedPreviousInpaintResult(livePage.inpaintResultDataUrl, previousDataUrl, previousSourceDataUrl))) {
      writeInpaintDebugLog("inpaint-result:state-skip", {
        chapterId,
        pageId: targetPage.id,
        incomingDataUrl: summarizeDataUrl(dataUrl),
        liveResultDataUrl: summarizeDataUrl(livePage ? livePage.inpaintResultDataUrl : undefined),
        previousDataUrl: summarizeDataUrl(previousDataUrl),
        previousSourceDataUrl: summarizeDataUrl(previousSourceDataUrl),
        reason: livePage === null ? "page-not-found" : "previous-result-mismatch",
        selectedPageResultDataUrl: summarizeDataUrl(selectedPage.inpaintResultDataUrl)
      });
      return;
    }
    const applyLocalUpdate = livePage !== undefined;
    const commitRevision = applyLocalUpdate ? nextInpaintLayerCommitRevision() : undefined;
    writeInpaintDebugLog("inpaint-result:state-update", {
      applyLocalUpdate,
      chapterId,
      commitRevision,
      incomingDataUrl: summarizeDataUrl(dataUrl),
      intermediateUndoCount: options.intermediateUndoDataUrls?.length ?? 0,
      pageId: targetPage.id,
      persist: options.persist !== false,
      previousDataUrl: summarizeDataUrl(previousDataUrl),
      previousSourceDataUrl: summarizeDataUrl(previousSourceDataUrl),
      recordUndo: options.recordUndo !== false
    });

    if (livePage) {
      if (options.recordUndo !== false) {
        const undoDataUrls = resolveInpaintUndoDataUrlSequence(previousDataUrl, options.intermediateUndoDataUrls, dataUrl);
        const stack = inpaintResultUndoStackRef.current.get(targetPage.id) ?? [];
        for (const undoDataUrl of undoDataUrls) {
          stack.push(undoDataUrl);
          recordGlobalUndoEntry({ kind: "inpaint-result", chapterId, pageId: targetPage.id });
          writeInpaintDebugLog("inpaint-undo:record-result", {
            chapterId,
            pageId: targetPage.id,
            snapshotDataUrl: summarizeDataUrl(undoDataUrl),
            stackSize: stack.length
          });
        }
        inpaintResultUndoStackRef.current.set(targetPage.id, stack.slice(-30));
        writeInpaintDebugLog("inpaint-undo:record-result-stack-trimmed", {
          chapterId,
          pageId: targetPage.id,
          stackSize: inpaintResultUndoStackRef.current.get(targetPage.id)?.length ?? 0
        });
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
      writeInpaintDebugLog("inpaint-result:save-scheduled", {
        chapterId,
        commitRevision,
        dataUrl: summarizeDataUrl(dataUrl),
        pageId: targetPage.id
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
    applyLocalUpdate: boolean;
    recordUndo: boolean;
    persist: boolean;
  }) => {
    if (applyLocalUpdate) {
      writeInpaintDebugLog("inpaint-layers:state-update", {
        chapterId,
        commitRevision,
        nextMaskDataUrl: summarizeDataUrl(nextMaskDataUrl),
        pageId: targetPage.id,
        previousMaskDataUrl: summarizeDataUrl(previousMaskDataUrl),
        previousResultDataUrl: summarizeDataUrl(previousResultDataUrl),
        recordUndo,
        resultDataUrl: summarizeDataUrl(resultDataUrl)
      });
      if (recordUndo) {
        const undoSnapshot = {
          maskDataUrl: previousMaskDataUrl,
          resultDataUrl: previousResultDataUrl
        };
        const nextSnapshot = {
          maskDataUrl: nextMaskDataUrl,
          resultDataUrl
        };
        if (!isInpaintLayerUndoSnapshotEqual(undoSnapshot, nextSnapshot)) {
          writeInpaintDebugLog("inpaint-undo:record-layer", {
            chapterId,
            pageId: targetPage.id,
            snapshot: {
              maskDataUrl: summarizeDataUrl(undoSnapshot.maskDataUrl),
              resultDataUrl: summarizeDataUrl(undoSnapshot.resultDataUrl)
            }
          });
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
      writeInpaintDebugLog("inpaint-layers:save-scheduled", {
        chapterId,
        commitRevision,
        maskDataUrl: summarizeDataUrl(nextMaskDataUrl),
        pageId: targetPage.id,
        resultDataUrl: summarizeDataUrl(resultDataUrl)
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
    const previousMaskSourceDataUrl = options.previousMaskSourceDataUrl;
    const previousResultDataUrl = "previousDataUrl" in options ? options.previousDataUrl : selectedPage.inpaintResultDataUrl;
    const previousResultSourceDataUrl = options.previousResultSourceDataUrl;
    const incomingMaskDataUrl = options.maskDataUrl;
    const maskDataUrlMode = options.maskDataUrlMode ?? "patch";
    const livePage = resolveOpenChapterPage(currentChapterRef.current, chapterId, targetPage.id);
    const liveMaskDataUrl = livePage ? resolvePageMaskDataUrl(livePage) : undefined;
    const liveResultDataUrl = livePage ? livePage.inpaintResultDataUrl : undefined;
    const liveStateChanged = Boolean(
      livePage && (
        !isExpectedPreviousInpaintMask(liveMaskDataUrl, previousMaskDataUrl, previousMaskSourceDataUrl) ||
        !isExpectedPreviousInpaintResult(liveResultDataUrl, previousResultDataUrl, previousResultSourceDataUrl)
      )
    );
    if (livePage === null || liveStateChanged) {
      writeInpaintDebugLog("inpaint-layers:state-skip", {
        chapterId,
        incomingMaskDataUrl: summarizeDataUrl(incomingMaskDataUrl),
        incomingResultDataUrl: summarizeDataUrl(dataUrl),
        liveMaskDataUrl: summarizeDataUrl(liveMaskDataUrl),
        liveResultDataUrl: summarizeDataUrl(liveResultDataUrl),
        maskDataUrlMode,
        pageId: targetPage.id,
        previousMaskDataUrl: summarizeDataUrl(previousMaskDataUrl),
        previousMaskSourceDataUrl: summarizeDataUrl(previousMaskSourceDataUrl),
        previousResultDataUrl: summarizeDataUrl(previousResultDataUrl),
        previousResultSourceDataUrl: summarizeDataUrl(previousResultSourceDataUrl),
        reason: livePage === null ? "page-not-found" : "previous-layer-mismatch"
      });
      return;
    }
    const applyLocalUpdate = livePage !== undefined;
    const commitRevision = applyLocalUpdate ? nextInpaintLayerCommitRevision() : undefined;
    writeInpaintDebugLog("inpaint-layers:state-update-request", {
      applyLocalUpdate,
      chapterId,
      commitRevision,
      incomingMaskDataUrl: summarizeDataUrl(incomingMaskDataUrl),
      incomingResultDataUrl: summarizeDataUrl(dataUrl),
      maskDataUrlMode,
      pageId: targetPage.id,
      persist: options.persist !== false,
      previousMaskDataUrl: summarizeDataUrl(previousMaskDataUrl),
      previousMaskSourceDataUrl: summarizeDataUrl(previousMaskSourceDataUrl),
      previousResultDataUrl: summarizeDataUrl(previousResultDataUrl),
      previousResultSourceDataUrl: summarizeDataUrl(previousResultSourceDataUrl),
      recordUndo: options.recordUndo !== false
    });

    const commitResolvedInpaintLayers = (nextMaskDataUrl: string | undefined) => {
      if (commitRevision !== undefined && inpaintLayerCommitRevisionRef.current !== commitRevision) {
        writeInpaintDebugLog("inpaint-layers:resolved-skip", {
          chapterId,
          commitRevision,
          currentCommitRevision: inpaintLayerCommitRevisionRef.current,
          nextMaskDataUrl: summarizeDataUrl(nextMaskDataUrl),
          pageId: targetPage.id,
          reason: "commit-revision-superseded",
          resultDataUrl: summarizeDataUrl(dataUrl)
        });
        return;
      }
      const currentLivePage = resolveOpenChapterPage(currentChapterRef.current, chapterId, targetPage.id);
      const currentLiveMaskDataUrl = currentLivePage ? resolvePageMaskDataUrl(currentLivePage) : undefined;
      const currentLiveResultDataUrl = currentLivePage ? currentLivePage.inpaintResultDataUrl : undefined;
      const currentLiveStateChanged = Boolean(
        currentLivePage && (
          !isExpectedPreviousInpaintMask(currentLiveMaskDataUrl, previousMaskDataUrl, previousMaskSourceDataUrl) ||
          !isExpectedPreviousInpaintResult(currentLiveResultDataUrl, previousResultDataUrl, previousResultSourceDataUrl)
        )
      );
      if (currentLivePage === null || currentLiveStateChanged) {
        writeInpaintDebugLog("inpaint-layers:resolved-skip", {
          chapterId,
          currentLiveMaskDataUrl: summarizeDataUrl(currentLiveMaskDataUrl),
          currentLiveResultDataUrl: summarizeDataUrl(currentLiveResultDataUrl),
          nextMaskDataUrl: summarizeDataUrl(nextMaskDataUrl),
          pageId: targetPage.id,
          previousMaskDataUrl: summarizeDataUrl(previousMaskDataUrl),
          previousMaskSourceDataUrl: summarizeDataUrl(previousMaskSourceDataUrl),
          previousResultDataUrl: summarizeDataUrl(previousResultDataUrl),
          previousResultSourceDataUrl: summarizeDataUrl(previousResultSourceDataUrl),
          reason: currentLivePage === null ? "page-not-found" : "previous-layer-mismatch-after-resolve",
          resultDataUrl: summarizeDataUrl(dataUrl)
        });
        return;
      }
      const applyCurrentLocalUpdate = applyLocalUpdate && Boolean(currentLivePage);
      writeInpaintDebugLog("inpaint-layers:resolved-apply", {
        applyCurrentLocalUpdate,
        chapterId,
        commitRevision: applyCurrentLocalUpdate ? commitRevision : undefined,
        nextMaskDataUrl: summarizeDataUrl(nextMaskDataUrl),
        pageId: targetPage.id,
        previousMaskDataUrl: summarizeDataUrl(previousMaskDataUrl),
        previousResultDataUrl: summarizeDataUrl(previousResultDataUrl),
        resultDataUrl: summarizeDataUrl(dataUrl)
      });

      commitSelectedPageInpaintLayers({
        chapterId,
        commitRevision: applyCurrentLocalUpdate ? commitRevision : undefined,
        resultDataUrl: dataUrl,
        nextMaskDataUrl,
        targetPage: currentLivePage ?? targetPage,
        previousMaskDataUrl,
        previousResultDataUrl,
        applyLocalUpdate: applyCurrentLocalUpdate,
        recordUndo: options.recordUndo !== false,
        persist: options.persist !== false
      });
    };

    if (!incomingMaskDataUrl || maskDataUrlMode === "full") {
      commitResolvedInpaintLayers(incomingMaskDataUrl ?? previousMaskDataUrl);
      return;
    }

    void mergePartialInpaintMask(previousMaskDataUrl, incomingMaskDataUrl, targetPage.width, targetPage.height).then(
      commitResolvedInpaintLayers
    ).catch((error) => {
      console.error(error);
      writeInpaintDebugLog("inpaint-layers:mask-merge-error", {
        chapterId,
        error: summarizeError(error),
        incomingMaskDataUrl: summarizeDataUrl(incomingMaskDataUrl),
        pageId: targetPage.id,
        previousMaskDataUrl: summarizeDataUrl(previousMaskDataUrl)
      });
      commitResolvedInpaintLayers(previousMaskDataUrl);
    });
  }, [commitSelectedPageInpaintLayers, commitSelectedPageInpaintResult, currentChapter, currentChapterRef, nextInpaintLayerCommitRevision, selectedPage, selectedPageEditLocked]);

  const restorePageInpaintMaskSnapshot = React.useCallback((pageId: string, snapshot: InpaintMaskUndoSnapshot) => {
    const chapter = currentChapterRef.current;
    if (!chapter || selectedPageEditLocked) {
      writeInpaintDebugLog("inpaint-undo:restore-mask-skip", {
        hasChapter: Boolean(chapter),
        pageId,
        reason: selectedPageEditLocked ? "edit-locked" : "missing-chapter",
        snapshot: summarizeInpaintMaskUndoSnapshot(snapshot)
      });
      return;
    }

    const commitRevision = nextInpaintLayerCommitRevision();
    const updatedAt = new Date().toISOString();
    writeInpaintDebugLog("inpaint-undo:restore-mask", {
      chapterId: chapter.id,
      commitRevision,
      pageId,
      snapshot: summarizeInpaintMaskUndoSnapshot(snapshot)
    });
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
    writeInpaintDebugLog("inpaint-undo:restore-mask-save-scheduled", {
      chapterId: chapter.id,
      commitRevision,
      pageId,
      snapshot: summarizeInpaintMaskUndoSnapshot(snapshot)
    });
  }, [currentChapterRef, nextInpaintLayerCommitRevision, scheduleInpaintLayersSave, selectedPageEditLocked, setCurrentChapter]);

  const undoPageInpaint = React.useCallback((pageId: string) => {
    if (selectedPageEditLocked) {
      writeInpaintDebugLog("inpaint-undo:mask-skip", {
        pageId,
        reason: "edit-locked"
      });
      return;
    }

    const stack = inpaintUndoStackRef.current.get(pageId) ?? [];
    if (stack.length === 0) {
      writeInpaintDebugLog("inpaint-undo:mask-skip", {
        pageId,
        reason: "empty-stack"
      });
      return;
    }
    const previousSnapshot = stack.pop();
    if (!previousSnapshot) {
      writeInpaintDebugLog("inpaint-undo:mask-skip", {
        pageId,
        reason: "missing-snapshot-after-pop"
      });
      return;
    }
    inpaintUndoStackRef.current.set(pageId, stack);
    consumeGlobalUndoEntry("inpaint-mask", pageId);
    writeInpaintDebugLog("inpaint-undo:mask-pop", {
      pageId,
      remainingStackSize: stack.length,
      snapshot: summarizeInpaintMaskUndoSnapshot(previousSnapshot)
    });
    restorePageInpaintMaskSnapshot(pageId, previousSnapshot);
  }, [consumeGlobalUndoEntry, restorePageInpaintMaskSnapshot, selectedPageEditLocked]);

  const undoPageInpaintResult = React.useCallback((pageId: string) => {
    if (selectedPageEditLocked) {
      writeInpaintDebugLog("inpaint-undo:result-skip", {
        pageId,
        reason: "edit-locked"
      });
      return;
    }

    const stack = inpaintResultUndoStackRef.current.get(pageId) ?? [];
    if (stack.length === 0) {
      writeInpaintDebugLog("inpaint-undo:result-skip", {
        pageId,
        reason: "empty-stack"
      });
      return;
    }
    const previousDataUrl = stack.pop();
    inpaintResultUndoStackRef.current.set(pageId, stack);
    consumeGlobalUndoEntry("inpaint-result", pageId);
    writeInpaintDebugLog("inpaint-undo:result-pop", {
      pageId,
      previousDataUrl: summarizeDataUrl(previousDataUrl),
      remainingStackSize: stack.length
    });

    const chapter = currentChapterRef.current;
    if (!chapter) {
      writeInpaintDebugLog("inpaint-undo:result-restore-skip", {
        pageId,
        reason: "missing-chapter"
      });
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
    writeInpaintDebugLog("inpaint-undo:result-save-scheduled", {
      chapterId: chapter.id,
      commitRevision,
      dataUrl: summarizeDataUrl(previousDataUrl),
      pageId
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

export function isExpectedPreviousInpaintMask(
  liveMaskDataUrl: string | undefined,
  previousDataUrl: string | undefined,
  previousSourceDataUrl: string | undefined
): boolean {
  return liveMaskDataUrl === previousDataUrl || Boolean(previousSourceDataUrl && liveMaskDataUrl === previousSourceDataUrl);
}

export function isExpectedPreviousInpaintResult(
  liveResultDataUrl: string | undefined,
  previousDataUrl: string | undefined,
  previousSourceDataUrl: string | undefined
): boolean {
  return liveResultDataUrl === previousDataUrl || Boolean(previousSourceDataUrl && liveResultDataUrl === previousSourceDataUrl);
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

function summarizeInpaintMaskUndoSnapshot(snapshot: InpaintMaskUndoSnapshot): {
  inpaintMaskDataUrl: ReturnType<typeof summarizeDataUrl>;
  inpaintMaskPath?: string;
  inpaintResultDataUrl: ReturnType<typeof summarizeDataUrl>;
  inpaintResultPath?: string;
  inpaintStatus?: MangaPage["inpaintStatus"];
} {
  return {
    inpaintMaskDataUrl: summarizeDataUrl(snapshot.inpaintMaskDataUrl),
    inpaintMaskPath: snapshot.inpaintMaskPath,
    inpaintResultDataUrl: summarizeDataUrl(snapshot.inpaintResultDataUrl),
    inpaintResultPath: snapshot.inpaintResultPath,
    inpaintStatus: snapshot.inpaintStatus
  };
}
