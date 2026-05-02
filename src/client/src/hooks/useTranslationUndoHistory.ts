import React from "react";
import type { ChapterSnapshot } from "../../../shared/types";
import {
  cloneTranslationBlock,
  createTranslationUndoSnapshot,
  type TranslationUndoSnapshot
} from "../lib/editorUtils";
import {
  TRANSLATION_UNDO_COALESCE_MS,
  TRANSLATION_UNDO_LIMIT,
  type GlobalUndoHistoryEntry,
  type GlobalUndoKind
} from "../lib/editorUndoHistory";

type UseTranslationUndoHistoryOptions = {
  consumeGlobalUndoEntry: (kind: GlobalUndoKind, pageId?: string) => void;
  currentChapter: ChapterSnapshot | null;
  currentChapterRef: React.RefObject<ChapterSnapshot | null>;
  editingFontPresetIdRef: React.RefObject<string | null>;
  markDirty: (pageId?: string) => void;
  recordGlobalUndoEntry: (entry: GlobalUndoHistoryEntry) => void;
  selectedBlockIdRef: React.RefObject<string | null>;
  selectedPageEditLocked: boolean;
  selectedPageIdRef: React.RefObject<string | null>;
  setCurrentChapter: React.Dispatch<React.SetStateAction<ChapterSnapshot | null>>;
  setEditingFontPresetId: React.Dispatch<React.SetStateAction<string | null>>;
  setSelectedBlockId: React.Dispatch<React.SetStateAction<string | null>>;
  setSelectedPageId: React.Dispatch<React.SetStateAction<string | null>>;
  undoVersion: number;
};

type UseTranslationUndoHistoryState = {
  canUndoTranslation: boolean;
  clearTranslationUndoStack: () => void;
  recordTranslationUndoSnapshot: (label: string) => boolean;
  undoTranslationEdit: () => void;
};

export function useTranslationUndoHistory({
  consumeGlobalUndoEntry,
  currentChapter,
  currentChapterRef,
  editingFontPresetIdRef,
  markDirty,
  recordGlobalUndoEntry,
  selectedBlockIdRef,
  selectedPageEditLocked,
  selectedPageIdRef,
  setCurrentChapter,
  setEditingFontPresetId,
  setSelectedBlockId,
  setSelectedPageId,
  undoVersion
}: UseTranslationUndoHistoryOptions): UseTranslationUndoHistoryState {
  const translationUndoStackRef = React.useRef<TranslationUndoSnapshot[]>([]);

  const canUndoTranslation = undoVersion >= 0 && currentChapter
    ? translationUndoStackRef.current.some((snapshot) => snapshot.chapterId === currentChapter.id) && !selectedPageEditLocked
    : false;

  const clearTranslationUndoStack = React.useCallback(() => {
    translationUndoStackRef.current = [];
  }, []);

  const recordTranslationUndoSnapshot = React.useCallback((label: string) => {
    const chapter = currentChapterRef.current;
    if (!chapter) {
      return false;
    }

    const now = Date.now();
    const selectedPageId = selectedPageIdRef.current;
    const selectedBlockId = selectedBlockIdRef.current;
    const editingFontPresetId = editingFontPresetIdRef.current;
    const lastSnapshot = translationUndoStackRef.current.at(-1);
    if (
      lastSnapshot &&
      lastSnapshot.chapterId === chapter.id &&
      lastSnapshot.label === label &&
      lastSnapshot.selectedPageId === selectedPageId &&
      lastSnapshot.selectedBlockId === selectedBlockId &&
      lastSnapshot.editingFontPresetId === editingFontPresetId &&
      now - lastSnapshot.createdAtMs <= TRANSLATION_UNDO_COALESCE_MS
    ) {
      lastSnapshot.createdAtMs = now;
      return true;
    }

    translationUndoStackRef.current = [
      ...translationUndoStackRef.current,
      createTranslationUndoSnapshot(
        chapter,
        label,
        now,
        selectedPageId,
        selectedBlockId,
        editingFontPresetId
      )
    ].slice(-TRANSLATION_UNDO_LIMIT);
    recordGlobalUndoEntry({ kind: "translation", chapterId: chapter.id });
    return true;
  }, [currentChapterRef, editingFontPresetIdRef, recordGlobalUndoEntry, selectedBlockIdRef, selectedPageIdRef]);

  const undoTranslationEdit = React.useCallback(() => {
    if (selectedPageEditLocked) {
      return;
    }

    const chapterId = currentChapterRef.current?.id;
    if (!chapterId) {
      return;
    }

    const stack = [...translationUndoStackRef.current];
    let snapshotIndex = -1;
    for (let index = stack.length - 1; index >= 0; index -= 1) {
      if (stack[index]?.chapterId === chapterId) {
        snapshotIndex = index;
        break;
      }
    }
    if (snapshotIndex < 0) {
      return;
    }

    const snapshot = stack[snapshotIndex];
    stack.splice(snapshotIndex, 1);
    translationUndoStackRef.current = stack;
    consumeGlobalUndoEntry("translation");

    const pageSnapshots = new Map(snapshot.pages.map((page) => [page.pageId, page]));
    const updatedAt = new Date().toISOString();
    setCurrentChapter((current) => {
      if (!current || current.id !== snapshot.chapterId) {
        return current;
      }

      const next = {
        ...current,
        updatedAt,
        fontPresets: snapshot.fontPresets?.map((preset) => ({ ...preset })),
        pages: current.pages.map((page) => {
          const pageSnapshot = pageSnapshots.get(page.id);
          return pageSnapshot
            ? {
                ...page,
                updatedAt,
                blocks: pageSnapshot.blocks.map(cloneTranslationBlock)
              }
            : page;
        })
      };
      markDirty(undefined);
      return next;
    });
    setSelectedBlockId(snapshot.selectedBlockId);
    setSelectedPageId(snapshot.selectedPageId);
    setEditingFontPresetId(snapshot.editingFontPresetId);
  }, [consumeGlobalUndoEntry, currentChapterRef, markDirty, selectedPageEditLocked, setCurrentChapter, setEditingFontPresetId, setSelectedBlockId, setSelectedPageId]);

  return {
    canUndoTranslation,
    clearTranslationUndoStack,
    recordTranslationUndoSnapshot,
    undoTranslationEdit
  };
}
