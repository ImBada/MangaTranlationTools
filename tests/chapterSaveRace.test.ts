import { afterEach, describe, expect, it, vi } from "vitest";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import sharp from "sharp";
import type { ChapterSnapshot, MangaPage } from "../src/shared/types";

const tempDirs: string[] = [];
const originalDataDir = process.env.MANGA_TRANSLATOR_DATA_DIR;

describe("chapter save conflict handling", () => {
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

  it("does not overwrite a freshly translated page when saving a dirty edit from a stale chapter snapshot", async () => {
    const dataDir = await mkdtemp(join(tmpdir(), "manga-chapter-save-race-"));
    tempDirs.push(dataDir);
    process.env.MANGA_TRANSLATOR_DATA_DIR = dataDir;
    vi.resetModules();

    const { saveChapterSnapshot, updatePageAfterAnalysis } = await import("../src/main/library");
    const pagePath = join(dataDir, "page.png");
    await writeFile(pagePath, await pngBuffer());
    await seedLibraryIndex(dataDir);

    const initial = makeChapterSnapshot(pagePath);
    await saveChapterSnapshot(initial);

    await updatePageAfterAnalysis(
      initial.id,
      {
        ...initial.pages[1],
        blocks: [{ ...initial.pages[1].blocks[0], translatedText: "번역 완료" }]
      },
      [],
      "completed"
    );

    const staleDirtySnapshot: ChapterSnapshot = {
      ...initial,
      pages: initial.pages.map((page) =>
        page.id === "page-1"
          ? {
              ...page,
              blocks: [{ ...page.blocks[0], translatedText: "사용자 수정" }],
              updatedAt: "2026-01-01T00:02:00.000Z"
            }
          : page
      )
    };

    await saveChapterSnapshot(staleDirtySnapshot, { dirtyPageIds: ["page-1"] });

    const chapterJson = JSON.parse(
      await readFile(join(dataDir, "library", "works", "work-1", "chapters", "chapter-1", "chapter.json"), "utf8")
    ) as ChapterSnapshot;
    expect(chapterJson.pages[0]?.blocks[0]?.translatedText).toBe("사용자 수정");
    expect(chapterJson.pages[1]?.blocks[0]?.translatedText).toBe("번역 완료");
    expect(chapterJson.pages[1]?.analysisStatus).toBe("completed");
  });
});

function makeChapterSnapshot(imagePath: string): ChapterSnapshot {
  const page = (id: string, translatedText: string, updatedAt: string): MangaPage => ({
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
        translatedText,
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
    updatedAt
  });

  return {
    id: "chapter-1",
    workId: "work-1",
    title: "chapter",
    sourceKind: "images",
    status: "partial",
    pageOrder: ["page-1", "page-2"],
    pages: [
      page("page-1", "원래 번역 1", "2026-01-01T00:00:00.000Z"),
      { ...page("page-2", "원래 번역 2", "2026-01-01T00:00:00.000Z"), analysisStatus: "running" }
    ],
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
