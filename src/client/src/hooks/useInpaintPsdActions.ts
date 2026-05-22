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
  lastExportedInpaintPsdFilename: string | null;
  lastExportedInpaintPsdReady: boolean;
  lastImportedInpaintPsdAt: string | null;
  lastImportedInpaintPsdLabel: string | null;
  quickLoadLastExportedInpaintPsd: () => Promise<void>;
  selectInpaintPsdFile: () => void;
};

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
  const [lastExportedInpaintPsdFilename, setLastExportedInpaintPsdFilename] = React.useState<string | null>(null);
  const [lastExportedInpaintPsdReady, setLastExportedInpaintPsdReady] = React.useState(false);
  const [lastImportedInpaintPsdAt, setLastImportedInpaintPsdAt] = React.useState<string | null>(null);
  const inpaintPsdInputRef = React.useRef<HTMLInputElement | null>(null);
  const psdMetaTargetRef = React.useRef<{ chapterId: string | null; pageId: string | null }>({ chapterId: null, pageId: null });
  psdMetaTargetRef.current = { chapterId: currentChapterId, pageId: selectedPageCurrentId };

  const lastImportedInpaintPsdLabel = lastImportedInpaintPsdAt
    ? new Intl.DateTimeFormat(undefined, { dateStyle: "short", timeStyle: "short" }).format(new Date(lastImportedInpaintPsdAt))
    : null;

  const refreshLastImportedInpaintPsdMeta = React.useCallback(async () => {
    if (!currentChapterId || !selectedPageCurrentId) {
      if (!psdMetaTargetRef.current.chapterId || !psdMetaTargetRef.current.pageId) {
        setLastImportedInpaintPsdAt(null);
        setLastExportedInpaintPsdFilename(null);
        setLastExportedInpaintPsdReady(false);
      }
      return { exists: false };
    }
    const meta = await window.mangaApi.getInpaintPsdMeta(currentChapterId, selectedPageCurrentId);
    if (psdMetaTargetRef.current.chapterId !== currentChapterId || psdMetaTargetRef.current.pageId !== selectedPageCurrentId) {
      return meta;
    }
    setLastImportedInpaintPsdAt(meta.exists && meta.importedAt ? meta.importedAt : null);
    setLastExportedInpaintPsdFilename(meta.quickLoad?.exists ? meta.quickLoad.filename ?? null : null);
    setLastExportedInpaintPsdReady(Boolean(meta.quickLoad?.exists));
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
      const { filename } = await window.mangaApi.exportInpaintPsd({
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
      const exportedChapterId = currentChapter.id;
      const exportedPageId = selectedPage.id;
      if (psdMetaTargetRef.current.chapterId === exportedChapterId && psdMetaTargetRef.current.pageId === exportedPageId) {
        void refreshLastImportedInpaintPsdMeta().catch((error) => {
          console.error(error);
        });
      }
      pushStatus(`인페인트 PSD를 내보냈습니다: ${filename}`);
    } catch (error) {
      console.error(error);
      pushStatus(error instanceof Error ? error.message : "인페인트 PSD 내보내기에 실패했습니다.");
    } finally {
      setInpaintPsdBusy(false);
    }
  }, [
    currentChapter,
    fontWeightAvailability,
    inpaintPsdBusy,
    pushStatus,
    refreshLastImportedInpaintPsdMeta,
    selectedPage,
    selectedPageEditLocked
  ]);

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

  const prepareInpaintPsdImport = React.useCallback(async () => {
    if (!selectedPage) {
      throw new Error("PSD를 가져올 페이지가 없습니다.");
    }
    clearPendingInpaintSaveTimers();
    await flushInpaintMaskSave();
    await flushInpaintResultSave();
    if (dirty) {
      await saveNow();
    }

    recordInpaintMaskUndoSnapshot(selectedPage);
  }, [
    clearPendingInpaintSaveTimers,
    dirty,
    flushInpaintMaskSave,
    flushInpaintResultSave,
    recordInpaintMaskUndoSnapshot,
    saveNow,
    selectedPage
  ]);

  const applyInpaintPsdImportResult = React.useCallback(async (result: Awaited<ReturnType<typeof window.mangaApi.importInpaintPsd>>) => {
    mergeLiveChapter(result.chapter);
    void refreshLastImportedInpaintPsdMeta();
    signalSaveComplete();
    void refreshLibrary();
    showInpaintLayers();
  }, [mergeLiveChapter, refreshLastImportedInpaintPsdMeta, refreshLibrary, showInpaintLayers, signalSaveComplete]);

  const handleInpaintPsdInputChange = React.useCallback(async (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0] ?? null;
    event.target.value = "";
    if (!file || !currentChapter || !selectedPage || selectedPageEditLocked || inpaintPsdBusy) {
      return;
    }

    setInpaintPsdBusy(true);
    try {
      await prepareInpaintPsdImport();

      const result = await window.mangaApi.importInpaintPsd(currentChapter.id, selectedPage.id, file);
      await applyInpaintPsdImportResult(result);
      pushStatus("PSD에서 인페인트 결과와 마스크를 가져왔습니다.");
    } catch (error) {
      console.error(error);
      pushStatus(error instanceof Error ? error.message : "인페인트 PSD 가져오기에 실패했습니다.");
    } finally {
      setInpaintPsdBusy(false);
    }
  }, [
    applyInpaintPsdImportResult,
    currentChapter,
    inpaintPsdBusy,
    prepareInpaintPsdImport,
    pushStatus,
    selectedPage,
    selectedPageEditLocked
  ]);

  const quickLoadLastExportedInpaintPsd = React.useCallback(async () => {
    if (!currentChapter || !selectedPage || selectedPageEditLocked || inpaintPsdBusy || !lastExportedInpaintPsdReady) {
      return;
    }

    setInpaintPsdBusy(true);
    try {
      await prepareInpaintPsdImport();
      const result = await window.mangaApi.quickLoadLastExportedInpaintPsd(currentChapter.id, selectedPage.id);
      await applyInpaintPsdImportResult(result);
      pushStatus("마지막으로 내보낸 PSD를 퀵로드했습니다.");
    } catch (error) {
      console.error(error);
      pushStatus(error instanceof Error ? error.message : "인페인트 PSD 퀵로드에 실패했습니다.");
      void refreshLastImportedInpaintPsdMeta();
    } finally {
      setInpaintPsdBusy(false);
    }
  }, [
    applyInpaintPsdImportResult,
    currentChapter,
    inpaintPsdBusy,
    lastExportedInpaintPsdReady,
    prepareInpaintPsdImport,
    pushStatus,
    refreshLastImportedInpaintPsdMeta,
    selectedPage,
    selectedPageEditLocked
  ]);

  return {
    downloadLastImportedInpaintPsd,
    exportSelectedPageInpaintPsd,
    handleInpaintPsdInputChange,
    inpaintPsdBusy,
    inpaintPsdInputRef,
    lastExportedInpaintPsdFilename,
    lastExportedInpaintPsdReady,
    lastImportedInpaintPsdAt,
    lastImportedInpaintPsdLabel,
    quickLoadLastExportedInpaintPsd,
    selectInpaintPsdFile
  };
}
