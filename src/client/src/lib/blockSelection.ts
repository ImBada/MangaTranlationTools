import { bboxToPixels, resolveBlockRenderBbox } from "../../../shared/geometry";
import type { ImageRect, MangaPage, TranslationBlock } from "../../../shared/types";

type Rect = {
  x: number;
  y: number;
  width: number;
  height: number;
};

export function resolveTranslationBlockIdsInSelection(page: MangaPage, selectionRect: ImageRect): string[] {
  return page.blocks
    .filter((block) => block.renderDirection !== "hidden" && blockIntersectsSelection(block, page, selectionRect))
    .map((block) => block.id);
}

export function resolveSelectedTranslationBlocks(page: MangaPage | null, blockIds: readonly string[]): TranslationBlock[] {
  if (!page || blockIds.length === 0) {
    return [];
  }

  const blocksById = new Map(page.blocks.map((block) => [block.id, block]));
  return blockIds.flatMap((blockId) => {
    const block = blocksById.get(blockId);
    return block ? [block] : [];
  });
}

export function resolveShiftSelectedTranslationBlockIds(
  selectedBlockId: string | null,
  selectedBlockIds: readonly string[],
  targetBlockId: string
): string[] | null {
  const currentSelection = resolveCurrentTranslationBlockSelection(selectedBlockId, selectedBlockIds);
  const nextSelection = resolveToggledTranslationBlockIds(selectedBlockId, selectedBlockIds, [targetBlockId]);
  return currentSelection.length > 0 || nextSelection.length > 1 ? nextSelection : null;
}

export function resolveToggledTranslationBlockIds(
  selectedBlockId: string | null,
  selectedBlockIds: readonly string[],
  toggledBlockIds: readonly string[]
): string[] {
  const currentSelection = resolveCurrentTranslationBlockSelection(selectedBlockId, selectedBlockIds);
  const currentSelectionSet = new Set(currentSelection);
  const toggledBlockIdSet = new Set(toggledBlockIds);
  const nextSelection = currentSelection.filter((blockId) => !toggledBlockIdSet.has(blockId));

  toggledBlockIds.forEach((blockId) => {
    if (!currentSelectionSet.has(blockId) && !nextSelection.includes(blockId)) {
      nextSelection.push(blockId);
    }
  });

  return nextSelection;
}

function resolveCurrentTranslationBlockSelection(
  selectedBlockId: string | null,
  selectedBlockIds: readonly string[]
): string[] {
  return selectedBlockIds.length > 1
    ? [...selectedBlockIds]
    : selectedBlockId
      ? [selectedBlockId]
      : [];
}

function blockIntersectsSelection(block: TranslationBlock, page: MangaPage, selectionRect: ImageRect): boolean {
  const blockBbox = bboxToPixels(resolveBlockRenderBbox(block), page.width, page.height);
  return rectsIntersect(
    {
      x: blockBbox.x,
      y: blockBbox.y,
      width: blockBbox.w,
      height: blockBbox.h
    },
    selectionRect
  );
}

function rectsIntersect(a: Rect, b: Rect): boolean {
  return (
    a.x < b.x + b.width &&
    a.x + a.width > b.x &&
    a.y < b.y + b.height &&
    a.y + a.height > b.y
  );
}
