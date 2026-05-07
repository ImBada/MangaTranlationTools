import type express from "express";
import { readFile, stat, unlink, writeFile } from "node:fs/promises";
import { extname } from "node:path";
import { resolveLamaCommandFromEnv, runInpaintEngine } from "./inpaintEngine";
import { exportInpaintPsd, importInpaintPsd } from "./inpaintPsd";
import { clipOpaqueInpaintResultToMask, normalizeInpaintLayerDataUrls, normalizeInpaintMaskDataUrl } from "./libraryImageData";
import {
  getInpaintPsdImportPath,
  openChapter,
  saveImportedInpaintLayers,
  saveInpaintMask,
  saveNormalizedInpaintLayers,
  saveInpaintResultLayer
} from "./library";
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

export async function exportInpaintPsdRequest(request: ExportInpaintPsdRequest): Promise<Buffer> {
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
  return exportInpaintPsd(request);
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
    const chapter = await openChapter(chapterId);
    const page = chapter.pages.find((candidate) => candidate.id === pageId);
    if (!page) {
      throw new Error("페이지를 찾지 못했습니다.");
    }
    const psdBuffer = await readFile(file.path);
    const imported = await importInpaintPsd(psdBuffer, page.width, page.height);
    await writeFile(await getInpaintPsdImportPath(chapterId, pageId), psdBuffer);
    return {
      chapter: await saveImportedInpaintLayers(chapterId, pageId, imported.maskDataUrl, imported.resultDataUrl)
    };
  } finally {
    await unlink(file.path).catch(() => undefined);
  }
}

export async function readLastImportedPsd(chapterId: string, pageId: string): Promise<Buffer | null> {
  return readFile(await getInpaintPsdImportPath(chapterId, pageId)).catch(() => null);
}

export async function readLastImportedPsdMeta(chapterId: string, pageId: string): Promise<{ exists: boolean; importedAt?: string }> {
  const stats = await stat(await getInpaintPsdImportPath(chapterId, pageId)).catch(() => null);
  return stats ? { exists: true, importedAt: stats.mtime.toISOString() } : { exists: false };
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
