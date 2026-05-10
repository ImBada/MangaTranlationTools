import { afterEach, describe, expect, it, vi } from "vitest";
import { mkdtemp, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import type { CreateImportRequest, ImportPageDraft, ImportSourceKind, ImportTarget } from "../src/shared/types";

const tempDirs: string[] = [];
const originalDataDir = process.env.MANGA_TRANSLATOR_DATA_DIR;

type ImportChapterInput = {
  title: string;
  pages: ImportPageDraft[];
};

describe("library import cleanup", () => {
  afterEach(async () => {
    process.env.MANGA_TRANSLATOR_DATA_DIR = originalDataDir;
    vi.resetModules();
    while (tempDirs.length > 0) {
      const dir = tempDirs.pop();
      if (dir) {
        await rm(dir, { recursive: true, force: true });
      }
    }
  });

  it("removes a newly created work when import materialization fails", async () => {
    const dataDir = await prepareDataDir("manga-import-cleanup-new-");
    const { createImport, listLibrary } = await import("../src/server/library");
    const sourcePath = join(dataDir, "source.png");
    await writeFile(sourcePath, pngBuffer());

    await expect(
      createImport(
        makeImportRequest({
          target: { mode: "new", title: "Broken Work" },
          chapters: [
            {
              title: "Broken Chapter",
              pages: [filePage("source.png", sourcePath), filePage("missing.png", join(dataDir, "missing.png"))]
            }
          ]
        })
      )
    ).rejects.toThrow();

    await expect(listLibrary()).resolves.toMatchObject({ workOrder: [], works: [] });
    expect(await readDirSafe(join(dataDir, "library", "works"))).toEqual([]);
  });

  it("removes partial chapter directories when importing into an existing work fails", async () => {
    const dataDir = await prepareDataDir("manga-import-cleanup-existing-");
    const { createImport, listLibrary } = await import("../src/server/library");
    const initialSourcePath = join(dataDir, "initial.png");
    const extraSourcePath = join(dataDir, "extra.png");
    await writeFile(initialSourcePath, pngBuffer());
    await writeFile(extraSourcePath, pngBuffer());

    const initialResult = await createImport(
      makeImportRequest({
        target: { mode: "new", title: "Existing Work" },
        chapters: [{ title: "Original Chapter", pages: [filePage("initial.png", initialSourcePath)] }]
      })
    );

    await expect(
      createImport(
        makeImportRequest({
          target: { mode: "existing", workId: initialResult.workId },
          chapters: [
            { title: "Completed Then Rolled Back", pages: [filePage("extra.png", extraSourcePath)] },
            { title: "Broken Chapter", pages: [filePage("missing.png", join(dataDir, "missing.png"))] }
          ]
        })
      )
    ).rejects.toThrow();

    const library = await listLibrary();
    expect(library.workOrder).toEqual([initialResult.workId]);
    expect(library.works[0]?.chapterOrder).toEqual(initialResult.chapterIds);
    expect(library.works[0]?.chapters.map((chapter) => chapter.title)).toEqual(["Original Chapter"]);
    expect(await readDirSafe(join(dataDir, "library", "works", initialResult.workId, "chapters"))).toEqual(initialResult.chapterIds);
  });
});

async function prepareDataDir(prefix: string): Promise<string> {
  const dataDir = await mkdtemp(join(tmpdir(), prefix));
  tempDirs.push(dataDir);
  process.env.MANGA_TRANSLATOR_DATA_DIR = dataDir;
  vi.resetModules();
  return dataDir;
}

function makeImportRequest(options: { target: ImportTarget; chapters: ImportChapterInput[] }): CreateImportRequest {
  const batch = options.chapters.length > 1;
  const previewSourceKind: ImportSourceKind = batch ? "zip-folder" : "images";
  const chapterSourceKind: ImportSourceKind = batch ? "folder" : "images";
  const chapters = options.chapters.map((chapter, index) => ({
    draftId: `draft-${index + 1}`,
    title: chapter.title,
    sourceKind: chapterSourceKind,
    pages: chapter.pages
  }));

  return {
    preview: {
      mode: batch ? "batch" : "single",
      sourceKind: previewSourceKind,
      suggestedWorkTitle: "Suggested Work",
      chapters
    },
    target: options.target,
    selections: chapters.map((chapter) => ({ draftId: chapter.draftId, title: chapter.title, enabled: true }))
  };
}

function filePage(name: string, sourcePath: string): ImportPageDraft {
  return {
    name,
    sourceKind: "file",
    sourcePath
  };
}

function pngBuffer(): Buffer {
  return Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==", "base64");
}

async function readDirSafe(path: string): Promise<string[]> {
  return (await readdir(path).catch(() => [])).sort((left, right) => left.localeCompare(right));
}
