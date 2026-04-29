import { afterEach, describe, expect, it, vi } from "vitest";
import { mkdir, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import sharp from "sharp";
import type { ChapterSnapshot } from "../src/shared/types";

const tempDirs: string[] = [];
const originalDataDir = process.env.MANGA_TRANSLATOR_DATA_DIR;

describe("inpaint artifacts", () => {
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

  it("stores inpaint mask and result as files and hydrates them as data URLs", async () => {
    const dataDir = await mkdtemp(join(tmpdir(), "manga-inpaint-artifacts-"));
    tempDirs.push(dataDir);
    process.env.MANGA_TRANSLATOR_DATA_DIR = dataDir;
    vi.resetModules();

    const { saveChapterSnapshot, saveInpaintResult } = await import("../src/main/library");
    const pagePath = join(dataDir, "page.png");
    await writeFile(pagePath, await pngBuffer([1, 2, 3, 255]));
    await seedLibraryIndex(dataDir);

    const chapter = await saveChapterSnapshot(makeChapterSnapshot(pagePath));
    const saved = await saveInpaintResult(
      chapter.id,
      "page-1",
      await rgbaDataUrl(2, 1, [[255, 255, 255, 255], [0, 0, 0, 0]]),
      await rgbaDataUrl(2, 1, [[9, 8, 7, 255], [1, 2, 3, 255]]),
      {
      engine: "local-fill-fallback",
      paddingPx: 1,
      featherPx: 0,
      tileSize: 128
      }
    );
    const hydratedPage = saved.pages[0];
    expect(hydratedPage?.inpaintMaskPath).toBeTruthy();
    expect(hydratedPage?.inpaintResultPath).toBeTruthy();
    expect(hydratedPage?.inpaintMaskDataUrl?.startsWith("data:image/png;base64,")).toBe(true);
    expect(hydratedPage?.inpaintResultDataUrl?.startsWith("data:image/png;base64,")).toBe(true);
    expect(existsSync(hydratedPage?.inpaintMaskPath ?? "")).toBe(true);
    expect(existsSync(hydratedPage?.inpaintResultPath ?? "")).toBe(true);
    const maskPixels = await decodePng(hydratedPage?.inpaintMaskPath ?? "");
    const resultPixels = await decodePng(hydratedPage?.inpaintResultPath ?? "");
    expect([...maskPixels.subarray(0, 8)]).toEqual([255, 255, 255, 255, 0, 0, 0, 0]);
    expect([...resultPixels.subarray(0, 8)]).toEqual([9, 8, 7, 255, 1, 2, 3, 0]);

    const chapterJson = JSON.parse(await readFile(join(dataDir, "library", "works", "work-1", "chapters", "chapter-1", "chapter.json"), "utf8")) as ChapterSnapshot;
    expect(chapterJson.pages[0]?.inpaintMaskPath).toBe(hydratedPage?.inpaintMaskPath);
    expect(chapterJson.pages[0]?.inpaintResultPath).toBe(hydratedPage?.inpaintResultPath);
    expect(chapterJson.pages[0]?.inpaintMaskDataUrl).toBeUndefined();
    expect(chapterJson.pages[0]?.inpaintResultDataUrl).toBeUndefined();
  });

  it("persists an edited inpaint mask before running inpaint", async () => {
    const dataDir = await mkdtemp(join(tmpdir(), "manga-inpaint-mask-"));
    tempDirs.push(dataDir);
    process.env.MANGA_TRANSLATOR_DATA_DIR = dataDir;
    vi.resetModules();

    const { openChapter, saveChapterSnapshot, saveInpaintMask } = await import("../src/main/library");
    const pagePath = join(dataDir, "page.png");
    await writeFile(pagePath, await pngBuffer([1, 2, 3, 255]));
    await seedLibraryIndex(dataDir);

    const chapter = await saveChapterSnapshot(makeChapterSnapshot(pagePath));
    const saved = await saveInpaintMask(
      chapter.id,
      "page-1",
      await rgbaDataUrl(2, 1, [[0, 0, 0, 0], [255, 255, 255, 255]])
    );
    const hydratedPage = saved.pages[0];

    expect(hydratedPage?.inpaintMaskPath).toBeTruthy();
    expect(hydratedPage?.inpaintMaskDataUrl?.startsWith("data:image/png;base64,")).toBe(true);
    expect(existsSync(hydratedPage?.inpaintMaskPath ?? "")).toBe(true);
    expect([...(await decodePng(hydratedPage?.inpaintMaskPath ?? ""))]).toEqual([0, 0, 0, 0, 255, 255, 255, 255]);

    const reopened = await openChapter(chapter.id);
    expect(reopened.pages[0]?.inpaintMaskDataUrl).toBe(hydratedPage?.inpaintMaskDataUrl);

    const chapterJson = JSON.parse(await readFile(join(dataDir, "library", "works", "work-1", "chapters", "chapter-1", "chapter.json"), "utf8")) as ChapterSnapshot;
    expect(chapterJson.pages[0]?.inpaintMaskPath).toBe(hydratedPage?.inpaintMaskPath);
    expect(chapterJson.pages[0]?.inpaintMaskDataUrl).toBeUndefined();
  });

  it("persists an edited inpaint result layer without changing the mask", async () => {
    const dataDir = await mkdtemp(join(tmpdir(), "manga-inpaint-result-layer-"));
    tempDirs.push(dataDir);
    process.env.MANGA_TRANSLATOR_DATA_DIR = dataDir;
    vi.resetModules();

    const { openChapter, saveChapterSnapshot, saveInpaintMask, saveInpaintResultLayer } = await import("../src/main/library");
    const pagePath = join(dataDir, "page.png");
    await writeFile(pagePath, await pngBuffer([1, 2, 3, 255]));
    await seedLibraryIndex(dataDir);

    const chapter = await saveChapterSnapshot(makeChapterSnapshot(pagePath));
    const masked = await saveInpaintMask(
      chapter.id,
      "page-1",
      await rgbaDataUrl(2, 1, [[255, 255, 255, 255], [0, 0, 0, 0]])
    );
    const saved = await saveInpaintResultLayer(
      chapter.id,
      "page-1",
      await rgbaDataUrl(2, 1, [[12, 34, 56, 255], [78, 90, 123, 128]])
    );
    const hydratedPage = saved.pages[0];

    expect(hydratedPage?.inpaintMaskPath).toBe(masked.pages[0]?.inpaintMaskPath);
    expect(hydratedPage?.inpaintResultPath).toBeTruthy();
    expect(hydratedPage?.inpaintResultDataUrl?.startsWith("data:image/png;base64,")).toBe(true);
    expect([...(await decodePng(hydratedPage?.inpaintResultPath ?? ""))]).toEqual([12, 34, 56, 255, 78, 90, 123, 128]);

    const reopened = await openChapter(chapter.id);
    expect(reopened.pages[0]?.inpaintResultDataUrl).toBe(hydratedPage?.inpaintResultDataUrl);

    const chapterJson = JSON.parse(await readFile(join(dataDir, "library", "works", "work-1", "chapters", "chapter-1", "chapter.json"), "utf8")) as ChapterSnapshot;
    expect(chapterJson.pages[0]?.inpaintMaskPath).toBe(masked.pages[0]?.inpaintMaskPath);
    expect(chapterJson.pages[0]?.inpaintResultPath).toBe(hydratedPage?.inpaintResultPath);
    expect(chapterJson.pages[0]?.inpaintResultDataUrl).toBeUndefined();
  });
});

function makeChapterSnapshot(imagePath: string): ChapterSnapshot {
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
        name: "page.png",
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

async function rgbaDataUrl(width: number, height: number, pixels: [number, number, number, number][]): Promise<string> {
  return `data:image/png;base64,${(await rgbaBuffer(width, height, pixels)).toString("base64")}`;
}

function pngBuffer(pixel: [number, number, number, number]): Promise<Buffer> {
  return rgbaBuffer(1, 1, [pixel]);
}

function rgbaBuffer(width: number, height: number, pixels: [number, number, number, number][]): Promise<Buffer> {
  return sharp(Buffer.from(pixels.flat()), {
    raw: {
      width,
      height,
      channels: 4
    }
  }).png().toBuffer();
}

async function decodePng(path: string): Promise<Buffer> {
  const { data } = await sharp(path).ensureAlpha().raw().toBuffer({ resolveWithObject: true });
  return data;
}
