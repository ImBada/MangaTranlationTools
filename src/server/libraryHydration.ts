import { existsSync } from "node:fs";
import { writeFile } from "node:fs/promises";
import type { ChapterPagePatch, ChapterSnapshot, LibraryPageRecord, MangaPage, PageImageLayer } from "../shared/types";
import { normalizeRenderDirection } from "../shared/geometry";
import { clipOpaqueInpaintResultToMask, dataUrlToBuffer, fileToDataUrl } from "./libraryImageData";
import { reorderRecords, resolveChapterStatus } from "./libraryOrdering";
import type { ChapterFile } from "./libraryStore";

export async function hydrateChapter(chapter: ChapterFile): Promise<ChapterSnapshot> {
  const pages = reorderRecords(chapter.pages, chapter.pageOrder).map((page) => hydratePageImageRefs(chapter.id, page));

  return {
    ...chapter,
    pageOrder: pages.map((page) => page.id),
    pages
  };
}

export function hydratePageImageRefs(chapterId: string, page: LibraryPageRecord): MangaPage {
  const hasInpaintMask = Boolean(
    (page.inpaintMaskPath && existsSync(page.inpaintMaskPath)) ||
      page.inpaintMaskDataUrl ||
      page.inpaintLayerDataUrl
  );
  const hasInpaintResult = Boolean((page.inpaintResultPath && existsSync(page.inpaintResultPath)) || page.inpaintResultDataUrl);

  return {
    ...page,
    blocks: page.blocks.map(normalizeStoredBlock),
    inpaintMaskDataUrl: hasInpaintMask ? pageImageUrl(chapterId, page.id, "inpaint-mask", page.updatedAt) : undefined,
    inpaintResultDataUrl: hasInpaintResult ? pageImageUrl(chapterId, page.id, "inpaint-result", page.updatedAt) : undefined,
    inpaintLayerDataUrl: undefined,
    inpaintStatus: page.inpaintStatus ?? (hasInpaintResult ? "completed" : "idle"),
    dataUrl: pageImageUrl(chapterId, page.id, "source", page.updatedAt)
  };
}

export async function hydratePageDataUrls(chapterId: string, page: LibraryPageRecord): Promise<MangaPage> {
  const inpaintMaskDataUrl = page.inpaintMaskPath && existsSync(page.inpaintMaskPath)
    ? await fileToDataUrl(page.inpaintMaskPath)
    : page.inpaintMaskDataUrl ?? page.inpaintLayerDataUrl;
  const inpaintResultDataUrl = page.inpaintResultPath && existsSync(page.inpaintResultPath)
    ? await fileToDataUrl(page.inpaintResultPath)
    : page.inpaintResultDataUrl;
  const clippedInpaintResultDataUrl = inpaintMaskDataUrl && inpaintResultDataUrl
    ? await clipOpaqueInpaintResultToMask(inpaintResultDataUrl, inpaintMaskDataUrl)
    : inpaintResultDataUrl;
  if (page.inpaintResultPath && inpaintResultDataUrl && clippedInpaintResultDataUrl && clippedInpaintResultDataUrl !== inpaintResultDataUrl) {
    await writeFile(page.inpaintResultPath, dataUrlToBuffer(clippedInpaintResultDataUrl));
  }

  return {
    ...page,
    blocks: page.blocks.map(normalizeStoredBlock),
    inpaintMaskDataUrl,
    inpaintResultDataUrl: clippedInpaintResultDataUrl,
    inpaintLayerDataUrl: undefined,
    inpaintStatus: page.inpaintStatus ?? (clippedInpaintResultDataUrl ? "completed" : "idle"),
    dataUrl: await fileToDataUrl(page.imagePath)
  };
}

export function pageImageUrl(chapterId: string, pageId: string, layer: PageImageLayer, version: string): string {
  return `/api/library/chapters/${encodeURIComponent(chapterId)}/pages/${encodeURIComponent(pageId)}/images/${layer}?v=${encodeURIComponent(version)}`;
}

export function toStoredChapter(snapshot: ChapterSnapshot): ChapterFile {
  return {
    ...snapshot,
    pages: snapshot.pages.map(({
      dataUrl: _dataUrl,
      inpaintMaskDataUrl: _inpaintMaskDataUrl,
      inpaintResultDataUrl: _inpaintResultDataUrl,
      inpaintLayerDataUrl: _inpaintLayerDataUrl,
      ...page
    }) => ({
      ...page,
      blocks: page.blocks.map(normalizeStoredBlock),
      inpaintStatus: page.inpaintStatus ?? (page.inpaintResultPath ? "completed" : "idle")
    }))
  };
}

export function toStoredPagePatch(page: ChapterPagePatch): ChapterPagePatch {
  const {
    dataUrl: _dataUrl,
    inpaintMaskDataUrl: _inpaintMaskDataUrl,
    inpaintResultDataUrl: _inpaintResultDataUrl,
    inpaintLayerDataUrl: _inpaintLayerDataUrl,
    imagePath: _imagePath,
    createdAt: _createdAt,
    ...patch
  } = page as ChapterPagePatch & Partial<MangaPage>;
  return {
    ...patch,
    blocks: patch.blocks?.map(normalizeStoredBlock)
  };
}

export function normalizeStoredBlock(block: MangaPage["blocks"][number]): MangaPage["blocks"][number] {
  return {
    ...block,
    renderDirection: normalizeRenderDirection(block.renderDirection)
  };
}

export function mergeChapterSnapshotForSave(incoming: ChapterFile, current: ChapterFile, dirtyPageIds?: string[]): ChapterFile {
  const dirtyPageIdSet = dirtyPageIds && dirtyPageIds.length > 0 ? new Set(dirtyPageIds) : null;
  const incomingPages = new Map(incoming.pages.map((page) => [page.id, page]));
  const currentPages = new Map(current.pages.map((page) => [page.id, page]));
  const pageOrder = incoming.pageOrder.length > 0 ? incoming.pageOrder : current.pageOrder;

  const mergedPages = pageOrder.map((pageId) => {
    const incomingPage = incomingPages.get(pageId);
    const currentPage = currentPages.get(pageId);
    if (!incomingPage) {
      return currentPage;
    }
    if (!currentPage) {
      return incomingPage;
    }
    if (dirtyPageIdSet) {
      return dirtyPageIdSet.has(pageId) ? incomingPage : currentPage;
    }
    return isAtLeastAsNew(incomingPage.updatedAt, currentPage.updatedAt) ? incomingPage : currentPage;
  }).filter((page): page is LibraryPageRecord => Boolean(page));

  for (const page of incoming.pages) {
    if (!pageOrder.includes(page.id) && !currentPages.has(page.id)) {
      mergedPages.push(page);
    }
  }

  return {
    ...current,
    ...incoming,
    lastOpenedPageId: current.lastOpenedPageId ?? incoming.lastOpenedPageId,
    status: resolveChapterStatus(mergedPages),
    updatedAt: maxIsoTimestamp(incoming.updatedAt, current.updatedAt, ...mergedPages.map((page) => page.updatedAt)),
    pageOrder: mergedPages.map((page) => page.id),
    pages: mergedPages
  };
}

export function maxIsoTimestamp(...values: string[]): string {
  return values.reduce((max, value) => (isAtLeastAsNew(value, max) ? value : max), values[0] ?? new Date().toISOString());
}

function isAtLeastAsNew(left: string, right: string): boolean {
  const leftTime = Date.parse(left);
  const rightTime = Date.parse(right);
  if (Number.isNaN(leftTime) || Number.isNaN(rightTime)) {
    return left >= right;
  }
  return leftTime >= rightTime;
}
