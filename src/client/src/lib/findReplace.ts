import type { MangaPage } from "../../../shared/types";

export type FindReplaceMatch = {
  id: string;
  pageId: string;
  pageName: string;
  blockId: string;
  blockIndex: number;
  before: string;
  after: string;
  occurrenceCount: number;
};

export function replaceTextLiteral(value: string, keyword: string, replacement: string): string {
  if (!keyword) {
    return value;
  }
  return value.split(keyword).join(replacement);
}

export function countTextOccurrences(value: string, keyword: string): number {
  if (!keyword) {
    return 0;
  }

  let count = 0;
  let index = value.indexOf(keyword);
  while (index >= 0) {
    count += 1;
    index = value.indexOf(keyword, index + keyword.length);
  }
  return count;
}

export function collectFindReplaceMatches(
  pages: MangaPage[],
  keyword: string,
  replacement: string
): FindReplaceMatch[] {
  if (!keyword) {
    return [];
  }

  return pages.flatMap((page) =>
    page.blocks.flatMap((block, blockIndex) => {
      const occurrenceCount = countTextOccurrences(block.translatedText, keyword);
      if (occurrenceCount === 0) {
        return [];
      }

      return [{
        id: `${page.id}:${block.id}`,
        pageId: page.id,
        pageName: page.name,
        blockId: block.id,
        blockIndex,
        before: block.translatedText,
        after: replaceTextLiteral(block.translatedText, keyword, replacement),
        occurrenceCount
      }];
    })
  );
}
