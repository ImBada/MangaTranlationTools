export type TranslationBlockDragPreviewOffset = {
  offsetX: number;
  offsetY: number;
};

const TRANSLATION_BLOCK_DRAG_PREVIEW_EVENT = "manga-translation-tools:translation-block-drag-preview";
const RESET_TRANSLATION_BLOCK_DRAG_PREVIEW_OFFSET: TranslationBlockDragPreviewOffset = {
  offsetX: 0,
  offsetY: 0
};

let currentTranslationBlockDragPreviewOffset = RESET_TRANSLATION_BLOCK_DRAG_PREVIEW_OFFSET;

export function getTranslationBlockDragPreviewOffset(): TranslationBlockDragPreviewOffset {
  return currentTranslationBlockDragPreviewOffset;
}

export function resetTranslationBlockDragPreviewOffset(): void {
  setTranslationBlockDragPreviewOffset(RESET_TRANSLATION_BLOCK_DRAG_PREVIEW_OFFSET);
}

export function clearTranslationBlockDragPreviewOffset(): void {
  currentTranslationBlockDragPreviewOffset = RESET_TRANSLATION_BLOCK_DRAG_PREVIEW_OFFSET;
}

export function setTranslationBlockDragPreviewOffset(offset: TranslationBlockDragPreviewOffset): void {
  currentTranslationBlockDragPreviewOffset = offset;
  if (typeof window === "undefined") {
    return;
  }

  window.dispatchEvent(new CustomEvent(TRANSLATION_BLOCK_DRAG_PREVIEW_EVENT, { detail: offset }));
}

export function addTranslationBlockDragPreviewOffsetListener(
  listener: (offset: TranslationBlockDragPreviewOffset) => void
): () => void {
  if (typeof window === "undefined") {
    return () => undefined;
  }

  const handleOffsetChange = (event: Event) => {
    listener((event as CustomEvent<TranslationBlockDragPreviewOffset>).detail);
  };
  window.addEventListener(TRANSLATION_BLOCK_DRAG_PREVIEW_EVENT, handleOffsetChange);
  return () => window.removeEventListener(TRANSLATION_BLOCK_DRAG_PREVIEW_EVENT, handleOffsetChange);
}
