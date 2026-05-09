import React from "react";
import type { ChapterSnapshot } from "../../../shared/types";
import { summarizeDataUrl, summarizeError, writeInpaintDebugLog } from "../lib/inpaintDiagnostics";
import type { RecoverableFailureId } from "./useRecoverableFailures";

const INPAINT_LAYER_SAVE_IDLE_DELAY_MS = 1000;

type PendingInpaintMaskSave = {
  chapterId: string;
  commitRevision?: number;
  pageId: string;
  dataUrl: string | undefined;
};

type PendingInpaintResultSave = {
  chapterId: string;
  commitRevision?: number;
  pageId: string;
  dataUrl: string | undefined;
};

type PendingInpaintLayersSave = {
  chapterId: string;
  commitRevision?: number;
  pageId: string;
  maskDataUrl: string | undefined;
  resultDataUrl: string | undefined;
};

export type PendingInpaintLayerSave =
  | (PendingInpaintMaskSave & { kind: "mask" })
  | (PendingInpaintResultSave & { kind: "result" })
  | (PendingInpaintLayersSave & { kind: "layers" });

type UseInpaintLayerSaveQueueOptions = {
  clearRecoverableFailure?: (id: RecoverableFailureId) => void;
  currentChapterRef: React.RefObject<ChapterSnapshot | null>;
  dirty: boolean;
  isInpaintLayerSaveCurrent?: (commitRevision: number | undefined) => boolean;
  mergeLiveChapter: (chapter: ChapterSnapshot) => void;
  pushStatus: (line: string) => void;
  refreshLibrary: () => Promise<void>;
  reportRecoverableFailure?: (failure: { id: RecoverableFailureId; message: string; title: string }) => void;
  resolveLatestInpaintLayerSave?: (chapterId: string, pageId: string) => PendingInpaintLayerSave | null;
  saveNow: () => Promise<void>;
  signalSaveComplete: () => void;
};

type UseInpaintLayerSaveQueueState = {
  beginInpaintLayerInteraction: () => void;
  clearPendingInpaintSaveTimers: () => void;
  clearPendingInpaintSaves: () => void;
  endInpaintLayerInteraction: () => void;
  flushInpaintMaskSave: () => Promise<void>;
  flushInpaintResultSave: () => Promise<void>;
  scheduleInpaintLayersSave: (pending: PendingInpaintLayersSave) => void;
  scheduleInpaintMaskSave: (pending: PendingInpaintMaskSave) => void;
  scheduleInpaintResultSave: (pending: PendingInpaintResultSave) => void;
};

export function useInpaintLayerSaveQueue({
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
}: UseInpaintLayerSaveQueueOptions): UseInpaintLayerSaveQueueState {
  const inpaintLayerSaveTimerRef = React.useRef<number | null>(null);
  const inpaintLayerSaveStateRef = React.useRef<PendingInpaintLayerSave[]>([]);
  const inpaintLayerFlushPromiseRef = React.useRef<Promise<void> | null>(null);
  const inpaintLayerInteractionDepthRef = React.useRef(0);
  const inpaintLayerLastInteractionAtRef = React.useRef(0);

  const clearPendingInpaintSaveTimers = React.useCallback(() => {
    if (inpaintLayerSaveTimerRef.current !== null) {
      window.clearTimeout(inpaintLayerSaveTimerRef.current);
      inpaintLayerSaveTimerRef.current = null;
      writeInpaintDebugLog("inpaint-save:timer-cleared", {
        queueLength: inpaintLayerSaveStateRef.current.length
      });
    }
  }, []);

  const clearPendingInpaintSaves = React.useCallback(() => {
    clearPendingInpaintSaveTimers();
    writeInpaintDebugLog("inpaint-save:queue-cleared", {
      queueLength: inpaintLayerSaveStateRef.current.length
    });
    inpaintLayerSaveStateRef.current = [];
  }, [clearPendingInpaintSaveTimers]);

  const queueLatestInpaintLayerSave = React.useCallback((pending: PendingInpaintLayerSave) => {
    const latest = resolveLatestInpaintLayerSave?.(pending.chapterId, pending.pageId);
    if (!latest || latest.commitRevision === pending.commitRevision) {
      writeInpaintDebugLog("inpaint-save:queue-latest-skip", {
        latest: latest ? summarizePendingInpaintLayerSave(latest) : null,
        pending: summarizePendingInpaintLayerSave(pending),
        reason: latest ? "same-revision" : "missing-latest"
      });
      return;
    }
    inpaintLayerSaveStateRef.current = coalescePendingInpaintLayerSaves(inpaintLayerSaveStateRef.current, latest);
    writeInpaintDebugLog("inpaint-save:queue-latest", {
      latest: summarizePendingInpaintLayerSave(latest),
      pending: summarizePendingInpaintLayerSave(pending),
      queueLength: inpaintLayerSaveStateRef.current.length
    });
  }, [resolveLatestInpaintLayerSave]);

  const flushInpaintLayerSave = React.useCallback((): Promise<void> => {
    const currentFlush = inpaintLayerFlushPromiseRef.current;
    if (currentFlush) {
      return currentFlush;
    }

    clearPendingInpaintSaveTimers();
    const flushPromise = Promise.resolve().then(async () => {
      writeInpaintDebugLog("inpaint-save:flush-start", {
        queueLength: inpaintLayerSaveStateRef.current.length
      });
      try {
        while (true) {
          const [pending, ...remaining] = inpaintLayerSaveStateRef.current;
          if (!pending) {
            writeInpaintDebugLog("inpaint-save:flush-empty");
            return;
          }

          inpaintLayerSaveStateRef.current = remaining;
          writeInpaintDebugLog("inpaint-save:dequeue", {
            pending: summarizePendingInpaintLayerSave(pending),
            remainingQueueLength: remaining.length
          });
          if (shouldSkipNonInlineInpaintLayerSave(pending)) {
            writeInpaintDebugLog("inpaint-save:skip-non-inline-url", {
              pending: summarizePendingInpaintLayerSave(pending),
              reason: "pending-points-at-existing-image-url"
            });
            if (inpaintLayerSaveStateRef.current.length === 0) {
              signalSaveComplete();
            }
            continue;
          }
          try {
            if (dirty && currentChapterRef.current?.id === pending.chapterId) {
              writeInpaintDebugLog("inpaint-save:save-now-before-layer-save", {
                pending: summarizePendingInpaintLayerSave(pending)
              });
              await saveNow();
            }
            const result = pending.kind === "mask"
              ? await window.mangaApi.saveInpaintMask({
                  chapterId: pending.chapterId,
                  pageId: pending.pageId,
                  maskDataUrl: await window.mangaApi.resolveOptionalImageDataUrl(pending.dataUrl)
                })
              : pending.kind === "result"
                ? await window.mangaApi.saveInpaintResultLayer({
                    chapterId: pending.chapterId,
                    pageId: pending.pageId,
                    resultDataUrl: await window.mangaApi.resolveOptionalImageDataUrl(pending.dataUrl)
                  })
                : await saveInpaintLayersState(pending);
            const saveCurrent = isInpaintLayerSaveCurrent?.(pending.commitRevision) ?? true;
            writeInpaintDebugLog("inpaint-save:save-complete", {
              pending: summarizePendingInpaintLayerSave(pending),
              resultPage: summarizeSavedChapterPage(result.chapter, pending.pageId),
              saveCurrent
            });

            if (!saveCurrent) {
              queueLatestInpaintLayerSave(pending);
            }

            if (inpaintLayerSaveStateRef.current.length === 0 && saveCurrent) {
              const chapterToMerge = preservePendingInpaintSaveDataUrls(result.chapter, pending, currentChapterRef.current);
              writeInpaintDebugLog("inpaint-save:merge-live-chapter", {
                mergedPage: summarizeSavedChapterPage(chapterToMerge, pending.pageId),
                pending: summarizePendingInpaintLayerSave(pending),
                resultPage: summarizeSavedChapterPage(result.chapter, pending.pageId)
              });
              mergeLiveChapter(chapterToMerge);
              signalSaveComplete();
              if (pending.kind === "mask" || pending.kind === "layers") {
                clearRecoverableFailure?.("inpaint-mask-save");
              }
              if (pending.kind === "result" || pending.kind === "layers") {
                clearRecoverableFailure?.("inpaint-result-save");
              }
              void refreshLibrary();
            }
          } catch (error) {
            console.error(error);
            const saveCurrent = isInpaintLayerSaveCurrent?.(pending.commitRevision) ?? true;
            if (!saveCurrent) {
              writeInpaintDebugLog("inpaint-save:error-superseded", {
                error: summarizeError(error),
                pending: summarizePendingInpaintLayerSave(pending)
              });
              queueLatestInpaintLayerSave(pending);
              continue;
            }

            const nextQueue = requeueFailedInpaintLayerSave(pending, inpaintLayerSaveStateRef.current);
            inpaintLayerSaveStateRef.current = nextQueue;
            writeInpaintDebugLog("inpaint-save:error-requeue", {
              error: summarizeError(error),
              nextQueueLength: nextQueue.length,
              pending: summarizePendingInpaintLayerSave(pending)
            });
            const message = error instanceof Error ? error.message : resolveInpaintLayerSaveFailureMessage(pending);
            const failure = resolveInpaintLayerSaveFailure(pending);
            pushStatus(message);
            reportRecoverableFailure?.(failure);
            if (nextQueue.length === 0 || nextQueue[0] === pending) {
              return;
            }
          }
        }
      } finally {
        writeInpaintDebugLog("inpaint-save:flush-end", {
          queueLength: inpaintLayerSaveStateRef.current.length
        });
        inpaintLayerFlushPromiseRef.current = null;
      }
    });
    inpaintLayerFlushPromiseRef.current = flushPromise;
    return flushPromise;
  }, [
    clearRecoverableFailure,
    clearPendingInpaintSaveTimers,
    currentChapterRef,
    dirty,
    isInpaintLayerSaveCurrent,
    mergeLiveChapter,
    pushStatus,
    queueLatestInpaintLayerSave,
    refreshLibrary,
    reportRecoverableFailure,
    saveNow,
    signalSaveComplete
  ]);

  const scheduleInpaintLayerFlush = React.useCallback((delayMs = INPAINT_LAYER_SAVE_IDLE_DELAY_MS) => {
    if (inpaintLayerFlushPromiseRef.current) {
      return;
    }
    if (inpaintLayerSaveTimerRef.current) {
      window.clearTimeout(inpaintLayerSaveTimerRef.current);
    }
    inpaintLayerSaveTimerRef.current = window.setTimeout(() => {
      inpaintLayerSaveTimerRef.current = null;
      const idleForMs = Date.now() - inpaintLayerLastInteractionAtRef.current;
      if (inpaintLayerInteractionDepthRef.current > 0 || idleForMs < INPAINT_LAYER_SAVE_IDLE_DELAY_MS) {
        writeInpaintDebugLog("inpaint-save:flush-deferred", {
          idleForMs,
          interactionDepth: inpaintLayerInteractionDepthRef.current,
          queueLength: inpaintLayerSaveStateRef.current.length
        });
        scheduleInpaintLayerFlush(Math.max(50, INPAINT_LAYER_SAVE_IDLE_DELAY_MS - idleForMs));
        return;
      }
      writeInpaintDebugLog("inpaint-save:flush-timer-fired", {
        idleForMs,
        queueLength: inpaintLayerSaveStateRef.current.length
      });
      void flushInpaintLayerSave();
    }, delayMs);
    writeInpaintDebugLog("inpaint-save:flush-scheduled", {
      delayMs,
      queueLength: inpaintLayerSaveStateRef.current.length
    });
  }, [flushInpaintLayerSave]);

  const scheduleInpaintLayerSave = React.useCallback((pending: PendingInpaintLayerSave) => {
    const previousQueueLength = inpaintLayerSaveStateRef.current.length;
    inpaintLayerSaveStateRef.current = coalescePendingInpaintLayerSaves(inpaintLayerSaveStateRef.current, pending);
    writeInpaintDebugLog("inpaint-save:enqueue", {
      pending: summarizePendingInpaintLayerSave(pending),
      previousQueueLength,
      queueLength: inpaintLayerSaveStateRef.current.length
    });
    if (inpaintLayerFlushPromiseRef.current) {
      writeInpaintDebugLog("inpaint-save:enqueue-during-flush", {
        pending: summarizePendingInpaintLayerSave(pending),
        queueLength: inpaintLayerSaveStateRef.current.length
      });
      return;
    }
    scheduleInpaintLayerFlush();
  }, [scheduleInpaintLayerFlush]);

  const beginInpaintLayerInteraction = React.useCallback(() => {
    inpaintLayerInteractionDepthRef.current += 1;
    inpaintLayerLastInteractionAtRef.current = Date.now();
    writeInpaintDebugLog("inpaint-save:interaction-begin", {
      interactionDepth: inpaintLayerInteractionDepthRef.current,
      queueLength: inpaintLayerSaveStateRef.current.length
    });
    if (inpaintLayerSaveTimerRef.current !== null) {
      window.clearTimeout(inpaintLayerSaveTimerRef.current);
      inpaintLayerSaveTimerRef.current = null;
    }
  }, []);

  const endInpaintLayerInteraction = React.useCallback(() => {
    inpaintLayerInteractionDepthRef.current = Math.max(0, inpaintLayerInteractionDepthRef.current - 1);
    inpaintLayerLastInteractionAtRef.current = Date.now();
    writeInpaintDebugLog("inpaint-save:interaction-end", {
      interactionDepth: inpaintLayerInteractionDepthRef.current,
      queueLength: inpaintLayerSaveStateRef.current.length
    });
    if (
      inpaintLayerInteractionDepthRef.current === 0 &&
      inpaintLayerSaveStateRef.current.length > 0 &&
      !inpaintLayerFlushPromiseRef.current &&
      !inpaintLayerSaveTimerRef.current
    ) {
      scheduleInpaintLayerFlush();
    }
  }, [scheduleInpaintLayerFlush]);

  const scheduleInpaintMaskSave = React.useCallback((pending: PendingInpaintMaskSave) => {
    scheduleInpaintLayerSave({ ...pending, kind: "mask" });
  }, [scheduleInpaintLayerSave]);

  const scheduleInpaintResultSave = React.useCallback((pending: PendingInpaintResultSave) => {
    scheduleInpaintLayerSave({ ...pending, kind: "result" });
  }, [scheduleInpaintLayerSave]);

  const scheduleInpaintLayersSave = React.useCallback((pending: PendingInpaintLayersSave) => {
    scheduleInpaintLayerSave({ ...pending, kind: "layers" });
  }, [scheduleInpaintLayerSave]);

  return {
    beginInpaintLayerInteraction,
    clearPendingInpaintSaveTimers,
    clearPendingInpaintSaves,
    endInpaintLayerInteraction,
    flushInpaintMaskSave: flushInpaintLayerSave,
    flushInpaintResultSave: flushInpaintLayerSave,
    scheduleInpaintLayersSave,
    scheduleInpaintMaskSave,
    scheduleInpaintResultSave
  };
}

function resolveInpaintLayerSaveFailureMessage(pending: PendingInpaintLayerSave): string {
  if (pending.kind === "mask") {
    return "인페인트 마스크 저장에 실패했습니다.";
  }
  if (pending.kind === "result") {
    return "인페인트 결과 레이어 저장에 실패했습니다.";
  }
  return "인페인트 레이어 저장에 실패했습니다.";
}

function resolveInpaintLayerSaveFailure(pending: PendingInpaintLayerSave): {
  id: RecoverableFailureId;
  message: string;
  title: string;
} {
  if (pending.kind === "mask") {
    return {
      id: "inpaint-mask-save",
      title: "인페인트 마스크 저장 실패",
      message: "마스크 편집 내용은 현재 화면에 남아 있습니다. 다시 저장을 시도하세요."
    };
  }
  if (pending.kind === "result") {
    return {
      id: "inpaint-result-save",
      title: "인페인트 결과 저장 실패",
      message: "결과 레이어는 현재 화면에 남아 있습니다. 다시 저장을 시도하세요."
    };
  }
  return {
    id: "inpaint-result-save",
    title: "인페인트 레이어 저장 실패",
    message: "마스크와 결과 레이어는 현재 화면에 남아 있습니다. 다시 저장을 시도하세요."
  };
}

export function coalescePendingInpaintLayerSaves(
  queue: PendingInpaintLayerSave[],
  pending: PendingInpaintLayerSave
): PendingInpaintLayerSave[] {
  const filtered = queue.filter((candidate) => {
    if (candidate.chapterId !== pending.chapterId || candidate.pageId !== pending.pageId) {
      return true;
    }
    if (pending.kind === "layers") {
      return false;
    }
    return candidate.kind !== pending.kind;
  });
  return [...filtered, pending];
}

export function requeueFailedInpaintLayerSave(
  failed: PendingInpaintLayerSave,
  queue: PendingInpaintLayerSave[]
): PendingInpaintLayerSave[] {
  const superseded = queue.some((candidate) => {
    if (candidate.chapterId !== failed.chapterId || candidate.pageId !== failed.pageId) {
      return false;
    }
    if (candidate.kind === "layers") {
      return true;
    }
    return failed.kind !== "layers" && candidate.kind === failed.kind;
  });
  return superseded ? queue : [failed, ...queue];
}

async function saveInpaintLayersState(pending: PendingInpaintLayersSave): Promise<{ chapter: ChapterSnapshot }> {
  if (pending.maskDataUrl && pending.resultDataUrl) {
    return window.mangaApi.saveInpaintLayers({
      chapterId: pending.chapterId,
      pageId: pending.pageId,
      maskDataUrl: await window.mangaApi.resolveImageDataUrl(pending.maskDataUrl),
      resultDataUrl: await window.mangaApi.resolveImageDataUrl(pending.resultDataUrl),
      preserveMaskDataUrl: true
    });
  }
  if (pending.maskDataUrl) {
    await window.mangaApi.saveInpaintMask({
      chapterId: pending.chapterId,
      pageId: pending.pageId,
      maskDataUrl: await window.mangaApi.resolveImageDataUrl(pending.maskDataUrl)
    });
    return window.mangaApi.saveInpaintResultLayer({
      chapterId: pending.chapterId,
      pageId: pending.pageId,
      resultDataUrl: undefined
    });
  }
  if (!pending.resultDataUrl) {
    return window.mangaApi.saveInpaintMask({
      chapterId: pending.chapterId,
      pageId: pending.pageId,
      maskDataUrl: undefined
    });
  }

  await window.mangaApi.saveInpaintMask({
    chapterId: pending.chapterId,
    pageId: pending.pageId,
    maskDataUrl: undefined
  });
  return window.mangaApi.saveInpaintResultLayer({
    chapterId: pending.chapterId,
    pageId: pending.pageId,
    resultDataUrl: await window.mangaApi.resolveImageDataUrl(pending.resultDataUrl)
  });
}

function summarizePendingInpaintLayerSave(pending: PendingInpaintLayerSave): {
  chapterId: string;
  commitRevision?: number;
  kind: PendingInpaintLayerSave["kind"];
  maskDataUrl?: ReturnType<typeof summarizeDataUrl>;
  pageId: string;
  resultDataUrl?: ReturnType<typeof summarizeDataUrl>;
} {
  if (pending.kind === "mask") {
    return {
      chapterId: pending.chapterId,
      commitRevision: pending.commitRevision,
      kind: pending.kind,
      maskDataUrl: summarizeDataUrl(pending.dataUrl),
      pageId: pending.pageId
    };
  }
  if (pending.kind === "result") {
    return {
      chapterId: pending.chapterId,
      commitRevision: pending.commitRevision,
      kind: pending.kind,
      pageId: pending.pageId,
      resultDataUrl: summarizeDataUrl(pending.dataUrl)
    };
  }
  return {
    chapterId: pending.chapterId,
    commitRevision: pending.commitRevision,
    kind: pending.kind,
    maskDataUrl: summarizeDataUrl(pending.maskDataUrl),
    pageId: pending.pageId,
    resultDataUrl: summarizeDataUrl(pending.resultDataUrl)
  };
}

function summarizeSavedChapterPage(chapter: ChapterSnapshot, pageId: string): {
  inpaintMaskDataUrl?: ReturnType<typeof summarizeDataUrl>;
  inpaintMaskPath?: string;
  inpaintResultDataUrl?: ReturnType<typeof summarizeDataUrl>;
  inpaintResultPath?: string;
  inpaintStatus?: string;
  pageFound: boolean;
  updatedAt?: string;
} {
  const page = chapter.pages.find((candidate) => candidate.id === pageId);
  if (!page) {
    return { pageFound: false };
  }
  return {
    inpaintMaskDataUrl: summarizeDataUrl(page.inpaintMaskDataUrl ?? page.inpaintLayerDataUrl),
    inpaintMaskPath: page.inpaintMaskPath,
    inpaintResultDataUrl: summarizeDataUrl(page.inpaintResultDataUrl),
    inpaintResultPath: page.inpaintResultPath,
    inpaintStatus: page.inpaintStatus,
    pageFound: true,
    updatedAt: page.updatedAt
  };
}

export function preservePendingInpaintSaveDataUrls(
  chapter: ChapterSnapshot,
  pending: PendingInpaintLayerSave,
  currentChapter: ChapterSnapshot | null
): ChapterSnapshot {
  const currentPage = currentChapter?.id === chapter.id
    ? currentChapter.pages.find((page) => page.id === pending.pageId)
    : undefined;

  return {
    ...chapter,
    pages: chapter.pages.map((page) => {
      if (page.id !== pending.pageId) {
        return page;
      }

      const currentMaskDataUrl = currentPage?.inpaintMaskDataUrl ??
        currentPage?.inpaintLayerDataUrl ??
        page.inpaintMaskDataUrl ??
        page.inpaintLayerDataUrl;
      const currentResultDataUrl = currentPage?.inpaintResultDataUrl ?? page.inpaintResultDataUrl;
      if (pending.kind === "mask") {
        return {
          ...page,
          inpaintMaskDataUrl: pending.dataUrl,
          inpaintResultDataUrl: page.inpaintResultPath ? currentResultDataUrl : undefined
        };
      }
      if (pending.kind === "result") {
        return {
          ...page,
          inpaintMaskDataUrl: currentMaskDataUrl,
          inpaintResultDataUrl: pending.dataUrl
        };
      }
      return {
        ...page,
        inpaintMaskDataUrl: pending.maskDataUrl,
        inpaintResultDataUrl: pending.resultDataUrl
      };
    })
  };
}

function shouldSkipNonInlineInpaintLayerSave(pending: PendingInpaintLayerSave): boolean {
  if (pending.kind === "mask") {
    return isExistingImageUrl(pending.dataUrl);
  }
  if (pending.kind === "result") {
    return isExistingImageUrl(pending.dataUrl);
  }
  return isExistingImageUrl(pending.maskDataUrl) || isExistingImageUrl(pending.resultDataUrl);
}

function isExistingImageUrl(dataUrl: string | undefined): boolean {
  return Boolean(dataUrl && !dataUrl.startsWith("data:"));
}
