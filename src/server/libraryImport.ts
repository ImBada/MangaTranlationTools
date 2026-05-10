import { randomUUID } from "node:crypto";
import { copyFile, mkdir, readdir, rm, writeFile } from "node:fs/promises";
import { basename, dirname, extname, join, relative } from "node:path";
import type {
  ChapterSnapshot,
  CreateImportRequest,
  CreateImportResult,
  ImportChapterDraft,
  ImportPageDraft,
  ImportPreviewResult,
  ImportSourceKind,
  LibraryChapter,
  LibraryPageRecord
} from "../shared/types";
import type { JobEvent } from "../shared/types";
import { readImageDimensions } from "./imageDimensions";
import { hydrateChapter } from "./libraryHydration";
import { isSupportedImagePath } from "./libraryImageData";
import { resolveChapterStatus, sortNaturally } from "./libraryOrdering";
import { WORKS_ROOT } from "./libraryPaths";
import {
  DEFAULT_WORK_TITLE,
  createWork,
  ensureExistingWork,
  makeUniqueChapterTitle,
  readIndexFile,
  readWorkFile,
  sanitizeTitle,
  writeChapterFile,
  writeIndexFile,
  writeWorkFile
} from "./libraryStore";

type ZipEntryLike = {
  entryName: string;
  isDirectory: boolean;
  getData: () => Buffer;
};

type AdmZipLike = {
  getEntries: () => ZipEntryLike[];
};

type CachedZip = {
  entriesByName: Map<string, ZipEntryLike>;
  imageEntries: ZipEntryLike[];
};

type ImportMaterializeContext = {
  current: number;
  emit?: (event: JobEvent) => void;
  jobId?: string;
  rollbackChapterDirs: string[];
  rollbackChapterIds: string[];
  signal?: AbortSignal;
  total: number;
  zipCache: Map<string, CachedZip>;
};

export type UploadedImportFile = {
  path: string;
  name: string;
  relativePath?: string;
};

export type CreateImportOptions = {
  emit?: (event: JobEvent) => void;
  jobId?: string;
  signal?: AbortSignal;
};

const IMPORT_PAGE_CONCURRENCY = 4;

const AdmZip = require("adm-zip") as {
  new (archivePath: string): AdmZipLike;
};

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

export async function createImport(request: CreateImportRequest, options: CreateImportOptions = {}): Promise<CreateImportResult> {
  const target = request.target.mode === "new" ? await createWork(request.target.title || request.preview.suggestedWorkTitle) : await ensureExistingWork(request.target.workId);
  const selections = new Map(request.selections.map((selection) => [selection.draftId, selection]));
  const importedChapterIds: string[] = [];
  const context: ImportMaterializeContext = {
    current: 0,
    emit: options.emit,
    jobId: options.jobId,
    rollbackChapterDirs: [],
    rollbackChapterIds: [],
    signal: options.signal,
    total: countSelectedImportPages(request),
    zipCache: new Map()
  };
  let openedChapter: ChapterSnapshot | undefined;

  try {
    throwIfAborted(options.signal);
    emitImportProgress(context, "가져오기 준비 중", "starting", "importing");

    for (const draft of request.preview.chapters) {
      const selection = selections.get(draft.draftId);
      if (!selection?.enabled) {
        continue;
      }

      const chapter = await createChapterFromDraft(target.id, draft, selection.title, context);
      importedChapterIds.push(chapter.id);
      if (!openedChapter) {
        openedChapter = await hydrateChapter(chapter);
      }
    }

    if (importedChapterIds.length === 0) {
      throw new Error("생성할 화가 없습니다.");
    }

    emitImportProgress(context, "가져오기 완료", "completed", "done");

    return {
      workId: target.id,
      chapterIds: importedChapterIds,
      openedChapter
    };
  } catch (error) {
    await cleanupFailedImport(target.id, request.target.mode === "new", context.rollbackChapterIds, context.rollbackChapterDirs).catch(() => undefined);
    throw error;
  }
}

export function isZipPath(path: string): boolean {
  return extname(path).toLowerCase() === ".zip";
}

async function createChapterFromDraft(
  workId: string,
  draft: ImportChapterDraft,
  requestedTitle: string,
  context: ImportMaterializeContext
): Promise<LibraryChapter> {
  const work = await ensureExistingWork(workId);
  const now = new Date().toISOString();
  const chapterId = randomUUID();
  const title = await makeUniqueChapterTitle(workId, sanitizeTitle(requestedTitle || draft.title, "제목없음"));
  const chapterDir = join(WORKS_ROOT, workId, "chapters", chapterId);
  const pagesDir = join(chapterDir, "pages");
  context.rollbackChapterIds.push(chapterId);
  context.rollbackChapterDirs.push(chapterDir);
  await mkdir(pagesDir, { recursive: true });

  const pages = await mapWithConcurrency(draft.pages, IMPORT_PAGE_CONCURRENCY, (pageDraft, index) =>
    materializePageRecord(pageDraft, pagesDir, index, context)
  );

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

async function materializePageRecord(
  pageDraft: ImportPageDraft,
  pagesDir: string,
  index: number,
  context: ImportMaterializeContext
): Promise<LibraryPageRecord> {
  throwIfAborted(context.signal);
  const pageId = randomUUID();
  const targetExt =
    pageDraft.sourceKind === "zip-entry" ? extname(pageDraft.zipEntryName ?? "").toLowerCase() || ".png" : extname(pageDraft.sourcePath).toLowerCase() || ".png";
  const outputPath = join(pagesDir, `${String(index + 1).padStart(3, "0")}-${pageId}${targetExt}`);

  if (pageDraft.sourceKind === "zip-entry") {
    const entry = getCachedZipEntry(context.zipCache, pageDraft.sourcePath, pageDraft.zipEntryName);
    if (!entry) {
      throw new Error(`ZIP 항목을 찾지 못했습니다: ${pageDraft.zipEntryName ?? pageDraft.sourcePath}`);
    }
    await writeFile(outputPath, entry.getData());
  } else {
    await copyFile(pageDraft.sourcePath, outputPath);
  }

  const size = await readImageDimensions(outputPath);
  const now = new Date().toISOString();
  context.current += 1;
  emitImportProgress(context, `${pageDraft.name} 가져오기 완료`, "running", "import_done");

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

function firstPathSegment(value: string | undefined): string {
  if (!value) {
    return "";
  }
  return normalizeImportPageName(normalizePathSeparators(value).split("/").filter(Boolean)[0] || "");
}

function normalizeImportPageName(entryName: string): string {
  return entryName.replace(/\\/g, "/");
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
  return getCachedZip(new Map(), zipPath).imageEntries;
}

function getCachedZipEntry(cache: Map<string, CachedZip>, zipPath: string, entryName: string | undefined): ZipEntryLike | undefined {
  if (!entryName) {
    return undefined;
  }
  return getCachedZip(cache, zipPath).entriesByName.get(entryName);
}

function getCachedZip(cache: Map<string, CachedZip>, zipPath: string): CachedZip {
  const cached = cache.get(zipPath);
  if (cached) {
    return cached;
  }

  const entries = new AdmZip(zipPath).getEntries();
  const next = {
    entriesByName: new Map(entries.map((entry) => [entry.entryName, entry])),
    imageEntries: entries
      .filter((entry) => !entry.isDirectory && isSupportedImagePath(entry.entryName))
      .sort((left, right) => left.entryName.localeCompare(right.entryName, undefined, { numeric: true, sensitivity: "base" }))
  };
  cache.set(zipPath, next);
  return next;
}

function countSelectedImportPages(request: CreateImportRequest): number {
  const selections = new Map(request.selections.map((selection) => [selection.draftId, selection]));
  return request.preview.chapters.reduce((total, draft) => total + (selections.get(draft.draftId)?.enabled ? draft.pages.length : 0), 0);
}

function emitImportProgress(
  context: ImportMaterializeContext,
  progressText: string,
  status: JobEvent["status"],
  phase: JobEvent["phase"]
): void {
  if (!context.emit || !context.jobId) {
    return;
  }
  context.emit({
    id: context.jobId,
    kind: "library-import",
    status,
    phase,
    progressText,
    progressCurrent: context.current,
    progressTotal: context.total,
    pageIndex: Math.min(context.current, context.total),
    pageTotal: context.total
  });
}

async function mapWithConcurrency<T, R>(items: T[], concurrency: number, mapper: (item: T, index: number) => Promise<R>): Promise<R[]> {
  const results: R[] = new Array(items.length);
  let nextIndex = 0;
  let firstError: unknown;
  let hasError = false;
  const workerCount = Math.min(Math.max(1, concurrency), items.length);

  async function worker(): Promise<void> {
    while (nextIndex < items.length) {
      if (hasError) {
        return;
      }
      const index = nextIndex;
      nextIndex += 1;
      try {
        results[index] = await mapper(items[index], index);
      } catch (error) {
        if (!hasError) {
          firstError = error;
          hasError = true;
        }
        return;
      }
    }
  }

  await Promise.all(Array.from({ length: workerCount }, () => worker()));
  if (hasError) {
    throw firstError;
  }
  return results;
}

function throwIfAborted(signal: AbortSignal | undefined): void {
  if (signal?.aborted) {
    throw new DOMException("Aborted", "AbortError");
  }
}

async function cleanupFailedImport(workId: string, removeWork: boolean, createdChapterIds: string[], createdChapterDirs: string[]): Promise<void> {
  if (removeWork) {
    const index = await readIndexFile();
    index.workOrder = index.workOrder.filter((id) => id !== workId);
    await writeIndexFile(index);
    await rm(join(WORKS_ROOT, workId), { recursive: true, force: true }).catch(() => undefined);
    return;
  }

  const work = await readWorkFile(workId);
  if (work && createdChapterIds.length > 0) {
    const nextChapterOrder = work.chapterOrder.filter((id) => !createdChapterIds.includes(id));
    if (nextChapterOrder.length !== work.chapterOrder.length) {
      work.chapterOrder = nextChapterOrder;
      work.updatedAt = new Date().toISOString();
      await writeWorkFile(work);
    }
  }

  await Promise.all(createdChapterDirs.map((chapterDir) => rm(chapterDir, { recursive: true, force: true }).catch(() => undefined)));
}
