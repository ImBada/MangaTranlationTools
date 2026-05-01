export type BlockType = "speech" | "sfx" | "caption" | "other";

export type SourceTextDirection = "horizontal" | "vertical";
export type RenderTextDirection = "horizontal" | "vertical" | "hidden";

export type JobKind = "gemma-analysis";
export type ModelProvider = "gemma" | "openai-codex" | "openai-compatible";
export type ModelSource = "huggingface" | "local";
export type CodexReasoningEffort = "none" | "low" | "medium" | "high" | "xhigh";

export type GemmaSettings = {
  modelSource: ModelSource;
  modelRepo: string;
  modelFile: string;
  localModelPath?: string;
  localMmprojPath?: string;
  gpuLayers: number;
};

export type CodexSettings = {
  model: string;
  reasoningEffort: CodexReasoningEffort;
  oauthPort: number;
};

export type OpenAICompatibleSettings = {
  baseUrl: string;
  apiKey: string;
  model: string;
};

export type TranslationMode = "fast" | "accuracy";

export type AppSettings = {
  modelProvider: ModelProvider;
  gemma: GemmaSettings;
  codex: CodexSettings;
  openAICompatible: OpenAICompatibleSettings;
  translationMode: TranslationMode;
  nsfwMode: boolean;
};

export type JobStatus =
  | "idle"
  | "starting"
  | "running"
  | "cancelling"
  | "cancelled"
  | "failed"
  | "completed";

export type JobPhase =
  | "booting"
  | "model_downloading"
  | "ready"
  | "page_running"
  | "page_retry"
  | "page_done"
  | "page_skipped"
  | "finalizing"
  | "done"
  | "cancelled"
  | "failed";

export type PageAnalysisStatus = "idle" | "running" | "completed" | "failed";

export type ChapterStatus = "idle" | "running" | "completed" | "partial" | "failed";

export type RunMode = "pending" | "all" | "single-page";

export type ImportSourceKind = "images" | "folder" | "zip" | "zip-folder";

export type BBox = {
  x: number;
  y: number;
  w: number;
  h: number;
};

export type TranslationBlock = {
  id: string;
  type: BlockType;
  bbox: BBox;
  renderBbox?: BBox;
  bboxSpace?: "normalized_1000" | "pixels";
  renderBboxSpace?: "normalized_1000" | "pixels";
  sourceText: string;
  translatedText: string;
  confidence: number;
  sourceDirection: SourceTextDirection;
  renderDirection: RenderTextDirection;
  rotationDeg?: number;
  fontPresetId?: string;
  fontSizeLinkedToPreset?: boolean;
  lineHeightLinkedToPreset?: boolean;
  outlineColorLinkedToPreset?: boolean;
  outlineWidthLinkedToPreset?: boolean;
  autoFitTextLinkedToPreset?: boolean;
  textColorLinkedToPreset?: boolean;
  screentoneFillEnabledLinkedToPreset?: boolean;
  screentoneFillIntensityLinkedToPreset?: boolean;
  screentoneFillDensityLinkedToPreset?: boolean;
  screentoneFillAntialiasLinkedToPreset?: boolean;
  fontFamily?: string;
  fontSizePx: number;
  lineHeight: number;
  outlineColor?: string;
  outlineWidthPx?: number;
  textPaddingPx?: number;
  textAlign: "left" | "center" | "right";
  textColor: string;
  screentoneFillEnabled?: boolean;
  screentoneFillIntensity?: number;
  screentoneFillDensity?: number;
  screentoneFillAntialias?: boolean;
  backgroundColor: string;
  opacity: number;
  autoFitText?: boolean;
  inpainted?: boolean;
};

export type FontPreset = {
  id: string;
  name: string;
  fontFamily?: string;
  fontSizePx: number;
  lineHeight: number;
  outlineColor?: string;
  outlineWidthPx?: number;
  autoFitText?: boolean;
  textColor?: string;
  screentoneFillEnabled?: boolean;
  screentoneFillIntensity?: number;
  screentoneFillDensity?: number;
  screentoneFillAntialias?: boolean;
};

export type SystemFont = {
  family: string;
  fullName?: string;
  postScriptName?: string;
  cssFamily: string;
};

export type MangaPage = {
  id: string;
  name: string;
  imagePath: string;
  dataUrl: string;
  inpaintMaskPath?: string;
  inpaintResultPath?: string;
  inpaintMaskDataUrl?: string;
  inpaintResultDataUrl?: string;
  inpaintStatus?: "idle" | "running" | "completed" | "failed";
  inpaintSettings?: InpaintSettings;
  /** @deprecated Use inpaintMaskDataUrl or inpaintResultDataUrl. */
  inpaintLayerDataUrl?: string;
  width: number;
  height: number;
  blocks: TranslationBlock[];
  analysisStatus: PageAnalysisStatus;
  lastError?: string;
  progressCompleted?: boolean;
  createdAt: string;
  updatedAt: string;
};

export type LibraryPageRecord = Omit<MangaPage, "dataUrl">;

export type LibraryChapter = {
  id: string;
  workId: string;
  title: string;
  sourceKind: ImportSourceKind;
  status: ChapterStatus;
  fontPresets?: FontPreset[];
  pageOrder: string[];
  pages: LibraryPageRecord[];
  createdAt: string;
  updatedAt: string;
};

export type ChapterSnapshot = Omit<LibraryChapter, "pages"> & {
  pages: MangaPage[];
};

export type SaveChapterSnapshotRequest = {
  chapter: ChapterSnapshot;
  dirtyPageIds?: string[];
};

export type LibraryChapterSummary = Pick<LibraryChapter, "id" | "workId" | "title" | "status" | "createdAt" | "updatedAt"> & {
  pageCount: number;
};

export type LibraryWork = {
  id: string;
  title: string;
  chapterOrder: string[];
  createdAt: string;
  updatedAt: string;
};

export type LibraryWorkSummary = LibraryWork & {
  chapters: LibraryChapterSummary[];
};

export type LibraryIndex = {
  workOrder: string[];
  works: LibraryWorkSummary[];
};

export type RenderPageRequest = {
  chapterId: string;
  pageId: string;
  dataUrl: string;
};

export type RenderPageResult = {
  outputPath: string;
};

export type InpaintEngine = "lama" | "opencv-fallback" | "local-fill-fallback" | "mask-fill-fallback";

export type InpaintSettings = {
  engine: InpaintEngine;
  paddingPx: number;
  featherPx: number;
  tileSize: number;
};

export type ImageRect = {
  x: number;
  y: number;
  width: number;
  height: number;
};

export type InpaintPageRequest = {
  chapterId: string;
  pageId: string;
  sourceDataUrl: string;
  maskDataUrl: string;
  settings: InpaintSettings;
  persistResult?: boolean;
};

export type InpaintPageResult = {
  chapter: ChapterSnapshot;
  resultDataUrl: string;
  engine: InpaintEngine;
};

export type LamaRuntimeStatus = {
  pythonAvailable: boolean;
  pythonCommand: string | null;
  pythonInstallCommand: string;
  pythonInstallHelp: string[];
  runtimeReady: boolean;
  runtimePreparing: boolean;
  modelExists: boolean;
  modelDownloading: boolean;
  modelPath: string;
  modelUrl: string;
  logPath: string;
  lastError?: string;
};

export type UpdateStatus = {
  currentVersion: string;
  latestVersion: string | null;
  updateAvailable: boolean;
  checkedAt: string;
  releaseUrl: string | null;
  releaseName: string | null;
  error?: string;
};

export type SaveInpaintMaskRequest = {
  chapterId: string;
  pageId: string;
  maskDataUrl?: string;
};

export type SaveInpaintMaskResult = {
  chapter: ChapterSnapshot;
};

export type SaveInpaintResultLayerRequest = {
  chapterId: string;
  pageId: string;
  resultDataUrl?: string;
};

export type SaveInpaintResultLayerResult = {
  chapter: ChapterSnapshot;
};

export type ImportPageDraft = {
  name: string;
  sourcePath: string;
  sourceKind: "file" | "zip-entry";
  zipEntryName?: string;
};

export type ImportChapterDraft = {
  draftId: string;
  title: string;
  sourceKind: ImportSourceKind;
  pages: ImportPageDraft[];
};

export type ImportPreviewResult = {
  mode: "single" | "batch";
  sourceKind: ImportSourceKind;
  suggestedWorkTitle: string;
  chapters: ImportChapterDraft[];
};

export type ImportTarget =
  | {
      mode: "new";
      title: string;
    }
  | {
      mode: "existing";
      workId: string;
    };

export type ImportCreateSelection = {
  draftId: string;
  title: string;
  enabled: boolean;
};

export type CreateImportRequest = {
  preview: ImportPreviewResult;
  target: ImportTarget;
  selections: ImportCreateSelection[];
};

export type CreateImportResult = {
  workId: string;
  chapterIds: string[];
  openedChapter?: ChapterSnapshot;
};

export type JobState = {
  id: string;
  kind: JobKind;
  status: JobStatus;
  progressText: string;
  detail?: string;
  phase?: JobPhase;
  progressCurrent?: number;
  progressTotal?: number;
  pageIndex?: number;
  pageTotal?: number;
  attempt?: number;
  attemptTotal?: number;
};

export type JobEvent = JobState & {
  detail?: string;
};

export type LocalModelPickResult = {
  modelPath: string;
  detectedMmprojPath?: string;
};

export type ModelTestResult = {
  ok: boolean;
  message: string;
  launchMode: "huggingface" | "cached-hf" | "local" | "openai-codex" | "openai-compatible";
  resolvedModelPath?: string | null;
  resolvedMmprojPath?: string | null;
  resolvedEndpoint?: string | null;
};

export type StartAnalysisRequest = {
  chapterId: string;
  runMode: RunMode;
  pageId?: string;
};

export type StartAnalysisResult = {
  status: "completed" | "cancelled" | "failed";
  chapter?: ChapterSnapshot;
  warnings?: string[];
  error?: string;
};
