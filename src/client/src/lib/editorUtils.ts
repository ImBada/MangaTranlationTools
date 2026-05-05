import type { ChapterSnapshot, MangaPage, TranslationBlock } from "../../../shared/types";
import { normalizeKoreanText } from "./textNormalization";

const TRANSLATION_BLOCK_CLIPBOARD_KIND = "manga-translation-tools/translation-block";

type TranslationBlockClipboardPayload = {
  kind: typeof TRANSLATION_BLOCK_CLIPBOARD_KIND;
  version: 1;
  block: TranslationBlock;
};

export type InpaintMaskUndoSnapshot = {
  inpaintMaskPath?: string;
  inpaintResultPath?: string;
  inpaintMaskDataUrl?: string;
  inpaintResultDataUrl?: string;
  inpaintStatus?: MangaPage["inpaintStatus"];
};

export type TranslationUndoSnapshot = {
  chapterId: string;
  label: string;
  createdAtMs: number;
  selectedPageId: string | null;
  selectedBlockId: string | null;
  editingFontPresetId: string | null;
  fontPresets: ChapterSnapshot["fontPresets"];
  fontSizePresets: ChapterSnapshot["fontSizePresets"];
  pages: {
    pageId: string;
    updatedAt: string;
    blocks: TranslationBlock[];
  }[];
};

export function cloneTranslationBlock(block: TranslationBlock): TranslationBlock {
  return {
    ...block,
    bbox: { ...block.bbox },
    renderBbox: block.renderBbox ? { ...block.renderBbox } : undefined
  };
}

export function serializeTranslationBlockForClipboard(block: TranslationBlock): string {
  return JSON.stringify({
    kind: TRANSLATION_BLOCK_CLIPBOARD_KIND,
    version: 1,
    block: cloneTranslationBlock(block)
  } satisfies TranslationBlockClipboardPayload);
}

export function parseTranslationBlockFromClipboard(value: string): TranslationBlock | null {
  try {
    const parsed = JSON.parse(value) as Partial<TranslationBlockClipboardPayload>;
    if (parsed.kind !== TRANSLATION_BLOCK_CLIPBOARD_KIND || parsed.version !== 1 || !parsed.block) {
      return null;
    }
    return cloneTranslationBlock(parsed.block);
  } catch {
    return null;
  }
}

export function createInpaintMaskUndoSnapshot(
  page: MangaPage,
  overrides: Partial<InpaintMaskUndoSnapshot> = {}
): InpaintMaskUndoSnapshot {
  return {
    inpaintMaskPath: page.inpaintMaskPath,
    inpaintResultPath: page.inpaintResultPath,
    inpaintMaskDataUrl: page.inpaintMaskDataUrl ?? page.inpaintLayerDataUrl,
    inpaintResultDataUrl: page.inpaintResultDataUrl,
    inpaintStatus: page.inpaintStatus,
    ...overrides
  };
}

export function createTranslationUndoSnapshot(
  chapter: ChapterSnapshot,
  label: string,
  createdAtMs: number,
  selectedPageId: string | null,
  selectedBlockId: string | null,
  editingFontPresetId: string | null
): TranslationUndoSnapshot {
  return {
    chapterId: chapter.id,
    label,
    createdAtMs,
    selectedPageId,
    selectedBlockId,
    editingFontPresetId,
    fontPresets: chapter.fontPresets?.map((preset) => ({ ...preset })),
    fontSizePresets: chapter.fontSizePresets?.map((preset) => ({ ...preset })),
    pages: chapter.pages.map((page) => ({
      pageId: page.id,
      updatedAt: page.updatedAt,
      blocks: page.blocks.map(cloneTranslationBlock)
    }))
  };
}

export function normalizeChapterTranslatedText(chapter: ChapterSnapshot): { chapter: ChapterSnapshot; dirtyPageIds: string[] } {
  const updatedAt = new Date().toISOString();
  const dirtyPageIds: string[] = [];
  let chapterChanged = false;

  const pages = chapter.pages.map((page) => {
    let pageChanged = false;
    const blocks = page.blocks.map((block) => {
      const translatedText = normalizeKoreanText(block.translatedText);
      if (translatedText === block.translatedText) {
        return block;
      }

      pageChanged = true;
      return {
        ...block,
        translatedText
      };
    });

    if (!pageChanged) {
      return page;
    }

    chapterChanged = true;
    dirtyPageIds.push(page.id);
    return {
      ...page,
      updatedAt,
      blocks
    };
  });

  return {
    chapter: chapterChanged ? { ...chapter, updatedAt, pages } : chapter,
    dirtyPageIds
  };
}

export function reorderByTarget(currentOrder: string[], sourceId: string, targetId: string): string[] {
  const next = [...currentOrder];
  const sourceIndex = next.indexOf(sourceId);
  const targetIndex = next.indexOf(targetId);
  if (sourceIndex < 0 || targetIndex < 0) {
    return currentOrder;
  }
  const [item] = next.splice(sourceIndex, 1);
  next.splice(targetIndex, 0, item);
  return next;
}

export function bringTranslationBlockToFront(blocks: TranslationBlock[], blockId: string): TranslationBlock[] {
  const blockIndex = blocks.findIndex((block) => block.id === blockId);
  if (blockIndex < 0 || blockIndex === blocks.length - 1) {
    return blocks;
  }

  const next = [...blocks];
  const [block] = next.splice(blockIndex, 1);
  next.push(block);
  return next;
}

export function isEditableTarget(target: EventTarget | null): boolean {
  if (typeof Element === "undefined" || !(target instanceof Element)) {
    return false;
  }

  if (target instanceof HTMLElement && target.isContentEditable) {
    return true;
  }

  return Boolean(target.closest("input, textarea, select, [contenteditable=''], [contenteditable='true'], [contenteditable='plaintext-only']"));
}

export function angleBetweenPointsDeg(centerX: number, centerY: number, x: number, y: number): number {
  return (Math.atan2(y - centerY, x - centerX) * 180) / Math.PI;
}
