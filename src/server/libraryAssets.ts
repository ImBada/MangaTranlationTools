import { existsSync } from "node:fs";
import { mkdir, readdir, rm, writeFile } from "node:fs/promises";
import { join } from "node:path";
import type { ChapterSnapshot, InpaintSettings, LibraryPageRecord, PageImageLayer, RenderPageResult } from "../shared/types";
import { safeUnlink } from "./libraryFileIO";
import {
  clipOpaqueInpaintResultBufferToMask,
  dataUrlToBuffer,
  dataUrlToImageAsset,
  normalizeInpaintLayerDataUrls,
  readImageFileAsset,
  sanitizeFileBasename,
  sanitizeRenderFilename,
  type PageImageAsset
} from "./libraryImageData";
import { hydrateChapter } from "./libraryHydration";
import { WORKS_ROOT } from "./libraryPaths";
import { findChapterLocation, readChapterFile, touchWork, writeChapterFile } from "./libraryStore";

export type { PageImageAsset } from "./libraryImageData";

export async function saveRenderedPage(chapterId: string, pageId: string, dataUrl: string): Promise<RenderPageResult> {
  const locator = await findChapterLocation(chapterId);
  if (!locator) {
    throw new Error("화를 찾지 못했습니다.");
  }
  const chapter = await readChapterFile(locator.workId, locator.chapterId);
  if (!chapter) {
    throw new Error("화를 찾지 못했습니다.");
  }
  const page = chapter.pages.find((candidate) => candidate.id === pageId);
  if (!page) {
    throw new Error("페이지를 찾지 못했습니다.");
  }

  const buffer = dataUrlToBuffer(dataUrl);
  const outputDir = join(WORKS_ROOT, locator.workId, "chapters", locator.chapterId, "renders");
  await mkdir(outputDir, { recursive: true });
  const outputFilename = sanitizeRenderFilename(page.name, page.id);
  const outputPath = join(outputDir, outputFilename);
  await writeFile(outputPath, buffer);
  return { outputPath };
}

export async function readPageImageAsset(chapterId: string, pageId: string, layer: PageImageLayer): Promise<PageImageAsset> {
  const locator = await findChapterLocation(chapterId);
  if (!locator) {
    throw new Error("화를 찾지 못했습니다.");
  }
  const chapter = await readChapterFile(locator.workId, locator.chapterId);
  if (!chapter) {
    throw new Error("화를 찾지 못했습니다.");
  }
  const page = chapter.pages.find((candidate) => candidate.id === pageId);
  if (!page) {
    throw new Error("페이지를 찾지 못했습니다.");
  }

  if (layer === "source") {
    return readImageFileAsset(page.imagePath, page.updatedAt);
  }

  if (layer === "inpaint-mask") {
    const maskAsset = await readInpaintMaskAsset(page);
    if (maskAsset) {
      return maskAsset;
    }
  }

  if (layer === "inpaint-result") {
    const resultAsset = await readInpaintResultAsset(page);
    if (resultAsset) {
      const resultPath = page.inpaintResultPath && existsSync(page.inpaintResultPath) ? page.inpaintResultPath : undefined;
      return normalizeInpaintResultAsset(resultAsset, await readInpaintMaskAsset(page), resultPath);
    }
  }

  throw new Error("요청한 이미지 레이어를 찾지 못했습니다.");
}

async function readInpaintMaskAsset(page: LibraryPageRecord): Promise<PageImageAsset | null> {
  if (page.inpaintMaskPath && existsSync(page.inpaintMaskPath)) {
    return readImageFileAsset(page.inpaintMaskPath, page.updatedAt);
  }
  const legacyDataUrl = page.inpaintMaskDataUrl ?? page.inpaintLayerDataUrl;
  return legacyDataUrl ? dataUrlToImageAsset(legacyDataUrl, page.updatedAt) : null;
}

async function readInpaintResultAsset(page: LibraryPageRecord): Promise<PageImageAsset | null> {
  if (page.inpaintResultPath && existsSync(page.inpaintResultPath)) {
    return readImageFileAsset(page.inpaintResultPath, page.updatedAt);
  }
  return page.inpaintResultDataUrl ? dataUrlToImageAsset(page.inpaintResultDataUrl, page.updatedAt) : null;
}

async function normalizeInpaintResultAsset(resultAsset: PageImageAsset, maskAsset: PageImageAsset | null, resultPath?: string): Promise<PageImageAsset> {
  if (!maskAsset) {
    return resultAsset;
  }

  const clipped = await clipOpaqueInpaintResultBufferToMask(resultAsset.buffer, maskAsset.buffer);
  if (!clipped) {
    return resultAsset;
  }
  if (resultPath) {
    await writeFile(resultPath, clipped);
  }
  return {
    ...resultAsset,
    buffer: clipped,
    mime: "image/png"
  };
}

export async function saveInpaintResult(
  chapterId: string,
  pageId: string,
  maskDataUrl: string,
  resultDataUrl: string,
  settings: InpaintSettings
): Promise<ChapterSnapshot> {
  const layers = await normalizeInpaintLayerDataUrls(maskDataUrl, resultDataUrl);
  return saveNormalizedInpaintLayers(chapterId, pageId, layers.maskDataUrl, layers.resultDataUrl, settings);
}

export async function saveNormalizedInpaintLayers(
  chapterId: string,
  pageId: string,
  maskDataUrl: string,
  resultDataUrl: string,
  settings?: InpaintSettings
): Promise<ChapterSnapshot> {
  const locator = await findChapterLocation(chapterId);
  if (!locator) {
    throw new Error("화를 찾지 못했습니다.");
  }
  const chapter = await readChapterFile(locator.workId, locator.chapterId);
  if (!chapter) {
    throw new Error("화를 찾지 못했습니다.");
  }
  const existingPage = chapter.pages.find((page) => page.id === pageId);
  if (!existingPage) {
    throw new Error("페이지를 찾지 못했습니다.");
  }

  const now = new Date().toISOString();
  const inpaintDir = join(WORKS_ROOT, locator.workId, "chapters", locator.chapterId, "inpaint");
  await mkdir(inpaintDir, { recursive: true });
  const maskPath = join(inpaintDir, `${sanitizeFileBasename(pageId, pageId)}-mask.png`);
  const resultPath = join(inpaintDir, `${sanitizeFileBasename(pageId, pageId)}-result.png`);
  await writeFile(maskPath, dataUrlToBuffer(maskDataUrl));
  await writeFile(resultPath, dataUrlToBuffer(resultDataUrl));

  chapter.pages = chapter.pages.map((page) => {
    if (page.id !== pageId) {
      return page;
    }
    return {
      ...page,
      inpaintMaskPath: maskPath,
      inpaintResultPath: resultPath,
      inpaintMaskDataUrl: undefined,
      inpaintResultDataUrl: undefined,
      inpaintStatus: "completed",
      inpaintSettings: settings ?? page.inpaintSettings,
      updatedAt: now
    };
  });

  chapter.updatedAt = now;
  await writeChapterFile(chapter);
  await touchWork(locator.workId, now);
  return hydrateChapter(chapter);
}

export async function saveInpaintMask(chapterId: string, pageId: string, maskDataUrl: string | undefined): Promise<ChapterSnapshot> {
  const locator = await findChapterLocation(chapterId);
  if (!locator) {
    throw new Error("화를 찾지 못했습니다.");
  }
  const chapter = await readChapterFile(locator.workId, locator.chapterId);
  if (!chapter) {
    throw new Error("화를 찾지 못했습니다.");
  }

  const page = chapter.pages.find((candidate) => candidate.id === pageId);
  if (!page) {
    throw new Error("페이지를 찾지 못했습니다.");
  }

  const now = new Date().toISOString();
  const inpaintDir = join(WORKS_ROOT, locator.workId, "chapters", locator.chapterId, "inpaint");
  const maskPath = join(inpaintDir, `${sanitizeFileBasename(pageId, pageId)}-mask.png`);

  if (maskDataUrl) {
    await mkdir(inpaintDir, { recursive: true });
    await writeFile(maskPath, dataUrlToBuffer(maskDataUrl));
  } else {
    await safeUnlink(page.inpaintMaskPath ?? maskPath);
    if (page.inpaintResultPath) {
      await safeUnlink(page.inpaintResultPath);
    }
  }

  chapter.pages = chapter.pages.map((candidate) =>
    candidate.id === pageId
      ? {
          ...candidate,
          inpaintMaskPath: maskDataUrl ? maskPath : undefined,
          inpaintResultPath: maskDataUrl ? candidate.inpaintResultPath : undefined,
          inpaintMaskDataUrl: undefined,
          inpaintResultDataUrl: undefined,
          inpaintStatus: "idle",
          updatedAt: now
        }
      : candidate
  );
  chapter.updatedAt = now;
  await writeChapterFile(chapter);
  await touchWork(locator.workId, now);
  return hydrateChapter(chapter);
}

export async function saveInpaintResultLayer(chapterId: string, pageId: string, resultDataUrl: string | undefined): Promise<ChapterSnapshot> {
  const locator = await findChapterLocation(chapterId);
  if (!locator) {
    throw new Error("화를 찾지 못했습니다.");
  }
  const chapter = await readChapterFile(locator.workId, locator.chapterId);
  if (!chapter) {
    throw new Error("화를 찾지 못했습니다.");
  }

  const page = chapter.pages.find((candidate) => candidate.id === pageId);
  if (!page) {
    throw new Error("페이지를 찾지 못했습니다.");
  }

  const now = new Date().toISOString();
  const inpaintDir = join(WORKS_ROOT, locator.workId, "chapters", locator.chapterId, "inpaint");
  const resultPath = join(inpaintDir, `${sanitizeFileBasename(pageId, pageId)}-result.png`);

  if (resultDataUrl) {
    await mkdir(inpaintDir, { recursive: true });
    await writeFile(resultPath, dataUrlToBuffer(resultDataUrl));
  } else {
    await safeUnlink(page.inpaintResultPath ?? resultPath);
  }

  chapter.pages = chapter.pages.map((candidate) =>
    candidate.id === pageId
      ? {
          ...candidate,
          inpaintResultPath: resultDataUrl ? resultPath : undefined,
          inpaintResultDataUrl: undefined,
          inpaintStatus: resultDataUrl ? "completed" : "idle",
          updatedAt: now
        }
      : candidate
  );
  chapter.updatedAt = now;
  await writeChapterFile(chapter);
  await touchWork(locator.workId, now);
  return hydrateChapter(chapter);
}

export async function saveImportedInpaintLayers(chapterId: string, pageId: string, maskDataUrl: string, resultDataUrl: string): Promise<ChapterSnapshot> {
  const layers = await normalizeInpaintLayerDataUrls(maskDataUrl, resultDataUrl);
  return saveNormalizedInpaintLayers(chapterId, pageId, layers.maskDataUrl, layers.resultDataUrl);
}

export async function getInpaintPsdImportPath(chapterId: string, pageId: string): Promise<string> {
  const locator = await findChapterLocation(chapterId);
  if (!locator) {
    throw new Error("화를 찾지 못했습니다.");
  }
  const chapter = await readChapterFile(locator.workId, locator.chapterId);
  if (!chapter) {
    throw new Error("화를 찾지 못했습니다.");
  }
  if (!chapter.pages.some((candidate) => candidate.id === pageId)) {
    throw new Error("페이지를 찾지 못했습니다.");
  }
  const inpaintDir = join(WORKS_ROOT, locator.workId, "chapters", locator.chapterId, "inpaint");
  await mkdir(inpaintDir, { recursive: true });
  return join(inpaintDir, `${sanitizeFileBasename(pageId, pageId)}-last-import.psd`);
}

export async function removePageArtifacts(workId: string, chapterId: string, pageId: string): Promise<void> {
  const inpaintRoot = join(WORKS_ROOT, workId, "chapters", chapterId, "inpaint");
  if (existsSync(inpaintRoot)) {
    const entries = await readdir(inpaintRoot, { withFileTypes: true });
    await Promise.all(
      entries
        .filter((entry) => entry.isFile() && entry.name.startsWith(pageId))
        .map((entry) => safeUnlink(join(inpaintRoot, entry.name)))
    );
  }

  const runsRoot = join(WORKS_ROOT, workId, "chapters", chapterId, "runs");
  if (!existsSync(runsRoot)) {
    return;
  }

  const runs = await readdir(runsRoot, { withFileTypes: true });
  for (const run of runs) {
    if (!run.isDirectory()) {
      continue;
    }
    const target = join(runsRoot, run.name, "pages", pageId);
    if (!existsSync(target)) {
      continue;
    }
    await rm(target, { recursive: true, force: true });
  }
}
