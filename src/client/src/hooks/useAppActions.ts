import { useCallback } from "react";
import type { ChapterSnapshot } from "../../../shared/types";
import type { RecoverableFailureId } from "./useRecoverableFailures";

export function useAppActions({
  clearRecoverableFailure,
  currentChapter,
  currentChapterRef,
  flushInpaintMaskSave,
  flushInpaintResultSave,
  mergeLiveChapter,
  pushStatus,
  refreshLibrary,
  rerunInpaintWithCurrentMask,
  retryLastAnalysis,
  saveNow,
  updateCurrentChapter
}: {
  clearRecoverableFailure: (id: RecoverableFailureId) => void;
  currentChapter: ChapterSnapshot | null;
  currentChapterRef: React.MutableRefObject<ChapterSnapshot | null>;
  flushInpaintMaskSave: () => Promise<void>;
  flushInpaintResultSave: () => Promise<void>;
  mergeLiveChapter: (chapter: ChapterSnapshot) => void;
  pushStatus: (message: string) => void;
  refreshLibrary: () => Promise<void>;
  rerunInpaintWithCurrentMask: () => Promise<void>;
  retryLastAnalysis: () => Promise<void>;
  saveNow: () => Promise<void>;
  updateCurrentChapter: (pageId: string, updater: (current: ChapterSnapshot) => ChapterSnapshot) => void;
}) {
  const retryRecoverableFailure = useCallback(async (id: RecoverableFailureId) => {
    try {
      let shouldClearAfterRetry = true;
      if (id === "chapter-save") {
        await saveNow();
      } else if (id === "inpaint-mask-save") {
        await flushInpaintMaskSave();
        shouldClearAfterRetry = false;
      } else if (id === "inpaint-result-save") {
        await flushInpaintResultSave();
        shouldClearAfterRetry = false;
      } else if (id === "analysis-run") {
        await retryLastAnalysis();
        shouldClearAfterRetry = false;
      } else if (id === "analysis-sync") {
        const chapterId = currentChapterRef.current?.id;
        if (chapterId) {
          mergeLiveChapter(await window.mangaApi.openChapter(chapterId));
        }
        await refreshLibrary();
      } else if (id === "inpaint-run") {
        await rerunInpaintWithCurrentMask();
        shouldClearAfterRetry = false;
      }
      if (shouldClearAfterRetry) {
        clearRecoverableFailure(id);
      }
    } catch (error) {
      console.error(error);
      pushStatus(error instanceof Error ? error.message : "재시도에 실패했습니다.");
    }
  }, [
    clearRecoverableFailure,
    currentChapterRef,
    flushInpaintMaskSave,
    flushInpaintResultSave,
    mergeLiveChapter,
    pushStatus,
    refreshLibrary,
    rerunInpaintWithCurrentMask,
    retryLastAnalysis,
    saveNow
  ]);

  const togglePageProgress = useCallback(
    (pageId: string, options?: { announce?: boolean }) => {
      if (!currentChapter) {
        return;
      }
      const targetPage = currentChapter.pages.find((page) => page.id === pageId);
      if (!targetPage) {
        return;
      }
      updateCurrentChapter(pageId, (current) => ({
        ...current,
        pages: current.pages.map((page) =>
          page.id !== pageId
            ? page
            : {
                ...page,
                updatedAt: new Date().toISOString(),
                progressCompleted: !page.progressCompleted
              }
        )
      }));
      if (options?.announce) {
        pushStatus(`${targetPage.name} 작업 완료 상태 변경`);
      }
    },
    [currentChapter, pushStatus, updateCurrentChapter]
  );

  return {
    retryRecoverableFailure,
    togglePageProgress
  };
}
