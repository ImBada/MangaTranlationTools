import { existsSync } from "node:fs";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterEach, describe, expect, it } from "vitest";
import type { ImportPreviewResult } from "../src/shared/types";
import {
  cleanupUnreferencedUploadedImportFiles,
  cleanupUploadDirectory,
  cleanupUploadedImportFiles,
  collectImportPreviewSourcePaths
} from "../src/server/importUploads";

const tempDirs: string[] = [];

describe("import upload cleanup", () => {
  afterEach(async () => {
    while (tempDirs.length > 0) {
      const dir = tempDirs.pop();
      if (dir) {
        await rm(dir, { recursive: true, force: true });
      }
    }
  });

  it("collects unique source paths from import previews", () => {
    const preview = makePreview(["/tmp/upload-a", "/tmp/upload-a", "/tmp/upload-b"]);

    expect(collectImportPreviewSourcePaths(preview)).toEqual(["/tmp/upload-a", "/tmp/upload-b"]);
  });

  it("deletes only files inside the upload directory", async () => {
    const root = await makeTempDir();
    const uploadDir = join(root, "uploads");
    const outsideFile = join(root, "outside.png");
    const uploadedFile = join(uploadDir, "upload.png");
    await mkdir(uploadDir, { recursive: true });
    await writeFile(uploadedFile, "uploaded");
    await writeFile(outsideFile, "outside");

    const deleted = await cleanupUploadedImportFiles(uploadDir, [uploadedFile, outsideFile, uploadDir]);

    expect(deleted).toEqual([uploadedFile]);
    expect(existsSync(uploadedFile)).toBe(false);
    expect(existsSync(outsideFile)).toBe(true);
    expect(existsSync(uploadDir)).toBe(true);
  });

  it("removes uploaded files that are not referenced by a preview", async () => {
    const root = await makeTempDir();
    const uploadDir = join(root, "uploads");
    const retainedFile = join(uploadDir, "retained.png");
    const unusedFile = join(uploadDir, "unused.txt");
    await mkdir(uploadDir, { recursive: true });
    await writeFile(retainedFile, "retained");
    await writeFile(unusedFile, "unused");

    const deleted = await cleanupUnreferencedUploadedImportFiles(
      uploadDir,
      [retainedFile, unusedFile],
      makePreview([retainedFile])
    );

    expect(deleted).toEqual([unusedFile]);
    expect(existsSync(retainedFile)).toBe(true);
    expect(existsSync(unusedFile)).toBe(false);
  });

  it("cleans stale files from the upload directory without deleting subdirectories", async () => {
    const root = await makeTempDir();
    const uploadDir = join(root, "uploads");
    const uploadedFile = join(uploadDir, "stale-upload");
    const nestedDir = join(uploadDir, "nested");
    const nestedFile = join(nestedDir, "kept");
    await mkdir(nestedDir, { recursive: true });
    await writeFile(uploadedFile, "stale");
    await writeFile(nestedFile, "nested");

    const deleted = await cleanupUploadDirectory(uploadDir);

    expect(deleted).toEqual([uploadedFile]);
    expect(existsSync(uploadedFile)).toBe(false);
    expect(existsSync(nestedDir)).toBe(true);
    expect(existsSync(nestedFile)).toBe(true);
  });
});

async function makeTempDir(): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), "manga-import-uploads-"));
  tempDirs.push(dir);
  return dir;
}

function makePreview(paths: string[]): ImportPreviewResult {
  return {
    mode: "single",
    sourceKind: "images",
    suggestedWorkTitle: "work",
    chapters: [
      {
        draftId: "draft",
        title: "chapter",
        sourceKind: "images",
        pages: paths.map((sourcePath, index) => ({
          name: `${index}.png`,
          sourceKind: "file",
          sourcePath
        }))
      }
    ]
  };
}
