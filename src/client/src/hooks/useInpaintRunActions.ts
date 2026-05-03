import React from "react";
import type { ChapterSnapshot, ImageRect, MangaPage, TranslationBlock } from "../../../shared/types";
import { drawBlocksOnInpaintMask, maskDataUrlForSelection, mergePartialInpaintResult } from "../lib/inpaintMaskImages";
import { DEFAULT_INPAINT_SETTINGS } from "../lib/inpaintToolSettings";
import type { RecoverableFailureId } from "./useRecoverableFailures";

type UseInpaintRunActionsOptions = {
  applyChapter: (chapter: ChapterSnapshot | undefined, fallbackStatus?: string) => void;
  clearRecoverableFailure?: (id: RecoverableFailureId) => void;
  currentChapter: ChapterSnapshot | null;
  currentChapterRef: React.RefObject<ChapterSnapshot | null>;
  dirty: boolean;
  inpaintSelectionRect: ImageRect | null;
  pushStatus: (line: string) => void;
  refreshLibrary: () => Promise<void>;
  reportRecoverableFailure?: (failure: { id: RecoverableFailureId; message: string; title: string }) => void;
  saveNow: () => Promise<void>;
  selectedBlock: TranslationBlock | null;
  selectedPage: MangaPage | null;
  selectedPageEditLocked: boolean;
  selectedPageIdRef: React.RefObject<string | null>;
  signalSaveComplete: () => void;
  updatePageInpaintStatus: (pageId: string, status: MangaPage["inpaintStatus"]) => void;
  updateSelectedPageInpaintMask: (dataUrl: string | undefined, options?: { persist?: boolean; recordUndo?: boolean }) => void;
  updateSelectedPageInpaintResult: (dataUrl: string | undefined, options?: { persist?: boolean; recordUndo?: boolean }) => void;
};

type UseInpaintRunActionsState = {
  applyInpaintAllBlocks: () => Promise<void>;
  applyInpaintAllPages: () => Promise<void>;
  applyInpaintSelectedBlock: () => Promise<void>;
  inpaintBusy: boolean;
  rerunInpaintForSelection: () => Promise<void>;
  rerunInpaintWithCurrentMask: () => Promise<void>;
};

export function useInpaintRunActions({
  applyChapter,
  clearRecoverableFailure,
  currentChapter,
  currentChapterRef,
  dirty,
  inpaintSelectionRect,
  pushStatus,
  refreshLibrary,
  reportRecoverableFailure,
  saveNow,
  selectedBlock,
  selectedPage,
  selectedPageEditLocked,
  selectedPageIdRef,
  signalSaveComplete,
  updatePageInpaintStatus,
  updateSelectedPageInpaintMask,
  updateSelectedPageInpaintResult
}: UseInpaintRunActionsOptions): UseInpaintRunActionsState {
  const [inpaintBusy, setInpaintBusy] = React.useState(false);

  const runInpaintForPage = React.useCallback(async (page: MangaPage, maskDataUrl: string, statusMessage = "인페인트 결과를 저장했습니다.") => {
    if (!currentChapter || inpaintBusy) {
      return;
    }

    setInpaintBusy(true);
    try {
      if (dirty) {
        await saveNow();
      }
      updatePageInpaintStatus(page.id, "running");
      const sourceDataUrl = await window.mangaApi.resolveImageDataUrl(page.dataUrl);
      const resolvedMaskDataUrl = await window.mangaApi.resolveImageDataUrl(maskDataUrl);
      const result = await window.mangaApi.inpaintPage({
        chapterId: currentChapter.id,
        pageId: page.id,
        sourceDataUrl,
        maskDataUrl: resolvedMaskDataUrl,
        settings: DEFAULT_INPAINT_SETTINGS
      });
      applyChapter(result.chapter);
      signalSaveComplete();
      clearRecoverableFailure?.("inpaint-run");
      void refreshLibrary();
      pushStatus(result.engine === "local-fill-fallback" ? "로컬 인페인트 결과를 저장했습니다." : statusMessage);
    } catch (error) {
      console.error(error);
      updatePageInpaintStatus(page.id, "failed");
      const message = error instanceof Error ? error.message : "인페인트 실행에 실패했습니다.";
      pushStatus(message);
      reportRecoverableFailure?.({
        id: "inpaint-run",
        title: "인페인트 실패",
        message: "마스크와 편집 상태는 유지됩니다. 설정을 확인한 뒤 다시 실행하세요."
      });
    } finally {
      setInpaintBusy(false);
    }
  }, [
    applyChapter,
    clearRecoverableFailure,
    currentChapter,
    dirty,
    inpaintBusy,
    pushStatus,
    refreshLibrary,
    reportRecoverableFailure,
    saveNow,
    signalSaveComplete,
    updatePageInpaintStatus
  ]);

  const applyInpaintSelectedBlock = React.useCallback(async () => {
    if (!selectedPage || !selectedBlock || selectedPageEditLocked || inpaintBusy) {
      return;
    }

    const maskDataUrl = await drawBlocksOnInpaintMask(selectedPage, [selectedBlock]);
    updateSelectedPageInpaintMask(maskDataUrl, { persist: false });
    await runInpaintForPage({ ...selectedPage, inpaintMaskDataUrl: maskDataUrl }, maskDataUrl, "선택 블록 인페인트 결과를 저장했습니다.");
  }, [inpaintBusy, runInpaintForPage, selectedBlock, selectedPage, selectedPageEditLocked, updateSelectedPageInpaintMask]);

  const applyInpaintAllBlocks = React.useCallback(async () => {
    if (!selectedPage || selectedPage.blocks.length === 0 || selectedPageEditLocked || inpaintBusy) {
      return;
    }

    const maskDataUrl = await drawBlocksOnInpaintMask(selectedPage, selectedPage.blocks);
    updateSelectedPageInpaintMask(maskDataUrl, { persist: false });
    await runInpaintForPage({ ...selectedPage, inpaintMaskDataUrl: maskDataUrl }, maskDataUrl, "전체 블록 인페인트 결과를 저장했습니다.");
  }, [inpaintBusy, runInpaintForPage, selectedPage, selectedPageEditLocked, updateSelectedPageInpaintMask]);

  const applyInpaintAllPages = React.useCallback(async () => {
    const initialChapter = currentChapterRef.current;
    if (!initialChapter || initialChapter.pages.length === 0 || selectedPageEditLocked || inpaintBusy) {
      return;
    }

    const pagesWithBlocks = initialChapter.pages.filter((page) => page.blocks.length > 0 && !page.progressCompleted);
    if (pagesWithBlocks.length === 0) {
      pushStatus("인페인트할 미완료 페이지가 없습니다.");
      return;
    }

    setInpaintBusy(true);
    try {
      if (dirty) {
        await saveNow();
      }

      const chapterId = currentChapterRef.current?.id;
      if (!chapterId) {
        return;
      }

      pushStatus(`전체 인페인트 시작: ${pagesWithBlocks.length}p`);
      for (const [index, queuedPage] of pagesWithBlocks.entries()) {
        const page = currentChapterRef.current?.pages.find((candidate) => candidate.id === queuedPage.id) ?? queuedPage;
        const maskDataUrl = await drawBlocksOnInpaintMask(page, page.blocks);

        updatePageInpaintStatus(page.id, "running");
        try {
          const sourceDataUrl = await window.mangaApi.resolveImageDataUrl(page.dataUrl);
          const result = await window.mangaApi.inpaintPage({
            chapterId,
            pageId: page.id,
            sourceDataUrl,
            maskDataUrl,
            settings: DEFAULT_INPAINT_SETTINGS
          });
          applyChapter(result.chapter);
          pushStatus(`${index + 1}/${pagesWithBlocks.length} ${page.name} 인페인트 완료`);
        } catch (error) {
          updatePageInpaintStatus(page.id, "failed");
          const message = error instanceof Error ? error.message : "인페인트 실행에 실패했습니다.";
          throw new Error(`${page.name} 인페인트 실패: ${message}`, { cause: error });
        }
      }

      signalSaveComplete();
      clearRecoverableFailure?.("inpaint-run");
      void refreshLibrary();
      pushStatus(`전체 인페인트 완료: ${pagesWithBlocks.length}p`);
    } catch (error) {
      console.error(error);
      const message = error instanceof Error ? error.message : "전체 인페인트에 실패했습니다.";
      pushStatus(message);
      reportRecoverableFailure?.({
        id: "inpaint-run",
        title: "전체 인페인트 실패",
        message: "완료된 페이지는 유지됩니다. 실패한 페이지부터 다시 실행하세요."
      });
    } finally {
      setInpaintBusy(false);
    }
  }, [
    applyChapter,
    clearRecoverableFailure,
    currentChapterRef,
    dirty,
    inpaintBusy,
    pushStatus,
    refreshLibrary,
    reportRecoverableFailure,
    saveNow,
    selectedPageEditLocked,
    signalSaveComplete,
    updatePageInpaintStatus
  ]);

  const rerunInpaintWithCurrentMask = React.useCallback(async () => {
    if (selectedPageEditLocked || inpaintBusy) {
      return;
    }

    const pageId = selectedPageIdRef.current;
    const page = pageId ? currentChapterRef.current?.pages.find((candidate) => candidate.id === pageId) : null;
    if (!page) {
      return;
    }

    const maskDataUrl = page.inpaintMaskDataUrl ?? page.inpaintLayerDataUrl;
    if (!maskDataUrl) {
      pushStatus("다시 인페인트할 마스크가 없습니다.");
      return;
    }

    await runInpaintForPage(page, maskDataUrl, "현재 마스크 기준으로 인페인트 결과를 다시 저장했습니다.");
  }, [currentChapterRef, inpaintBusy, pushStatus, runInpaintForPage, selectedPageEditLocked, selectedPageIdRef]);

  const rerunInpaintForSelection = React.useCallback(async () => {
    if (!currentChapter || selectedPageEditLocked || inpaintBusy) {
      return;
    }

    const pageId = selectedPageIdRef.current;
    const page = pageId ? currentChapterRef.current?.pages.find((candidate) => candidate.id === pageId) : null;
    if (!page) {
      return;
    }

    const maskDataUrl = page.inpaintMaskDataUrl ?? page.inpaintLayerDataUrl;
    if (!maskDataUrl) {
      pushStatus("다시 인페인트할 마스크가 없습니다.");
      return;
    }
    if (!inpaintSelectionRect) {
      pushStatus("부분 인페인트할 범위를 먼저 선택하세요.");
      return;
    }

    setInpaintBusy(true);
    try {
      if (dirty) {
        await saveNow();
      }
      const selectionMaskDataUrl = await maskDataUrlForSelection(maskDataUrl, page.width, page.height, inpaintSelectionRect);
      if (!selectionMaskDataUrl) {
        pushStatus("선택 범위 안에 마스크 픽셀이 없습니다.");
        return;
      }

      updatePageInpaintStatus(page.id, "running");
      const sourceDataUrl = await window.mangaApi.resolveImageDataUrl(page.dataUrl);
      const result = await window.mangaApi.inpaintPage({
        chapterId: currentChapter.id,
        pageId: page.id,
        sourceDataUrl,
        maskDataUrl: selectionMaskDataUrl,
        settings: DEFAULT_INPAINT_SETTINGS,
        persistResult: false
      });
      const mergedResultDataUrl = await mergePartialInpaintResult(
        page.inpaintResultDataUrl,
        result.resultDataUrl,
        selectionMaskDataUrl,
        page.width,
        page.height
      );
      updateSelectedPageInpaintResult(mergedResultDataUrl, { persist: false });
      const saved = await window.mangaApi.saveInpaintResultLayer({
        chapterId: currentChapter.id,
        pageId: page.id,
        resultDataUrl: mergedResultDataUrl
      });
      applyChapter(saved.chapter);
      signalSaveComplete();
      clearRecoverableFailure?.("inpaint-run");
      void refreshLibrary();
      pushStatus(result.engine === "local-fill-fallback" ? "선택 범위 로컬 인페인트 결과를 저장했습니다." : "선택 범위만 다시 인페인트했습니다.");
    } catch (error) {
      console.error(error);
      updatePageInpaintStatus(page.id, "failed");
      const message = error instanceof Error ? error.message : "부분 인페인트 실행에 실패했습니다.";
      pushStatus(message);
      reportRecoverableFailure?.({
        id: "inpaint-run",
        title: "부분 인페인트 실패",
        message: "기존 결과 레이어와 선택 범위는 유지됩니다. 다시 실행하세요."
      });
    } finally {
      setInpaintBusy(false);
    }
  }, [
    applyChapter,
    clearRecoverableFailure,
    currentChapter,
    currentChapterRef,
    dirty,
    inpaintBusy,
    inpaintSelectionRect,
    pushStatus,
    refreshLibrary,
    reportRecoverableFailure,
    saveNow,
    selectedPageEditLocked,
    selectedPageIdRef,
    signalSaveComplete,
    updatePageInpaintStatus,
    updateSelectedPageInpaintResult
  ]);

  return {
    applyInpaintAllBlocks,
    applyInpaintAllPages,
    applyInpaintSelectedBlock,
    inpaintBusy,
    rerunInpaintForSelection,
    rerunInpaintWithCurrentMask
  };
}
