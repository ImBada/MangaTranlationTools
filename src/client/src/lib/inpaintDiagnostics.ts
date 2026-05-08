export type DataUrlDebugSummary = {
  fingerprint?: string;
  length?: number;
  mime?: string;
  present: boolean;
};

const DATA_URL_HASH_CHUNK_SIZE = 8192;

export function writeInpaintDebugLog(message: string, detail?: unknown): void {
  try {
    if (typeof window === "undefined" || !window.mangaApi?.writeLog) {
      return;
    }
    void window.mangaApi.writeLog("debug", message, detail).catch(() => undefined);
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
    fingerprint: hashStringSample(dataUrl)
  };
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

function fnv1a(value: string): string {
  let hash = 0x811c9dc5;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}
