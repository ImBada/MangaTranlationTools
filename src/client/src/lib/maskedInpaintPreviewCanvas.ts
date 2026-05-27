import { loadCanvasImage } from "./canvasImageDrawing";
import { applyInpaintMaskPixelsToLayer, drawInpaintMaskImageForPixelRead } from "./inpaintMaskImages";
import type { ViewportSize } from "./overlayLayout";

type LoadPreviewCanvasImage = (src: string, message: string) => Promise<CanvasImageSource>;

export type DrawMaskedInpaintResultPreviewCanvasOptions = {
  canvas: HTMLCanvasElement;
  context: CanvasRenderingContext2D;
  createMaskCanvas?: () => HTMLCanvasElement;
  dataUrl: string;
  isCancelled?: () => boolean;
  loadImage?: LoadPreviewCanvasImage;
  maskDataUrl?: string;
  pageSize: ViewportSize;
};

export async function drawMaskedInpaintResultPreviewCanvas({
  canvas,
  context,
  createMaskCanvas = () => document.createElement("canvas"),
  dataUrl,
  isCancelled = () => false,
  loadImage = loadCanvasImage,
  maskDataUrl,
  pageSize
}: DrawMaskedInpaintResultPreviewCanvasOptions): Promise<void> {
  const [resultImage, maskImage] = await Promise.all([
    loadImage(dataUrl, "인페인트 결과 레이어를 불러오지 못했습니다."),
    maskDataUrl ? loadImage(maskDataUrl, "인페인트 마스크를 불러오지 못했습니다.") : Promise.resolve(null)
  ]);
  if (isCancelled()) {
    return;
  }

  context.clearRect(0, 0, canvas.width, canvas.height);
  context.imageSmoothingEnabled = true;
  context.drawImage(resultImage, 0, 0, pageSize.width, pageSize.height);

  if (!maskImage) {
    return;
  }

  const maskCanvas = createMaskCanvas();
  maskCanvas.width = pageSize.width;
  maskCanvas.height = pageSize.height;
  const maskContext = maskCanvas.getContext("2d", { willReadFrequently: true });
  if (!maskContext) {
    context.clearRect(0, 0, pageSize.width, pageSize.height);
    return;
  }

  drawInpaintMaskImageForPixelRead(maskContext, maskImage, pageSize.width, pageSize.height);
  const layerPixels = context.getImageData(0, 0, pageSize.width, pageSize.height);
  const maskPixels = maskContext.getImageData(0, 0, pageSize.width, pageSize.height).data;
  applyInpaintMaskPixelsToLayer(layerPixels.data, maskPixels);
  context.putImageData(layerPixels, 0, 0);
}
