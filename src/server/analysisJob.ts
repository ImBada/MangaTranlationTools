import { randomUUID } from "node:crypto";
import {
  finalizeRunningPages,
  getRunPaths,
  markChapterPagesRunning,
  openChapter,
  resolvePagesForRun,
  updatePageAfterAnalysis
} from "./library";
import { emitJobEvent, getActiveJob, recordJobEvent, setActiveJob, updateActiveJob } from "./jobState";
import { logError } from "./logger";
import { isAbortError } from "./serverUtils";
import { runWholePagePipeline } from "./wholePagePipeline";
import type { JobEvent, StartAnalysisRequest, StartAnalysisResult } from "../shared/types";

export async function startAnalysis(request: StartAnalysisRequest): Promise<StartAnalysisResult> {
  if (getActiveJob()) {
    return { status: "failed", error: "이미 실행 중인 작업이 있습니다." };
  }

  const resolved = await resolvePagesForRun(request.chapterId, request.runMode, request.pageId);
  if (resolved.pages.length === 0) {
    return { status: "completed", chapter: resolved.chapter, warnings: [] };
  }

  const id = randomUUID();
  const abortController = new AbortController();
  const pageIds = resolved.pages.map((page) => page.id);
  let runPaths: Awaited<ReturnType<typeof getRunPaths>> | null = null;
  await markChapterPagesRunning(request.chapterId, pageIds);
  setActiveJob({ id, abortController });

  const emit = (event: JobEvent) => {
    recordJobEvent(id, event);
    emitJobEvent(event);
  };

  try {
    runPaths = await getRunPaths(request.chapterId, id);
    const result = await runWholePagePipeline({
      jobId: id,
      emit,
      onCleanupReady: (cleanup) => {
        updateActiveJob(id, { cleanup });
      },
      onPageComplete: async (page) => updatePageAfterAnalysis(request.chapterId, page, [], "completed"),
      onPageFailed: async (page, errorMessage) => updatePageAfterAnalysis(request.chapterId, page, [errorMessage], "failed"),
      pages: resolved.pages,
      runPaths,
      signal: abortController.signal
    });

    if (abortController.signal.aborted) {
      throw new DOMException("Aborted", "AbortError");
    }

    emit({
      id,
      kind: "gemma-analysis",
      status: "completed",
      progressText: "번역 작업 완료",
      phase: "done",
      progressCurrent: resolved.pages.length,
      progressTotal: resolved.pages.length,
      pageTotal: resolved.pages.length
    });

    return { status: "completed", chapter: await openChapter(request.chapterId), warnings: result.warnings };
  } catch (error) {
    const lastEvent = getActiveJob()?.id === id ? getActiveJob()?.lastEvent : undefined;
    if (isAbortError(error) || abortController.signal.aborted) {
      await finalizeRunningPages(request.chapterId, pageIds, "idle");
      emit({
        id,
        kind: "gemma-analysis",
        status: "cancelled",
        progressText: "작업이 취소되었습니다.",
        phase: "cancelled",
        progressCurrent: lastEvent?.progressCurrent,
        progressTotal: lastEvent?.progressTotal,
        pageIndex: lastEvent?.pageIndex,
        pageTotal: lastEvent?.pageTotal,
        attempt: lastEvent?.attempt,
        attemptTotal: lastEvent?.attemptTotal
      });
      return { status: "cancelled", chapter: await openChapter(request.chapterId) };
    }

    const message = error instanceof Error ? error.message : String(error);
    await finalizeRunningPages(request.chapterId, pageIds, "failed", message);
    logError("Analysis job failed", { jobId: id, request, runPaths, lastEvent, error });
    emit({
      id,
      kind: "gemma-analysis",
      status: "failed",
      progressText: "작업 실패",
      phase: "failed",
      detail: message
    });
    return { status: "failed", error: message, chapter: await openChapter(request.chapterId) };
  } finally {
    setActiveJob(null);
  }
}
