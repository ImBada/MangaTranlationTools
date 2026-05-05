import type { ChapterSnapshot, RunMode } from "../../../shared/types";
import { filterAnalysisTargetsForRun } from "../../../shared/analysisTargets";

type ChapterSelection = {
  selectedPageId: string | null;
  selectedBlockId: string | null;
};

export type LiveChapterMergeResult = {
  chapter: ChapterSnapshot;
  preservedDirtyPageIds: string[];
};

type LiveChapterMergeOptions = {
  preserveLocalChapterPresets?: boolean;
};

export function resolveSelectionAfterChapterSync(
  chapter: ChapterSnapshot,
  selectedPageId: string | null,
  selectedBlockId: string | null
): ChapterSelection {
  const nextSelectedPageId = chapter.pages.some((page) => page.id === selectedPageId) ? selectedPageId : chapter.pages[0]?.id ?? null;
  const nextSelectedPage = chapter.pages.find((page) => page.id === nextSelectedPageId) ?? null;
  const nextSelectedBlockId =
    nextSelectedPage && nextSelectedPage.blocks.some((block) => block.id === selectedBlockId) ? selectedBlockId : null;

  return {
    selectedPageId: nextSelectedPageId,
    selectedBlockId: nextSelectedBlockId
  };
}

export function mergeLiveChapterPreservingDirtyCompletedPages(
  liveChapter: ChapterSnapshot,
  localChapter: ChapterSnapshot | null,
  dirtyPageIds: Iterable<string>,
  options: LiveChapterMergeOptions = {}
): LiveChapterMergeResult {
  if (!localChapter || localChapter.id !== liveChapter.id) {
    return {
      chapter: liveChapter,
      preservedDirtyPageIds: []
    };
  }

  const dirtyPageIdSet = new Set(dirtyPageIds);
  const preserveLocalChapterPresets = options.preserveLocalChapterPresets === true;
  if (dirtyPageIdSet.size === 0 && !preserveLocalChapterPresets) {
    return {
      chapter: liveChapter,
      preservedDirtyPageIds: []
    };
  }

  const localPages = new Map(localChapter.pages.map((page) => [page.id, page]));
  const preservedDirtyPageIds: string[] = [];

  return {
    chapter: {
      ...liveChapter,
      favoriteFontPresetIds: preserveLocalChapterPresets ? localChapter.favoriteFontPresetIds : liveChapter.favoriteFontPresetIds,
      fontPresets: preserveLocalChapterPresets ? localChapter.fontPresets : liveChapter.fontPresets,
      fontSizePresets: preserveLocalChapterPresets ? localChapter.fontSizePresets : liveChapter.fontSizePresets,
      pages: liveChapter.pages.map((page) => {
        const localPage = localPages.get(page.id);
        if (!localPage || !dirtyPageIdSet.has(page.id)) {
          return page;
        }
        if (page.analysisStatus !== "completed" || localPage.analysisStatus !== "completed") {
          return page;
        }

        preservedDirtyPageIds.push(page.id);
        return {
          ...localPage,
          lastError: page.lastError
        };
      })
    },
    preservedDirtyPageIds
  };
}

export function markChapterPagesRunning(chapter: ChapterSnapshot, runMode: RunMode, pageId?: string): ChapterSnapshot {
  const targetPageIds = new Set(filterAnalysisTargetsForRun(chapter.pages, runMode, pageId).map((page) => page.id));

  if (targetPageIds.size === 0) {
    return chapter;
  }

  const now = new Date().toISOString();
  return {
    ...chapter,
    status: "running",
    updatedAt: now,
    pages: chapter.pages.map((page) =>
      targetPageIds.has(page.id)
        ? {
            ...page,
            analysisStatus: "running",
            lastError: undefined,
            updatedAt: now
          }
        : page
    )
  };
}
