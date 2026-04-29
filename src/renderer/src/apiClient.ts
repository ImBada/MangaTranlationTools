import type {
  AppSettings,
  ChapterSnapshot,
  CreateImportRequest,
  CreateImportResult,
  ImportPreviewResult,
  InpaintPageRequest,
  InpaintPageResult,
  JobEvent,
  LibraryIndex,
  ModelTestResult,
  RenderPageRequest,
  RenderPageResult,
  SaveInpaintResultLayerRequest,
  SaveInpaintResultLayerResult,
  SaveInpaintMaskRequest,
  SaveInpaintMaskResult,
  StartAnalysisRequest,
  StartAnalysisResult,
  SystemFont
} from "../../shared/types";

type ImportKind = "images" | "folder" | "zip" | "zip-folder";

async function requestJson<T>(url: string, init?: RequestInit): Promise<T> {
  const response = await fetch(url, {
    ...init,
    headers: init?.body instanceof FormData ? init.headers : { "Content-Type": "application/json", ...init?.headers }
  });
  if (!response.ok) {
    const payload = await response.json().catch(() => null);
    throw new Error(payload?.error || `${response.status} ${response.statusText}`);
  }
  return response.json() as Promise<T>;
}

function postJson<T>(url: string, body?: unknown): Promise<T> {
  return requestJson<T>(url, { method: "POST", body: JSON.stringify(body ?? {}) });
}

async function previewImport(kind: ImportKind, files: File[]): Promise<ImportPreviewResult | null> {
  if (files.length === 0) {
    return null;
  }
  const formData = new FormData();
  const relativePaths: string[] = [];
  for (const file of files) {
    formData.append("files", file, file.name);
    relativePaths.push((file as File & { webkitRelativePath?: string }).webkitRelativePath || file.name);
  }
  formData.append("relativePaths", JSON.stringify(relativePaths));
  return requestJson<ImportPreviewResult | null>(`/api/import/preview/${kind}`, { method: "POST", body: formData });
}

export const mangaApi = {
  previewImagesImport: (files: File[]): Promise<ImportPreviewResult | null> => previewImport("images", files),
  previewFolderImport: (files: File[]): Promise<ImportPreviewResult | null> => previewImport("folder", files),
  previewZipImport: (files: File[]): Promise<ImportPreviewResult | null> => previewImport("zip", files),
  previewZipFolderImport: (files: File[]): Promise<ImportPreviewResult | null> => previewImport("zip-folder", files),
  createImport: (request: CreateImportRequest): Promise<CreateImportResult> => postJson("/api/import/create", request),
  getLibrary: (): Promise<LibraryIndex> => requestJson("/api/library"),
  openChapter: (chapterId: string): Promise<ChapterSnapshot> => requestJson(`/api/library/chapters/${encodeURIComponent(chapterId)}`),
  saveChapter: (chapter: ChapterSnapshot, dirtyPageIds?: string[]): Promise<ChapterSnapshot> =>
    postJson("/api/library/chapters", dirtyPageIds ? { chapter, dirtyPageIds } : chapter),
  renderPage: (request: RenderPageRequest): Promise<RenderPageResult> => postJson("/api/render/page", request),
  inpaintPage: (request: InpaintPageRequest): Promise<InpaintPageResult> => postJson("/api/inpaint/page", request),
  saveInpaintMask: (request: SaveInpaintMaskRequest): Promise<SaveInpaintMaskResult> => postJson("/api/inpaint/mask", request),
  saveInpaintResultLayer: (request: SaveInpaintResultLayerRequest): Promise<SaveInpaintResultLayerResult> =>
    postJson("/api/inpaint/result-layer", request),
  renameWork: (workId: string, title: string): Promise<LibraryIndex> => postJson(`/api/library/works/${encodeURIComponent(workId)}/rename`, { title }),
  renameChapter: (chapterId: string, title: string): Promise<LibraryIndex> =>
    postJson(`/api/library/chapters/${encodeURIComponent(chapterId)}/rename`, { title }),
  deleteWork: (workId: string): Promise<LibraryIndex> => requestJson(`/api/library/works/${encodeURIComponent(workId)}`, { method: "DELETE" }),
  deleteChapter: (chapterId: string): Promise<LibraryIndex> => requestJson(`/api/library/chapters/${encodeURIComponent(chapterId)}`, { method: "DELETE" }),
  reorderChapters: (workId: string, chapterIds: string[]): Promise<LibraryIndex> =>
    postJson(`/api/library/works/${encodeURIComponent(workId)}/reorder-chapters`, { chapterIds }),
  reorderPages: (chapterId: string, pageIds: string[]): Promise<ChapterSnapshot> =>
    postJson(`/api/library/chapters/${encodeURIComponent(chapterId)}/reorder-pages`, { pageIds }),
  deletePage: (chapterId: string, pageId: string): Promise<ChapterSnapshot> =>
    requestJson(`/api/library/chapters/${encodeURIComponent(chapterId)}/pages/${encodeURIComponent(pageId)}`, { method: "DELETE" }),
  getSettings: (): Promise<AppSettings> => requestJson("/api/settings"),
  getSystemFonts: (): Promise<SystemFont[]> => requestJson("/api/fonts"),
  saveSettings: (settings: AppSettings): Promise<AppSettings> => postJson("/api/settings", settings),
  resetSettings: (): Promise<AppSettings> => postJson("/api/settings/reset"),
  testModelSettings: (settings: AppSettings): Promise<ModelTestResult> => postJson("/api/settings/test-model", settings),
  confirm: async (_title: string, message: string, detail?: string): Promise<boolean> => window.confirm(detail ? `${message}\n\n${detail}` : message),
  writeLog: (level: "debug" | "info" | "warn" | "error", message: string, detail?: unknown) =>
    postJson("/api/logs/write", { level, message, detail }),
  startAnalysis: (request: StartAnalysisRequest): Promise<StartAnalysisResult> => postJson("/api/jobs/start-analysis", request),
  cancelJob: () => postJson("/api/jobs/cancel"),
  onJobEvent: (callback: (event: JobEvent) => void) => {
    const events = new EventSource("/api/jobs/events");
    events.onmessage = (message) => callback(JSON.parse(message.data) as JobEvent);
    return () => events.close();
  }
};

export type MangaApi = typeof mangaApi;
