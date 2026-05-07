import React from "react";
import type { ChapterSnapshot, MangaPage } from "../../../shared/types";
import type { FontWeightAvailability } from "../lib/overlayLayout";
import { renderPageToPngDataUrl } from "../lib/pageRender";

type UseInpaintPsdActionsOptions = {
  clearPendingInpaintSaveTimers: () => void;
  currentChapter: ChapterSnapshot | null;
  currentChapterId: string | null;
  dirty: boolean;
  flushInpaintMaskSave: () => Promise<void>;
  flushInpaintResultSave: () => Promise<void>;
  fontWeightAvailability: readonly FontWeightAvailability[];
  mergeLiveChapter: (chapter: ChapterSnapshot) => void;
  pushStatus: (line: string) => void;
  recordInpaintMaskUndoSnapshot: (page: MangaPage) => void;
  refreshLibrary: () => Promise<void>;
  saveNow: () => Promise<void>;
  selectedPage: MangaPage | null;
  selectedPageCurrentId: string | null;
  selectedPageEditLocked: boolean;
  showInpaintLayers: () => void;
  signalSaveComplete: () => void;
};

type UseInpaintPsdActionsState = {
  downloadLastImportedInpaintPsd: () => Promise<void>;
  exportSelectedPageInpaintPsd: () => Promise<void>;
  handleInpaintPsdInputChange: (event: React.ChangeEvent<HTMLInputElement>) => Promise<void>;
  inpaintPsdBusy: boolean;
  inpaintPsdInputRef: React.RefObject<HTMLInputElement | null>;
  lastImportedInpaintPsdAt: string | null;
  lastImportedInpaintPsdLabel: string | null;
  selectInpaintPsdFile: () => void;
};

function sanitizeDownloadBasename(value: string, fallback: string): string {
  const base = value.replace(/\.[^.]+$/u, "").replace(/[<>:"/\\|?*\x00-\x1f]/g, "_").trim();
  return base || fallback;
}

export function useInpaintPsdActions({
  clearPendingInpaintSaveTimers,
  currentChapter,
  currentChapterId,
  dirty,
  flushInpaintMaskSave,
  flushInpaintResultSave,
  fontWeightAvailability,
  mergeLiveChapter,
  pushStatus,
  recordInpaintMaskUndoSnapshot,
  refreshLibrary,
  saveNow,
  selectedPage,
  selectedPageCurrentId,
  selectedPageEditLocked,
  showInpaintLayers,
  signalSaveComplete
}: UseInpaintPsdActionsOptions): UseInpaintPsdActionsState {
  const [inpaintPsdBusy, setInpaintPsdBusy] = React.useState(false);
  const [lastImportedInpaintPsdAt, setLastImportedInpaintPsdAt] = React.useState<string | null>(null);
  const inpaintPsdInputRef = React.useRef<HTMLInputElement | null>(null);

  const lastImportedInpaintPsdLabel = lastImportedInpaintPsdAt
    ? new Intl.DateTimeFormat(undefined, { dateStyle: "short", timeStyle: "short" }).format(new Date(lastImportedInpaintPsdAt))
    : null;

  const refreshLastImportedInpaintPsdMeta = React.useCallback(async () => {
    if (!currentChapterId || !selectedPageCurrentId) {
      setLastImportedInpaintPsdAt(null);
      return { exists: false };
    }
    const meta = await window.mangaApi.getLastImportedInpaintPsdMeta(currentChapterId, selectedPageCurrentId);
    setLastImportedInpaintPsdAt(meta.exists && meta.importedAt ? meta.importedAt : null);
    return meta;
  }, [currentChapterId, selectedPageCurrentId]);

  React.useEffect(() => {
    void refreshLastImportedInpaintPsdMeta().catch((error) => {
      console.error(error);
    });
  }, [refreshLastImportedInpaintPsdMeta]);

  const exportSelectedPageInpaintPsd = React.useCallback(async () => {
    if (!currentChapter || !selectedPage || selectedPageEditLocked || inpaintPsdBusy) {
      return;
    }

    setInpaintPsdBusy(true);
    try {
      const sourceDataUrl = await window.mangaApi.resolveImageDataUrl(selectedPage.dataUrl);
      const maskDataUrl = await window.mangaApi.resolveOptionalImageDataUrl(selectedPage.inpaintMaskDataUrl ?? selectedPage.inpaintLayerDataUrl);
      const resultDataUrl = await window.mangaApi.resolveOptionalImageDataUrl(selectedPage.inpaintResultDataUrl);
      const translationBlocksDataUrl = await renderPageToPngDataUrl(selectedPage, {
        layerVisibility: {
          image: false,
          inpaint: false,
          inpaintResult: false,
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
        activeLayer: "output",
        fontWeightAvailability
      });
      const blob = await window.mangaApi.exportInpaintPsd({
        chapterId: currentChapter.id,
        pageId: selectedPage.id,
        pageName: selectedPage.name,
        width: selectedPage.width,
        height: selectedPage.height,
        sourceDataUrl,
        maskDataUrl,
        resultDataUrl,
        translationBlocksDataUrl
      });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = `${sanitizeDownloadBasename(selectedPage.name, selectedPage.id)}-inpaint.psd`;
      document.body.appendChild(anchor);
      anchor.click();
      anchor.remove();
      window.setTimeout(() => URL.revokeObjectURL(url), 1000);
      pushStatus("인페인트 PSD를 내보냈습니다.");
    } catch (error) {
      console.error(error);
      pushStatus(error instanceof Error ? error.message : "인페인트 PSD 내보내기에 실패했습니다.");
    } finally {
      setInpaintPsdBusy(false);
    }
  }, [currentChapter, fontWeightAvailability, inpaintPsdBusy, pushStatus, selectedPage, selectedPageEditLocked]);

  const selectInpaintPsdFile = React.useCallback(() => {
    if (!currentChapter || !selectedPage || selectedPageEditLocked || inpaintPsdBusy) {
      return;
    }
    inpaintPsdInputRef.current?.click();
  }, [currentChapter, inpaintPsdBusy, selectedPage, selectedPageEditLocked]);

  const downloadLastImportedInpaintPsd = React.useCallback(async () => {
    if (!currentChapterId || !selectedPageCurrentId || inpaintPsdBusy) {
      return;
    }
    setInpaintPsdBusy(true);
    try {
      const blob = await window.mangaApi.downloadLastImportedInpaintPsd(currentChapterId, selectedPageCurrentId);
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = "last-imported-inpaint.psd";
      document.body.appendChild(anchor);
      anchor.click();
      anchor.remove();
      window.setTimeout(() => URL.revokeObjectURL(url), 1000);
      pushStatus("마지막으로 사용한 PSD를 내려받았습니다.");
    } catch (error) {
      console.error(error);
      pushStatus(error instanceof Error ? error.message : "마지막으로 사용한 PSD 내려받기에 실패했습니다.");
    } finally {
      setInpaintPsdBusy(false);
    }
  }, [currentChapterId, inpaintPsdBusy, pushStatus, selectedPageCurrentId]);

  const handleInpaintPsdInputChange = React.useCallback(async (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0] ?? null;
    event.target.value = "";
    if (!file || !currentChapter || !selectedPage || selectedPageEditLocked || inpaintPsdBusy) {
      return;
    }

    setInpaintPsdBusy(true);
    try {
      clearPendingInpaintSaveTimers();
      await flushInpaintMaskSave();
      await flushInpaintResultSave();
      if (dirty) {
        await saveNow();
      }

      recordInpaintMaskUndoSnapshot(selectedPage);

      const result = await window.mangaApi.importInpaintPsd(currentChapter.id, selectedPage.id, file);
      mergeLiveChapter(result.chapter);
      void refreshLastImportedInpaintPsdMeta();
      signalSaveComplete();
      void refreshLibrary();
      showInpaintLayers();
      pushStatus("PSD에서 인페인트 결과와 마스크를 가져왔습니다.");
    } catch (error) {
      console.error(error);
      pushStatus(error instanceof Error ? error.message : "인페인트 PSD 가져오기에 실패했습니다.");
    } finally {
      setInpaintPsdBusy(false);
    }
  }, [
    clearPendingInpaintSaveTimers,
    currentChapter,
    dirty,
    flushInpaintMaskSave,
    flushInpaintResultSave,
    inpaintPsdBusy,
    mergeLiveChapter,
    pushStatus,
    recordInpaintMaskUndoSnapshot,
    refreshLastImportedInpaintPsdMeta,
    refreshLibrary,
    saveNow,
    selectedPage,
    selectedPageEditLocked,
    showInpaintLayers,
    signalSaveComplete
  ]);

  return {
    downloadLastImportedInpaintPsd,
    exportSelectedPageInpaintPsd,
    handleInpaintPsdInputChange,
    inpaintPsdBusy,
    inpaintPsdInputRef,
    lastImportedInpaintPsdAt,
    lastImportedInpaintPsdLabel,
    selectInpaintPsdFile
  };
}
