import { afterEach, describe, expect, it, vi } from "vitest";
import {
  createCanvasSnapshotWorkerSource,
  encodeCanvasSnapshotDataUrl,
  resetCanvasSnapshotEncoderWorkerForTest
} from "../src/client/src/lib/canvasSnapshotEncoding";
import { resolveInpaintMaskRgbaCoverageAlpha } from "../src/client/src/lib/inpaintMaskImages";

describe("canvas snapshot encoding worker", () => {
  afterEach(() => {
    resetCanvasSnapshotEncoderWorkerForTest();
    vi.unstubAllGlobals();
  });

  it("uses the shared inpaint mask coverage helper when normalizing masks", () => {
    const source = createCanvasSnapshotWorkerSource();

    expect(source).toContain(resolveInpaintMaskRgbaCoverageAlpha.toString());
    expect(source).toContain("resolveInpaintMaskRgbaCoverageAlpha(pixels[offset], pixels[offset + 1], pixels[offset + 2], pixels[offset + 3])");
    expect(source).not.toContain("function isInpaintMaskPixelCovered");

    const evaluateWorkerSource = new Function(`
      const self = {};
      class OffscreenCanvas {}
      ${source}
      const pixels = new Uint8ClampedArray([
        255, 0, 0, 255,
        0, 0, 0, 255
      ]);
      const hasContent = normalizeOpaqueMaskPixels(pixels);
      return {
        redCoverage: resolveInpaintMaskRgbaCoverageAlpha(255, 0, 0, 255),
        hasContent,
        pixels: Array.from(pixels)
      };
    `);

    const result = evaluateWorkerSource() as {
      hasContent: boolean;
      pixels: number[];
      redCoverage: number;
    };

    expect(result.redCoverage).toBeCloseTo(0.299);
    expect(result.hasContent).toBe(true);
    expect(result.pixels).toEqual([
      255, 255, 255, 255,
      0, 0, 0, 255
    ]);
  });

  it("encodes mask snapshots through the generated worker source", async () => {
    const workerBlobs = new Map<string, Blob>();
    const workers: FakeWorker[] = [];
    const postedMessages: unknown[] = [];
    const fakeBitmap = {
      close: vi.fn(),
      pixels: new Uint8ClampedArray([
        255, 0, 0, 255,
        0, 0, 0, 255
      ])
    };
    let nextWorkerUrlId = 0;
    let normalizedPixels: number[] | null = null;

    class FakeOffscreenCanvas {
      readonly height: number;
      readonly width: number;
      private readonly context = {
        drawImage: vi.fn((bitmap: typeof fakeBitmap) => {
          this.context.pixels = new Uint8ClampedArray(bitmap.pixels);
        }),
        getImageData: vi.fn((_x: number, _y: number, width: number, height: number) => ({
          data: this.context.pixels,
          height,
          width
        })),
        pixels: new Uint8ClampedArray(),
        putImageData: vi.fn((imageData: { data: Uint8ClampedArray }) => {
          normalizedPixels = Array.from(imageData.data);
        })
      };

      constructor(width: number, height: number) {
        this.width = width;
        this.height = height;
      }

      getContext(): typeof this.context {
        return this.context;
      }

      async convertToBlob(options: { type?: string } = {}): Promise<Blob> {
        return new Blob(["encoded-mask"], { type: options.type ?? "image/png" });
      }
    }

    class FakeWorker {
      onerror: ((event: ErrorEvent) => void) | null = null;
      onmessage: ((event: MessageEvent) => void) | null = null;
      private readonly sourcePromise: Promise<string>;

      constructor(url: string) {
        const blob = workerBlobs.get(url);
        if (!blob) {
          throw new Error(`No worker source for ${url}`);
        }
        this.sourcePromise = blob.text();
        workers.push(this);
      }

      postMessage(message: unknown): void {
        postedMessages.push(message);
        void this.sourcePromise
          .then((source) => {
            const workerScope = {
              onmessage: null as ((event: { data: unknown }) => Promise<void>) | null,
              postMessage: (data: unknown) => {
                this.onmessage?.({ data } as MessageEvent);
              }
            };
            const loadWorker = new Function(
              "self",
              "OffscreenCanvas",
              "btoa",
              `${source}\nreturn self.onmessage;`
            );
            const handleWorkerMessage = loadWorker(
              workerScope,
              FakeOffscreenCanvas,
              (value: string) => Buffer.from(value, "binary").toString("base64")
            ) as (event: { data: unknown }) => Promise<void>;
            return handleWorkerMessage({ data: message });
          })
          .catch((error: unknown) => {
            this.onerror?.({
              error,
              message: error instanceof Error ? error.message : String(error)
            } as ErrorEvent);
          });
      }

      terminate = vi.fn();
    }

    vi.stubGlobal("createImageBitmap", vi.fn(async () => fakeBitmap));
    vi.stubGlobal("OffscreenCanvas", FakeOffscreenCanvas);
    vi.stubGlobal("Worker", FakeWorker);
    vi.stubGlobal("URL", {
      createObjectURL: vi.fn((blob: Blob) => {
        const url = `blob:canvas-snapshot-worker-${nextWorkerUrlId}`;
        nextWorkerUrlId += 1;
        workerBlobs.set(url, blob);
        return url;
      }),
      revokeObjectURL: vi.fn()
    });

    const dataUrl = await encodeCanvasSnapshotDataUrl(
      { height: 1, width: 2 } as HTMLCanvasElement,
      { mode: "mask" }
    );

    expect(dataUrl).toBe(`data:image/png;base64,${Buffer.from("encoded-mask").toString("base64")}`);
    expect(fakeBitmap.close).toHaveBeenCalledTimes(1);
    expect(workers).toHaveLength(1);
    expect(postedMessages).toHaveLength(1);
    expect(normalizedPixels).toEqual([
      255, 255, 255, 255,
      0, 0, 0, 255
    ]);

    resetCanvasSnapshotEncoderWorkerForTest();

    expect(workers[0].terminate).toHaveBeenCalledTimes(1);
  });

  it("rejects pending worker jobs when resetting the encoder worker", async () => {
    const workerBlobs = new Map<string, Blob>();
    const workers: SilentWorker[] = [];
    const fakeBitmap = {
      close: vi.fn()
    };
    let nextWorkerUrlId = 0;

    class SilentWorker {
      onerror: ((event: ErrorEvent) => void) | null = null;
      onmessage: ((event: MessageEvent) => void) | null = null;
      postMessage = vi.fn();
      terminate = vi.fn();

      constructor(url: string) {
        if (!workerBlobs.has(url)) {
          throw new Error(`No worker source for ${url}`);
        }
        workers.push(this);
      }
    }

    vi.stubGlobal("createImageBitmap", vi.fn(async () => fakeBitmap));
    vi.stubGlobal("OffscreenCanvas", class {});
    vi.stubGlobal("Worker", SilentWorker);
    vi.stubGlobal("URL", {
      createObjectURL: vi.fn((blob: Blob) => {
        const url = `blob:pending-canvas-snapshot-worker-${nextWorkerUrlId}`;
        nextWorkerUrlId += 1;
        workerBlobs.set(url, blob);
        return url;
      }),
      revokeObjectURL: vi.fn()
    });

    const pendingEncode = encodeCanvasSnapshotDataUrl(
      { height: 1, width: 1 } as HTMLCanvasElement,
      { mode: "mask" }
    );
    await vi.waitFor(() => {
      expect(workers).toHaveLength(1);
    });

    resetCanvasSnapshotEncoderWorkerForTest();

    await expect(pendingEncode).rejects.toThrow("Canvas snapshot encoder worker reset");
    expect(workers[0].terminate).toHaveBeenCalledTimes(1);
  });
});
