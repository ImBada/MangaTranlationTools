import { join } from "node:path";
import { getAppPaths } from "./appPaths";

export const LIBRARY_ROOT = getAppPaths().libraryDir;
export const INDEX_PATH = join(LIBRARY_ROOT, "index.json");
export const WORKS_ROOT = join(LIBRARY_ROOT, "works");

export function getLibraryRoot(): string {
  return LIBRARY_ROOT;
}

export function workFilePath(workId: string): string {
  return join(WORKS_ROOT, workId, "work.json");
}

export function chapterFilePath(workId: string, chapterId: string): string {
  return join(WORKS_ROOT, workId, "chapters", chapterId, "chapter.json");
}
