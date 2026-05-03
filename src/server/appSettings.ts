import type { AppSettings, CodexReasoningEffort, ModelProvider, TranslationMode } from "../shared/types";

export const DEFAULT_MODEL_PROVIDER: ModelProvider = "openai-codex";
export const DEFAULT_CODEX_MODEL = "gpt-5.5";
export const DEFAULT_CODEX_REASONING_EFFORT: CodexReasoningEffort = "medium";
export const DEFAULT_CODEX_OAUTH_PORT = 10531;
export const DEFAULT_OPENAI_COMPATIBLE_BASE_URL = "http://127.0.0.1:11434/v1";
export const DEFAULT_OPENAI_COMPATIBLE_MODEL = "gemma4:31b";
export const DEFAULT_TRANSLATION_MODE: TranslationMode = "fast";

type TranslationModeDefaults = {
  maxTokens: number;
  imageMinTokens: number;
  imageMaxTokens: number;
  includeEnhancedVariant: boolean;
};

const TRANSLATION_MODE_DEFAULTS: Record<TranslationMode, TranslationModeDefaults> = {
  fast: {
    maxTokens: 900,
    imageMinTokens: 640,
    imageMaxTokens: 640,
    includeEnhancedVariant: false
  },
  accuracy: {
    maxTokens: 1400,
    imageMinTokens: 1120,
    imageMaxTokens: 1120,
    includeEnhancedVariant: true
  }
};

export type TranslationOptions = {
  imagePath: string;
  outputDir: string;
  modelProvider: ModelProvider;
  promptMode: string;
  promptOverrideText?: string;
  nsfwMode: boolean;
  temperature: number;
  topP: number;
  maxTokens: number;
  imageMinTokens: number;
  imageMaxTokens: number;
  includeEnhancedVariant: boolean;
  enhancedMaxLongSide: number;
  enhancedContrast: number;
  imageFirst: boolean;
  codexModel: string;
  codexReasoningEffort: CodexReasoningEffort;
  codexOauthPort: number;
  openAICompatibleBaseUrl: string;
  openAICompatibleApiKey: string;
  openAICompatibleModel: string;
  label: string;
  abortSignal?: AbortSignal;
};

export function resolveDefaultAppSettings(env: NodeJS.ProcessEnv = process.env): AppSettings {
  return {
    modelProvider: resolveModelProvider(env.MANGA_TRANSLATOR_MODEL_PROVIDER, DEFAULT_MODEL_PROVIDER),
    codex: {
      model: resolveNonEmptyString(env.MANGA_TRANSLATOR_CODEX_MODEL, DEFAULT_CODEX_MODEL),
      reasoningEffort: resolveCodexReasoningEffort(
        env.MANGA_TRANSLATOR_CODEX_REASONING_EFFORT,
        DEFAULT_CODEX_REASONING_EFFORT
      ),
      oauthPort: resolvePortNumber(env.MANGA_TRANSLATOR_CODEX_OAUTH_PORT, DEFAULT_CODEX_OAUTH_PORT)
    },
    openAICompatible: {
      baseUrl: resolveBaseUrl(env.MANGA_TRANSLATOR_OPENAI_COMPATIBLE_BASE_URL, DEFAULT_OPENAI_COMPATIBLE_BASE_URL),
      apiKey: resolveString(env.MANGA_TRANSLATOR_OPENAI_COMPATIBLE_API_KEY, ""),
      model: resolveNonEmptyString(env.MANGA_TRANSLATOR_OPENAI_COMPATIBLE_MODEL, DEFAULT_OPENAI_COMPATIBLE_MODEL)
    },
    translationMode: DEFAULT_TRANSLATION_MODE,
    nsfwMode: false
  };
}

export function normalizeAppSettings(raw: unknown, defaults = resolveDefaultAppSettings()): AppSettings {
  const record = asRecord(raw);
  const codex = record?.codex;
  const openAICompatible = record?.openAICompatible;

  return {
    modelProvider: resolveModelProvider(record?.modelProvider, defaults.modelProvider),
    codex: {
      model: resolveNonEmptyString(asRecord(codex)?.model, defaults.codex.model),
      reasoningEffort: resolveCodexReasoningEffort(asRecord(codex)?.reasoningEffort, defaults.codex.reasoningEffort),
      oauthPort: resolvePortNumber(asRecord(codex)?.oauthPort, defaults.codex.oauthPort)
    },
    openAICompatible: {
      baseUrl: resolveBaseUrl(asRecord(openAICompatible)?.baseUrl, defaults.openAICompatible.baseUrl),
      apiKey: resolveString(asRecord(openAICompatible)?.apiKey, defaults.openAICompatible.apiKey),
      model: resolveNonEmptyString(asRecord(openAICompatible)?.model, defaults.openAICompatible.model)
    },
    translationMode: resolveTranslationMode(record?.translationMode, defaults.translationMode),
    nsfwMode: resolveBoolean(record?.nsfwMode, defaults.nsfwMode)
  };
}

export function parseStoredAppSettings(rawText: string | null | undefined, defaults = resolveDefaultAppSettings()): AppSettings {
  if (!rawText?.trim()) {
    return defaults;
  }

  try {
    return normalizeAppSettings(JSON.parse(rawText), defaults);
  } catch {
    return defaults;
  }
}

export function buildBaseTranslationOptions({
  jobId,
  runDir,
  settings,
  env = process.env
}: {
  jobId: string;
  runDir: string;
  settings: AppSettings;
  env?: NodeJS.ProcessEnv;
}): TranslationOptions {
  const modeDefaults = resolveTranslationModeDefaults(settings.translationMode);
  return {
    imagePath: "",
    outputDir: runDir,
    modelProvider: settings.modelProvider,
    promptMode: "ko_bbox_lines_multiview",
    nsfwMode: settings.nsfwMode,
    temperature: readNumberEnv(env, "MANGA_TRANSLATOR_TEMPERATURE", 0),
    topP: readNumberEnv(env, "MANGA_TRANSLATOR_TOP_P", 0.85),
    maxTokens: readNumberEnv(env, "MANGA_TRANSLATOR_MAX_TOKENS", modeDefaults.maxTokens),
    imageMinTokens: readNumberEnv(env, "MANGA_TRANSLATOR_IMAGE_MIN_TOKENS", modeDefaults.imageMinTokens),
    imageMaxTokens: readNumberEnv(env, "MANGA_TRANSLATOR_IMAGE_MAX_TOKENS", modeDefaults.imageMaxTokens),
    includeEnhancedVariant: modeDefaults.includeEnhancedVariant,
    enhancedMaxLongSide: 1900,
    enhancedContrast: 1.35,
    imageFirst: true,
    codexModel: settings.codex.model,
    codexReasoningEffort: settings.codex.reasoningEffort,
    codexOauthPort: settings.codex.oauthPort,
    openAICompatibleBaseUrl: settings.openAICompatible.baseUrl,
    openAICompatibleApiKey: settings.openAICompatible.apiKey,
    openAICompatibleModel: settings.openAICompatible.model,
    label: `app-${jobId}`
  };
}

function readNumberEnv(env: NodeJS.ProcessEnv, name: string, fallback: number): number {
  const value = Number(env[name]);
  return Number.isFinite(value) ? value : fallback;
}

function resolveTranslationMode(value: unknown, fallback: TranslationMode): TranslationMode {
  return value === "fast" || value === "accuracy" ? value : fallback;
}

function resolveModelProvider(value: unknown, fallback: ModelProvider): ModelProvider {
  return value === "openai-codex" || value === "openai-compatible" ? value : fallback;
}

function resolveCodexReasoningEffort(value: unknown, fallback: CodexReasoningEffort): CodexReasoningEffort {
  if (value === "minimal") {
    return "low";
  }
  return value === "none" || value === "low" || value === "medium" || value === "high" || value === "xhigh"
    ? value
    : fallback;
}

function resolveTranslationModeDefaults(mode: TranslationMode): TranslationModeDefaults {
  return TRANSLATION_MODE_DEFAULTS[mode];
}

function resolveNonEmptyString(value: unknown, fallback: string): string {
  return typeof value === "string" && value.trim() ? value.trim() : fallback;
}

function resolveString(value: unknown, fallback: string): string {
  return typeof value === "string" ? value.trim() : fallback;
}

function resolveBaseUrl(value: unknown, fallback: string): string {
  const raw = resolveNonEmptyString(value, fallback);
  return raw.replace(/\/+$/, "");
}

function resolvePortNumber(value: unknown, fallback: number): number {
  const parsed = typeof value === "number" ? value : Number(value);
  if (!Number.isInteger(parsed)) {
    return fallback;
  }
  return clampInteger(parsed, 0, 65535);
}

function clampInteger(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function resolveBoolean(value: unknown, fallback: boolean): boolean {
  if (typeof value === "boolean") {
    return value;
  }
  if (typeof value === "string") {
    const normalized = value.trim().toLowerCase();
    if (["true", "1", "yes", "on"].includes(normalized)) {
      return true;
    }
    if (["false", "0", "no", "off"].includes(normalized)) {
      return false;
    }
  }
  if (typeof value === "number") {
    if (value === 1) {
      return true;
    }
    if (value === 0) {
      return false;
    }
  }
  return fallback;
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" ? (value as Record<string, unknown>) : null;
}
