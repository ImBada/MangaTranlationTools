export type DataUrlDebugSummary = {
  fingerprint?: string;
  length?: number;
  mime?: string;
  present: boolean;
};

const DATA_URL_HASH_CHUNK_SIZE = 8192;
const INPAINT_DEBUG_STORAGE_KEY = "mangaTranslationTools.inpaintDebugLogs";
const ENABLED_DEBUG_VALUES = new Set(["1", "true", "yes", "on"]);
let nextInpaintDebugId = 1;

export function writeInpaintDebugLog(message: string, detail?: unknown | (() => unknown)): void {
  try {
    if (typeof window === "undefined" || !window.mangaApi?.writeLog || !isInpaintDebugLogEnabled()) {
      return;
    }
    const resolvedDetail = typeof detail === "function" ? detail() : detail;
    void window.mangaApi.writeLog("debug", message, resolvedDetail).catch(() => undefined);
  } catch {
    // Diagnostics should never interrupt editing.
  }
}

export function summarizeDataUrl(dataUrl: string | undefined): DataUrlDebugSummary {
  if (!dataUrl) {
    return { present: false };
  }

  const match = /^data:([^;,]+);base64,/u.exec(dataUrl);
  return {
    present: true,
    length: dataUrl.length,
    mime: match?.[1] ?? "unknown",
    ...(isInpaintDebugLogEnabled() ? { fingerprint: hashStringSample(dataUrl) } : {})
  };
}

export function isInpaintDebugLogEnabled(): boolean {
  try {
    if (typeof window === "undefined") {
      return false;
    }
    const debugWindow = window as Window & { __MANGA_INPAINT_DEBUG__?: boolean };
    if (debugWindow.__MANGA_INPAINT_DEBUG__ === true) {
      return true;
    }
    const searchParams = new URLSearchParams(window.location.search);
    if (isEnabledDebugValue(searchParams.get("inpaintDebug"))) {
      return true;
    }
    return isEnabledDebugValue(window.localStorage.getItem(INPAINT_DEBUG_STORAGE_KEY));
  } catch {
    return false;
  }
}

export function createInpaintDebugId(prefix: string): string {
  const id = nextInpaintDebugId;
  nextInpaintDebugId += 1;
  return `${prefix}-${id}`;
}

export function summarizeCanvasSyncState(
  state: { dataUrl?: string; height?: number; width?: number } | null | undefined
): DataUrlDebugSummary & { height?: number; width?: number } {
  return {
    ...summarizeDataUrl(state?.dataUrl),
    height: state?.height,
    width: state?.width
  };
}

export function summarizeError(error: unknown): { message: string; name?: string; stack?: string } {
  if (error instanceof Error) {
    return {
      message: error.message,
      name: error.name,
      stack: error.stack
    };
  }
  return { message: String(error) };
}

function hashStringSample(value: string): string {
  if (value.length <= DATA_URL_HASH_CHUNK_SIZE * 3) {
    return fnv1a(value);
  }

  const middleStart = Math.max(0, Math.floor(value.length / 2) - Math.floor(DATA_URL_HASH_CHUNK_SIZE / 2));
  const sample = [
    value.slice(0, DATA_URL_HASH_CHUNK_SIZE),
    value.slice(middleStart, middleStart + DATA_URL_HASH_CHUNK_SIZE),
    value.slice(-DATA_URL_HASH_CHUNK_SIZE)
  ].join("");
  return `${fnv1a(sample)}:${value.length}`;
}

function isEnabledDebugValue(value: string | null | undefined): boolean {
  return value ? ENABLED_DEBUG_VALUES.has(value.toLowerCase()) : false;
}

function fnv1a(value: string): string {
  let hash = 0x811c9dc5;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}
