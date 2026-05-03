import React from "react";
import type { ChapterSnapshot, JobState } from "../../../shared/types";
import { markChapterPagesRunning } from "../lib/chapterSync";
import { formatJobEventLine, formatJobLabel, resolveProgressSnapshot, summarizeWarnings } from "../lib/jobProgress";
import type { RecoverableFailureId } from "./useRecoverableFailures";

type AnalysisRunMode = "pending" | "all" | "single-page";

type UseAnalysisJobOptions = {
  appendStatusLine: (line: string) => void;
  applyChapter: (chapter: ChapterSnapshot | undefined, fallbackStatus?: string) => void;
  clearRecoverableFailure?: (id: RecoverableFailureId) => void;
  currentChapter: ChapterSnapshot | null;
  currentChapterRef: React.RefObject<ChapterSnapshot | null>;
  mergeLiveChapter: (chapter: ChapterSnapshot) => void;
  pushStatus: (line: string) => void;
  refreshLibrary: () => Promise<void>;
  reportRecoverableFailure?: (failure: { id: RecoverableFailureId; message: string; title: string }) => void;
  resetStatusLog: () => void;
  saveNow: () => Promise<void>;
  setCurrentChapter: React.Dispatch<React.SetStateAction<ChapterSnapshot | null>>;
};

type UseAnalysisJobState = {
  jobActive: boolean;
  jobState: JobState;
  progressSnapshot: ReturnType<typeof resolveProgressSnapshot>;
  retranslatePage: (pageId: string) => Promise<void>;
  retryLastAnalysis: () => Promise<void>;
  runAnalysis: (runMode: AnalysisRunMode, pageId?: string) => Promise<void>;
  showProgressBar: boolean;
};

const EMPTY_JOB: JobState = {
  id: "idle",
  kind: "model-analysis",
  status: "idle",
  progressText: "대기 중"
};

export function useAnalysisJob({
  appendStatusLine,
  applyChapter,
  clearRecoverableFailure,
  currentChapter,
  currentChapterRef,
  mergeLiveChapter,
  pushStatus,
  refreshLibrary,
  reportRecoverableFailure,
  resetStatusLog,
  saveNow,
  setCurrentChapter
}: UseAnalysisJobOptions): UseAnalysisJobState {
  const [jobState, setJobState] = React.useState<JobState>(EMPTY_JOB);
  const lastAnalysisRequestRef = React.useRef<{ pageId?: string; runMode: AnalysisRunMode } | null>(null);
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
            const message = error instanceof Error ? error.message : "분석 결과를 불러오지 못했습니다.";
            pushStatus(message);
            reportRecoverableFailure?.({
              id: "analysis-sync",
              title: "분석 결과 새로고침 실패",
              message: "서버 저장 결과를 화면에 반영하지 못했습니다. 다시 불러오기를 시도하세요."
            });
          });
      }
    });
    return unsubscribe;
  }, [appendStatusLine, currentChapterRef, mergeLiveChapter, pushStatus, refreshLibrary, reportRecoverableFailure]);

  const runAnalysis = React.useCallback(
    async (runMode: AnalysisRunMode, pageId?: string) => {
      if (!currentChapter || jobActive) {
        return;
      }
      lastAnalysisRequestRef.current = { runMode, pageId };

      try {
        await saveNow();
        resetStatusLog();
        setJobState({
          id: "pending",
          kind: "model-analysis",
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
          clearRecoverableFailure?.("analysis-run");
          const warningSummary = summarizeWarnings(result.warnings ?? []);
          if (warningSummary) {
            pushStatus(warningSummary);
          }
          return;
        }

        if (result.status === "failed" && result.error) {
          pushStatus(result.error);
          reportRecoverableFailure?.({
            id: "analysis-run",
            title: pageId ? "페이지 번역 실패" : "번역 실행 실패",
            message: "기존 편집 내용은 유지됩니다. 설정과 모델 상태를 확인한 뒤 다시 실행하세요."
          });
        }
      } catch (error) {
        console.error(error);
        const message = error instanceof Error ? error.message : "번역 실행에 실패했습니다.";
        pushStatus(message);
        setJobState((current) => ({
          ...current,
          status: "failed",
          progressText: "실패",
          detail: message
        }));
        reportRecoverableFailure?.({
          id: "analysis-run",
          title: pageId ? "페이지 번역 실패" : "번역 실행 실패",
          message: "기존 편집 내용은 유지됩니다. 설정과 모델 상태를 확인한 뒤 다시 실행하세요."
        });
      }
    },
    [
      applyChapter,
      clearRecoverableFailure,
      currentChapter,
      jobActive,
      pushStatus,
      refreshLibrary,
      reportRecoverableFailure,
      resetStatusLog,
      saveNow,
      setCurrentChapter
    ]
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

  const retryLastAnalysis = React.useCallback(async () => {
    const request = lastAnalysisRequestRef.current;
    if (!request) {
      await runAnalysis("pending");
      return;
    }
    await runAnalysis(request.runMode, request.pageId);
  }, [runAnalysis]);

  return {
    jobActive,
    jobState,
    progressSnapshot,
    retranslatePage,
    retryLastAnalysis,
    runAnalysis,
    showProgressBar
  };
}
