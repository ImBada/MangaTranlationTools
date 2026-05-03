import { randomUUID } from "node:crypto";
import { existsSync } from "node:fs";
import { mkdir } from "node:fs/promises";
import { dirname } from "node:path";
import type { LibraryChapter, LibraryWork } from "../shared/types";
import { readJsonFile, writeJsonFile } from "./libraryFileIO";
import { INDEX_PATH, WORKS_ROOT, chapterFilePath, workFilePath } from "./libraryPaths";

export const DEFAULT_WORK_TITLE = "미정 작품";

export type StoredIndexFile = {
  workOrder: string[];
};

export type WorkFile = LibraryWork;

export type ChapterFile = LibraryChapter;

const chapterWriteQueues = new Map<string, Promise<void>>();

export async function readIndexFile(): Promise<StoredIndexFile> {
  await ensureLibraryStructure();
  if (!existsSync(INDEX_PATH)) {
    return { workOrder: [] };
  }
  return readJsonFile<StoredIndexFile>(INDEX_PATH, { workOrder: [] });
}

export async function writeIndexFile(index: StoredIndexFile): Promise<void> {
  await ensureLibraryStructure();
  await writeJsonFile(INDEX_PATH, index);
}

export async function readWorkFile(workId: string): Promise<WorkFile | null> {
  const path = workFilePath(workId);
  if (!existsSync(path)) {
    return null;
  }
  return readJsonFile<WorkFile>(path);
}

export async function writeWorkFile(work: WorkFile): Promise<void> {
  await mkdir(dirname(workFilePath(work.id)), { recursive: true });
  await writeJsonFile(workFilePath(work.id), work);
}

export async function touchWork(workId: string, updatedAt: string): Promise<void> {
  const work = await readWorkFile(workId);
  if (!work) {
    return;
  }
  work.updatedAt = updatedAt;
  await writeWorkFile(work);
}

export async function readChapterFile(workId: string, chapterId: string): Promise<ChapterFile | null> {
  const path = chapterFilePath(workId, chapterId);
  if (!existsSync(path)) {
    return null;
  }
  return readJsonFile<ChapterFile>(path);
}

export async function writeChapterFile(chapter: ChapterFile): Promise<void> {
  await mkdir(dirname(chapterFilePath(chapter.workId, chapter.id)), { recursive: true });
  await writeJsonFile(chapterFilePath(chapter.workId, chapter.id), chapter);
}

export async function enqueueChapterMutation<T>(chapterId: string, task: () => Promise<T>): Promise<T> {
  const previous = chapterWriteQueues.get(chapterId) ?? Promise.resolve();
  const run = previous.catch(() => undefined).then(task);
  const release = run.then(
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

export async function mutateExistingChapterFile(
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

export async function findChapterLocation(chapterId: string): Promise<{ workId: string; chapterId: string } | null> {
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

export async function ensureLibraryStructure(): Promise<void> {
  await mkdir(WORKS_ROOT, { recursive: true });
}

export async function createWork(title: string): Promise<LibraryWork> {
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

export async function ensureExistingWork(workId: string): Promise<LibraryWork> {
  const work = await readWorkFile(workId);
  if (!work) {
    throw new Error("선택한 작품을 찾지 못했습니다.");
  }
  return work;
}

export async function makeUniqueChapterTitle(workId: string, desired: string, excludeChapterId?: string): Promise<string> {
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

export function sanitizeTitle(title: string, fallback: string): string {
  const trimmed = title.trim();
  return trimmed || fallback;
}
