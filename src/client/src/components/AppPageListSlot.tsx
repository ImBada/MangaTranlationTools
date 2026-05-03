import React from "react";
import type { MangaPage } from "../../../shared/types";
import { PageList } from "./PageList";

type AppPageListSlotProps = {
  jobActive: boolean;
  pages: MangaPage[];
  selectedPageId: string | null;
  onRemovePage: (pageId: string) => void | Promise<void>;
  onRetranslatePage: (pageId: string) => void | Promise<void>;
  onReorderPages: (sourcePageId: string, targetPageId: string) => void | Promise<void>;
  onSelectPage: (pageId: string) => void;
  onToggleProgress: (pageId: string) => void;
};

export function AppPageListSlot({
  jobActive,
  onRemovePage,
  onRetranslatePage,
  onReorderPages,
  onSelectPage,
  onToggleProgress,
  pages,
  selectedPageId
}: AppPageListSlotProps): React.JSX.Element {
  return (
    <PageList
      pages={pages}
      selectedPageId={selectedPageId}
      jobActive={jobActive}
      onSelect={onSelectPage}
      onRetranslate={(pageId) => void onRetranslatePage(pageId)}
      onRemove={(pageId) => void onRemovePage(pageId)}
      onToggleProgress={onToggleProgress}
      onReorder={(sourcePageId, targetPageId) => {
        void onReorderPages(sourcePageId, targetPageId);
      }}
    />
  );
}
