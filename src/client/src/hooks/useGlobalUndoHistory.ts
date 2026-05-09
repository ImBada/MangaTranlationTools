import React from "react";
import {
  GLOBAL_UNDO_HISTORY_LIMIT,
  type GlobalUndoHistoryEntry,
  type GlobalUndoKind
} from "../lib/editorUndoHistory";
import { writeInpaintDebugLog } from "../lib/inpaintDiagnostics";
import type { GlobalUndoAction } from "../lib/globalUndo";

type ResolveGlobalUndoActionsOptions = {
  canUndoInpaintMask: (pageId: string) => boolean;
  canUndoInpaintResult: (pageId: string) => boolean;
  canUndoTranslation: boolean;
  currentChapterId: string | null | undefined;
  selectedPageEditLocked: boolean;
  undoPageInpaint: (pageId: string) => void;
  undoPageInpaintResult: (pageId: string) => void;
  undoTranslationEdit: () => void;
};

type UseGlobalUndoHistoryState = {
  clearUndoStacks: () => void;
  consumeGlobalUndoEntry: (kind: GlobalUndoKind, pageId?: string) => void;
  recordGlobalUndoEntry: (entry: GlobalUndoHistoryEntry) => void;
  registerInpaintUndoClearer: (clearer: () => void) => void;
  registerTranslationUndoClearer: (clearer: () => void) => void;
  resolveGlobalUndoActions: (options: ResolveGlobalUndoActionsOptions) => GlobalUndoAction[];
  undoVersion: number;
};

export function useGlobalUndoHistory(): UseGlobalUndoHistoryState {
  const [undoVersion, setUndoVersion] = React.useState(0);
  const globalUndoHistoryRef = React.useRef<GlobalUndoHistoryEntry[]>([]);
  const clearTranslationUndoStackRef = React.useRef<() => void>(() => undefined);
  const clearInpaintUndoStacksRef = React.useRef<() => void>(() => undefined);

  const clearUndoStacks = React.useCallback(() => {
    writeInpaintDebugLog("global-undo:clear", {
      historyLength: globalUndoHistoryRef.current.length
    });
    globalUndoHistoryRef.current = [];
    clearTranslationUndoStackRef.current();
    clearInpaintUndoStacksRef.current();
    setUndoVersion((current) => current + 1);
  }, []);

  const recordGlobalUndoEntry = React.useCallback((entry: GlobalUndoHistoryEntry) => {
    globalUndoHistoryRef.current = [...globalUndoHistoryRef.current, entry].slice(-GLOBAL_UNDO_HISTORY_LIMIT);
    writeInpaintDebugLog("global-undo:record", {
      entry,
      historyLength: globalUndoHistoryRef.current.length
    });
    setUndoVersion((current) => current + 1);
  }, []);

  const consumeGlobalUndoEntry = React.useCallback((kind: GlobalUndoKind, pageId?: string) => {
    const next = [...globalUndoHistoryRef.current];
    for (let index = next.length - 1; index >= 0; index -= 1) {
      const entry = next[index];
      if (entry?.kind === kind && entry.pageId === pageId) {
        next.splice(index, 1);
        globalUndoHistoryRef.current = next;
        writeInpaintDebugLog("global-undo:consume", {
          consumedEntry: entry,
          historyLength: globalUndoHistoryRef.current.length,
          kind,
          pageId
        });
        setUndoVersion((current) => current + 1);
        return;
      }
    }
    writeInpaintDebugLog("global-undo:consume-skip", {
      historyLength: globalUndoHistoryRef.current.length,
      kind,
      pageId,
      reason: "entry-not-found"
    });
  }, []);

  const registerTranslationUndoClearer = React.useCallback((clearer: () => void) => {
    clearTranslationUndoStackRef.current = clearer;
  }, []);

  const registerInpaintUndoClearer = React.useCallback((clearer: () => void) => {
    clearInpaintUndoStacksRef.current = clearer;
  }, []);

  const resolveGlobalUndoActions = React.useCallback(({
    canUndoInpaintMask,
    canUndoInpaintResult,
    canUndoTranslation,
    currentChapterId,
    selectedPageEditLocked,
    undoPageInpaint,
    undoPageInpaintResult,
    undoTranslationEdit
  }: ResolveGlobalUndoActionsOptions): GlobalUndoAction[] => {
    if (!currentChapterId || selectedPageEditLocked) {
      return [];
    }

    return [...globalUndoHistoryRef.current].reverse().map((entry, index): GlobalUndoAction => {
      const id = `${entry.kind}-${entry.pageId ?? "chapter"}-${index}`;
      if (entry.chapterId !== currentChapterId) {
        return { id, label: "다른 화 되돌리기", canUndo: false, run: () => undefined };
      }

      if (entry.kind === "translation") {
        return {
          id,
          label: "번역 블록 되돌리기",
          canUndo: canUndoTranslation,
          run: undoTranslationEdit
        };
      }

      if (entry.kind === "inpaint-mask" && entry.pageId) {
        return {
          id,
          label: "인페인트 마스크 되돌리기",
          canUndo: canUndoInpaintMask(entry.pageId),
          run: () => undoPageInpaint(entry.pageId!)
        };
      }

      if (entry.kind === "inpaint-result" && entry.pageId) {
        return {
          id,
          label: "인페인트 결과 되돌리기",
          canUndo: canUndoInpaintResult(entry.pageId),
          run: () => undoPageInpaintResult(entry.pageId!)
        };
      }

      return { id, label: "되돌리기", canUndo: false, run: () => undefined };
    });
  }, []);

  return {
    clearUndoStacks,
    consumeGlobalUndoEntry,
    recordGlobalUndoEntry,
    registerInpaintUndoClearer,
    registerTranslationUndoClearer,
    resolveGlobalUndoActions,
    undoVersion
  };
}
