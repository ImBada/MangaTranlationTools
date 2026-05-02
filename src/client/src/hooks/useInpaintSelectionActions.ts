import React from "react";
import type { ChapterSnapshot, ImageRect } from "../../../shared/types";
import { clearImageDataUrlRect, fillImageDataUrlRect } from "../lib/inpaintMaskImages";
import type { ActiveLayer } from "../lib/layerState";

type UseInpaintSelectionActionsOptions = {
  activeLayer: ActiveLayer;
  currentChapterRef: React.RefObject<ChapterSnapshot | null>;
  inpaintBusy: boolean;
  inpaintResultBrushColor: string;
  inpaintSelectionRect: ImageRect | null;
  pushStatus: (line: string) => void;
  rangeToolActive: boolean;
  selectedPageEditLocked: boolean;
  selectedPageIdRef: React.RefObject<string | null>;
  updateSelectedPageInpaintMask: (dataUrl: string | undefined, options?: { persist?: boolean; recordUndo?: boolean }) => void;
  updateSelectedPageInpaintResult: (dataUrl: string | undefined, options?: { persist?: boolean; recordUndo?: boolean }) => void;
};

type UseInpaintSelectionActionsState = {
  clearSelectedInpaintSelection: () => Promise<boolean>;
  fillSelectedInpaintSelection: () => Promise<void>;
};

export function useInpaintSelectionActions({
  activeLayer,
  currentChapterRef,
  inpaintBusy,
  inpaintResultBrushColor,
  inpaintSelectionRect,
  pushStatus,
  rangeToolActive,
  selectedPageEditLocked,
  selectedPageIdRef,
  updateSelectedPageInpaintMask,
  updateSelectedPageInpaintResult
}: UseInpaintSelectionActionsOptions): UseInpaintSelectionActionsState {
  const clearSelectedInpaintSelection = React.useCallback(async () => {
    if (selectedPageEditLocked || inpaintBusy || !inpaintSelectionRect) {
      return false;
    }

    const pageId = selectedPageIdRef.current;
    const page = pageId ? currentChapterRef.current?.pages.find((candidate) => candidate.id === pageId) : null;
    if (!page) {
      return false;
    }

    if (activeLayer === "inpaintMask" && rangeToolActive) {
      const maskDataUrl = page.inpaintMaskDataUrl ?? page.inpaintLayerDataUrl;
      if (!maskDataUrl) {
        return false;
      }
      const nextDataUrl = await clearImageDataUrlRect(maskDataUrl, page.width, page.height, inpaintSelectionRect);
      updateSelectedPageInpaintMask(nextDataUrl);
      pushStatus("선택 범위의 인페인트 마스크를 지웠습니다.");
      return true;
    }

    if (activeLayer === "inpaintResult" && rangeToolActive) {
      if (!page.inpaintResultDataUrl) {
        return false;
      }
      const nextDataUrl = await clearImageDataUrlRect(page.inpaintResultDataUrl, page.width, page.height, inpaintSelectionRect);
      updateSelectedPageInpaintResult(nextDataUrl);
      pushStatus("선택 범위의 인페인트 결과를 지웠습니다.");
      return true;
    }

    return false;
  }, [
    activeLayer,
    currentChapterRef,
    inpaintBusy,
    inpaintSelectionRect,
    pushStatus,
    rangeToolActive,
    selectedPageEditLocked,
    selectedPageIdRef,
    updateSelectedPageInpaintMask,
    updateSelectedPageInpaintResult
  ]);

  const fillSelectedInpaintSelection = React.useCallback(async () => {
    if (selectedPageEditLocked || inpaintBusy || !inpaintSelectionRect) {
      return;
    }

    const pageId = selectedPageIdRef.current;
    const page = pageId ? currentChapterRef.current?.pages.find((candidate) => candidate.id === pageId) : null;
    if (!page) {
      return;
    }

    if (activeLayer === "inpaintMask" && rangeToolActive) {
      const nextDataUrl = await fillImageDataUrlRect({
        dataUrl: page.inpaintMaskDataUrl ?? page.inpaintLayerDataUrl,
        width: page.width,
        height: page.height,
        rect: inpaintSelectionRect,
        fillStyle: "#ffffff"
      });
      updateSelectedPageInpaintMask(nextDataUrl);
      pushStatus("선택 범위를 인페인트 마스크로 채웠습니다.");
      return;
    }

    if (activeLayer === "inpaintResult" && rangeToolActive) {
      const nextDataUrl = await fillImageDataUrlRect({
        dataUrl: page.inpaintResultDataUrl,
        width: page.width,
        height: page.height,
        rect: inpaintSelectionRect,
        fillStyle: inpaintResultBrushColor
      });
      updateSelectedPageInpaintResult(nextDataUrl);
      pushStatus("선택 범위를 인페인트 결과 색상으로 채웠습니다.");
    }
  }, [
    activeLayer,
    currentChapterRef,
    inpaintBusy,
    inpaintResultBrushColor,
    inpaintSelectionRect,
    pushStatus,
    rangeToolActive,
    selectedPageEditLocked,
    selectedPageIdRef,
    updateSelectedPageInpaintMask,
    updateSelectedPageInpaintResult
  ]);

  return {
    clearSelectedInpaintSelection,
    fillSelectedInpaintSelection
  };
}
