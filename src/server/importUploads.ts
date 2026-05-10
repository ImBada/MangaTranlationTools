import { readdir, stat, unlink } from "node:fs/promises";
import { isAbsolute, relative, resolve, sep } from "node:path";
import type { ImportPreviewResult } from "../shared/types";

export function collectImportPreviewSourcePaths(preview: ImportPreviewResult | undefined): string[] {
  if (!preview) {
    return [];
  }

  const paths = new Set<string>();
  for (const chapter of preview.chapters) {
    for (const page of chapter.pages) {
      paths.add(page.sourcePath);
    }
  }
  return [...paths];
}

export async function cleanupUploadedImportFiles(uploadDir: string, filePaths: Iterable<string>): Promise<string[]> {
  const deleted: string[] = [];
  for (const filePath of new Set(filePaths)) {
    if (!isPathInsideDirectory(filePath, uploadDir)) {
      continue;
    }
    try {
      await unlink(filePath);
      deleted.push(filePath);
    } catch {
      // Upload cleanup is best-effort; a missing file should not fail import flow.
    }
  }
  return deleted;
}

export async function cleanupUnreferencedUploadedImportFiles(
  uploadDir: string,
  uploadedFilePaths: Iterable<string>,
  preview: ImportPreviewResult | undefined
): Promise<string[]> {
  const retainedPaths = new Set(collectImportPreviewSourcePaths(preview));
  return cleanupUploadedImportFiles(
    uploadDir,
    [...uploadedFilePaths].filter((filePath) => !retainedPaths.has(filePath))
  );
}

export async function cleanupUploadDirectory(uploadDir: string): Promise<string[]> {
  const entries = await readdir(uploadDir).catch(() => []);
  const deleted: string[] = [];

  for (const entry of entries) {
    const filePath = resolve(uploadDir, entry);
    if (!isPathInsideDirectory(filePath, uploadDir)) {
      continue;
    }
    try {
      const fileStat = await stat(filePath);
      if (!fileStat.isFile()) {
        continue;
      }
      await unlink(filePath);
      deleted.push(filePath);
    } catch {
      // Ignore files already removed by a concurrent cleanup.
    }
  }

  return deleted;
}

function isPathInsideDirectory(filePath: string, directory: string): boolean {
  const resolvedFilePath = resolve(filePath);
  const resolvedDirectory = resolve(directory);
  const directoryRelativePath = relative(resolvedDirectory, resolvedFilePath);
  return (
    directoryRelativePath !== "" &&
    directoryRelativePath !== ".." &&
    !directoryRelativePath.startsWith(`..${sep}`) &&
    !isAbsolute(directoryRelativePath)
  );
}
