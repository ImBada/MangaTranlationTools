export type GlobalUndoKind = "translation" | "inpaint-mask" | "inpaint-result";

export type GlobalUndoHistoryEntry = {
  kind: GlobalUndoKind;
  chapterId: string;
  pageId?: string;
};

export const GLOBAL_UNDO_HISTORY_LIMIT = 150;
export const TRANSLATION_UNDO_COALESCE_MS = 1000;
export const TRANSLATION_UNDO_LIMIT = 50;
