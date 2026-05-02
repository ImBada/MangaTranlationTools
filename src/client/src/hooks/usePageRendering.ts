import React from "react";
import type { ChapterSnapshot, MangaPage } from "../../../shared/types";
import { renderPageToPngDataUrl, type RenderPageOptions } from "../lib/pageRender";

type RenderProgress = {
  mode: "page" | "all";
  current: number;
  total: number;
};

type UsePageRenderingOptions = {
  currentChapterRef: React.RefObject<ChapterSnapshot | null>;
  dirty: boolean;
  pushStatus: (line: string) => void;
  saveNow: () => Promise<void>;
  selectedPageIdRef: React.RefObject<string | null>;
  signalSaveComplete: () => void;
};

type UsePageRenderingState = {
  renderAllPages: () => Promise<void>;
  renderBusy: boolean;
  renderProgress: RenderProgress | null;
  renderSelectedPage: () => Promise<void>;
};

const OUTPUT_RENDER_OPTIONS: RenderPageOptions = {
  layerVisibility: {
    image: true,
    inpaint: true,
    inpaintResult: true,
    inpaintMask: false,
    overlay: true
  },
  layerOpacity: {
    image: 1,
    inpaint: 1,
    inpaintResult: 1,
    inpaintMask: 1,
    overlay: 1
  },
  activeLayer: "output"
};

async function renderPageOutput(chapterId: string, page: MangaPage): Promise<string> {
  const dataUrl = await renderPageToPngDataUrl(page, OUTPUT_RENDER_OPTIONS);
  const result = await window.mangaApi.renderPage({
    chapterId,
    pageId: page.id,
    dataUrl
  });
  return result.outputPath;
}

export function usePageRendering({
  currentChapterRef,
  dirty,
  pushStatus,
  saveNow,
  selectedPageIdRef,
  signalSaveComplete
}: UsePageRenderingOptions): UsePageRenderingState {
  const [renderBusy, setRenderBusy] = React.useState(false);
  const [renderProgress, setRenderProgress] = React.useState<RenderProgress | null>(null);

  const renderSelectedPage = React.useCallback(async () => {
    const chapter = currentChapterRef.current;
    const pageId = selectedPageIdRef.current;
    if (!chapter || !pageId || renderBusy) {
      return;
    }

    setRenderBusy(true);
    setRenderProgress({ mode: "page", current: 1, total: 1 });
    try {
      if (dirty) {
        await saveNow();
      }
      const page = currentChapterRef.current?.pages.find((candidate) => candidate.id === pageId) ?? null;
      if (!page) {
        return;
      }
      const outputPath = await renderPageOutput(currentChapterRef.current!.id, page);
      signalSaveComplete();
      pushStatus(`페이지 렌더 저장: ${outputPath}`);
    } catch (error) {
      console.error(error);
      pushStatus(error instanceof Error ? error.message : "페이지 렌더에 실패했습니다.");
    } finally {
      setRenderProgress(null);
      setRenderBusy(false);
    }
  }, [currentChapterRef, dirty, pushStatus, renderBusy, saveNow, selectedPageIdRef, signalSaveComplete]);

  const renderAllPages = React.useCallback(async () => {
    const initialChapter = currentChapterRef.current;
    if (!initialChapter || initialChapter.pages.length === 0 || renderBusy) {
      return;
    }

    setRenderBusy(true);
    setRenderProgress({ mode: "all", current: 0, total: initialChapter.pages.length });
    try {
      if (dirty) {
        await saveNow();
      }
      const chapter = currentChapterRef.current;
      if (!chapter || chapter.pages.length === 0) {
        return;
      }

      const pages = [...chapter.pages];
      pushStatus(`전체 페이지 출력 시작: ${pages.length}p`);
      for (const [index, page] of pages.entries()) {
        setRenderProgress({ mode: "all", current: index + 1, total: pages.length });
        try {
          await renderPageOutput(chapter.id, page);
        } catch (error) {
          const message = error instanceof Error ? error.message : "페이지 렌더에 실패했습니다.";
          throw new Error(`${page.name} 출력 실패: ${message}`);
        }
      }

      signalSaveComplete();
      pushStatus(`전체 페이지 렌더 저장 완료: ${pages.length}p`);
    } catch (error) {
      console.error(error);
      pushStatus(error instanceof Error ? error.message : "전체 페이지 렌더에 실패했습니다.");
    } finally {
      setRenderProgress(null);
      setRenderBusy(false);
    }
  }, [currentChapterRef, dirty, pushStatus, renderBusy, saveNow, signalSaveComplete]);

  return {
    renderAllPages,
    renderBusy,
    renderProgress,
    renderSelectedPage
  };
}
