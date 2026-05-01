import { existsSync } from "node:fs";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { basename, join } from "node:path";
import sharp from "sharp";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ChapterSnapshot } from "../src/shared/types";

const tempDirs: string[] = [];
const originalDataDir = process.env.MANGA_TRANSLATOR_DATA_DIR;

describe("render output", () => {
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

  it("saves rendered pages with the original basename and png extension", async () => {
    const dataDir = await mkdtemp(join(tmpdir(), "manga-render-output-"));
    tempDirs.push(dataDir);
    process.env.MANGA_TRANSLATOR_DATA_DIR = dataDir;
    vi.resetModules();

    const { saveChapterSnapshot, saveRenderedPage } = await import("../src/server/library");
    const pagePath = join(dataDir, "001.webp");
    await writeFile(pagePath, await pngBuffer([1, 2, 3, 255]));
    await seedLibraryIndex(dataDir);

    await saveChapterSnapshot(makeChapterSnapshot(pagePath, "001.webp"));
    const result = await saveRenderedPage("chapter-1", "page-1", await pngDataUrl([9, 8, 7, 255]));

    expect(basename(result.outputPath)).toBe("001.png");
    expect(existsSync(result.outputPath)).toBe(true);
    expect((await sharp(result.outputPath).metadata()).format).toBe("png");
  });
});

function makeChapterSnapshot(imagePath: string, pageName: string): ChapterSnapshot {
  return {
    id: "chapter-1",
    workId: "work-1",
    title: "chapter",
    sourceKind: "images",
    status: "idle",
    pageOrder: ["page-1"],
    pages: [
      {
        id: "page-1",
        name: pageName,
        imagePath,
        dataUrl: "",
        width: 1,
        height: 1,
        blocks: [],
        analysisStatus: "idle",
        createdAt: "2026-01-01T00:00:00.000Z",
        updatedAt: "2026-01-01T00:00:00.000Z"
      }
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

async function pngDataUrl(pixel: [number, number, number, number]): Promise<string> {
  return `data:image/png;base64,${(await pngBuffer(pixel)).toString("base64")}`;
}

function pngBuffer(pixel: [number, number, number, number]): Promise<Buffer> {
  return sharp(Buffer.from(pixel), {
    raw: {
      width: 1,
      height: 1,
      channels: 4
    }
  }).png().toBuffer();
}
