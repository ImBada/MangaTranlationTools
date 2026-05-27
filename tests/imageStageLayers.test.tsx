import React from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import type { MangaPage } from "../src/shared/types";
import { ImageStageLayers } from "../src/client/src/components/ImageStageLayers";
import { drawMaskedInpaintResultPreviewCanvas } from "../src/client/src/lib/maskedInpaintPreviewCanvas";

describe("ImageStageLayers", () => {
  it("renders composited inpaint result previews without browser CSS masks", () => {
    const markup = renderToStaticMarkup(<ImageStageLayers {...baseImageStageLayersProps()} />);

    expect(markup.match(/class="inpaint-result-canvas"/g)).toHaveLength(1);
    expect(markup).not.toContain("mask-image");
    expect(markup).not.toContain("-webkit-mask-image");
  });

  it("keeps the masked result preview visible while editing the inpaint mask layer", () => {
    const markup = renderToStaticMarkup(
      <ImageStageLayers
        {...baseImageStageLayersProps({
          activeLayer: "inpaintMask",
          inpaintStrokeActive: true
        })}
      />
    );

    expect(markup.match(/class="inpaint-result-canvas"/g)).toHaveLength(1);
    expect(markup).not.toContain("opacity:0");
    expect(markup).not.toContain("mask-image");
    expect(markup).not.toContain("-webkit-mask-image");
  });

  it("waits for the result and mask images before drawing the masked preview", async () => {
    const resultImage = {} as CanvasImageSource;
    const maskImage = {} as CanvasImageSource;
    const resultLoad = deferred<CanvasImageSource>();
    const maskLoad = deferred<CanvasImageSource>();
    const layerPixels = {
      data: new Uint8ClampedArray([
        10, 20, 30, 255,
        40, 50, 60, 200
      ])
    } as ImageData;
    const maskPixels = {
      data: new Uint8ClampedArray([
        0, 0, 0, 255,
        255, 255, 255, 128
      ])
    } as ImageData;
    const context = createCanvasContextMock(layerPixels, { imageSmoothingEnabled: false });
    const maskContext = createCanvasContextMock(maskPixels, { imageSmoothingEnabled: true });

    const drawPromise = drawMaskedInpaintResultPreviewCanvas({
      canvas: { width: 2, height: 1 } as HTMLCanvasElement,
      context,
      createMaskCanvas: () => ({
        getContext: () => maskContext,
        height: 0,
        width: 0
      }) as unknown as HTMLCanvasElement,
      dataUrl: "result",
      loadImage: (src) => src === "result" ? resultLoad.promise : maskLoad.promise,
      maskDataUrl: "mask",
      pageSize: { width: 2, height: 1 }
    });

    await Promise.resolve();
    expect(context.drawImage).not.toHaveBeenCalled();

    resultLoad.resolve(resultImage);
    await Promise.resolve();
    expect(context.drawImage).not.toHaveBeenCalled();

    maskLoad.resolve(maskImage);
    await drawPromise;

    expect(context.imageSmoothingEnabled).toBe(true);
    expect(maskContext.imageSmoothingEnabled).toBe(false);
    expect(context.drawImage).toHaveBeenCalledWith(resultImage, 0, 0, 2, 1);
    expect(maskContext.drawImage).toHaveBeenCalledWith(maskImage, 0, 0, 2, 1);
    expect(context.putImageData).toHaveBeenCalledWith(layerPixels, 0, 0);
    expect([...layerPixels.data]).toEqual([
      10, 20, 30, 0,
      40, 50, 60, 100
    ]);
  });
});

function baseImageStageLayersProps(
  overrides: Partial<React.ComponentProps<typeof ImageStageLayers>> = {}
): React.ComponentProps<typeof ImageStageLayers> {
  const page = makePage();
  return {
    activeBlockDragIds: [],
    activeBlockDragMode: null,
    activeLayer: "output",
    blockRangeSelectionActive: false,
    favoriteFontPresets: [],
    finalOutputPreviewActive: true,
    fontWeightAvailability: [],
    imageRef: React.createRef<HTMLCanvasElement>(),
    inpaintBrushSize: 12,
    inpaintDisabled: true,
    inpaintResultBrushColor: "#ffffff",
    inpaintResultBrushHardness: 1,
    inpaintResultBrushSize: 12,
    inpaintResultDisabled: true,
    inpaintResultTool: "brush",
    inpaintResultToolStrength: 1,
    inpaintResultComposite: true,
    inpaintSelectionRect: null,
    inpaintStrokeActive: false,
    inpaintTool: "brush",
    layerOpacity: {
      image: 1,
      inpaint: 1,
      inpaintMask: 0,
      inpaintResult: 1,
      overlay: 1
    },
    layerVisibility: {
      image: true,
      inpaint: true,
      inpaintMask: false,
      inpaintResult: true,
      overlay: false
    },
    onBlockFontSizeChange: () => undefined,
    onBlockFontStyleCopy: () => undefined,
    onBlockInlineEditActiveChange: () => undefined,
    onBlockPointerDown: () => undefined,
    onBlockTextAlignChange: () => undefined,
    onBlockTextSelectionSplitDuplicate: () => false,
    onBlockTextUpdate: () => undefined,
    onBlockAutoFitDisable: () => undefined,
    onFavoriteFontPresetSelect: () => undefined,
    onInpaintLayerChange: () => undefined,
    onInpaintLayerEditEnd: () => undefined,
    onInpaintLayerEditStart: () => undefined,
    onInpaintResultColorPick: () => undefined,
    onInpaintResultLayerChange: () => undefined,
    onInpaintSelectionChange: () => undefined,
    page,
    pageSize: {
      width: page.width,
      height: page.height
    },
    rangeSelectionPreviewRect: null,
    rangeToolActive: false,
    selectedBlockId: null,
    selectedBlockIds: [],
    stageSize: null,
    temporaryPanActive: false,
    ...overrides
  };
}

function makePage(): MangaPage {
  return {
    id: "page-1",
    name: "001.png",
    imagePath: "/library/001.png",
    dataUrl: "data:image/png;base64,source",
    inpaintMaskDataUrl: "data:image/png;base64,mask",
    inpaintResultDataUrl: "data:image/png;base64,result",
    width: 100,
    height: 120,
    blocks: [],
    analysisStatus: "idle",
    createdAt: "2026-01-01T00:00:00.000Z",
    updatedAt: "2026-01-01T00:00:00.000Z"
  };
}

function deferred<T>(): { promise: Promise<T>; resolve: (value: T) => void } {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((promiseResolve) => {
    resolve = promiseResolve;
  });
  return { promise, resolve };
}

function createCanvasContextMock(
  imageData: ImageData,
  options: { imageSmoothingEnabled: boolean }
): CanvasRenderingContext2D & {
  clearRect: ReturnType<typeof vi.fn>;
  drawImage: ReturnType<typeof vi.fn>;
  getImageData: ReturnType<typeof vi.fn>;
  putImageData: ReturnType<typeof vi.fn>;
} {
  return {
    imageSmoothingEnabled: options.imageSmoothingEnabled,
    clearRect: vi.fn(),
    drawImage: vi.fn(),
    getImageData: vi.fn(() => imageData),
    putImageData: vi.fn()
  } as unknown as CanvasRenderingContext2D & {
    clearRect: ReturnType<typeof vi.fn>;
    drawImage: ReturnType<typeof vi.fn>;
    getImageData: ReturnType<typeof vi.fn>;
    putImageData: ReturnType<typeof vi.fn>;
  };
}
