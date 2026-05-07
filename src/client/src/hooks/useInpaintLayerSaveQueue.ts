import React from "react";
import type { ChapterSnapshot } from "../../../shared/types";
import type { RecoverableFailureId } from "./useRecoverableFailures";

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
  clearPendingInpaintSaveTimers: () => void;
  clearPendingInpaintSaves: () => void;
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

  const clearPendingInpaintSaveTimers = React.useCallback(() => {
    if (inpaintLayerSaveTimerRef.current !== null) {
      window.clearTimeout(inpaintLayerSaveTimerRef.current);
      inpaintLayerSaveTimerRef.current = null;
    }
  }, []);

  const clearPendingInpaintSaves = React.useCallback(() => {
    clearPendingInpaintSaveTimers();
    inpaintLayerSaveStateRef.current = [];
  }, [clearPendingInpaintSaveTimers]);

  const queueLatestInpaintLayerSave = React.useCallback((pending: PendingInpaintLayerSave) => {
    const latest = resolveLatestInpaintLayerSave?.(pending.chapterId, pending.pageId);
    if (!latest || latest.commitRevision === pending.commitRevision) {
      return;
    }
    inpaintLayerSaveStateRef.current = coalescePendingInpaintLayerSaves(inpaintLayerSaveStateRef.current, latest);
  }, [resolveLatestInpaintLayerSave]);

  const flushInpaintLayerSave = React.useCallback((): Promise<void> => {
    const currentFlush = inpaintLayerFlushPromiseRef.current;
    if (currentFlush) {
      return currentFlush;
    }

    clearPendingInpaintSaveTimers();
    const flushPromise = Promise.resolve().then(async () => {
      try {
        while (true) {
          const [pending, ...remaining] = inpaintLayerSaveStateRef.current;
          if (!pending) {
            return;
          }

          inpaintLayerSaveStateRef.current = remaining;
          try {
            if (dirty && currentChapterRef.current?.id === pending.chapterId) {
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

            if (!saveCurrent) {
              queueLatestInpaintLayerSave(pending);
            }

            if (inpaintLayerSaveStateRef.current.length === 0 && saveCurrent) {
              mergeLiveChapter(result.chapter);
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
              queueLatestInpaintLayerSave(pending);
              continue;
            }

            const nextQueue = requeueFailedInpaintLayerSave(pending, inpaintLayerSaveStateRef.current);
            inpaintLayerSaveStateRef.current = nextQueue;
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

  const scheduleInpaintLayerSave = React.useCallback((pending: PendingInpaintLayerSave) => {
    inpaintLayerSaveStateRef.current = coalescePendingInpaintLayerSaves(inpaintLayerSaveStateRef.current, pending);
    if (inpaintLayerFlushPromiseRef.current) {
      return;
    }
    if (inpaintLayerSaveTimerRef.current) {
      window.clearTimeout(inpaintLayerSaveTimerRef.current);
    }
    inpaintLayerSaveTimerRef.current = window.setTimeout(() => {
      inpaintLayerSaveTimerRef.current = null;
      void flushInpaintLayerSave();
    }, 250);
  }, [flushInpaintLayerSave]);

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
    clearPendingInpaintSaveTimers,
    clearPendingInpaintSaves,
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
