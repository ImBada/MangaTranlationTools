import React from "react";
import type { ChapterSnapshot, ImportPreviewResult, LibraryIndex } from "../../../shared/types";
import type { ImportModalSubmit } from "../components/ImportModal";
import { reorderByTarget } from "../lib/editorUtils";

type ImportMode = "images" | "folder" | "zip" | "zip-folder";

type RenameTarget =
  | {
      kind: "work";
      id: string;
      title: string;
    }
  | {
      kind: "chapter";
      id: string;
      title: string;
    };

type UseLibraryActionsOptions = {
  applyChapter: (chapter: ChapterSnapshot | undefined, fallbackStatus?: string) => void;
  clearCurrentChapter: () => void;
  currentChapter: ChapterSnapshot | null;
  dirty: boolean;
  openChapter: (chapterId: string) => Promise<void>;
  pushStatus: (line: string) => void;
  saveNow: () => Promise<void>;
  setSelectedPageId: React.Dispatch<React.SetStateAction<string | null>>;
};

type UseLibraryActionsState = {
  batchImportInputRef: React.RefObject<HTMLInputElement | null>;
  deleteRenameTarget: () => Promise<void>;
  folderImportInputRef: React.RefObject<HTMLInputElement | null>;
  handleImportInputChange: (mode: ImportMode, event: React.ChangeEvent<HTMLInputElement>) => Promise<void>;
  imageImportInputRef: React.RefObject<HTMLInputElement | null>;
  importBusy: boolean;
  importPreview: ImportPreviewResult | null;
  library: LibraryIndex;
  refreshLibrary: () => Promise<void>;
  removePage: (pageId: string) => Promise<void>;
  renameBusy: boolean;
  renameChapter: (chapterId: string) => void;
  renameTarget: RenameTarget | null;
  renameWork: (workId: string) => void;
  reorderChapters: (workId: string, sourceChapterId: string, targetChapterId: string) => Promise<void>;
  reorderPages: (sourcePageId: string, targetPageId: string) => Promise<void>;
  selectImportFiles: (mode: ImportMode) => void;
  setImportPreview: React.Dispatch<React.SetStateAction<ImportPreviewResult | null>>;
  setRenameTarget: React.Dispatch<React.SetStateAction<RenameTarget | null>>;
  submitImport: (payload: ImportModalSubmit) => Promise<void>;
  submitRename: (title: string) => Promise<void>;
  zipImportInputRef: React.RefObject<HTMLInputElement | null>;
};

export function useLibraryActions({
  applyChapter,
  clearCurrentChapter,
  currentChapter,
  dirty,
  openChapter,
  pushStatus,
  saveNow,
  setSelectedPageId
}: UseLibraryActionsOptions): UseLibraryActionsState {
  const [library, setLibrary] = React.useState<LibraryIndex>({ workOrder: [], works: [] });
  const [importPreview, setImportPreview] = React.useState<ImportPreviewResult | null>(null);
  const [importBusy, setImportBusy] = React.useState(false);
  const [renameTarget, setRenameTarget] = React.useState<RenameTarget | null>(null);
  const [renameBusy, setRenameBusy] = React.useState(false);
  const imageImportInputRef = React.useRef<HTMLInputElement | null>(null);
  const folderImportInputRef = React.useRef<HTMLInputElement | null>(null);
  const zipImportInputRef = React.useRef<HTMLInputElement | null>(null);
  const batchImportInputRef = React.useRef<HTMLInputElement | null>(null);

  const refreshLibrary = React.useCallback(async () => {
    const next = await window.mangaApi.getLibrary();
    setLibrary(next);
  }, []);

  const openImportPreview = React.useCallback(async (mode: ImportMode, files: File[]) => {
    const preview =
      mode === "images"
        ? await window.mangaApi.previewImagesImport(files)
        : mode === "folder"
          ? await window.mangaApi.previewFolderImport(files)
          : mode === "zip"
            ? await window.mangaApi.previewZipImport(files)
            : await window.mangaApi.previewZipFolderImport(files);
    if (!preview) {
      return;
    }
    setImportPreview(preview);
  }, []);

  const selectImportFiles = React.useCallback((mode: ImportMode) => {
    const input =
      mode === "images"
        ? imageImportInputRef.current
        : mode === "folder"
          ? folderImportInputRef.current
          : mode === "zip"
            ? zipImportInputRef.current
            : batchImportInputRef.current;
    input?.click();
  }, []);

  const handleImportInputChange = React.useCallback(
    async (mode: ImportMode, event: React.ChangeEvent<HTMLInputElement>) => {
      const files = Array.from(event.target.files ?? []);
      event.target.value = "";
      await openImportPreview(mode, files);
    },
    [openImportPreview]
  );

  const submitImport = React.useCallback(
    async ({ target, selections }: ImportModalSubmit) => {
      if (!importPreview) {
        return;
      }

      setImportBusy(true);
      try {
        const result = await window.mangaApi.createImport({
          preview: importPreview,
          target,
          selections
        });
        await refreshLibrary();
        applyChapter(result.openedChapter, `${result.chapterIds.length}개 화를 보관함에 추가했습니다.`);
        setImportPreview(null);

        if (importPreview.mode === "batch") {
          for (const chapterId of result.chapterIds) {
            await openChapter(chapterId);
            const runResult = await window.mangaApi.startAnalysis({ chapterId, runMode: "pending" });
            if (runResult.chapter) {
              applyChapter(runResult.chapter);
            }
            await refreshLibrary();
            if (runResult.status !== "completed") {
              break;
            }
          }
        }
      } finally {
        setImportBusy(false);
      }
    },
    [applyChapter, importPreview, openChapter, refreshLibrary]
  );

  const renameWork = React.useCallback((workId: string) => {
    const work = library.works.find((candidate) => candidate.id === workId);
    if (!work) {
      return;
    }
    setRenameTarget({ kind: "work", id: workId, title: work.title });
  }, [library.works]);

  const renameChapter = React.useCallback((chapterId: string) => {
    const chapter =
      library.works.flatMap((work) => work.chapters).find((candidate) => candidate.id === chapterId) ??
      (currentChapter ? { id: currentChapter.id, title: currentChapter.title } : null);
    if (!chapter) {
      return;
    }
    setRenameTarget({ kind: "chapter", id: chapterId, title: chapter.title });
  }, [currentChapter, library.works]);

  const submitRename = React.useCallback(async (title: string) => {
    if (!renameTarget) {
      return;
    }

    setRenameBusy(true);
    try {
      if (renameTarget.kind === "work") {
        setLibrary(await window.mangaApi.renameWork(renameTarget.id, title));
      } else {
        if (currentChapter?.id === renameTarget.id && dirty) {
          await saveNow();
        }
        setLibrary(await window.mangaApi.renameChapter(renameTarget.id, title));
        if (currentChapter?.id === renameTarget.id) {
          applyChapter(await window.mangaApi.openChapter(renameTarget.id));
        }
      }
      setRenameTarget(null);
    } finally {
      setRenameBusy(false);
    }
  }, [applyChapter, currentChapter, dirty, renameTarget, saveNow]);

  const deleteRenameTarget = React.useCallback(async () => {
    if (!renameTarget) {
      return;
    }

    const isCurrentChapter = currentChapter?.id === renameTarget.id;
    const isCurrentWork = renameTarget.kind === "work" && currentChapter?.workId === renameTarget.id;
    const confirmed = await window.mangaApi.confirm(
      renameTarget.kind === "work" ? "작품 삭제" : "화 삭제",
      "정말 삭제하시겠습니까?",
      renameTarget.kind === "work"
        ? `"${renameTarget.title}" 작품과 포함된 모든 화, 페이지, 번역 결과가 보관함에서 삭제됩니다.`
        : `"${renameTarget.title}" 화와 포함된 모든 페이지, 번역 결과가 보관함에서 삭제됩니다.`
    );
    if (!confirmed) {
      return;
    }

    setRenameBusy(true);
    try {
      if ((isCurrentChapter || isCurrentWork) && dirty) {
        await saveNow();
      }

      if (renameTarget.kind === "work") {
        setLibrary(await window.mangaApi.deleteWork(renameTarget.id));
        if (isCurrentWork) {
          clearCurrentChapter();
        }
        pushStatus(`${renameTarget.title} 작품을 삭제했습니다.`);
      } else {
        setLibrary(await window.mangaApi.deleteChapter(renameTarget.id));
        if (isCurrentChapter) {
          clearCurrentChapter();
        }
        pushStatus(`${renameTarget.title} 화를 삭제했습니다.`);
      }

      setRenameTarget(null);
    } catch (error) {
      console.error(error);
      pushStatus(renameTarget.kind === "work" ? "작품을 삭제하지 못했습니다." : "화를 삭제하지 못했습니다.");
    } finally {
      setRenameBusy(false);
    }
  }, [clearCurrentChapter, currentChapter?.id, currentChapter?.workId, dirty, pushStatus, renameTarget, saveNow]);

  const removePage = React.useCallback(
    async (pageId: string) => {
      if (!currentChapter) {
        return;
      }
      const page = currentChapter.pages.find((candidate) => candidate.id === pageId);
      if (!page) {
        return;
      }
      const confirmed = await window.mangaApi.confirm(
        "페이지 삭제",
        "정말 삭제하시겠습니까?",
        "이 페이지와 해당 번역 결과가 보관함에서 삭제됩니다."
      );
      if (!confirmed) {
        return;
      }

      const previousOrder = currentChapter.pages.map((candidate) => candidate.id);
      const nextChapter = await window.mangaApi.deletePage(currentChapter.id, pageId);
      applyChapter(nextChapter);
      const currentIndex = previousOrder.indexOf(pageId);
      const nextId = previousOrder[currentIndex + 1] ?? previousOrder[currentIndex - 1] ?? null;
      setSelectedPageId(nextId && nextChapter.pages.some((candidate) => candidate.id === nextId) ? nextId : nextChapter.pages[0]?.id ?? null);
      pushStatus(`${page.name} 페이지를 삭제했습니다.`);
      await refreshLibrary();
    },
    [applyChapter, currentChapter, pushStatus, refreshLibrary, setSelectedPageId]
  );

  const reorderChapters = React.useCallback(async (workId: string, sourceChapterId: string, targetChapterId: string) => {
    const work = library.works.find((candidate) => candidate.id === workId);
    if (!work) {
      return;
    }
    const nextOrder = reorderByTarget(work.chapterOrder, sourceChapterId, targetChapterId);
    setLibrary(await window.mangaApi.reorderChapters(workId, nextOrder));
  }, [library.works]);

  const reorderPages = React.useCallback(async (sourcePageId: string, targetPageId: string) => {
    if (!currentChapter) {
      return;
    }
    const nextOrder = reorderByTarget(currentChapter.pageOrder, sourcePageId, targetPageId);
    applyChapter(await window.mangaApi.reorderPages(currentChapter.id, nextOrder));
    await refreshLibrary();
  }, [applyChapter, currentChapter, refreshLibrary]);

  return {
    batchImportInputRef,
    deleteRenameTarget,
    folderImportInputRef,
    handleImportInputChange,
    imageImportInputRef,
    importBusy,
    importPreview,
    library,
    refreshLibrary,
    removePage,
    renameBusy,
    renameChapter,
    renameTarget,
    renameWork,
    reorderChapters,
    reorderPages,
    selectImportFiles,
    setImportPreview,
    setRenameTarget,
    submitImport,
    submitRename,
    zipImportInputRef
  };
}
