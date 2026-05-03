import express from "express";
import multer from "multer";
import { randomUUID } from "node:crypto";
import { mkdirSync } from "node:fs";
import { readFile, stat, unlink, writeFile } from "node:fs/promises";
import { createServer } from "node:http";
import { join } from "node:path";
import { ensureWritableAppDirectories } from "./appPaths";
import { buildBaseTranslationOptions } from "./appSettings";
import { resolveLamaCommandFromEnv, runInpaintEngine } from "./inpaintEngine";
import { exportInpaintPsd, importInpaintPsd, sanitizePsdFileBasename } from "./inpaintPsd";
import { configureLamaEnvironment, getLamaRuntimeStatus, startLamaModelDownload, startLamaRuntimePrepare } from "./lamaRuntime";
import {
  cleanupLegacyLogs,
  createImport,
  deleteChapter,
  deletePage,
  deleteWork,
  finalizeRunningPages,
  getInpaintPsdImportPath,
  getLibraryRoot,
  getRunPaths,
  listLibrary,
  markChapterPagesRunning,
  openChapter,
  patchChapterSnapshot,
  previewUploadedFiles,
  readPageImageAsset,
  renameChapter,
  renameWork,
  reorderChapters,
  reorderPages,
  resolvePagesForRun,
  saveInpaintMask,
  saveImportedInpaintLayers,
  saveInpaintResult,
  saveInpaintResultLayer,
  saveRenderedPage,
  saveChapterSnapshot,
  updatePageAfterAnalysis,
  type UploadedImportFile
} from "./library";
import { getLogPath, logError, logInfo, resetAppLog, writeLog } from "./logger";
import { createOpenAICompatibleEndpoint, stopOpenAICompatibleEndpoint, type OpenAICompatibleEndpoint } from "./openaiCompatibleEndpoint";
import { startOpenAIOAuthEndpoint, stopOpenAIOAuthEndpoint, type OpenAIOAuthEndpoint } from "./openaiOauthEndpoint";
import { getAppSettings, resetAppSettings, saveAppSettings } from "./settingsStore";
import { listSystemFonts } from "./systemFonts";
import { getUpdateStatus } from "./updateCheck";
import { runWholePagePipeline } from "./wholePagePipeline";
import type {
  AppSettings,
  CreateImportRequest,
  ExportInpaintPsdRequest,
  ImportInpaintPsdResult,
  InpaintEngine,
  InpaintPageRequest,
  InpaintPageResult,
  ImportSourceKind,
  JobEvent,
  ModelTestResult,
  PageImageLayer,
  RenderPageRequest,
  SaveInpaintResultLayerRequest,
  SaveInpaintResultLayerResult,
  SaveInpaintMaskRequest,
  SaveInpaintMaskResult,
  StartAnalysisRequest,
  StartAnalysisResult
} from "../shared/types";

const appPaths = ensureWritableAppDirectories();
configureLamaEnvironment(appPaths);
const serverPort = Number(process.env.PORT || process.env.MANGA_TRANSLATOR_PORT || 3000);
const uploadDir = join(appPaths.dataRoot, "uploads");
mkdirSync(uploadDir, { recursive: true });
resetAppLog();

type SimplePageRuntime = {
  startServer: (options: Record<string, unknown>) => Promise<{ baseUrl: string; child: unknown; startedByScript: boolean }>;
  stopServer: (server: { baseUrl: string; child: unknown; startedByScript: boolean } | null | undefined) => Promise<void>;
  testModelReply: (server: { baseUrl: string }, options: Record<string, unknown>) => Promise<{
    outputText: string;
    launchTarget: { launchMode: string; modelPath?: string | null; mmprojPath?: string | null };
  }>;
};

let activeJob: {
  id: string;
  abortController: AbortController;
  cleanup?: () => Promise<void>;
  lastEvent?: JobEvent;
} | null = null;
const eventClients = new Set<express.Response>();
let cachedSimplePageRuntime: SimplePageRuntime | null = null;

logInfo("Web server starting", {
  cwd: process.cwd(),
  logPath: getLogPath(),
  libraryPath: getLibraryRoot(),
  settingsPath: appPaths.settingsPath,
  dataRoot: appPaths.dataRoot,
  runtimeDir: appPaths.runtimeDir,
  port: serverPort
});

process.on("uncaughtException", (error) => {
  logError("Uncaught exception", error);
});

process.on("unhandledRejection", (reason) => {
  logError("Unhandled rejection", reason);
});

const app = express();
const upload = multer({ dest: uploadDir });

app.use(express.json({ limit: "120mb" }));
app.use((req, res, next) => {
  res.setHeader("Access-Control-Allow-Origin", req.headers.origin || "*");
  res.setHeader("Access-Control-Allow-Headers", "Content-Type");
  res.setHeader("Access-Control-Allow-Methods", "GET,POST,DELETE,OPTIONS");
  if (req.method === "OPTIONS") {
    res.sendStatus(204);
    return;
  }
  next();
});

app.get("/api/health", (_req, res) => {
  res.json({ ok: true });
});

app.get("/api/update/status", asyncHandler(async (req, res) => {
  res.json(await getUpdateStatus(appPaths, { refresh: req.query.refresh === "1" }));
}));

app.post("/api/logs/write", (req, res) => {
  const { level, message, detail } = req.body as { level: "debug" | "info" | "warn" | "error"; message: string; detail?: unknown };
  writeLog(level, `client: ${message}`, detail);
  res.json({ logged: true });
});

app.get("/api/settings", asyncHandler(async (_req, res) => {
  res.json(await getAppSettings());
}));

app.get("/api/fonts", asyncHandler(async (_req, res) => {
  res.json(await listSystemFonts());
}));

app.post("/api/settings", asyncHandler(async (req, res) => {
  res.json(await saveAppSettings(req.body));
}));

app.post("/api/settings/reset", asyncHandler(async (_req, res) => {
  res.json(await resetAppSettings());
}));

app.post("/api/settings/test-model", asyncHandler(async (req, res) => {
  res.json(await testModelSettings(req.body));
}));

app.get("/api/library", asyncHandler(async (_req, res) => {
  res.json(await listLibrary());
}));

app.get("/api/library/chapters/:chapterId", asyncHandler(async (req, res) => {
  res.json(await openChapter(String(req.params.chapterId)));
}));

app.get("/api/library/chapters/:chapterId/pages/:pageId/images/:layer", asyncHandler(async (req, res) => {
  const layer = String(req.params.layer);
  if (!isPageImageLayer(layer)) {
    res.status(404).json({ error: "요청한 이미지 레이어를 찾지 못했습니다." });
    return;
  }
  const asset = await readPageImageAsset(String(req.params.chapterId), String(req.params.pageId), layer);
  res.setHeader("Content-Type", asset.mime);
  res.setHeader("Cache-Control", "no-store");
  res.setHeader("Last-Modified", new Date(asset.updatedAt).toUTCString());
  res.end(asset.buffer);
}));

app.post("/api/library/chapters", asyncHandler(async (req, res) => {
  const body = req.body;
  const chapter = body?.chapter ?? body;
  const dirtyPageIds = Array.isArray(body?.dirtyPageIds) ? body.dirtyPageIds.filter((id: unknown) => typeof id === "string") : undefined;
  res.json(await saveChapterSnapshot(chapter, { dirtyPageIds }));
}));

app.post("/api/library/chapters/:chapterId/patch", asyncHandler(async (req, res) => {
  res.json(await patchChapterSnapshot(String(req.params.chapterId), req.body));
}));

app.post("/api/render/page", asyncHandler(async (req, res) => {
  const request = req.body as RenderPageRequest;
  res.json(await saveRenderedPage(request.chapterId, request.pageId, request.dataUrl));
}));

app.post("/api/inpaint/page", asyncHandler(async (req, res) => {
  res.json(await inpaintPage(req.body as InpaintPageRequest));
}));

app.post("/api/inpaint/mask", asyncHandler(async (req, res) => {
  res.json(await saveInpaintMaskRequest(req.body as SaveInpaintMaskRequest));
}));

app.post("/api/inpaint/result-layer", asyncHandler(async (req, res) => {
  res.json(await saveInpaintResultLayerRequest(req.body as SaveInpaintResultLayerRequest));
}));

app.post("/api/inpaint/psd/export", asyncHandler(async (req, res) => {
  const request = req.body as ExportInpaintPsdRequest;
  const buffer = await exportInpaintPsdRequest(request);
  const filename = `${sanitizePsdFileBasename(request.pageName || request.pageId, request.pageId || "inpaint")}-inpaint.psd`;
  res.setHeader("Content-Type", "image/vnd.adobe.photoshop");
  res.setHeader("Content-Disposition", `attachment; filename="inpaint.psd"; filename*=UTF-8''${encodeURIComponent(filename)}`);
  res.end(buffer);
}));

app.post("/api/inpaint/psd/import", upload.single("file"), asyncHandler(async (req, res) => {
  res.json(await importInpaintPsdRequest(req));
}));

app.get("/api/inpaint/psd/last-import", asyncHandler(async (req, res) => {
  const { chapterId, pageId } = readPsdPageQuery(req);
  const psdPath = await getInpaintPsdImportPath(chapterId, pageId);
  const buffer = await readFile(psdPath).catch(() => null);
  if (!buffer) {
    res.status(404).json({ error: "현재 페이지에서 마지막으로 가져온 PSD 파일이 없습니다." });
    return;
  }
  res.setHeader("Content-Type", "image/vnd.adobe.photoshop");
  res.setHeader("Content-Disposition", "attachment; filename=\"last-imported-inpaint.psd\"");
  res.end(buffer);
}));

app.get("/api/inpaint/psd/last-import/meta", asyncHandler(async (req, res) => {
  const pageQuery = readOptionalPsdPageQuery(req);
  if (!pageQuery) {
    res.json({ exists: false });
    return;
  }
  const psdPath = await getInpaintPsdImportPath(pageQuery.chapterId, pageQuery.pageId);
  const stats = await stat(psdPath).catch(() => null);
  res.json(stats ? { exists: true, importedAt: stats.mtime.toISOString() } : { exists: false });
}));

app.get("/api/lama/status", asyncHandler(async (_req, res) => {
  res.json(getLamaRuntimeStatus(appPaths));
}));

app.post("/api/lama/prepare", asyncHandler(async (_req, res) => {
  res.json(startLamaRuntimePrepare(appPaths));
}));

app.post("/api/lama/model/download", asyncHandler(async (_req, res) => {
  res.json(startLamaModelDownload(appPaths));
}));

app.post("/api/library/works/:workId/rename", asyncHandler(async (req, res) => {
  res.json(await renameWork(String(req.params.workId), String(req.body.title ?? "")));
}));

app.post("/api/library/chapters/:chapterId/rename", asyncHandler(async (req, res) => {
  res.json(await renameChapter(String(req.params.chapterId), String(req.body.title ?? "")));
}));

app.delete("/api/library/works/:workId", asyncHandler(async (req, res) => {
  res.json(await deleteWork(String(req.params.workId)));
}));

app.delete("/api/library/chapters/:chapterId", asyncHandler(async (req, res) => {
  res.json(await deleteChapter(String(req.params.chapterId)));
}));

app.post("/api/library/works/:workId/reorder-chapters", asyncHandler(async (req, res) => {
  res.json(await reorderChapters(String(req.params.workId), req.body.chapterIds));
}));

app.post("/api/library/chapters/:chapterId/reorder-pages", asyncHandler(async (req, res) => {
  res.json(await reorderPages(String(req.params.chapterId), req.body.pageIds));
}));

app.delete("/api/library/chapters/:chapterId/pages/:pageId", asyncHandler(async (req, res) => {
  res.json(await deletePage(String(req.params.chapterId), String(req.params.pageId)));
}));

app.post("/api/import/preview/:kind", upload.array("files"), asyncHandler(async (req, res) => {
  const kind = req.params.kind as ImportSourceKind;
  const relativePaths = parseRelativePaths(req.body.relativePaths);
  const files = ((req.files as Express.Multer.File[] | undefined) ?? []).map((file, index): UploadedImportFile => ({
    path: file.path,
    name: file.originalname,
    relativePath: relativePaths[index] || file.originalname
  }));
  const preview = await previewUploadedFiles(kind, files);
  res.json(preview.chapters.length ? preview : null);
}));

app.post("/api/import/create", asyncHandler(async (req, res) => {
  if (activeJob) {
    res.status(409).json({ error: "이미 실행 중인 작업이 있습니다." });
    return;
  }

  const request = req.body as CreateImportRequest;
  const id = randomUUID();
  const abortController = new AbortController();
  activeJob = { id, abortController };
  const emit = (event: JobEvent) => {
    if (activeJob?.id === id) {
      activeJob.lastEvent = event;
    }
    writeLog(event.status === "failed" ? "error" : event.status === "cancelled" ? "warn" : "info", `job:${event.kind}:${event.status}`, event);
    emitJobEvent(event);
  };

  try {
    const result = await createImport(request, { jobId: id, signal: abortController.signal, emit });
    res.json(result);
  } catch (error) {
    const lastEvent = activeJob?.id === id ? activeJob.lastEvent : undefined;
    if (isAbortError(error) || abortController.signal.aborted) {
      emit({
        id,
        kind: "library-import",
        status: "cancelled",
        progressText: "가져오기가 취소되었습니다.",
        phase: "cancelled",
        progressCurrent: lastEvent?.progressCurrent,
        progressTotal: lastEvent?.progressTotal,
        pageIndex: lastEvent?.pageIndex,
        pageTotal: lastEvent?.pageTotal
      });
      res.status(499).json({ error: "가져오기가 취소되었습니다." });
      return;
    }

    const message = error instanceof Error ? error.message : String(error);
    logError("Import job failed", { jobId: id, request, lastEvent, error });
    emit({
      id,
      kind: "library-import",
      status: "failed",
      progressText: "가져오기 실패",
      phase: "failed",
      detail: message
    });
    res.status(500).json({ error: message });
  } finally {
    activeJob = null;
  }
}));

app.post("/api/jobs/start-analysis", asyncHandler(async (req, res) => {
  res.json(await startAnalysis(req.body as StartAnalysisRequest));
}));

app.post("/api/jobs/cancel", asyncHandler(async (_req, res) => {
  if (!activeJob) {
    res.json({ cancelled: false });
    return;
  }

  const job = activeJob;
  emitJobEvent({
    id: job.id,
    kind: job.lastEvent?.kind ?? "gemma-analysis",
    status: "cancelling",
    progressText: "작업 취소 중",
    progressCurrent: job.lastEvent?.progressCurrent,
    progressTotal: job.lastEvent?.progressTotal,
    pageIndex: job.lastEvent?.pageIndex,
    pageTotal: job.lastEvent?.pageTotal,
    attempt: job.lastEvent?.attempt,
    attemptTotal: job.lastEvent?.attemptTotal
  });
  job.abortController.abort();
  await job.cleanup?.();
  res.json({ cancelled: true });
}));

app.get("/api/jobs/events", (req, res) => {
  res.writeHead(200, {
    "Content-Type": "text/event-stream",
    "Cache-Control": "no-cache, no-transform",
    Connection: "keep-alive"
  });
  res.write("\n");
  eventClients.add(res);
  req.on("close", () => {
    eventClients.delete(res);
  });
});

app.use(express.static(join(appPaths.repoRoot, "out", "client")));

const httpServer = createServer(app);
httpServer.listen(serverPort, "127.0.0.1", async () => {
  await cleanupLegacyLogs();
  logInfo("Manga translator web app ready", { url: `http://127.0.0.1:${serverPort}` });
});

process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);

async function startAnalysis(request: StartAnalysisRequest): Promise<StartAnalysisResult> {
  if (activeJob) {
    return { status: "failed", error: "이미 실행 중인 작업이 있습니다." };
  }

  const resolved = await resolvePagesForRun(request.chapterId, request.runMode, request.pageId);
  if (resolved.pages.length === 0) {
    return { status: "completed", chapter: resolved.chapter, warnings: [] };
  }

  const id = randomUUID();
  const abortController = new AbortController();
  const pageIds = resolved.pages.map((page) => page.id);
  let runPaths: Awaited<ReturnType<typeof getRunPaths>> | null = null;
  await markChapterPagesRunning(request.chapterId, pageIds);
  activeJob = { id, abortController };

  const emit = (event: JobEvent) => {
    if (activeJob?.id === id) {
      activeJob.lastEvent = event;
    }
    writeLog(event.status === "failed" ? "error" : event.status === "cancelled" ? "warn" : "info", `job:${event.kind}:${event.status}`, event);
    emitJobEvent(event);
  };

  try {
    runPaths = await getRunPaths(request.chapterId, id);
    const result = await runWholePagePipeline({
      jobId: id,
      emit,
      onCleanupReady: (cleanup) => {
        if (activeJob?.id === id) {
          activeJob.cleanup = cleanup;
        }
      },
      onPageComplete: async (page) => updatePageAfterAnalysis(request.chapterId, page, [], "completed"),
      onPageFailed: async (page, errorMessage) => updatePageAfterAnalysis(request.chapterId, page, [errorMessage], "failed"),
      pages: resolved.pages,
      runPaths,
      signal: abortController.signal
    });

    if (abortController.signal.aborted) {
      throw new DOMException("Aborted", "AbortError");
    }

    emit({
      id,
      kind: "gemma-analysis",
      status: "completed",
      progressText: "번역 작업 완료",
      phase: "done",
      progressCurrent: resolved.pages.length,
      progressTotal: resolved.pages.length,
      pageTotal: resolved.pages.length
    });

    return { status: "completed", chapter: await openChapter(request.chapterId), warnings: result.warnings };
  } catch (error) {
    const lastEvent = activeJob?.id === id ? activeJob.lastEvent : undefined;
    if (isAbortError(error) || abortController.signal.aborted) {
      await finalizeRunningPages(request.chapterId, pageIds, "idle");
      emit({
        id,
        kind: "gemma-analysis",
        status: "cancelled",
        progressText: "작업이 취소되었습니다.",
        phase: "cancelled",
        progressCurrent: lastEvent?.progressCurrent,
        progressTotal: lastEvent?.progressTotal,
        pageIndex: lastEvent?.pageIndex,
        pageTotal: lastEvent?.pageTotal,
        attempt: lastEvent?.attempt,
        attemptTotal: lastEvent?.attemptTotal
      });
      return { status: "cancelled", chapter: await openChapter(request.chapterId) };
    }

    const message = error instanceof Error ? error.message : String(error);
    await finalizeRunningPages(request.chapterId, pageIds, "failed", message);
    logError("Analysis job failed", { jobId: id, request, runPaths, lastEvent, error });
    emit({
      id,
      kind: "gemma-analysis",
      status: "failed",
      progressText: "작업 실패",
      phase: "failed",
      detail: message
    });
    return { status: "failed", error: message, chapter: await openChapter(request.chapterId) };
  } finally {
    activeJob = null;
  }
}

async function inpaintPage(request: InpaintPageRequest): Promise<InpaintPageResult> {
  if (activeJob) {
    throw new Error("번역 작업 중에는 인페인트를 실행할 수 없습니다.");
  }

  if (!request.chapterId || !request.pageId) {
    throw new Error("인페인트할 페이지 정보가 없습니다.");
  }
  assertImageDataUrl(request.sourceDataUrl, "원본 이미지");
  assertImageDataUrl(request.maskDataUrl, "인페인트 마스크");

  const engine = resolveAvailableInpaintEngine(request.settings.engine);
  const settings = {
    ...request.settings,
    engine
  };
  const resultDataUrl = await runInpaintEngine(request.sourceDataUrl, request.maskDataUrl, engine, {
    ...resolveLamaCommandFromEnv(process.env),
    settings
  });
  if (request.persistResult === false) {
    return {
      chapter: await openChapter(request.chapterId),
      resultDataUrl,
      engine
    };
  }
  const chapter = await saveInpaintResult(request.chapterId, request.pageId, request.maskDataUrl, resultDataUrl, settings);
  return {
    chapter,
    resultDataUrl,
    engine
  };
}

async function saveInpaintMaskRequest(request: SaveInpaintMaskRequest): Promise<SaveInpaintMaskResult> {
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

async function saveInpaintResultLayerRequest(request: SaveInpaintResultLayerRequest): Promise<SaveInpaintResultLayerResult> {
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

async function exportInpaintPsdRequest(request: ExportInpaintPsdRequest): Promise<Buffer> {
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
  return exportInpaintPsd(request);
}

async function importInpaintPsdRequest(req: express.Request): Promise<ImportInpaintPsdResult> {
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

function readOptionalPsdPageQuery(req: express.Request): { chapterId: string; pageId: string } | null {
  const chapterId = typeof req.query.chapterId === "string" ? req.query.chapterId : "";
  const pageId = typeof req.query.pageId === "string" ? req.query.pageId : "";
  return chapterId && pageId ? { chapterId, pageId } : null;
}

function readPsdPageQuery(req: express.Request): { chapterId: string; pageId: string } {
  const pageQuery = readOptionalPsdPageQuery(req);
  if (!pageQuery) {
    throw new Error("내려받을 PSD 페이지 정보가 없습니다.");
  }
  return pageQuery;
}

function resolveAvailableInpaintEngine(requested: InpaintEngine): InpaintEngine {
  if (requested === "lama" && process.env.MANGA_TRANSLATOR_LAMA_COMMAND?.trim()) {
    return "lama";
  }
  return "local-fill-fallback";
}

function assertImageDataUrl(value: string, label: string): void {
  if (!/^data:image\/(?:png|jpeg|jpg|webp);base64,[A-Za-z0-9+/=]+$/u.test(value)) {
    throw new Error(`${label} 데이터 URL이 올바르지 않습니다.`);
  }
}

function isPageImageLayer(value: string): value is PageImageLayer {
  return value === "source" || value === "inpaint-mask" || value === "inpaint-result";
}

async function testModelSettings(settings: AppSettings): Promise<ModelTestResult> {
  if (activeJob) {
    return { ok: false, message: "번역 작업 중에는 모델 테스트를 실행할 수 없습니다.", launchMode: modelProviderToTestLaunchMode(settings.modelProvider) };
  }

  const testId = randomUUID();
  const options = buildBaseTranslationOptions({
    jobId: `settings-test-${testId}`,
    runDir: join(appPaths.dataRoot, "model-tests", testId),
    paths: appPaths,
    settings
  });
  let server: OpenAIOAuthEndpoint | OpenAICompatibleEndpoint | { baseUrl: string; child: unknown; startedByScript: boolean } | null = null;

  try {
    server = options.modelProvider === "openai-compatible"
      ? await createOpenAICompatibleEndpoint(options)
      : options.modelProvider === "openai-codex"
        ? await startOpenAIOAuthEndpoint(options)
        : await loadSimplePageRuntime().startServer(options);
    const result = await loadSimplePageRuntime().testModelReply(server, options);
    return {
      ok: true,
      message: `모델 응답 확인 완료: ${result.outputText}`,
      launchMode: result.launchTarget.launchMode as ModelTestResult["launchMode"],
      resolvedEndpoint: server.baseUrl
    };
  } catch (error) {
    return {
      ok: false,
      message: error instanceof Error ? error.message : String(error),
      launchMode: modelProviderToTestLaunchMode(options.modelProvider)
    };
  } finally {
    if (server) {
      if (isOpenAICompatibleEndpoint(server)) {
        await stopOpenAICompatibleEndpoint(server);
      } else if (isOpenAIOAuthEndpoint(server)) {
        await stopOpenAIOAuthEndpoint(server);
      } else {
        await loadSimplePageRuntime().stopServer(server);
      }
    }
  }
}

function isOpenAICompatibleEndpoint(
  server: OpenAIOAuthEndpoint | OpenAICompatibleEndpoint | { baseUrl: string; child: unknown; startedByScript: boolean }
): server is OpenAICompatibleEndpoint {
  return "provider" in server && server.provider === "openai-compatible";
}

function isOpenAIOAuthEndpoint(
  server: OpenAIOAuthEndpoint | OpenAICompatibleEndpoint | { baseUrl: string; child: unknown; startedByScript: boolean }
): server is OpenAIOAuthEndpoint {
  return "provider" in server && server.provider === "openai-codex";
}

function modelProviderToTestLaunchMode(modelProvider: AppSettings["modelProvider"]): ModelTestResult["launchMode"] {
  if (modelProvider === "openai-compatible" || modelProvider === "openai-codex") {
    return modelProvider;
  }
  return "huggingface";
}

function loadSimplePageRuntime(): SimplePageRuntime {
  if (!cachedSimplePageRuntime) {
    cachedSimplePageRuntime = require(join(appPaths.runtimeDir, "simple-page-translate.cjs")) as SimplePageRuntime;
  }
  return cachedSimplePageRuntime;
}

function emitJobEvent(event: JobEvent): void {
  for (const client of eventClients) {
    client.write(`data: ${JSON.stringify(event)}\n\n`);
  }
}

function parseRelativePaths(value: unknown): string[] {
  if (typeof value !== "string") {
    return [];
  }
  try {
    const parsed = JSON.parse(value);
    return Array.isArray(parsed) ? parsed.map(String) : [];
  } catch {
    return [];
  }
}

function asyncHandler(handler: express.RequestHandler): express.RequestHandler {
  return (req, res, next) => {
    Promise.resolve(handler(req, res, next)).catch(next);
  };
}

function isAbortError(error: unknown): boolean {
  return error instanceof DOMException && error.name === "AbortError";
}

function shutdown(): void {
  if (activeJob) {
    activeJob.abortController.abort();
    void activeJob.cleanup?.();
  }
  httpServer.close(() => process.exit(0));
}

app.use((error: unknown, _req: express.Request, res: express.Response, _next: express.NextFunction) => {
  const message = error instanceof Error ? error.message : String(error);
  logError("HTTP request failed", error);
  res.status(500).json({ error: message });
});
