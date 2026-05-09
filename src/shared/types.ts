export type BlockType = "speech" | "sfx" | "caption" | "other";

export type SourceTextDirection = "horizontal" | "vertical";
export type RenderTextDirection = "horizontal" | "vertical" | "hidden";
export type TextPosition =
  | "top-left"
  | "top"
  | "top-right"
  | "left"
  | "center"
  | "right"
  | "bottom-left"
  | "bottom"
  | "bottom-right";
export type TextFontStyle = "normal" | "italic";
export type TextDecoration = "none" | "underline";

export type FontCharacterOverride = {
  character: string;
  fontFamily: string;
};

export type JobKind = "model-analysis" | "library-import";
export type ModelProvider = "openai-codex" | "openai-compatible";
export type CodexReasoningEffort = "none" | "low" | "medium" | "high" | "xhigh";

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

export type TranslationParallelSettings = {
  enabled: boolean;
  maxConcurrency: number;
};

export const DEFAULT_TRANSLATION_PARALLEL_ENABLED = false;
export const DEFAULT_TRANSLATION_PARALLEL_MAX_CONCURRENCY = 2;
export const TRANSLATION_PARALLEL_MAX_CONCURRENCY_MIN = 1;
export const TRANSLATION_PARALLEL_MAX_CONCURRENCY_MAX = 8;
export const DEFAULT_ONE_HAND_MODE = false;

export type AppSettings = {
  modelProvider: ModelProvider;
  codex: CodexSettings;
  openAICompatible: OpenAICompatibleSettings;
  translationMode: TranslationMode;
  translationParallel: TranslationParallelSettings;
  nsfwMode: boolean;
  oneHandMode: boolean;
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
  | "importing"
  | "import_done"
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
  letterSpacingLinkedToPreset?: boolean;
  outlineColorLinkedToPreset?: boolean;
  outlineWidthLinkedToPreset?: boolean;
  secondaryOutlineColorLinkedToPreset?: boolean;
  secondaryOutlineWidthLinkedToPreset?: boolean;
  shadowEnabledLinkedToPreset?: boolean;
  shadowColorLinkedToPreset?: boolean;
  shadowAngleDegLinkedToPreset?: boolean;
  shadowDistancePxLinkedToPreset?: boolean;
  autoFitTextLinkedToPreset?: boolean;
  textColorLinkedToPreset?: boolean;
  screentoneFillEnabledLinkedToPreset?: boolean;
  screentoneFillIntensityLinkedToPreset?: boolean;
  screentoneFillDensityLinkedToPreset?: boolean;
  screentoneFillAntialiasLinkedToPreset?: boolean;
  fontWeightLinkedToPreset?: boolean;
  fontStyleLinkedToPreset?: boolean;
  textDecorationLinkedToPreset?: boolean;
  fontFamily?: string;
  characterFontOverrides?: FontCharacterOverride[];
  fontWeight?: number;
  fontStyle?: TextFontStyle;
  textDecoration?: TextDecoration;
  fontSizePx: number;
  lineHeight: number;
  letterSpacingPx?: number;
  outlineColor?: string;
  outlineWidthPx?: number;
  secondaryOutlineColor?: string;
  secondaryOutlineWidthPx?: number;
  shadowEnabled?: boolean;
  shadowColor?: string;
  shadowAngleDeg?: number;
  shadowDistancePx?: number;
  textPaddingPx?: number;
  textAlign: "left" | "center" | "right";
  textPosition?: TextPosition;
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
  fontSizePresetId?: string;
  fontFamily?: string;
  characterFontOverrides?: FontCharacterOverride[];
  fontWeight?: number;
  fontStyle?: TextFontStyle;
  textDecoration?: TextDecoration;
  fontSizePx: number;
  lineHeight: number;
  letterSpacingPx?: number;
  outlineColor?: string;
  outlineWidthPx?: number;
  secondaryOutlineColor?: string;
  secondaryOutlineWidthPx?: number;
  shadowEnabled?: boolean;
  shadowColor?: string;
  shadowAngleDeg?: number;
  shadowDistancePx?: number;
  autoFitText?: boolean;
  textColor?: string;
  screentoneFillEnabled?: boolean;
  screentoneFillIntensity?: number;
  screentoneFillDensity?: number;
  screentoneFillAntialias?: boolean;
};

export type FontSizePreset = {
  id: string;
  name: string;
  fontSizePx: number;
};

export type FontPresetBackupSummary = {
  id: string;
  name: string;
  createdAt: string;
  fontPresetCount: number;
  fontSizePresetCount: number;
};

export type FontPresetBackupSnapshot = FontPresetBackupSummary & {
  fontPresets: FontPreset[];
  fontSizePresets: FontSizePreset[];
};

export type CreateFontPresetBackupRequest = {
  name: string;
  fontPresets: FontPreset[];
  fontSizePresets: FontSizePreset[];
};

export type SystemFont = {
  family: string;
  fullName?: string;
  postScriptName?: string;
  weights?: number[];
  cssFamily: string;
};

export type MangaPage = {
  id: string;
  name: string;
  imagePath: string;
  /** Same-origin image URL for client snapshots, or a data URL when an image payload is explicitly hydrated. */
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
  lastOpenedPageId?: string;
  fontPresets?: FontPreset[];
  fontSizePresets?: FontSizePreset[];
  favoriteFontPresetIds?: string[];
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

export type ChapterPagePatch = Partial<LibraryPageRecord> & {
  id: string;
};

export type SaveChapterPatchRequest = {
  chapter: Pick<ChapterSnapshot, "id" | "workId" | "updatedAt"> &
    Partial<Pick<ChapterSnapshot, "favoriteFontPresetIds" | "fontPresets" | "fontSizePresets" | "status" | "title" | "pageOrder">>;
  pages?: ChapterPagePatch[];
};

export type PageImageLayer = "source" | "inpaint-mask" | "inpaint-result";

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
  artifactCleanupPx?: number;
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
  maskDataUrl: string;
  engine: InpaintEngine;
};

export type ExportInpaintPsdRequest = {
  chapterId: string;
  pageId: string;
  pageName: string;
  width: number;
  height: number;
  sourceDataUrl: string;
  maskDataUrl?: string;
  resultDataUrl?: string;
  translationBlocksDataUrl?: string;
};

export type ImportInpaintPsdResult = {
  chapter: ChapterSnapshot;
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

export type SaveInpaintLayersRequest = {
  chapterId: string;
  pageId: string;
  maskDataUrl: string;
  resultDataUrl: string;
  preserveMaskDataUrl?: boolean;
};

export type SaveInpaintLayersResult = {
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

export type ModelTestResult = {
  ok: boolean;
  message: string;
  launchMode: "openai-codex" | "openai-compatible";
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
