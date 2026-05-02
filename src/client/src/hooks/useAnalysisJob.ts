import React from "react";
import type { ChapterSnapshot, JobState } from "../../../shared/types";
import { markChapterPagesRunning } from "../lib/chapterSync";
import { formatJobEventLine, formatJobLabel, resolveProgressSnapshot, summarizeWarnings } from "../lib/jobProgress";

type AnalysisRunMode = "pending" | "all" | "single-page";

type UseAnalysisJobOptions = {
  appendStatusLine: (line: string) => void;
  applyChapter: (chapter: ChapterSnapshot | undefined, fallbackStatus?: string) => void;
  currentChapter: ChapterSnapshot | null;
  currentChapterRef: React.RefObject<ChapterSnapshot | null>;
  mergeLiveChapter: (chapter: ChapterSnapshot) => void;
  pushStatus: (line: string) => void;
  refreshLibrary: () => Promise<void>;
  resetStatusLog: () => void;
  saveNow: () => Promise<void>;
  setCurrentChapter: React.Dispatch<React.SetStateAction<ChapterSnapshot | null>>;
};

type UseAnalysisJobState = {
  jobActive: boolean;
  jobState: JobState;
  progressSnapshot: ReturnType<typeof resolveProgressSnapshot>;
  retranslatePage: (pageId: string) => Promise<void>;
  runAnalysis: (runMode: AnalysisRunMode, pageId?: string) => Promise<void>;
  showProgressBar: boolean;
};

const EMPTY_JOB: JobState = {
  id: "idle",
  kind: "gemma-analysis",
  status: "idle",
  progressText: "대기 중"
};

export function useAnalysisJob({
  appendStatusLine,
  applyChapter,
  currentChapter,
  currentChapterRef,
  mergeLiveChapter,
  pushStatus,
  refreshLibrary,
  resetStatusLog,
  saveNow,
  setCurrentChapter
}: UseAnalysisJobOptions): UseAnalysisJobState {
  const [jobState, setJobState] = React.useState<JobState>(EMPTY_JOB);
  const jobActive = ["starting", "running", "cancelling"].includes(jobState.status);
  const progressSnapshot = React.useMemo(() => resolveProgressSnapshot(jobState), [jobState]);
  const showProgressBar = jobState.status !== "idle" && !!progressSnapshot;

  React.useEffect(() => {
    const unsubscribe = window.mangaApi.onJobEvent((event) => {
      const friendlyText = formatJobLabel(event);
      setJobState((current) => ({
        id: event.id,
        kind: event.kind,
        status: event.status,
        progressText: friendlyText,
        detail: event.detail ?? current.detail,
        phase: event.phase ?? current.phase,
        progressCurrent: event.progressCurrent ?? current.progressCurrent,
        progressTotal: event.progressTotal ?? current.progressTotal,
        pageIndex: event.pageIndex ?? current.pageIndex,
        pageTotal: event.pageTotal ?? current.pageTotal,
        attempt: event.attempt ?? current.attempt,
        attemptTotal: event.attemptTotal ?? current.attemptTotal
      }));
      appendStatusLine(formatJobEventLine(event));

      if (event.phase === "page_done" || event.phase === "page_skipped") {
        const chapterId = currentChapterRef.current?.id;
        if (!chapterId) {
          return;
        }

        void window.mangaApi
          .openChapter(chapterId)
          .then((chapter) => {
            if (currentChapterRef.current?.id === chapter.id) {
              mergeLiveChapter(chapter);
            }
          })
          .then(() => refreshLibrary())
          .catch((error) => {
            console.error(error);
          });
      }
    });
    return unsubscribe;
  }, [appendStatusLine, currentChapterRef, mergeLiveChapter, refreshLibrary]);

  const runAnalysis = React.useCallback(
    async (runMode: AnalysisRunMode, pageId?: string) => {
      if (!currentChapter || jobActive) {
        return;
      }

      await saveNow();
      resetStatusLog();
      setJobState({
        id: "pending",
        kind: "gemma-analysis",
        status: "starting",
        progressText: "모델 준비 중",
        phase: "booting"
      });
      setCurrentChapter((chapter) => (chapter ? markChapterPagesRunning(chapter, runMode, pageId) : chapter));

      const result = await window.mangaApi.startAnalysis({ chapterId: currentChapter.id, runMode, pageId });
      if (result.chapter) {
        applyChapter(result.chapter);
      }
      await refreshLibrary();

      if (result.status === "completed") {
        const warningSummary = summarizeWarnings(result.warnings ?? []);
        if (warningSummary) {
          pushStatus(warningSummary);
        }
        return;
      }

      if (result.status === "failed" && result.error) {
        pushStatus(result.error);
      }
    },
    [applyChapter, currentChapter, jobActive, pushStatus, refreshLibrary, resetStatusLog, saveNow, setCurrentChapter]
  );

  const retranslatePage = React.useCallback(
    async (pageId: string) => {
      const page = currentChapter?.pages.find((candidate) => candidate.id === pageId);
      if (!page || !currentChapter) {
        return;
      }
      const confirmed = await window.mangaApi.confirm(
        "페이지 재번역",
        "정말 재번역 하시겠습니까?",
        "기존 번역 결과와 수정 내용이 이 페이지에서 덮어써집니다."
      );
      if (!confirmed) {
        return;
      }
      await runAnalysis("single-page", pageId);
    },
    [currentChapter, runAnalysis]
  );

  return {
    jobActive,
    jobState,
    progressSnapshot,
    retranslatePage,
    runAnalysis,
    showProgressBar
  };
}
