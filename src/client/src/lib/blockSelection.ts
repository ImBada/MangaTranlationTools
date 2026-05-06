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
