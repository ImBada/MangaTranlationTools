import { existsSync } from "node:fs";
import { rm } from "node:fs/promises";
import { join, resolve } from "node:path";
import type {
  ChapterSnapshot,
  LibraryChapterSummary,
  LibraryIndex,
  LibraryPageRecord,
  LibraryWorkSummary,
  MangaPage,
  SaveChapterPatchRequest
} from "../shared/types";
import { filterAnalysisTargetsForRun } from "../shared/analysisTargets";
import { applyBlockTypeFontPresetToBlock, ensureBlockTypeFontPresets } from "../shared/fontPresets";
import { getAppPaths } from "./appPaths";
import { safeUnlink } from "./libraryFileIO";
import {
  hydrateChapter,
  hydratePageDataUrls,
  maxIsoTimestamp,
  mergeChapterSnapshotForSave,
  normalizeStoredBlock,
  toStoredChapter,
  toStoredPagePatch
} from "./libraryHydration";
import { reorderIds, reorderRecords, resolveChapterStatus, toChapterSummary } from "./libraryOrdering";
import { WORKS_ROOT } from "./libraryPaths";
import { removePageArtifacts } from "./libraryAssets";
import {
  DEFAULT_WORK_TITLE,
  enqueueChapterMutation,
  findChapterLocation,
  makeUniqueChapterTitle,
  mutateExistingChapterFile,
  readChapterFile,
  readIndexFile,
  readWorkFile,
  sanitizeTitle,
  touchWork,
  writeChapterFile,
  writeIndexFile,
  writeWorkFile
} from "./libraryStore";

export type ChapterRunPaths = {
  chapterDir: string;
  runDir: string;
};

type SaveChapterSnapshotOptions = {
  dirtyPageIds?: string[];
};

export async function listLibrary(): Promise<LibraryIndex> {
  const index = await readIndexFile();
  const works: LibraryWorkSummary[] = [];

  for (const workId of index.workOrder) {
    const work = await readWorkFile(workId);
    if (!work) {
      continue;
    }
    const chapters: LibraryChapterSummary[] = [];
    for (const chapterId of work.chapterOrder) {
      const chapter = await readChapterFile(workId, chapterId);
      if (!chapter) {
        continue;
      }
      chapters.push(toChapterSummary(chapter));
    }
    works.push({ ...work, chapters });
  }

  return {
    workOrder: works.map((work) => work.id),
    works
  };
}

export async function openChapter(chapterId: string): Promise<ChapterSnapshot> {
  const locator = await findChapterLocation(chapterId);
  if (!locator) {
    throw new Error("열려는 화를 찾지 못했습니다.");
  }
  const chapter = await readChapterFile(locator.workId, locator.chapterId);
  if (!chapter) {
    throw new Error("열려는 화를 찾지 못했습니다.");
  }
  return hydrateChapter(chapter);
}

export async function saveChapterSnapshot(snapshot: ChapterSnapshot, options: SaveChapterSnapshotOptions = {}): Promise<ChapterSnapshot> {
  const saved = await enqueueChapterMutation(snapshot.id, async () => {
    const current = await readChapterFile(snapshot.workId, snapshot.id);
    const stored = toStoredChapter(snapshot);
    const next = current ? mergeChapterSnapshotForSave(stored, current, options.dirtyPageIds) : stored;
    await writeChapterFile(next);
    return next;
  });
  return hydrateChapter(saved);
}

export async function patchChapterSnapshot(chapterId: string, request: SaveChapterPatchRequest): Promise<ChapterSnapshot> {
  const saved = await mutateExistingChapterFile(chapterId, (current) => {
    if (request.chapter.id && request.chapter.id !== current.id) {
      throw new Error("저장할 화 정보가 일치하지 않습니다.");
    }
    if (request.chapter.workId && request.chapter.workId !== current.workId) {
      throw new Error("저장할 작품 정보가 일치하지 않습니다.");
    }

    const pagePatches = new Map((request.pages ?? []).map((page) => [page.id, toStoredPagePatch(page)]));
    const pages = current.pages.map((page) => {
      const patch = pagePatches.get(page.id);
      if (!patch) {
        return page;
      }
      return {
        ...page,
        ...patch,
        id: page.id,
        imagePath: page.imagePath,
        createdAt: page.createdAt,
        blocks: patch.blocks ? patch.blocks.map(normalizeStoredBlock) : page.blocks
      };
    });
    const pageOrder = request.chapter.pageOrder ? reorderIds(current.pageOrder, request.chapter.pageOrder) : current.pageOrder;
    const updatedAt = maxIsoTimestamp(
      request.chapter.updatedAt ?? current.updatedAt,
      current.updatedAt,
      ...pages.map((page) => page.updatedAt)
    );

    return {
      ...current,
      title: request.chapter.title ?? current.title,
      fontPresets: request.chapter.fontPresets ?? current.fontPresets,
      pageOrder,
      pages: reorderRecords(pages, pageOrder),
      status: resolveChapterStatus(pages),
      updatedAt
    };
  });
  return hydrateChapter(saved);
}

export async function renameWork(workId: string, title: string): Promise<LibraryIndex> {
  const work = await readWorkFile(workId);
  if (!work) {
    throw new Error("작품을 찾지 못했습니다.");
  }
  work.title = sanitizeTitle(title, DEFAULT_WORK_TITLE);
  work.updatedAt = new Date().toISOString();
  await writeWorkFile(work);
  return listLibrary();
}

export async function renameChapter(chapterId: string, title: string): Promise<LibraryIndex> {
  const locator = await findChapterLocation(chapterId);
  if (!locator) {
    throw new Error("화를 찾지 못했습니다.");
  }
  const chapter = await readChapterFile(locator.workId, locator.chapterId);
  if (!chapter) {
    throw new Error("화를 찾지 못했습니다.");
  }
  chapter.title = await makeUniqueChapterTitle(locator.workId, sanitizeTitle(title, "제목없음"), chapter.id);
  chapter.updatedAt = new Date().toISOString();
  await writeChapterFile(chapter);
  await touchWork(locator.workId, chapter.updatedAt);
  return listLibrary();
}

export async function deleteWork(workId: string): Promise<LibraryIndex> {
  const work = await readWorkFile(workId);
  if (!work) {
    throw new Error("작품을 찾지 못했습니다.");
  }

  const index = await readIndexFile();
  index.workOrder = index.workOrder.filter((id) => id !== workId);
  await writeIndexFile(index);

  const workDir = join(WORKS_ROOT, workId);
  if (existsSync(workDir)) {
    await rm(workDir, { recursive: true, force: true });
  }

  return listLibrary();
}

export async function deleteChapter(chapterId: string): Promise<LibraryIndex> {
  const locator = await findChapterLocation(chapterId);
  if (!locator) {
    throw new Error("화를 찾지 못했습니다.");
  }

  const work = await readWorkFile(locator.workId);
  if (!work) {
    throw new Error("작품을 찾지 못했습니다.");
  }

  const chapter = await readChapterFile(locator.workId, locator.chapterId);
  if (!chapter) {
    throw new Error("화를 찾지 못했습니다.");
  }

  work.chapterOrder = work.chapterOrder.filter((id) => id !== chapter.id);
  work.updatedAt = new Date().toISOString();
  await writeWorkFile(work);

  const chapterDir = join(WORKS_ROOT, locator.workId, "chapters", locator.chapterId);
  if (existsSync(chapterDir)) {
    await rm(chapterDir, { recursive: true, force: true });
  }

  return listLibrary();
}

export async function reorderChapters(workId: string, chapterIds: string[]): Promise<LibraryIndex> {
  const work = await readWorkFile(workId);
  if (!work) {
    throw new Error("작품을 찾지 못했습니다.");
  }
  work.chapterOrder = reorderIds(work.chapterOrder, chapterIds);
  work.updatedAt = new Date().toISOString();
  await writeWorkFile(work);
  return listLibrary();
}

export async function reorderPages(chapterId: string, pageIds: string[]): Promise<ChapterSnapshot> {
  const locator = await findChapterLocation(chapterId);
  if (!locator) {
    throw new Error("화를 찾지 못했습니다.");
  }
  const chapter = await readChapterFile(locator.workId, locator.chapterId);
  if (!chapter) {
    throw new Error("화를 찾지 못했습니다.");
  }
  chapter.pageOrder = reorderIds(chapter.pageOrder, pageIds);
  chapter.pages = reorderRecords(chapter.pages, chapter.pageOrder);
  chapter.updatedAt = new Date().toISOString();
  chapter.status = resolveChapterStatus(chapter.pages);
  await writeChapterFile(chapter);
  await touchWork(locator.workId, chapter.updatedAt);
  return hydrateChapter(chapter);
}

export async function deletePage(chapterId: string, pageId: string): Promise<ChapterSnapshot> {
  const locator = await findChapterLocation(chapterId);
  if (!locator) {
    throw new Error("화를 찾지 못했습니다.");
  }
  const chapter = await readChapterFile(locator.workId, locator.chapterId);
  if (!chapter) {
    throw new Error("화를 찾지 못했습니다.");
  }

  const target = chapter.pages.find((page) => page.id === pageId);
  if (!target) {
    return hydrateChapter(chapter);
  }

  chapter.pageOrder = chapter.pageOrder.filter((id) => id !== pageId);
  chapter.pages = chapter.pages.filter((page) => page.id !== pageId);
  chapter.updatedAt = new Date().toISOString();
  chapter.status = resolveChapterStatus(chapter.pages);

  await writeChapterFile(chapter);
  await touchWork(locator.workId, chapter.updatedAt);

  await safeUnlink(target.imagePath);
  await removePageArtifacts(locator.workId, locator.chapterId, pageId);

  return hydrateChapter(chapter);
}

export async function markChapterPagesRunning(chapterId: string, pageIds: string[]): Promise<ChapterSnapshot> {
  const chapter = await mutateExistingChapterFile(chapterId, (chapter) => {
    const now = new Date().toISOString();
    chapter.pages = chapter.pages.map((page) =>
      pageIds.includes(page.id)
        ? {
            ...page,
            analysisStatus: "running",
            lastError: undefined,
            updatedAt: now
          }
        : page
    );
    chapter.status = resolveChapterStatus(chapter.pages);
    chapter.updatedAt = now;
    return chapter;
  });
  return hydrateChapter(chapter);
}

export async function updatePageAfterAnalysis(chapterId: string, page: MangaPage, warnings: string[], status: "completed" | "failed"): Promise<void> {
  await mutateExistingChapterFile(chapterId, (chapter) => {
    const now = new Date().toISOString();
    const fontPresets = ensureBlockTypeFontPresets(chapter.fontPresets);
    const blocks = applyAnalysisFontPresetsToBlocks(page.blocks, fontPresets);
    chapter.pages = chapter.pages.map((record) =>
      record.id === page.id
        ? {
            ...record,
            blocks,
            analysisStatus: status,
            lastError: status === "failed" ? warnings[warnings.length - 1] : undefined,
            updatedAt: now
          }
        : record
    );
    chapter.fontPresets = fontPresets;
    chapter.updatedAt = now;
    chapter.status = resolveChapterStatus(chapter.pages);
    return chapter;
  }).catch(() => undefined);
}

export async function finalizeRunningPages(
  chapterId: string,
  pageIds: string[],
  status: "idle" | "failed",
  errorMessage?: string
): Promise<void> {
  await mutateExistingChapterFile(chapterId, (chapter) => {
    const now = new Date().toISOString();
    chapter.pages = chapter.pages.map((page) =>
      pageIds.includes(page.id) && page.analysisStatus === "running"
        ? {
            ...page,
            analysisStatus: status,
            lastError: status === "failed" ? errorMessage : undefined,
            updatedAt: now
          }
        : page
    );
    chapter.updatedAt = now;
    chapter.status = resolveChapterStatus(chapter.pages);
    return chapter;
  }).catch(() => undefined);
}

export async function updatePagesAfterAnalysis(chapterId: string, pages: MangaPage[]): Promise<ChapterSnapshot> {
  const pageMap = new Map(pages.map((page) => [page.id, page]));
  const chapter = await mutateExistingChapterFile(chapterId, (chapter) => {
    const now = new Date().toISOString();
    const fontPresets = ensureBlockTypeFontPresets(chapter.fontPresets);
    chapter.pages = chapter.pages.map((record) => {
      const next = pageMap.get(record.id);
      if (!next) {
        return record;
      }
      return {
        ...record,
        blocks: applyAnalysisFontPresetsToBlocks(next.blocks, fontPresets),
        analysisStatus: next.analysisStatus,
        lastError: next.lastError,
        updatedAt: now
      };
    });
    chapter.fontPresets = fontPresets;
    chapter.updatedAt = now;
    chapter.status = resolveChapterStatus(chapter.pages);
    return chapter;
  });
  return hydrateChapter(chapter);
}

function applyAnalysisFontPresetsToBlocks(blocks: MangaPage["blocks"], fontPresets: ReturnType<typeof ensureBlockTypeFontPresets>): MangaPage["blocks"] {
  return blocks.map((block) => applyBlockTypeFontPresetToBlock(block, fontPresets));
}

export async function resolvePagesForRun(chapterId: string, runMode: "pending" | "all" | "single-page", pageId?: string): Promise<{
  chapter: ChapterSnapshot;
  pages: MangaPage[];
}> {
  const locator = await findChapterLocation(chapterId);
  if (!locator) {
    throw new Error("화를 찾지 못했습니다.");
  }
  const chapterFile = await readChapterFile(locator.workId, locator.chapterId);
  if (!chapterFile) {
    throw new Error("화를 찾지 못했습니다.");
  }

  const orderedPages = reorderRecords(chapterFile.pages, chapterFile.pageOrder);
  const selectedRecords = filterAnalysisTargetsForRun(orderedPages, runMode, pageId);
  const pages = await Promise.all(selectedRecords.map((page: LibraryPageRecord) => hydratePageDataUrls(chapterFile.id, page)));

  return {
    chapter: await hydrateChapter(chapterFile),
    pages
  };
}

export function getRunPaths(chapterId: string, runId: string): Promise<ChapterRunPaths> {
  return (async () => {
    const locator = await findChapterLocation(chapterId);
    if (!locator) {
      throw new Error("화를 찾지 못했습니다.");
    }
    const chapterDir = join(WORKS_ROOT, locator.workId, "chapters", locator.chapterId);
    const runDir = join(chapterDir, "runs", runId);
    return { chapterDir, runDir };
  })();
}

export async function cleanupLegacyLogs(): Promise<void> {
  const logsRoot = resolve(getAppPaths().logsDir);
  const targets = [
    join(logsRoot, "app-jobs"),
    join(logsRoot, "bench"),
    join(logsRoot, "debug"),
    join(logsRoot, "runtime")
  ];

  for (const target of targets) {
    if (!existsSync(target)) {
      continue;
    }
    const resolved = resolve(target);
    if (!resolved.startsWith(logsRoot)) {
      continue;
    }
    await rm(resolved, { recursive: true, force: true });
  }
}
