import { afterEach, describe, expect, it, vi } from "vitest";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import sharp from "sharp";
import type { ChapterSnapshot, MangaPage } from "../src/shared/types";

const tempDirs: string[] = [];
const originalDataDir = process.env.MANGA_TRANSLATOR_DATA_DIR;

describe("last opened page", () => {
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

  it("persists the last opened page on the chapter without changing content timestamps", async () => {
    const dataDir = await prepareDataDir("manga-last-opened-page-");
    const { openChapter, saveChapterLastOpenedPage, saveChapterSnapshot } = await import("../src/server/library");
    const pagePath = join(dataDir, "page.png");
    await writeFile(pagePath, await pngBuffer());
    await seedLibraryIndex(dataDir);

    await saveChapterSnapshot(makeChapterSnapshot(pagePath));
    const saved = await saveChapterLastOpenedPage("chapter-1", "page-2");
    const reopened = await openChapter("chapter-1");
    const chapterJson = await readStoredChapter(dataDir);

    expect(saved.lastOpenedPageId).toBe("page-2");
    expect(reopened.lastOpenedPageId).toBe("page-2");
    expect(chapterJson.lastOpenedPageId).toBe("page-2");
    expect(chapterJson.updatedAt).toBe("2026-01-01T00:00:00.000Z");
  });

  it("keeps the last opened page when later content patches are saved", async () => {
    const dataDir = await prepareDataDir("manga-last-opened-page-patch-");
    const { patchChapterSnapshot, saveChapterLastOpenedPage, saveChapterSnapshot } = await import("../src/server/library");
    const pagePath = join(dataDir, "page.png");
    await writeFile(pagePath, await pngBuffer());
    await seedLibraryIndex(dataDir);

    const initial = makeChapterSnapshot(pagePath);
    await saveChapterSnapshot(initial);
    await saveChapterLastOpenedPage("chapter-1", "page-2");
    await patchChapterSnapshot("chapter-1", {
      chapter: {
        id: initial.id,
        workId: initial.workId,
        updatedAt: "2026-01-01T00:01:00.000Z"
      },
      pages: [
        {
          id: "page-1",
          blocks: [{ ...initial.pages[0].blocks[0], translatedText: "수정된 번역" }],
          updatedAt: "2026-01-01T00:01:00.000Z"
        }
      ]
    });

    const chapterJson = await readStoredChapter(dataDir);
    expect(chapterJson.lastOpenedPageId).toBe("page-2");
    expect(chapterJson.pages[0]?.blocks[0]?.translatedText).toBe("수정된 번역");
  });
});

async function prepareDataDir(prefix: string): Promise<string> {
  const dataDir = await mkdtemp(join(tmpdir(), prefix));
  tempDirs.push(dataDir);
  process.env.MANGA_TRANSLATOR_DATA_DIR = dataDir;
  vi.resetModules();
  return dataDir;
}

function makeChapterSnapshot(imagePath: string): ChapterSnapshot {
  const page = (id: string): MangaPage => ({
    id,
    name: `${id}.png`,
    imagePath,
    dataUrl: "",
    width: 1,
    height: 1,
    blocks: [
      {
        id: `${id}-block`,
        type: "speech",
        bbox: { x: 0, y: 0, w: 100, h: 100 },
        sourceText: "JP",
        translatedText: "KO",
        confidence: 0.9,
        sourceDirection: "vertical",
        renderDirection: "horizontal",
        fontSizePx: 24,
        lineHeight: 1.2,
        textAlign: "center",
        textColor: "#111111",
        backgroundColor: "#ffffff",
        opacity: 1
      }
    ],
    analysisStatus: "completed",
    createdAt: "2026-01-01T00:00:00.000Z",
    updatedAt: "2026-01-01T00:00:00.000Z"
  });

  return {
    id: "chapter-1",
    workId: "work-1",
    title: "chapter",
    sourceKind: "images",
    status: "completed",
    pageOrder: ["page-1", "page-2"],
    pages: [page("page-1"), page("page-2")],
    createdAt: "2026-01-01T00:00:00.000Z",
    updatedAt: "2026-01-01T00:00:00.000Z"
  };
}

async function seedLibraryIndex(dataDir: string): Promise<void> {
  const workDir = join(dataDir, "library", "works", "work-1");
  await mkdir(workDir, { recursive: true });
  await writeFile(join(dataDir, "library", "index.json"), JSON.stringify({ workOrder: ["work-1"] }, null, 2), "utf8");
  await writeFile(
    join(workDir, "work.json"),
    JSON.stringify(
      {
        id: "work-1",
        title: "work",
        chapterOrder: ["chapter-1"],
        createdAt: "2026-01-01T00:00:00.000Z",
        updatedAt: "2026-01-01T00:00:00.000Z"
      },
      null,
      2
    ),
    "utf8"
  );
}

async function readStoredChapter(dataDir: string): Promise<ChapterSnapshot> {
  return JSON.parse(
    await readFile(join(dataDir, "library", "works", "work-1", "chapters", "chapter-1", "chapter.json"), "utf8")
  ) as ChapterSnapshot;
}

async function pngBuffer(): Promise<Buffer> {
  return sharp({
    create: {
      width: 1,
      height: 1,
      channels: 4,
      background: { r: 1, g: 2, b: 3, alpha: 1 }
    }
  }).png().toBuffer();
}
