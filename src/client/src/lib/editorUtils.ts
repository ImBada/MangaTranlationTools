import type { ChapterSnapshot, MangaPage, TranslationBlock } from "../../../shared/types";
import { clearFontPresetLinkFields } from "./fontPresets";
import { normalizeKoreanText } from "./textNormalization";

const TRANSLATION_BLOCK_CLIPBOARD_KIND = "manga-translation-tools/translation-block";
const TRANSLATION_BLOCK_FONT_STYLE_CLIPBOARD_KIND = "manga-translation-tools/translation-block-font-style";

const TRANSLATION_BLOCK_FONT_STYLE_KEYS = [
  "fontFamily",
  "fontWeight",
  "fontStyle",
  "textDecoration",
  "fontSizePx",
  "lineHeight",
  "letterSpacingPx",
  "outlineColor",
  "outlineWidthPx",
  "secondaryOutlineColor",
  "secondaryOutlineWidthPx",
  "shadowEnabled",
  "shadowColor",
  "shadowAngleDeg",
  "shadowDistancePx",
  "autoFitText",
  "textAlign",
  "textPosition",
  "textColor",
  "screentoneFillEnabled",
  "screentoneFillIntensity",
  "screentoneFillDensity",
  "screentoneFillAntialias"
] satisfies readonly (keyof TranslationBlock)[];

type TranslationBlockFontStyleKey = typeof TRANSLATION_BLOCK_FONT_STYLE_KEYS[number];
const TRANSLATION_BLOCK_FONT_STYLE_KEY_SET = new Set<string>(TRANSLATION_BLOCK_FONT_STYLE_KEYS);

type TranslationBlockClipboardPayload = {
  kind: typeof TRANSLATION_BLOCK_CLIPBOARD_KIND;
  version: 1;
  block: TranslationBlock;
};

type TranslationBlockFontStyleClipboardPayload = {
  kind: typeof TRANSLATION_BLOCK_FONT_STYLE_CLIPBOARD_KIND;
  version: 1;
  values: Partial<Record<TranslationBlockFontStyleKey, unknown>>;
  unset: TranslationBlockFontStyleKey[];
};

export type TranslationBlockFontStylePatch = Partial<Pick<TranslationBlock, TranslationBlockFontStyleKey>>;

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
  favoriteFontPresetIds: ChapterSnapshot["favoriteFontPresetIds"];
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

export function extractTranslationBlockFontStyle(block: TranslationBlock): TranslationBlockFontStylePatch {
  const style: TranslationBlockFontStylePatch = {};
  for (const key of TRANSLATION_BLOCK_FONT_STYLE_KEYS) {
    Object.assign(style, { [key]: block[key] });
  }
  return style;
}

export function applyTranslationBlockFontStyle(
  block: TranslationBlock,
  style: TranslationBlockFontStylePatch
): TranslationBlock {
  const { fontPresetId: _fontPresetId, ...blockWithoutFontPreset } = block;

  return {
    ...clearFontPresetLinkFields(blockWithoutFontPreset),
    ...style
  };
}

export function serializeTranslationBlockFontStyleForClipboard(style: TranslationBlockFontStylePatch): string {
  const values: Partial<Record<TranslationBlockFontStyleKey, unknown>> = {};
  const unset: TranslationBlockFontStyleKey[] = [];

  for (const key of TRANSLATION_BLOCK_FONT_STYLE_KEYS) {
    const value = style[key];
    if (value === undefined) {
      unset.push(key);
    } else {
      values[key] = value;
    }
  }

  return JSON.stringify({
    kind: TRANSLATION_BLOCK_FONT_STYLE_CLIPBOARD_KIND,
    version: 1,
    values,
    unset
  } satisfies TranslationBlockFontStyleClipboardPayload);
}

export function parseTranslationBlockFontStyleFromClipboard(value: string): TranslationBlockFontStylePatch | null {
  try {
    const parsed = JSON.parse(value) as Partial<TranslationBlockFontStyleClipboardPayload>;
    if (
      parsed.kind !== TRANSLATION_BLOCK_FONT_STYLE_CLIPBOARD_KIND ||
      parsed.version !== 1 ||
      !isRecord(parsed.values) ||
      !Array.isArray(parsed.unset)
    ) {
      return null;
    }

    const unset = new Set(parsed.unset.filter(isTranslationBlockFontStyleKey));
    const style: TranslationBlockFontStylePatch = {};
    for (const key of TRANSLATION_BLOCK_FONT_STYLE_KEYS) {
      if (unset.has(key)) {
        Object.assign(style, { [key]: undefined });
        continue;
      }
      if (Object.prototype.hasOwnProperty.call(parsed.values, key)) {
        Object.assign(style, { [key]: parsed.values[key] });
      }
    }
    return style;
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
    favoriteFontPresetIds: chapter.favoriteFontPresetIds ? [...chapter.favoriteFontPresetIds] : undefined,
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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isTranslationBlockFontStyleKey(value: unknown): value is TranslationBlockFontStyleKey {
  return typeof value === "string" && TRANSLATION_BLOCK_FONT_STYLE_KEY_SET.has(value);
}
