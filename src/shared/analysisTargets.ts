import type { MangaPage, RunMode } from "./types";

type AnalysisTargetPage = Pick<MangaPage, "analysisStatus" | "id" | "progressCompleted">;

export function isAnalysisTargetForRun(page: AnalysisTargetPage, runMode: RunMode, pageId?: string): boolean {
  if (runMode === "single-page") {
    return page.id === pageId;
  }
  if (page.progressCompleted) {
    return false;
  }
  return runMode === "all" || page.analysisStatus !== "completed";
}

export function filterAnalysisTargetsForRun<Page extends AnalysisTargetPage>(pages: Page[], runMode: RunMode, pageId?: string): Page[] {
  return pages.filter((page) => isAnalysisTargetForRun(page, runMode, pageId));
}
