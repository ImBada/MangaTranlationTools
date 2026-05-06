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

  it("stores inpaint mask and result as files and exposes them as image URLs", async () => {
    const dataDir = await mkdtemp(join(tmpdir(), "manga-inpaint-artifacts-"));
    tempDirs.push(dataDir);
    process.env.MANGA_TRANSLATOR_DATA_DIR = dataDir;
    vi.resetModules();

    const { readPageImageAsset, saveChapterSnapshot, saveInpaintResult } = await import("../src/server/library");
    const pagePath = join(dataDir, "page.png");
    await writeFile(pagePath, await pngBuffer([1, 2, 3, 255]));
    await seedLibraryIndex(dataDir);

    const chapter = await saveChapterSnapshot(makeChapterSnapshot(pagePath));
    expect(chapter.pages[0]?.dataUrl.startsWith("/api/library/chapters/chapter-1/pages/page-1/images/source")).toBe(true);
    const sourceAsset = await readPageImageAsset(chapter.id, "page-1", "source");
    expect(sourceAsset.mime).toBe("image/png");
    expect(sourceAsset.buffer.length).toBeGreaterThan(0);
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
    expect(hydratedPage?.inpaintMaskDataUrl?.startsWith("/api/library/chapters/chapter-1/pages/page-1/images/inpaint-mask")).toBe(true);
    expect(hydratedPage?.inpaintResultDataUrl?.startsWith("/api/library/chapters/chapter-1/pages/page-1/images/inpaint-result")).toBe(true);
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

  it("preserves inpaint result pixels outside the mask when the result layer has alpha", async () => {
    const dataDir = await mkdtemp(join(tmpdir(), "manga-inpaint-cleanup-artifacts-"));
    tempDirs.push(dataDir);
    process.env.MANGA_TRANSLATOR_DATA_DIR = dataDir;
    vi.resetModules();

    const { saveChapterSnapshot, saveInpaintResult } = await import("../src/server/library");
    const pagePath = join(dataDir, "page.png");
    await writeFile(pagePath, await pngBuffer([1, 2, 3, 255]));
    await seedLibraryIndex(dataDir);

    const chapter = await saveChapterSnapshot(makeChapterSnapshot(pagePath));
    const saved = await saveInpaintResult(
      chapter.id,
      "page-1",
      await rgbaDataUrl(3, 1, [[0, 0, 0, 0], [255, 255, 255, 255], [0, 0, 0, 0]]),
      await rgbaDataUrl(3, 1, [[250, 250, 250, 128], [9, 8, 7, 255], [1, 2, 3, 0]]),
      {
        engine: "lama",
        paddingPx: 0,
        featherPx: 0,
        tileSize: 128,
        artifactCleanupPx: 8
      }
    );

    const resultPixels = await decodePng(saved.pages[0]?.inpaintResultPath ?? "");
    const maskPixels = await decodePng(saved.pages[0]?.inpaintMaskPath ?? "");
    expect([...maskPixels.subarray(0, 12)]).toEqual([255, 255, 255, 255, 255, 255, 255, 255, 0, 0, 0, 0]);
    expect([...resultPixels.subarray(0, 12)]).toEqual([250, 250, 250, 128, 9, 8, 7, 255, 1, 2, 3, 0]);
  });

  it("saves normalized inpaint layers through one request", async () => {
    const dataDir = await mkdtemp(join(tmpdir(), "manga-inpaint-layers-request-"));
    tempDirs.push(dataDir);
    process.env.MANGA_TRANSLATOR_DATA_DIR = dataDir;
    vi.resetModules();

    const { saveChapterSnapshot } = await import("../src/server/library");
    const { saveInpaintLayersRequest } = await import("../src/server/inpaintRequests");
    const pagePath = join(dataDir, "page.png");
    await writeFile(pagePath, await pngBuffer([1, 2, 3, 255]));
    await seedLibraryIndex(dataDir);
    await saveChapterSnapshot(makeChapterSnapshot(pagePath));

    const saved = await saveInpaintLayersRequest({
      chapterId: "chapter-1",
      pageId: "page-1",
      maskDataUrl: await rgbaDataUrl(2, 1, [[0, 0, 0, 0], [255, 255, 255, 255]]),
      resultDataUrl: await rgbaDataUrl(2, 1, [[250, 250, 250, 128], [9, 8, 7, 255]])
    });

    const maskPixels = await decodePng(saved.chapter.pages[0]?.inpaintMaskPath ?? "");
    const resultPixels = await decodePng(saved.chapter.pages[0]?.inpaintResultPath ?? "");
    expect([...maskPixels.subarray(0, 8)]).toEqual([255, 255, 255, 255, 255, 255, 255, 255]);
    expect([...resultPixels.subarray(0, 8)]).toEqual([250, 250, 250, 128, 9, 8, 7, 255]);
  });

  it("does not create inpaint files when saving layers for a missing page", async () => {
    const dataDir = await mkdtemp(join(tmpdir(), "manga-inpaint-missing-page-"));
    tempDirs.push(dataDir);
    process.env.MANGA_TRANSLATOR_DATA_DIR = dataDir;
    vi.resetModules();

    const { saveChapterSnapshot } = await import("../src/server/library");
    const { saveInpaintLayersRequest } = await import("../src/server/inpaintRequests");
    const pagePath = join(dataDir, "page.png");
    await writeFile(pagePath, await pngBuffer([1, 2, 3, 255]));
    await seedLibraryIndex(dataDir);
    await saveChapterSnapshot(makeChapterSnapshot(pagePath));

    await expect(saveInpaintLayersRequest({
      chapterId: "chapter-1",
      pageId: "missing-page",
      maskDataUrl: await rgbaDataUrl(1, 1, [[255, 255, 255, 255]]),
      resultDataUrl: await rgbaDataUrl(1, 1, [[9, 8, 7, 255]])
    })).rejects.toThrow("페이지를 찾지 못했습니다.");
    expect(existsSync(join(dataDir, "library", "works", "work-1", "chapters", "chapter-1", "inpaint"))).toBe(false);
  });

  it("clips opaque legacy inpaint result image assets to the saved mask", async () => {
    const dataDir = await mkdtemp(join(tmpdir(), "manga-inpaint-legacy-asset-"));
    tempDirs.push(dataDir);
    process.env.MANGA_TRANSLATOR_DATA_DIR = dataDir;
    vi.resetModules();

    const { readPageImageAsset, saveChapterSnapshot } = await import("../src/server/library");
    const pagePath = join(dataDir, "page.png");
    const maskPath = join(dataDir, "legacy-mask.png");
    const resultPath = join(dataDir, "legacy-result.png");
    await writeFile(pagePath, await pngBuffer([1, 2, 3, 255]));
    await writeFile(maskPath, await rgbaBuffer(2, 1, [[255, 255, 255, 255], [0, 0, 0, 0]]));
    await writeFile(resultPath, await rgbaBuffer(2, 1, [[9, 8, 7, 255], [1, 2, 3, 255]]));
    await seedLibraryIndex(dataDir);

    const chapter = makeChapterSnapshot(pagePath);
    chapter.pages[0] = {
      ...chapter.pages[0],
      width: 2,
      inpaintMaskPath: maskPath,
      inpaintResultPath: resultPath
    };
    await saveChapterSnapshot(chapter);

    const asset = await readPageImageAsset(chapter.id, "page-1", "inpaint-result");
    const assetPixels = await decodePngBuffer(asset.buffer);
    const filePixels = await decodePng(resultPath);

    expect(asset.mime).toBe("image/png");
    expect([...assetPixels.subarray(0, 8)]).toEqual([9, 8, 7, 255, 1, 2, 3, 0]);
    expect([...filePixels.subarray(0, 8)]).toEqual([9, 8, 7, 255, 1, 2, 3, 0]);
  });

  it("disables inpaint artifact cleanup for PNG source pages", async () => {
    const dataDir = await mkdtemp(join(tmpdir(), "manga-inpaint-png-cleanup-"));
    tempDirs.push(dataDir);
    process.env.MANGA_TRANSLATOR_DATA_DIR = dataDir;
    vi.resetModules();

    const { saveChapterSnapshot } = await import("../src/server/library");
    const { inpaintPage } = await import("../src/server/inpaintRequests");
    const pagePath = join(dataDir, "page.png");
    await writeFile(pagePath, await pngBuffer([1, 2, 3, 255]));
    await seedLibraryIndex(dataDir);
    await saveChapterSnapshot(makeChapterSnapshot(pagePath));

    const result = await inpaintPage({
      chapterId: "chapter-1",
      pageId: "page-1",
      sourceDataUrl: await cleanupSourceDataUrl(),
      maskDataUrl: await cleanupMaskDataUrl(),
      settings: {
        engine: "local-fill-fallback",
        paddingPx: 0,
        featherPx: 0,
        tileSize: 128,
        artifactCleanupPx: 1
      }
    });
    const pixels = await decodePngDataUrl(result.resultDataUrl);
    const maskPixels = await decodePngDataUrl(result.maskDataUrl);
    const savedMaskPixels = await decodePng(result.chapter.pages[0]?.inpaintMaskPath ?? "");

    expect(pixels[1 * 4 + 3]).toBe(0);
    expect(pixels[2 * 4 + 3]).toBe(255);
    expect(maskPixels[1 * 4 + 3]).toBe(0);
    expect(maskPixels[2 * 4 + 3]).toBe(255);
    expect(savedMaskPixels[1 * 4 + 3]).toBe(0);
    expect(savedMaskPixels[2 * 4 + 3]).toBe(255);
    expect(result.chapter.pages[0]?.inpaintSettings?.artifactCleanupPx).toBe(0);
  });

  it("keeps inpaint artifact cleanup enabled for JPEG source pages", async () => {
    const dataDir = await mkdtemp(join(tmpdir(), "manga-inpaint-jpeg-cleanup-"));
    tempDirs.push(dataDir);
    process.env.MANGA_TRANSLATOR_DATA_DIR = dataDir;
    vi.resetModules();

    const { saveChapterSnapshot } = await import("../src/server/library");
    const { inpaintPage } = await import("../src/server/inpaintRequests");
    const pagePath = join(dataDir, "page.jpeg");
    await writeFile(pagePath, await pngBuffer([1, 2, 3, 255]));
    await seedLibraryIndex(dataDir);
    await saveChapterSnapshot(makeChapterSnapshot(pagePath));

    const result = await inpaintPage({
      chapterId: "chapter-1",
      pageId: "page-1",
      sourceDataUrl: await cleanupSourceDataUrl(),
      maskDataUrl: await cleanupMaskDataUrl(),
      settings: {
        engine: "local-fill-fallback",
        paddingPx: 0,
        featherPx: 0,
        tileSize: 128,
        artifactCleanupPx: 1
      }
    });
    const pixels = await decodePngDataUrl(result.resultDataUrl);
    const maskPixels = await decodePngDataUrl(result.maskDataUrl);
    const savedMaskPixels = await decodePng(result.chapter.pages[0]?.inpaintMaskPath ?? "");

    expect(pixels[1 * 4 + 3]).toBeGreaterThan(0);
    expect(pixels[2 * 4 + 3]).toBe(255);
    expect(maskPixels[1 * 4 + 3]).toBe(255);
    expect(maskPixels[2 * 4 + 3]).toBe(255);
    expect(savedMaskPixels[1 * 4 + 3]).toBe(255);
    expect(savedMaskPixels[2 * 4 + 3]).toBe(255);
    expect(result.chapter.pages[0]?.inpaintSettings?.artifactCleanupPx).toBe(1);
  });

  it("persists an edited inpaint mask before running inpaint", async () => {
    const dataDir = await mkdtemp(join(tmpdir(), "manga-inpaint-mask-"));
    tempDirs.push(dataDir);
    process.env.MANGA_TRANSLATOR_DATA_DIR = dataDir;
    vi.resetModules();

    const { openChapter, saveChapterSnapshot, saveInpaintMask } = await import("../src/server/library");
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
    expect(hydratedPage?.inpaintMaskDataUrl?.startsWith("/api/library/chapters/chapter-1/pages/page-1/images/inpaint-mask")).toBe(true);
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

    const { openChapter, saveChapterSnapshot, saveInpaintMask, saveInpaintResultLayer } = await import("../src/server/library");
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
    expect(hydratedPage?.inpaintResultDataUrl?.startsWith("/api/library/chapters/chapter-1/pages/page-1/images/inpaint-result")).toBe(true);
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

async function rgbaDataUrl(width: number, height: number, pixels: [number, number, number, number][]): Promise<string> {
  return `data:image/png;base64,${(await rgbaBuffer(width, height, pixels)).toString("base64")}`;
}

function cleanupSourceDataUrl(): Promise<string> {
  return rgbaDataUrl(5, 1, [
    [252, 252, 252, 255],
    [228, 228, 228, 255],
    [24, 24, 24, 255],
    [236, 236, 236, 255],
    [18, 18, 18, 255]
  ]);
}

function cleanupMaskDataUrl(): Promise<string> {
  return rgbaDataUrl(5, 1, [
    [0, 0, 0, 0],
    [0, 0, 0, 0],
    [255, 255, 255, 255],
    [0, 0, 0, 0],
    [0, 0, 0, 0]
  ]);
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
  return decodePngBuffer(await readFile(path));
}

async function decodePngBuffer(buffer: Buffer): Promise<Buffer> {
  const { data } = await sharp(buffer).ensureAlpha().raw().toBuffer({ resolveWithObject: true });
  return data;
}

async function decodePngDataUrl(dataUrl: string): Promise<Buffer> {
  const match = /^data:image\/png;base64,(.+)$/u.exec(dataUrl);
  if (!match) {
    throw new Error("PNG data URL expected");
  }
  const { data } = await sharp(Buffer.from(match[1], "base64")).ensureAlpha().raw().toBuffer({ resolveWithObject: true });
  return data;
}
