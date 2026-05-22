import type express from "express";
import { existsSync } from "node:fs";
import { homedir } from "node:os";
import { mkdir, open, readFile, stat, unlink, writeFile } from "node:fs/promises";
import { basename, extname, join, parse } from "node:path";
import { resolveLamaCommandFromEnv, runInpaintEngine } from "./inpaintEngine";
import { exportInpaintPsd, importInpaintPsd, sanitizePsdFileBasename } from "./inpaintPsd";
import { clipOpaqueInpaintResultToMask, normalizeInpaintLayerDataUrls, normalizeInpaintMaskDataUrl } from "./libraryImageData";
import {
  getInpaintPsdExportMetaPath,
  getInpaintPsdImportPath,
  openChapter,
  saveImportedInpaintLayers,
  saveInpaintMask,
  saveNormalizedInpaintLayers,
  saveInpaintResultLayer
} from "./library";
import { writeJsonFile } from "./libraryFileIO";
import { getActiveJob } from "./jobState";
import type {
  ExportInpaintPsdRequest,
  ImportInpaintPsdResult,
  InpaintEngine,
  InpaintSettings,
  MangaPage,
  InpaintPageRequest,
  InpaintPageResult,
  PageImageLayer,
  SaveInpaintLayersRequest,
  SaveInpaintLayersResult,
  SaveInpaintMaskRequest,
  SaveInpaintMaskResult,
  SaveInpaintResultLayerRequest,
  SaveInpaintResultLayerResult
} from "../shared/types";

type ExportedInpaintPsdMeta = {
  exportedAt: string;
  filename: string;
  path: string;
};

type ExportedInpaintPsdResult = {
  filename: string;
};

export async function inpaintPage(request: InpaintPageRequest): Promise<InpaintPageResult> {
  if (getActiveJob()) {
    throw new Error("번역 작업 중에는 인페인트를 실행할 수 없습니다.");
  }

  if (!request.chapterId || !request.pageId) {
    throw new Error("인페인트할 페이지 정보가 없습니다.");
  }
  assertImageDataUrl(request.sourceDataUrl, "원본 이미지");
  assertImageDataUrl(request.maskDataUrl, "인페인트 마스크");

  const currentChapter = await openChapter(request.chapterId);
  const currentPage = currentChapter.pages.find((candidate) => candidate.id === request.pageId);
  if (!currentPage) {
    throw new Error("페이지를 찾지 못했습니다.");
  }

  const engine = resolveAvailableInpaintEngine(request.settings.engine);
  const settings = resolvePageInpaintSettings(request.settings, engine, currentPage);
  const resultDataUrl = await runInpaintEngine(request.sourceDataUrl, request.maskDataUrl, engine, {
    ...resolveLamaCommandFromEnv(process.env),
    settings
  });
  const layers = await normalizeInpaintLayerDataUrls(request.maskDataUrl, resultDataUrl);
  if (request.persistResult === false) {
    return {
      chapter: currentChapter,
      resultDataUrl: layers.resultDataUrl,
      maskDataUrl: layers.maskDataUrl,
      engine
    };
  }
  const chapter = await saveNormalizedInpaintLayers(request.chapterId, request.pageId, layers.maskDataUrl, layers.resultDataUrl, settings);
  return {
    chapter,
    resultDataUrl: layers.resultDataUrl,
    maskDataUrl: layers.maskDataUrl,
    engine
  };
}

export async function saveInpaintMaskRequest(request: SaveInpaintMaskRequest): Promise<SaveInpaintMaskResult> {
  if (!request.chapterId || !request.pageId) {
    throw new Error("저장할 인페인트 마스크 페이지 정보가 없습니다.");
  }
  if (request.maskDataUrl) {
    assertImageDataUrl(request.maskDataUrl, "인페인트 마스크");
  }

  return {
    chapter: await saveInpaintMask(request.chapterId, request.pageId, request.maskDataUrl)
  };
}

export async function saveInpaintResultLayerRequest(request: SaveInpaintResultLayerRequest): Promise<SaveInpaintResultLayerResult> {
  if (!request.chapterId || !request.pageId) {
    throw new Error("저장할 인페인트 결과 레이어 페이지 정보가 없습니다.");
  }
  if (request.resultDataUrl) {
    assertImageDataUrl(request.resultDataUrl, "인페인트 결과 레이어");
  }

  return {
    chapter: await saveInpaintResultLayer(request.chapterId, request.pageId, request.resultDataUrl)
  };
}

export async function saveInpaintLayersRequest(request: SaveInpaintLayersRequest): Promise<SaveInpaintLayersResult> {
  if (!request.chapterId || !request.pageId) {
    throw new Error("저장할 인페인트 레이어 페이지 정보가 없습니다.");
  }
  assertImageDataUrl(request.maskDataUrl, "인페인트 마스크");
  assertImageDataUrl(request.resultDataUrl, "인페인트 결과 레이어");

  const layers = request.preserveMaskDataUrl
    ? await resolvePreservedInpaintLayers(request.maskDataUrl, request.resultDataUrl)
    : await normalizeInpaintLayerDataUrls(request.maskDataUrl, request.resultDataUrl);
  return {
    chapter: await saveNormalizedInpaintLayers(request.chapterId, request.pageId, layers.maskDataUrl, layers.resultDataUrl)
  };
}

async function resolvePreservedInpaintLayers(maskDataUrl: string, resultDataUrl: string): Promise<{ maskDataUrl: string; resultDataUrl: string }> {
  const normalizedMaskDataUrl = await normalizeInpaintMaskDataUrl(maskDataUrl);
  return {
    maskDataUrl: normalizedMaskDataUrl,
    resultDataUrl: await clipOpaqueInpaintResultToMask(resultDataUrl, normalizedMaskDataUrl)
  };
}

export async function exportInpaintPsdRequest(request: ExportInpaintPsdRequest): Promise<ExportedInpaintPsdResult> {
  if (!request.chapterId || !request.pageId) {
    throw new Error("내보낼 인페인트 PSD 페이지 정보가 없습니다.");
  }
  assertImageDataUrl(request.sourceDataUrl, "원본 이미지");
  if (request.maskDataUrl) {
    assertImageDataUrl(request.maskDataUrl, "인페인트 마스크");
  }
  if (request.resultDataUrl) {
    assertImageDataUrl(request.resultDataUrl, "인페인트 결과 레이어");
  }
  if (request.translationBlocksDataUrl) {
    assertImageDataUrl(request.translationBlocksDataUrl, "번역 블록 레이어");
  }
  const filename = `${sanitizePsdFileBasename(request.pageName || request.pageId, request.pageId || "inpaint")}-inpaint.psd`;
  const buffer = await exportInpaintPsd(request);
  return writeExportedInpaintPsd(request.chapterId, request.pageId, filename, buffer);
}

export async function importInpaintPsdRequest(req: express.Request): Promise<ImportInpaintPsdResult> {
  const chapterId = typeof req.body?.chapterId === "string" ? req.body.chapterId : "";
  const pageId = typeof req.body?.pageId === "string" ? req.body.pageId : "";
  const file = req.file;
  if (!chapterId || !pageId) {
    throw new Error("가져올 인페인트 PSD 페이지 정보가 없습니다.");
  }
  if (!file) {
    throw new Error("가져올 PSD 파일이 없습니다.");
  }

  try {
    return await importInpaintPsdBuffer(chapterId, pageId, await readFile(file.path));
  } finally {
    await unlink(file.path).catch(() => undefined);
  }
}

export async function quickImportLastExportedInpaintPsdRequest(chapterId: string, pageId: string): Promise<ImportInpaintPsdResult> {
  if (!chapterId || !pageId) {
    throw new Error("퀵로드할 인페인트 PSD 페이지 정보가 없습니다.");
  }
  const meta = await readExportedInpaintPsdMeta(chapterId, pageId);
  if (!meta) {
    throw new Error("현재 페이지에서 마지막으로 내보낸 PSD 파일 정보가 없습니다.");
  }
  if (!existsSync(meta.path)) {
    throw new Error(`마지막으로 내보낸 PSD 파일을 찾지 못했습니다: ${meta.filename}`);
  }
  return importInpaintPsdBuffer(chapterId, pageId, await readFile(meta.path));
}

export async function readLastImportedPsd(chapterId: string, pageId: string): Promise<Buffer | null> {
  return readFile(await getInpaintPsdImportPath(chapterId, pageId)).catch(() => null);
}

export async function readInpaintPsdMeta(
  chapterId: string,
  pageId: string
): Promise<{ exists: boolean; importedAt?: string; quickLoad: { exists: boolean; exportedAt?: string; filename?: string } }> {
  const stats = await stat(await getInpaintPsdImportPath(chapterId, pageId)).catch(() => null);
  const exported = await readExportedInpaintPsdMeta(chapterId, pageId);
  const quickLoad = exported && existsSync(exported.path)
    ? { exists: true, exportedAt: exported.exportedAt, filename: exported.filename }
    : { exists: false };
  return stats ? { exists: true, importedAt: stats.mtime.toISOString(), quickLoad } : { exists: false, quickLoad };
}

export function readOptionalPsdPageQuery(req: express.Request): { chapterId: string; pageId: string } | null {
  const chapterId = typeof req.query.chapterId === "string" ? req.query.chapterId : "";
  const pageId = typeof req.query.pageId === "string" ? req.query.pageId : "";
  return chapterId && pageId ? { chapterId, pageId } : null;
}

export function readPsdPageQuery(req: express.Request): { chapterId: string; pageId: string } {
  const pageQuery = readOptionalPsdPageQuery(req);
  if (!pageQuery) {
    throw new Error("내려받을 PSD 페이지 정보가 없습니다.");
  }
  return pageQuery;
}

export function isPageImageLayer(value: string): value is PageImageLayer {
  return value === "source" || value === "inpaint-mask" || value === "inpaint-result";
}

function resolveAvailableInpaintEngine(requested: InpaintEngine): InpaintEngine {
  if (requested === "lama" && process.env.MANGA_TRANSLATOR_LAMA_COMMAND?.trim()) {
    return "lama";
  }
  return "local-fill-fallback";
}

function resolvePageInpaintSettings(settings: InpaintSettings, engine: InpaintEngine, page: MangaPage): InpaintSettings {
  return {
    ...settings,
    engine,
    artifactCleanupPx: isJpegSourcePage(page) ? settings.artifactCleanupPx : 0
  };
}

function isJpegSourcePage(page: MangaPage): boolean {
  const extension = extname(page.imagePath || page.name).toLowerCase();
  return extension === ".jpg" || extension === ".jpeg";
}

function assertImageDataUrl(value: string, label: string): void {
  if (!/^data:image\/(?:png|jpeg|jpg|webp);base64,[A-Za-z0-9+/=]+$/u.test(value)) {
    throw new Error(`${label} 데이터 URL이 올바르지 않습니다.`);
  }
}

async function importInpaintPsdBuffer(chapterId: string, pageId: string, psdBuffer: Buffer): Promise<ImportInpaintPsdResult> {
  const chapter = await openChapter(chapterId);
  const page = chapter.pages.find((candidate) => candidate.id === pageId);
  if (!page) {
    throw new Error("페이지를 찾지 못했습니다.");
  }
  const imported = await importInpaintPsd(psdBuffer, page.width, page.height);
  await writeFile(await getInpaintPsdImportPath(chapterId, pageId), psdBuffer);
  return {
    chapter: await saveImportedInpaintLayers(chapterId, pageId, imported.maskDataUrl, imported.resultDataUrl)
  };
}

async function writeExportedInpaintPsd(chapterId: string, pageId: string, filename: string, buffer: Buffer): Promise<ExportedInpaintPsdResult> {
  const outputPath = await writeUniqueDownloadFile(filename, buffer);
  const meta: ExportedInpaintPsdMeta = {
    exportedAt: new Date().toISOString(),
    filename: basename(outputPath),
    path: outputPath
  };
  await writeJsonFile(await getInpaintPsdExportMetaPath(chapterId, pageId), meta);
  return {
    filename: meta.filename
  };
}

async function readExportedInpaintPsdMeta(chapterId: string, pageId: string): Promise<ExportedInpaintPsdMeta | null> {
  const raw = await readFile(await getInpaintPsdExportMetaPath(chapterId, pageId), "utf8").catch(() => null);
  if (!raw) {
    return null;
  }
  const parsed = parseExportedInpaintPsdMeta(raw);
  if (!parsed.path || !parsed.filename || !parsed.exportedAt || basename(parsed.path) !== parsed.filename) {
    return null;
  }
  return {
    exportedAt: parsed.exportedAt,
    filename: parsed.filename,
    path: parsed.path
  };
}

async function writeUniqueDownloadFile(filename: string, buffer: Buffer): Promise<string> {
  const downloadsDir = process.env.MANGA_TRANSLATOR_DOWNLOADS_DIR?.trim() || join(homedir(), "Downloads");
  const safeFilename = basename(filename);
  const parsed = parse(safeFilename);
  await mkdir(downloadsDir, { recursive: true });

  let index = 1;

  while (true) {
    const candidate = join(downloadsDir, index === 1 ? safeFilename : `${parsed.name} (${index - 1})${parsed.ext}`);
    const handle = await openUniqueFile(candidate);
    if (!handle) {
      index += 1;
      continue;
    }

    try {
      await handle.writeFile(buffer);
    } catch (error) {
      await unlink(candidate).catch(() => undefined);
      throw error;
    } finally {
      await handle.close();
    }
    return candidate;
  }
}

async function openUniqueFile(path: string) {
  try {
    return await open(path, "wx");
  } catch (error) {
    if (isFileExistsError(error)) {
      return null;
    }
    throw error;
  }
}

function parseExportedInpaintPsdMeta(raw: string): Partial<ExportedInpaintPsdMeta> {
  try {
    return JSON.parse(raw) as Partial<ExportedInpaintPsdMeta>;
  } catch {
    return {};
  }
}

function isFileExistsError(error: unknown): boolean {
  return typeof error === "object" && error !== null && "code" in error && (error as { code?: unknown }).code === "EEXIST";
}
