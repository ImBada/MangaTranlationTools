import { useMemo } from "react";
import type { ChapterSnapshot } from "../../../shared/types";

export function useAppWorkspaceState({
  currentChapter,
  jobActive,
  selectedBlockId,
  selectedPageId
}: {
  currentChapter: ChapterSnapshot | null;
  jobActive: boolean;
  selectedBlockId: string | null;
  selectedPageId: string | null;
}) {
  const selectedPage = useMemo(
    () => currentChapter?.pages.find((page) => page.id === selectedPageId) ?? currentChapter?.pages[0] ?? null,
    [currentChapter?.pages, selectedPageId]
  );
  const currentChapterId = currentChapter?.id ?? null;
  const selectedPageCurrentId = selectedPage?.id ?? null;
  const selectedBlock = selectedPage?.blocks.find((block) => block.id === selectedBlockId) ?? null;
  const selectedPageEditLocked = Boolean(jobActive && selectedPage && selectedPage.analysisStatus !== "completed");
  const overlayBackgroundOpacity = selectedPage?.blocks[0]?.opacity ?? 1;

  return {
    currentChapterId,
    overlayBackgroundOpacity,
    selectedBlock,
    selectedPage,
    selectedPageCurrentId,
    selectedPageEditLocked
  };
}
