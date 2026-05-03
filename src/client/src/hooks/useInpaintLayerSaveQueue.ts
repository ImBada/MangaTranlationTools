import React from "react";
import type { ChapterSnapshot } from "../../../shared/types";
import type { RecoverableFailureId } from "./useRecoverableFailures";

export type PendingInpaintMaskSave = {
  chapterId: string;
  pageId: string;
  dataUrl: string | undefined;
};

export type PendingInpaintResultSave = {
  chapterId: string;
  pageId: string;
  dataUrl: string | undefined;
};

type UseInpaintLayerSaveQueueOptions = {
  clearRecoverableFailure?: (id: RecoverableFailureId) => void;
  currentChapterRef: React.RefObject<ChapterSnapshot | null>;
  dirty: boolean;
  mergeLiveChapter: (chapter: ChapterSnapshot) => void;
  pushStatus: (line: string) => void;
  refreshLibrary: () => Promise<void>;
  reportRecoverableFailure?: (failure: { id: RecoverableFailureId; message: string; title: string }) => void;
  saveNow: () => Promise<void>;
  signalSaveComplete: () => void;
};

type UseInpaintLayerSaveQueueState = {
  clearPendingInpaintSaves: () => void;
  flushInpaintMaskSave: () => Promise<void>;
  flushInpaintResultSave: () => Promise<void>;
  scheduleInpaintMaskSave: (pending: PendingInpaintMaskSave) => void;
  scheduleInpaintResultSave: (pending: PendingInpaintResultSave) => void;
};

export function useInpaintLayerSaveQueue({
  clearRecoverableFailure,
  currentChapterRef,
  dirty,
  mergeLiveChapter,
  pushStatus,
  refreshLibrary,
  reportRecoverableFailure,
  saveNow,
  signalSaveComplete
}: UseInpaintLayerSaveQueueOptions): UseInpaintLayerSaveQueueState {
  const inpaintMaskSaveTimerRef = React.useRef<number | null>(null);
  const inpaintMaskSaveStateRef = React.useRef<PendingInpaintMaskSave | null>(null);
  const inpaintMaskSavingRef = React.useRef(false);
  const inpaintResultSaveTimerRef = React.useRef<number | null>(null);
  const inpaintResultSaveStateRef = React.useRef<PendingInpaintResultSave | null>(null);
  const inpaintResultSavingRef = React.useRef(false);

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
      const maskDataUrl = await window.mangaApi.resolveOptionalImageDataUrl(pending.dataUrl);
      const result = await window.mangaApi.saveInpaintMask({
        chapterId: pending.chapterId,
        pageId: pending.pageId,
        maskDataUrl
      });

      if (!inpaintMaskSaveStateRef.current) {
        mergeLiveChapter(result.chapter);
        signalSaveComplete();
        clearRecoverableFailure?.("inpaint-mask-save");
        void refreshLibrary();
      }
    } catch (error) {
      console.error(error);
      if (!inpaintMaskSaveStateRef.current) {
        inpaintMaskSaveStateRef.current = pending;
      }
      const message = error instanceof Error ? error.message : "인페인트 마스크 저장에 실패했습니다.";
      pushStatus(message);
      reportRecoverableFailure?.({
        id: "inpaint-mask-save",
        title: "인페인트 마스크 저장 실패",
        message: "마스크 편집 내용은 현재 화면에 남아 있습니다. 다시 저장을 시도하세요."
      });
    } finally {
      inpaintMaskSavingRef.current = false;
      if (inpaintMaskSaveStateRef.current) {
        if (inpaintMaskSaveStateRef.current !== pending) {
          void flushInpaintMaskSave();
        }
      }
    }
  }, [
    clearRecoverableFailure,
    currentChapterRef,
    dirty,
    mergeLiveChapter,
    pushStatus,
    refreshLibrary,
    reportRecoverableFailure,
    saveNow,
    signalSaveComplete
  ]);

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
      const resultDataUrl = await window.mangaApi.resolveOptionalImageDataUrl(pending.dataUrl);
      const result = await window.mangaApi.saveInpaintResultLayer({
        chapterId: pending.chapterId,
        pageId: pending.pageId,
        resultDataUrl
      });

      if (!inpaintResultSaveStateRef.current) {
        mergeLiveChapter(result.chapter);
        signalSaveComplete();
        clearRecoverableFailure?.("inpaint-result-save");
        void refreshLibrary();
      }
    } catch (error) {
      console.error(error);
      if (!inpaintResultSaveStateRef.current) {
        inpaintResultSaveStateRef.current = pending;
      }
      const message = error instanceof Error ? error.message : "인페인트 결과 레이어 저장에 실패했습니다.";
      pushStatus(message);
      reportRecoverableFailure?.({
        id: "inpaint-result-save",
        title: "인페인트 결과 저장 실패",
        message: "결과 레이어는 현재 화면에 남아 있습니다. 다시 저장을 시도하세요."
      });
    } finally {
      inpaintResultSavingRef.current = false;
      if (inpaintResultSaveStateRef.current) {
        if (inpaintResultSaveStateRef.current !== pending) {
          void flushInpaintResultSave();
        }
      }
    }
  }, [
    clearRecoverableFailure,
    currentChapterRef,
    dirty,
    mergeLiveChapter,
    pushStatus,
    refreshLibrary,
    reportRecoverableFailure,
    saveNow,
    signalSaveComplete
  ]);

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

  return {
    clearPendingInpaintSaves,
    flushInpaintMaskSave,
    flushInpaintResultSave,
    scheduleInpaintMaskSave,
    scheduleInpaintResultSave
  };
}
