import { randomUUID } from "node:crypto";
import {
  copyFile,
  mkdir,
  readFile,
  readdir,
  rename,
  rm,
  stat,
  unlink,
  writeFile
} from "node:fs/promises";
import { existsSync } from "node:fs";
import { basename, dirname, extname, join, relative, resolve } from "node:path";
import sharp from "sharp";
import type {
  ChapterSnapshot,
  CreateImportRequest,
  CreateImportResult,
  ImportChapterDraft,
  ImportPageDraft,
  ImportPreviewResult,
  ImportSourceKind,
  InpaintSettings,
  LibraryChapter,
  LibraryChapterSummary,
  LibraryIndex,
  LibraryPageRecord,
  LibraryWork,
  LibraryWorkSummary,
  MangaPage,
  RenderPageResult
} from "../shared/types";
import { normalizeRenderDirection } from "../shared/geometry";
import { getAppPaths } from "./appPaths";
import { readImageDimensions } from "./imageDimensions";

type ZipEntryLike = {
  entryName: string;
  isDirectory: boolean;
  getData: () => Buffer;
};

type AdmZipLike = {
  getEntries: () => ZipEntryLike[];
};

const LIBRARY_ROOT = getAppPaths().libraryDir;
const INDEX_PATH = join(LIBRARY_ROOT, "index.json");
const WORKS_ROOT = join(LIBRARY_ROOT, "works");
const DEFAULT_WORK_TITLE = "미정 작품";

const AdmZip = require("adm-zip") as {
  new (archivePath: string): AdmZipLike;
};

const chapterWriteQueues = new Map<string, Promise<void>>();

type StoredIndexFile = {
  workOrder: string[];
};

type WorkFile = LibraryWork;

type ChapterFile = LibraryChapter;

type SaveChapterSnapshotOptions = {
  dirtyPageIds?: string[];
};

export type ChapterRunPaths = {
  chapterDir: string;
  runDir: string;
};

export type UploadedImportFile = {
  path: string;
  name: string;
  relativePath?: string;
};

export function getLibraryRoot(): string {
  return LIBRARY_ROOT;
}

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
  const outputPath = join(outputDir, `${sanitizeFileBasename(page.name, page.id)}-${page.id}.png`);
  await writeFile(outputPath, buffer);
  return { outputPath };
}

export async function saveInpaintResult(
  chapterId: string,
  pageId: string,
  maskDataUrl: string,
  resultDataUrl: string,
  settings: InpaintSettings
): Promise<ChapterSnapshot> {
  const locator = await findChapterLocation(chapterId);
  if (!locator) {
    throw new Error("화를 찾지 못했습니다.");
  }
  const chapter = await readChapterFile(locator.workId, locator.chapterId);
  if (!chapter) {
    throw new Error("화를 찾지 못했습니다.");
  }

  const now = new Date().toISOString();
  const inpaintDir = join(WORKS_ROOT, locator.workId, "chapters", locator.chapterId, "inpaint");
  await mkdir(inpaintDir, { recursive: true });
  const maskPath = join(inpaintDir, `${sanitizeFileBasename(pageId, pageId)}-mask.png`);
  const resultPath = join(inpaintDir, `${sanitizeFileBasename(pageId, pageId)}-result.png`);
  const normalizedResultDataUrl = await clipOpaqueInpaintResultToMask(resultDataUrl, maskDataUrl);
  await writeFile(maskPath, dataUrlToBuffer(maskDataUrl));
  await writeFile(resultPath, dataUrlToBuffer(normalizedResultDataUrl));

  let found = false;
  chapter.pages = chapter.pages.map((page) => {
    if (page.id !== pageId) {
      return page;
    }
    found = true;
    return {
      ...page,
      inpaintMaskPath: maskPath,
      inpaintResultPath: resultPath,
      inpaintMaskDataUrl: undefined,
      inpaintResultDataUrl: undefined,
      inpaintStatus: "completed",
      inpaintSettings: settings,
      updatedAt: now
    };
  });
  if (!found) {
    throw new Error("페이지를 찾지 못했습니다.");
  }

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

export async function previewImages(filePaths: string[]): Promise<ImportPreviewResult> {
  const normalized = sortNaturally(filePaths.filter((filePath) => isSupportedImagePath(filePath)));
  const pages = normalized.map((filePath) => ({
    name: basename(filePath),
    sourceKind: "file" as const,
    sourcePath: filePath
  }));

  return {
    mode: "single",
    sourceKind: "images",
    suggestedWorkTitle: DEFAULT_WORK_TITLE,
    chapters: [
      {
        draftId: randomUUID(),
        title: "제목없음",
        sourceKind: "images",
        pages
      }
    ]
  };
}

export async function previewFolder(folderPath: string): Promise<ImportPreviewResult> {
  const filePaths = await listImageFiles(folderPath);
  return {
    mode: "single",
    sourceKind: "folder",
    suggestedWorkTitle: DEFAULT_WORK_TITLE,
    chapters: [
      {
        draftId: randomUUID(),
        title: basename(folderPath),
        sourceKind: "folder",
        pages: filePaths.map((filePath) => ({
          name: basename(filePath),
          sourceKind: "file" as const,
          sourcePath: filePath
        }))
      }
    ]
  };
}

export async function previewZip(zipPath: string): Promise<ImportPreviewResult> {
  const pages = listImageEntriesInZip(zipPath).map((entry) => ({
    name: normalizeImportPageName(entry.entryName),
    sourceKind: "zip-entry" as const,
    sourcePath: zipPath,
    zipEntryName: entry.entryName
  }));

  return {
    mode: "single",
    sourceKind: "zip",
    suggestedWorkTitle: DEFAULT_WORK_TITLE,
    chapters: [
      {
        draftId: randomUUID(),
        title: basename(zipPath, extname(zipPath)),
        sourceKind: "zip",
        pages
      }
    ]
  };
}

export async function previewZipFolder(folderPath: string): Promise<ImportPreviewResult> {
  const entries = await readdir(folderPath, { withFileTypes: true });
  const zipPaths = sortNaturally(
    entries.filter((entry) => entry.isFile() && extname(entry.name).toLowerCase() === ".zip").map((entry) => join(folderPath, entry.name))
  );
  const imageFolderPaths = await listNestedImageFolders(folderPath);
  const chapters = [
    ...zipPaths.map((zipPath) => ({
      sortKey: relative(folderPath, zipPath),
      chapter: {
        draftId: randomUUID(),
        title: basename(zipPath, extname(zipPath)),
        sourceKind: "zip-folder" as const,
        pages: listImageEntriesInZip(zipPath).map((entry) => ({
          name: normalizeImportPageName(entry.entryName),
          sourceKind: "zip-entry" as const,
          sourcePath: zipPath,
          zipEntryName: entry.entryName
        }))
      }
    })),
    ...(await Promise.all(
      imageFolderPaths.map(async (imageFolderPath) => ({
        sortKey: relative(folderPath, imageFolderPath),
        chapter: {
          draftId: randomUUID(),
          title: normalizeImportPageName(relative(folderPath, imageFolderPath)) || basename(imageFolderPath),
          sourceKind: "folder" as const,
          pages: (await listImageFiles(imageFolderPath)).map((filePath) => ({
            name: basename(filePath),
            sourceKind: "file" as const,
            sourcePath: filePath
          }))
        }
      }))
    ))
  ]
    .sort((left, right) => left.sortKey.localeCompare(right.sortKey, undefined, { numeric: true, sensitivity: "base" }))
    .map(({ chapter }) => chapter);

  return {
    mode: "batch",
    sourceKind: "zip-folder",
    suggestedWorkTitle: basename(folderPath),
    chapters
  };
}

export async function previewUploadedFiles(kind: ImportSourceKind, files: UploadedImportFile[]): Promise<ImportPreviewResult> {
  const normalized = files
    .map((file) => ({
      ...file,
      relativePath: normalizePathSeparators(file.relativePath || file.name),
      name: basename(file.name)
    }))
    .sort((left, right) => (left.relativePath || left.name).localeCompare(right.relativePath || right.name, undefined, { numeric: true, sensitivity: "base" }));

  if (kind === "images") {
    return previewUploadedImageChapter("images", DEFAULT_WORK_TITLE, "제목없음", normalized.filter((file) => isSupportedImagePath(file.name)));
  }

  if (kind === "folder") {
    const imageFiles = normalized.filter((file) => isSupportedImagePath(file.name));
    const title = firstPathSegment(imageFiles[0]?.relativePath) || DEFAULT_WORK_TITLE;
    return previewUploadedImageChapter("folder", DEFAULT_WORK_TITLE, title, imageFiles);
  }

  if (kind === "zip") {
    const zipFile = normalized.find((file) => isZipPath(file.name));
    if (!zipFile) {
      return emptyPreview("zip", "single", DEFAULT_WORK_TITLE);
    }
    const preview = await previewZip(zipFile.path);
    return {
      ...preview,
      chapters: preview.chapters.map((chapter) => ({
        ...chapter,
        pages: chapter.pages.map((page) => ({ ...page, sourcePath: zipFile.path }))
      }))
    };
  }

  const chapters: ImportChapterDraft[] = [];
  for (const zipFile of normalized.filter((file) => isZipPath(file.name))) {
    const preview = await previewZip(zipFile.path);
    chapters.push(
      ...preview.chapters.map((chapter) => ({
        ...chapter,
        sourceKind: "zip-folder" as const,
        pages: chapter.pages.map((page) => ({ ...page, sourceKind: "zip-entry" as const, sourcePath: zipFile.path }))
      }))
    );
  }

  const groups = new Map<string, UploadedImportFile[]>();
  for (const imageFile of normalized.filter((file) => isSupportedImagePath(file.name))) {
    const groupKey = dirname(imageFile.relativePath || imageFile.name);
    const key = groupKey === "." ? "images" : groupKey;
    groups.set(key, [...(groups.get(key) ?? []), imageFile]);
  }

  for (const [groupName, groupFiles] of groups) {
    chapters.push({
      draftId: randomUUID(),
      title: normalizeImportPageName(groupName),
      sourceKind: "folder",
      pages: groupFiles.map(uploadedFileToPageDraft)
    });
  }

  return {
    mode: "batch",
    sourceKind: "zip-folder",
    suggestedWorkTitle: firstPathSegment(normalized[0]?.relativePath) || DEFAULT_WORK_TITLE,
    chapters
  };
}

export async function createImport(request: CreateImportRequest): Promise<CreateImportResult> {
  const target = request.target.mode === "new" ? await createWork(request.target.title || request.preview.suggestedWorkTitle) : await ensureExistingWork(request.target.workId);
  const selections = new Map(request.selections.map((selection) => [selection.draftId, selection]));
  const createdChapterIds: string[] = [];
  let openedChapter: ChapterSnapshot | undefined;

  for (const draft of request.preview.chapters) {
    const selection = selections.get(draft.draftId);
    if (!selection?.enabled) {
      continue;
    }

    const chapter = await createChapterFromDraft(target.id, draft, selection.title);
    createdChapterIds.push(chapter.id);
    if (!openedChapter) {
      openedChapter = await hydrateChapter(chapter);
    }
  }

  if (createdChapterIds.length === 0) {
    throw new Error("생성할 화가 없습니다.");
  }

  return {
    workId: target.id,
    chapterIds: createdChapterIds,
    openedChapter
  };
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
    chapter.pages = chapter.pages.map((record) =>
      record.id === page.id
        ? {
            ...record,
            blocks: page.blocks,
            analysisStatus: status,
            lastError: status === "failed" ? warnings[warnings.length - 1] : undefined,
            updatedAt: now
          }
        : record
    );
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
    chapter.pages = chapter.pages.map((record) => {
      const next = pageMap.get(record.id);
      if (!next) {
        return record;
      }
      return {
        ...record,
        blocks: next.blocks,
        analysisStatus: next.analysisStatus,
        lastError: next.lastError,
        updatedAt: now
      };
    });
    chapter.updatedAt = now;
    chapter.status = resolveChapterStatus(chapter.pages);
    return chapter;
  });
  return hydrateChapter(chapter);
}

export async function resolvePagesForRun(chapterId: string, runMode: "pending" | "all" | "single-page", pageId?: string): Promise<{
  chapter: ChapterSnapshot;
  pages: MangaPage[];
}> {
  const chapter = await openChapter(chapterId);
  const pages =
    runMode === "all"
      ? chapter.pages
      : runMode === "single-page"
        ? chapter.pages.filter((page) => page.id === pageId)
        : chapter.pages.filter((page) => page.analysisStatus !== "completed");

  return {
    chapter,
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

export function isZipPath(path: string): boolean {
  return extname(path).toLowerCase() === ".zip";
}

async function createWork(title: string): Promise<LibraryWork> {
  const now = new Date().toISOString();
  const work: LibraryWork = {
    id: randomUUID(),
    title: sanitizeTitle(title, DEFAULT_WORK_TITLE),
    chapterOrder: [],
    createdAt: now,
    updatedAt: now
  };
  const index = await readIndexFile();
  index.workOrder.push(work.id);
  await writeIndexFile(index);
  await writeWorkFile(work);
  return work;
}

async function ensureExistingWork(workId: string): Promise<LibraryWork> {
  const work = await readWorkFile(workId);
  if (!work) {
    throw new Error("선택한 작품을 찾지 못했습니다.");
  }
  return work;
}

async function createChapterFromDraft(workId: string, draft: ImportChapterDraft, requestedTitle: string): Promise<LibraryChapter> {
  const work = await ensureExistingWork(workId);
  const now = new Date().toISOString();
  const chapterId = randomUUID();
  const title = await makeUniqueChapterTitle(workId, sanitizeTitle(requestedTitle || draft.title, "제목없음"));
  const chapterDir = join(WORKS_ROOT, workId, "chapters", chapterId);
  const pagesDir = join(chapterDir, "pages");
  await mkdir(pagesDir, { recursive: true });

  const pages: LibraryPageRecord[] = [];
  for (const [index, pageDraft] of draft.pages.entries()) {
    pages.push(await materializePageRecord(pageDraft, pagesDir, index));
  }

  const chapter: LibraryChapter = {
    id: chapterId,
    workId,
    title,
    sourceKind: draft.sourceKind,
    status: resolveChapterStatus(pages),
    pageOrder: pages.map((page) => page.id),
    pages,
    createdAt: now,
    updatedAt: now
  };

  work.chapterOrder = [...work.chapterOrder, chapterId];
  work.updatedAt = now;
  await writeWorkFile(work);
  await writeChapterFile(chapter);
  return chapter;
}

async function materializePageRecord(pageDraft: ImportPageDraft, pagesDir: string, index: number): Promise<LibraryPageRecord> {
  const pageId = randomUUID();
  const targetExt =
    pageDraft.sourceKind === "zip-entry" ? extname(pageDraft.zipEntryName ?? "").toLowerCase() || ".png" : extname(pageDraft.sourcePath).toLowerCase() || ".png";
  const outputPath = join(pagesDir, `${String(index + 1).padStart(3, "0")}-${pageId}${targetExt}`);

  if (pageDraft.sourceKind === "zip-entry") {
    const zip = new AdmZip(pageDraft.sourcePath);
    const entry = zip.getEntries().find((candidate) => candidate.entryName === pageDraft.zipEntryName);
    if (!entry) {
      throw new Error(`ZIP 항목을 찾지 못했습니다: ${pageDraft.zipEntryName ?? pageDraft.sourcePath}`);
    }
    await writeFile(outputPath, entry.getData());
  } else {
    await copyFile(pageDraft.sourcePath, outputPath);
  }

  const size = await readImageDimensions(outputPath);
  const now = new Date().toISOString();

  return {
    id: pageId,
    name: pageDraft.name,
    imagePath: outputPath,
    width: size.width || 1000,
    height: size.height || 1400,
    blocks: [],
    analysisStatus: "idle",
    createdAt: now,
    updatedAt: now
  };
}

function previewUploadedImageChapter(
  sourceKind: ImportSourceKind,
  suggestedWorkTitle: string,
  title: string,
  files: UploadedImportFile[]
): ImportPreviewResult {
  return {
    mode: "single",
    sourceKind,
    suggestedWorkTitle,
    chapters: [
      {
        draftId: randomUUID(),
        title,
        sourceKind,
        pages: files.map(uploadedFileToPageDraft)
      }
    ]
  };
}

function uploadedFileToPageDraft(file: UploadedImportFile): ImportPageDraft {
  return {
    name: basename(file.relativePath || file.name),
    sourceKind: "file",
    sourcePath: file.path
  };
}

function emptyPreview(sourceKind: ImportSourceKind, mode: "single" | "batch", suggestedWorkTitle: string): ImportPreviewResult {
  return {
    mode,
    sourceKind,
    suggestedWorkTitle,
    chapters: []
  };
}

function normalizePathSeparators(value: string): string {
  return value.replace(/\\/g, "/").replace(/^\/+/, "");
}

function dataUrlToBuffer(dataUrl: string): Buffer {
  const match = /^data:image\/png;base64,(.+)$/u.exec(dataUrl);
  if (!match) {
    throw new Error("PNG 데이터 URL이 아닙니다.");
  }
  return Buffer.from(match[1], "base64");
}

async function clipOpaqueInpaintResultToMask(resultDataUrl: string, maskDataUrl: string): Promise<string> {
  const result = await sharp(dataUrlToBuffer(resultDataUrl)).ensureAlpha().raw().toBuffer({ resolveWithObject: true });
  const resultData = Buffer.from(result.data);
  let hasTransparentPixel = false;
  for (let offset = 3; offset < resultData.length; offset += 4) {
    if (resultData[offset] < 255) {
      hasTransparentPixel = true;
      break;
    }
  }
  if (hasTransparentPixel) {
    return resultDataUrl;
  }

  const mask = await sharp(dataUrlToBuffer(maskDataUrl))
    .ensureAlpha()
    .resize(result.info.width, result.info.height, { fit: "fill" })
    .raw()
    .toBuffer({ resolveWithObject: true });

  for (let offset = 0; offset < resultData.length; offset += 4) {
    const alpha = mask.data[offset + 3] / 255;
    const luma = (mask.data[offset] * 0.299 + mask.data[offset + 1] * 0.587 + mask.data[offset + 2] * 0.114) / 255;
    resultData[offset + 3] = Math.round(255 * alpha * luma);
  }

  const buffer = await sharp(resultData, {
    raw: {
      width: result.info.width,
      height: result.info.height,
      channels: 4
    }
  }).png().toBuffer();
  return `data:image/png;base64,${buffer.toString("base64")}`;
}

function sanitizeFileBasename(value: string, fallback: string): string {
  const base = basename(value, extname(value)).replace(/[<>:"/\\|?*\x00-\x1f]/g, "_").trim();
  return base || fallback;
}

function firstPathSegment(value: string | undefined): string {
  if (!value) {
    return "";
  }
  return normalizeImportPageName(normalizePathSeparators(value).split("/").filter(Boolean)[0] || "");
}

async function hydrateChapter(chapter: ChapterFile): Promise<ChapterSnapshot> {
  const pages = await Promise.all(
    reorderRecords(chapter.pages, chapter.pageOrder).map(async (page) => {
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
        inpaintStatus: page.inpaintStatus ?? (clippedInpaintResultDataUrl ? "completed" : "idle"),
        dataUrl: await fileToDataUrl(page.imagePath)
      };
    })
  );

  return {
    ...chapter,
    pageOrder: pages.map((page) => page.id),
    pages
  };
}

function toStoredChapter(snapshot: ChapterSnapshot): ChapterFile {
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

function normalizeStoredBlock(block: MangaPage["blocks"][number]): MangaPage["blocks"][number] {
  return {
    ...block,
    renderDirection: normalizeRenderDirection(block.renderDirection)
  };
}

function mergeChapterSnapshotForSave(incoming: ChapterFile, current: ChapterFile, dirtyPageIds?: string[]): ChapterFile {
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
    status: resolveChapterStatus(mergedPages),
    updatedAt: maxIsoTimestamp(incoming.updatedAt, current.updatedAt, ...mergedPages.map((page) => page.updatedAt)),
    pageOrder: mergedPages.map((page) => page.id),
    pages: mergedPages
  };
}

function maxIsoTimestamp(...values: string[]): string {
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

async function readIndexFile(): Promise<StoredIndexFile> {
  await ensureLibraryStructure();
  if (!existsSync(INDEX_PATH)) {
    return { workOrder: [] };
  }
  return readJsonFile<StoredIndexFile>(INDEX_PATH, { workOrder: [] });
}

async function writeIndexFile(index: StoredIndexFile): Promise<void> {
  await ensureLibraryStructure();
  await writeJsonFile(INDEX_PATH, index);
}

async function readWorkFile(workId: string): Promise<WorkFile | null> {
  const path = workFilePath(workId);
  if (!existsSync(path)) {
    return null;
  }
  return readJsonFile<WorkFile>(path);
}

async function writeWorkFile(work: WorkFile): Promise<void> {
  await mkdir(dirname(workFilePath(work.id)), { recursive: true });
  await writeJsonFile(workFilePath(work.id), work);
}

async function touchWork(workId: string, updatedAt: string): Promise<void> {
  const work = await readWorkFile(workId);
  if (!work) {
    return;
  }
  work.updatedAt = updatedAt;
  await writeWorkFile(work);
}

async function readChapterFile(workId: string, chapterId: string): Promise<ChapterFile | null> {
  const path = chapterFilePath(workId, chapterId);
  if (!existsSync(path)) {
    return null;
  }
  return readJsonFile<ChapterFile>(path);
}

async function writeChapterFile(chapter: ChapterFile): Promise<void> {
  await mkdir(dirname(chapterFilePath(chapter.workId, chapter.id)), { recursive: true });
  await writeJsonFile(chapterFilePath(chapter.workId, chapter.id), chapter);
}

async function enqueueChapterMutation<T>(chapterId: string, task: () => Promise<T>): Promise<T> {
  const previous = chapterWriteQueues.get(chapterId) ?? Promise.resolve();
  let release: Promise<void> | null = null;
  const run = previous.catch(() => undefined).then(task);
  release = run.then(
    () => undefined,
    () => undefined
  );
  chapterWriteQueues.set(chapterId, release);
  try {
    return await run;
  } finally {
    if (chapterWriteQueues.get(chapterId) === release) {
      chapterWriteQueues.delete(chapterId);
    }
  }
}

async function mutateExistingChapterFile(
  chapterId: string,
  mutator: (chapter: ChapterFile) => ChapterFile | Promise<ChapterFile>
): Promise<ChapterFile> {
  return enqueueChapterMutation(chapterId, async () => {
    const locator = await findChapterLocation(chapterId);
    if (!locator) {
      throw new Error("화를 찾지 못했습니다.");
    }
    const chapter = await readChapterFile(locator.workId, locator.chapterId);
    if (!chapter) {
      throw new Error("화를 찾지 못했습니다.");
    }
    const next = await mutator(chapter);
    await writeChapterFile(next);
    await touchWork(locator.workId, next.updatedAt);
    return next;
  });
}

async function findChapterLocation(chapterId: string): Promise<{ workId: string; chapterId: string } | null> {
  const index = await readIndexFile();
  for (const workId of index.workOrder) {
    const work = await readWorkFile(workId);
    if (!work) {
      continue;
    }
    if (work.chapterOrder.includes(chapterId)) {
      return { workId, chapterId };
    }
  }
  return null;
}

async function ensureLibraryStructure(): Promise<void> {
  await mkdir(WORKS_ROOT, { recursive: true });
}

async function makeUniqueChapterTitle(workId: string, desired: string, excludeChapterId?: string): Promise<string> {
  const work = await ensureExistingWork(workId);
  const used = new Set<string>();
  for (const chapterId of work.chapterOrder) {
    if (chapterId === excludeChapterId) {
      continue;
    }
    const chapter = await readChapterFile(workId, chapterId);
    if (chapter) {
      used.add(chapter.title);
    }
  }

  if (!used.has(desired)) {
    return desired;
  }

  let index = 1;
  while (used.has(`${desired} (${index})`)) {
    index += 1;
  }
  return `${desired} (${index})`;
}

function sanitizeTitle(title: string, fallback: string): string {
  const trimmed = title.trim();
  return trimmed || fallback;
}

async function listImageFiles(folderPath: string): Promise<string[]> {
  const entries = await readdir(folderPath, { withFileTypes: true });
  return sortNaturally(
    entries.filter((entry) => entry.isFile() && isSupportedImagePath(entry.name)).map((entry) => join(folderPath, entry.name))
  );
}

async function listNestedImageFolders(rootPath: string): Promise<string[]> {
  const found: string[] = [];

  async function walk(currentPath: string): Promise<void> {
    const entries = await readdir(currentPath, { withFileTypes: true });
    const childDirectories = sortNaturally(entries.filter((entry) => entry.isDirectory()).map((entry) => join(currentPath, entry.name)));

    if (currentPath !== rootPath && entries.some((entry) => entry.isFile() && isSupportedImagePath(entry.name))) {
      found.push(currentPath);
    }

    for (const childPath of childDirectories) {
      await walk(childPath);
    }
  }

  await walk(rootPath);
  return found;
}

function listImageEntriesInZip(zipPath: string): ZipEntryLike[] {
  const zip = new AdmZip(zipPath);
  return zip
    .getEntries()
    .filter((entry) => !entry.isDirectory && isSupportedImagePath(entry.entryName))
    .sort((left, right) => left.entryName.localeCompare(right.entryName, undefined, { numeric: true, sensitivity: "base" }));
}

function normalizeImportPageName(entryName: string): string {
  return entryName.replace(/\\/g, "/");
}

function isSupportedImagePath(filePath: string): boolean {
  return [".png", ".jpg", ".jpeg", ".webp"].includes(extname(filePath).toLowerCase());
}

async function fileToDataUrl(filePath: string): Promise<string> {
  const buffer = await readFile(filePath);
  return `data:${mimeFromPath(filePath)};base64,${buffer.toString("base64")}`;
}

function mimeFromPath(filePath: string): string {
  const ext = extname(filePath).toLowerCase();
  if (ext === ".jpg" || ext === ".jpeg") {
    return "image/jpeg";
  }
  if (ext === ".webp") {
    return "image/webp";
  }
  return "image/png";
}

async function writeJsonFile(path: string, payload: unknown): Promise<void> {
  const tempPath = `${path}.${process.pid}.${randomUUID()}.tmp`;
  await writeFile(tempPath, `${JSON.stringify(payload, null, 2)}\n`, "utf8");
  await rename(tempPath, path);
}

async function readJsonFile<T>(path: string, fallback?: T): Promise<T> {
  try {
    const raw = await readFile(path, "utf8");
    return JSON.parse(raw) as T;
  } catch (error) {
    if (fallback !== undefined) {
      return fallback;
    }
    throw error;
  }
}

function workFilePath(workId: string): string {
  return join(WORKS_ROOT, workId, "work.json");
}

function chapterFilePath(workId: string, chapterId: string): string {
  return join(WORKS_ROOT, workId, "chapters", chapterId, "chapter.json");
}

function reorderIds(currentOrder: string[], nextOrder: string[]): string[] {
  const currentSet = new Set(currentOrder);
  const filtered = nextOrder.filter((id) => currentSet.has(id));
  const remainder = currentOrder.filter((id) => !filtered.includes(id));
  return [...filtered, ...remainder];
}

function reorderRecords<T extends { id: string }>(records: T[], order: string[]): T[] {
  const recordMap = new Map(records.map((record) => [record.id, record]));
  const ordered: T[] = [];
  for (const id of order) {
    const record = recordMap.get(id);
    if (record) {
      ordered.push(record);
      recordMap.delete(id);
    }
  }
  return [...ordered, ...recordMap.values()];
}

function resolveChapterStatus(pages: Array<Pick<LibraryPageRecord, "analysisStatus">>): LibraryChapter["status"] {
  if (pages.length === 0) {
    return "idle";
  }
  const statuses = pages.map((page) => page.analysisStatus);
  if (statuses.every((status) => status === "completed")) {
    return "completed";
  }
  if (statuses.some((status) => status === "running")) {
    return "running";
  }
  if (statuses.every((status) => status === "failed")) {
    return "failed";
  }
  return statuses.some((status) => status === "completed") ? "partial" : "idle";
}

function toChapterSummary(chapter: LibraryChapter): LibraryChapterSummary {
  return {
    id: chapter.id,
    workId: chapter.workId,
    title: chapter.title,
    status: chapter.status,
    createdAt: chapter.createdAt,
    updatedAt: chapter.updatedAt,
    pageCount: chapter.pages.length
  };
}

function sortNaturally(values: string[]): string[] {
  return [...values].sort((left, right) => left.localeCompare(right, undefined, { numeric: true, sensitivity: "base" }));
}

async function safeUnlink(path: string): Promise<void> {
  try {
    await unlink(path);
  } catch {
    // no-op
  }
}

async function removePageArtifacts(workId: string, chapterId: string, pageId: string): Promise<void> {
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

export async function resetAppLog(logPath: string): Promise<void> {
  await mkdir(dirname(logPath), { recursive: true });
  await writeFile(logPath, "", "utf8");
}

export async function pathExists(path: string): Promise<boolean> {
  try {
    await stat(path);
    return true;
  } catch {
    return false;
  }
}
