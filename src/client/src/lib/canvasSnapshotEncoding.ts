import { normalizeOpaqueMaskPixels } from "./inpaintMaskImages";

type CanvasSnapshotEncodeMode = "image" | "mask";

type CanvasSnapshotEncodeOptions = {
  includeBlank?: boolean;
  mode: CanvasSnapshotEncodeMode;
};

type WorkerRequest = CanvasSnapshotEncodeOptions & {
  id: number;
  bitmap: ImageBitmap;
  height: number;
  width: number;
};

type WorkerResponse = {
  dataUrl?: string;
  error?: string;
  id: number;
};

type PendingWorkerJob = {
  reject: (reason?: unknown) => void;
  resolve: (dataUrl: string | undefined) => void;
};

let worker: Worker | null = null;
let nextWorkerJobId = 1;
const pendingWorkerJobs = new Map<number, PendingWorkerJob>();

export async function encodeCanvasSnapshotDataUrl(
  canvas: HTMLCanvasElement,
  options: CanvasSnapshotEncodeOptions
): Promise<string | undefined> {
  if (typeof createImageBitmap === "undefined") {
    return encodeCanvasOnMainThread(canvas, options);
  }
  const bitmap = await createImageBitmap(canvas);
  if (!canUseWorkerEncoder()) {
    return encodeBitmapOnMainThread(bitmap, canvas.width, canvas.height, options);
  }
  return encodeBitmapInWorker(bitmap, canvas.width, canvas.height, options);
}

function canUseWorkerEncoder(): boolean {
  return typeof Worker !== "undefined" && typeof OffscreenCanvas !== "undefined" && typeof createImageBitmap !== "undefined";
}

function encodeBitmapInWorker(
  bitmap: ImageBitmap,
  width: number,
  height: number,
  options: CanvasSnapshotEncodeOptions
): Promise<string | undefined> {
  let encoder: Worker;
  try {
    encoder = resolveWorker();
  } catch {
    return encodeBitmapOnMainThread(bitmap, width, height, options);
  }
  const id = nextWorkerJobId;
  nextWorkerJobId += 1;
  return new Promise((resolve, reject) => {
    pendingWorkerJobs.set(id, { resolve, reject });
    try {
      encoder.postMessage({ id, bitmap, width, height, ...options } satisfies WorkerRequest, [bitmap]);
    } catch {
      pendingWorkerJobs.delete(id);
      void encodeBitmapOnMainThread(bitmap, width, height, options).then(resolve, reject);
    }
  });
}

function resolveWorker(): Worker {
  if (worker) {
    return worker;
  }

  const workerUrl = URL.createObjectURL(new Blob([workerSource], { type: "text/javascript" }));
  worker = new Worker(workerUrl);
  URL.revokeObjectURL(workerUrl);
  worker.onmessage = (event: MessageEvent<WorkerResponse>) => {
    const { dataUrl, error, id } = event.data;
    const job = pendingWorkerJobs.get(id);
    if (!job) {
      return;
    }
    pendingWorkerJobs.delete(id);
    if (error) {
      job.reject(new Error(error));
      return;
    }
    job.resolve(dataUrl);
  };
  worker.onerror = (event) => {
    for (const job of pendingWorkerJobs.values()) {
      job.reject(event.error instanceof Error ? event.error : new Error(event.message));
    }
    pendingWorkerJobs.clear();
    worker?.terminate();
    worker = null;
  };
  return worker;
}

async function encodeBitmapOnMainThread(
  bitmap: ImageBitmap,
  width: number,
  height: number,
  options: CanvasSnapshotEncodeOptions
): Promise<string | undefined> {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) {
    bitmap.close();
    return undefined;
  }
  context.drawImage(bitmap, 0, 0);
  bitmap.close();
  const hasContent = options.mode === "mask"
    ? normalizeMaskCanvas(context, width, height)
    : options.includeBlank === true || canvasHasVisiblePixels(context, width, height);
  if (!hasContent && !options.includeBlank) {
    return undefined;
  }
  return canvas.toDataURL("image/png");
}

async function encodeCanvasOnMainThread(
  sourceCanvas: HTMLCanvasElement,
  options: CanvasSnapshotEncodeOptions
): Promise<string | undefined> {
  const canvas = document.createElement("canvas");
  canvas.width = sourceCanvas.width;
  canvas.height = sourceCanvas.height;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) {
    return undefined;
  }
  context.drawImage(sourceCanvas, 0, 0);
  const hasContent = options.mode === "mask"
    ? normalizeMaskCanvas(context, canvas.width, canvas.height)
    : options.includeBlank === true || canvasHasVisiblePixels(context, canvas.width, canvas.height);
  if (!hasContent && !options.includeBlank) {
    return undefined;
  }
  return canvas.toDataURL("image/png");
}

function normalizeMaskCanvas(context: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D, width: number, height: number): boolean {
  const imageData = context.getImageData(0, 0, width, height);
  const hasContent = normalizeOpaqueMaskPixels(imageData.data);
  context.putImageData(imageData, 0, 0);
  return hasContent;
}

function canvasHasVisiblePixels(context: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D, width: number, height: number): boolean {
  const pixels = context.getImageData(0, 0, width, height).data;
  for (let offset = 3; offset < pixels.length; offset += 4) {
    if (pixels[offset] !== 0) {
      return true;
    }
  }
  return false;
}

const workerSource = `
self.onmessage = async (event) => {
  const { bitmap, height, id, includeBlank, mode, width } = event.data;
  let bitmapClosed = false;
  const closeBitmap = () => {
    if (bitmapClosed) {
      return;
    }
    bitmap.close?.();
    bitmapClosed = true;
  };
  try {
    const canvas = new OffscreenCanvas(width, height);
    const context = canvas.getContext("2d", { willReadFrequently: true });
    if (!context) {
      closeBitmap();
      self.postMessage({ id, dataUrl: undefined });
      return;
    }
    context.drawImage(bitmap, 0, 0);
    closeBitmap();
    const hasContent = mode === "mask"
      ? normalizeMaskCanvas(context, width, height)
      : includeBlank === true || canvasHasVisiblePixels(context, width, height);
    if (!hasContent && !includeBlank) {
      self.postMessage({ id, dataUrl: undefined });
      return;
    }
    const blob = await canvas.convertToBlob({ type: "image/png" });
    const dataUrl = await blobToDataUrl(blob);
    self.postMessage({ id, dataUrl });
  } catch (error) {
    closeBitmap();
    self.postMessage({ id, error: error instanceof Error ? error.message : String(error) });
  }
};

function normalizeMaskCanvas(context, width, height) {
  const imageData = context.getImageData(0, 0, width, height);
  const hasContent = normalizeOpaqueMaskPixels(imageData.data);
  context.putImageData(imageData, 0, 0);
  return hasContent;
}

function canvasHasVisiblePixels(context, width, height) {
  const pixels = context.getImageData(0, 0, width, height).data;
  for (let offset = 3; offset < pixels.length; offset += 4) {
    if (pixels[offset] !== 0) {
      return true;
    }
  }
  return false;
}

function normalizeOpaqueMaskPixels(pixels) {
  let hasContent = false;
  for (let offset = 0; offset + 3 < pixels.length; offset += 4) {
    const covered = isInpaintMaskPixelCovered(pixels, offset);
    pixels[offset] = covered ? 255 : 0;
    pixels[offset + 1] = covered ? 255 : 0;
    pixels[offset + 2] = covered ? 255 : 0;
    pixels[offset + 3] = 255;
    hasContent = hasContent || covered;
  }
  return hasContent;
}

function isInpaintMaskPixelCovered(pixels, offset) {
  const alpha = pixels[offset + 3] / 255;
  const luma = (pixels[offset] * 0.299 + pixels[offset + 1] * 0.587 + pixels[offset + 2] * 0.114) / 255;
  return alpha * luma > 0;
}

async function blobToDataUrl(blob) {
  const bytes = new Uint8Array(await blob.arrayBuffer());
  let binary = "";
  const chunkSize = 0x8000;
  for (let offset = 0; offset < bytes.length; offset += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + chunkSize));
  }
  return "data:" + (blob.type || "image/png") + ";base64," + btoa(binary);
}
`;
