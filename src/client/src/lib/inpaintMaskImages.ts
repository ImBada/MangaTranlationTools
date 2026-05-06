import type { ImageRect, MangaPage, TranslationBlock } from "../../../shared/types";
import { bboxToPixels } from "../../../shared/geometry";

const TEXT_MASK_MIN_ALPHA = 48;
const TEXT_MASK_MAX_LUMA = 205;
const TEXT_MASK_MIN_COMPONENT_RATIO = 0.00002;
const TEXT_MASK_EDGE_COMPONENT_RATIO = 0.008;
const TEXT_MASK_BROAD_OUTER_RATIO = 0.01;
const TEXT_MASK_CORNER_STROKE_RATIO = 0.0015;

type TextMaskComponent = {
  pixels: number[];
  minX: number;
  minY: number;
  maxX: number;
  maxY: number;
  touchesEdge: boolean;
};

type DrawBlocksOnInpaintMaskOptions = {
  includeExistingMask?: boolean;
};

export async function drawBlocksOnInpaintMask(
  page: MangaPage,
  blocks: TranslationBlock[],
  options: DrawBlocksOnInpaintMaskOptions = {}
): Promise<string> {
  const canvas = document.createElement("canvas");
  canvas.width = page.width;
  canvas.height = page.height;

  const context = canvas.getContext("2d");
  if (!context) {
    return options.includeExistingMask === false ? "" : page.inpaintMaskDataUrl ?? page.inpaintLayerDataUrl ?? "";
  }

  const existingMask = page.inpaintMaskDataUrl ?? page.inpaintLayerDataUrl;
  if (existingMask && options.includeExistingMask !== false) {
    const existingImage = await loadImage(existingMask);
    context.drawImage(existingImage, 0, 0, page.width, page.height);
  }

  const sourceCanvas = document.createElement("canvas");
  sourceCanvas.width = page.width;
  sourceCanvas.height = page.height;
  const sourceContext = sourceCanvas.getContext("2d", { willReadFrequently: true });
  if (!sourceContext) {
    return canvas.toDataURL("image/png");
  }

  const sourceImage = await loadImage(page.dataUrl);
  sourceContext.drawImage(sourceImage, 0, 0, page.width, page.height);
  const sourcePixels = sourceContext.getImageData(0, 0, page.width, page.height);

  for (const block of blocks) {
    paintSourceTextPixelsOnMask(context, sourcePixels, page, block);
  }
  expandCanvasMask(context, page.width, page.height, 1);

  return canvas.toDataURL("image/png");
}

export async function maskDataUrlForSelection(maskDataUrl: string, width: number, height: number, rect: ImageRect): Promise<string | null> {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) {
    return null;
  }

  const maskImage = await loadImage(maskDataUrl);
  context.drawImage(maskImage, 0, 0, width, height);
  const pixels = context.getImageData(0, 0, width, height);
  const selection = clampImageRect(rect, width, height);
  let hasMaskPixel = false;

  for (let y = 0; y < height; y += 1) {
    for (let x = 0; x < width; x += 1) {
      const offset = (y * width + x) * 4;
      const insideSelection = x >= selection.x && x < selection.x + selection.width && y >= selection.y && y < selection.y + selection.height;
      const alpha = pixels.data[offset + 3] / 255;
      const luma = (pixels.data[offset] * 0.299 + pixels.data[offset + 1] * 0.587 + pixels.data[offset + 2] * 0.114) / 255;
      const active = insideSelection && alpha * luma > 0.03;
      if (active) {
        hasMaskPixel = true;
        continue;
      }
      pixels.data[offset] = 0;
      pixels.data[offset + 1] = 0;
      pixels.data[offset + 2] = 0;
      pixels.data[offset + 3] = 0;
    }
  }

  if (!hasMaskPixel) {
    return null;
  }
  context.putImageData(pixels, 0, 0);
  return canvas.toDataURL("image/png");
}

export async function clearImageDataUrlRect(dataUrl: string, width: number, height: number, rect: ImageRect): Promise<string | undefined> {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) {
    return dataUrl;
  }

  const image = await loadImage(dataUrl);
  context.drawImage(image, 0, 0, width, height);
  const selection = clampImageRect(rect, width, height);
  context.clearRect(selection.x, selection.y, selection.width, selection.height);
  return canvasHasVisiblePixels(canvas) ? canvas.toDataURL("image/png") : undefined;
}

export async function fillImageDataUrlRect({
  dataUrl,
  width,
  height,
  rect,
  fillStyle
}: {
  dataUrl: string | undefined;
  width: number;
  height: number;
  rect: ImageRect;
  fillStyle: string;
}): Promise<string> {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d");
  if (!context) {
    return dataUrl ?? "";
  }

  if (dataUrl) {
    const image = await loadImage(dataUrl);
    context.drawImage(image, 0, 0, width, height);
  }

  const selection = clampImageRect(rect, width, height);
  context.fillStyle = fillStyle;
  context.fillRect(selection.x, selection.y, selection.width, selection.height);
  return canvas.toDataURL("image/png");
}

export async function mergePartialInpaintResult(
  previousDataUrl: string | undefined,
  patchDataUrl: string,
  patchMaskDataUrl: string,
  width: number,
  height: number
): Promise<string> {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) {
    return patchDataUrl;
  }

  if (previousDataUrl) {
    const previousImage = await loadImage(previousDataUrl);
    context.drawImage(previousImage, 0, 0, width, height);
  }
  const mergedPixels = context.getImageData(0, 0, width, height);

  context.clearRect(0, 0, width, height);
  const patchImage = await loadImage(patchDataUrl);
  context.drawImage(patchImage, 0, 0, width, height);
  const patchPixels = context.getImageData(0, 0, width, height);

  context.clearRect(0, 0, width, height);
  const maskImage = await loadImage(patchMaskDataUrl);
  context.drawImage(maskImage, 0, 0, width, height);
  const maskPixels = context.getImageData(0, 0, width, height);

  replacePixelsInsideMask(mergedPixels.data, patchPixels.data, maskPixels.data);
  context.putImageData(mergedPixels, 0, 0);
  return canvas.toDataURL("image/png");
}

export async function mergeInpaintMaskDataUrls(
  baseMaskDataUrl: string,
  patchMaskDataUrl: string,
  width: number,
  height: number
): Promise<string> {
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) {
    return baseMaskDataUrl;
  }

  const baseImage = await loadImage(baseMaskDataUrl);
  context.drawImage(baseImage, 0, 0, width, height);
  const mergedPixels = context.getImageData(0, 0, width, height);
  context.clearRect(0, 0, width, height);

  const patchImage = await loadImage(patchMaskDataUrl);
  context.drawImage(patchImage, 0, 0, width, height);
  const patchPixels = context.getImageData(0, 0, width, height);

  mergeInpaintMaskPixels(mergedPixels.data, patchPixels.data);
  context.putImageData(mergedPixels, 0, 0);
  return canvas.toDataURL("image/png");
}

export async function mergePartialInpaintMask(
  previousMaskDataUrl: string | undefined,
  patchMaskDataUrl: string,
  width: number,
  height: number
): Promise<string> {
  return previousMaskDataUrl
    ? mergeInpaintMaskDataUrls(previousMaskDataUrl, patchMaskDataUrl, width, height)
    : patchMaskDataUrl;
}

export function mergeInpaintMaskPixels(basePixels: Uint8ClampedArray, patchPixels: Uint8ClampedArray): void {
  const length = Math.min(basePixels.length, patchPixels.length);
  for (let offset = 0; offset + 3 < length; offset += 4) {
    const covered = maskPixelCovered(basePixels, offset) || maskPixelCovered(patchPixels, offset);
    basePixels[offset] = covered ? 255 : basePixels[offset];
    basePixels[offset + 1] = covered ? 255 : basePixels[offset + 1];
    basePixels[offset + 2] = covered ? 255 : basePixels[offset + 2];
    basePixels[offset + 3] = covered ? 255 : 0;
  }
}

function replacePixelsInsideMask(
  targetPixels: Uint8ClampedArray,
  patchPixels: Uint8ClampedArray,
  maskPixels: Uint8ClampedArray
): void {
  const length = Math.min(targetPixels.length, patchPixels.length, maskPixels.length);
  for (let offset = 0; offset + 3 < length; offset += 4) {
    if (!maskPixelCovered(maskPixels, offset) && patchPixels[offset + 3] === 0) {
      continue;
    }
    targetPixels[offset] = patchPixels[offset];
    targetPixels[offset + 1] = patchPixels[offset + 1];
    targetPixels[offset + 2] = patchPixels[offset + 2];
    targetPixels[offset + 3] = patchPixels[offset + 3];
  }
}

function maskPixelCovered(pixels: Uint8ClampedArray, offset: number): boolean {
  const alpha = pixels[offset + 3] / 255;
  const luma = (pixels[offset] * 0.299 + pixels[offset + 1] * 0.587 + pixels[offset + 2] * 0.114) / 255;
  return alpha * luma > 0;
}

function clampImageRect(rect: ImageRect, width: number, height: number): ImageRect {
  const x = Math.max(0, Math.min(width, Math.floor(rect.x)));
  const y = Math.max(0, Math.min(height, Math.floor(rect.y)));
  const right = Math.max(x, Math.min(width, Math.ceil(rect.x + rect.width)));
  const bottom = Math.max(y, Math.min(height, Math.ceil(rect.y + rect.height)));
  return {
    x,
    y,
    width: right - x,
    height: bottom - y
  };
}

function canvasHasVisiblePixels(canvas: HTMLCanvasElement): boolean {
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) {
    return false;
  }

  const pixels = context.getImageData(0, 0, canvas.width, canvas.height).data;
  for (let offset = 3; offset < pixels.length; offset += 4) {
    if (pixels[offset] > 0) {
      return true;
    }
  }
  return false;
}

function paintSourceTextPixelsOnMask(
  context: CanvasRenderingContext2D,
  sourcePixels: ImageData,
  page: MangaPage,
  block: TranslationBlock
): void {
  const rect = textMaskScanRect(block, page.width, page.height);
  if (rect.width <= 0 || rect.height <= 0) {
    return;
  }

  const maskPixels = context.getImageData(rect.x, rect.y, rect.width, rect.height);
  const candidateMask = new Uint8Array(rect.width * rect.height);
  let painted = 0;

  for (let y = 0; y < rect.height; y += 1) {
    for (let x = 0; x < rect.width; x += 1) {
      const sourceOffset = ((rect.y + y) * page.width + rect.x + x) * 4;
      const alpha = sourcePixels.data[sourceOffset + 3];
      if (alpha < TEXT_MASK_MIN_ALPHA) {
        continue;
      }

      const red = sourcePixels.data[sourceOffset];
      const green = sourcePixels.data[sourceOffset + 1];
      const blue = sourcePixels.data[sourceOffset + 2];
      const luma = red * 0.299 + green * 0.587 + blue * 0.114;
      if (luma > TEXT_MASK_MAX_LUMA) {
        continue;
      }

      candidateMask[y * rect.width + x] = 255;
    }
  }

  const filteredMask = filterTextCandidateMask(candidateMask, rect.width, rect.height);
  for (let index = 0; index < filteredMask.length; index += 1) {
    if (filteredMask[index] <= 0) {
      continue;
    }
    const targetOffset = index * 4;
    if (maskPixels.data[targetOffset + 3] > 0) {
      continue;
    }
    maskPixels.data[targetOffset] = 255;
    maskPixels.data[targetOffset + 1] = 255;
    maskPixels.data[targetOffset + 2] = 255;
    maskPixels.data[targetOffset + 3] = 255;
    painted += 1;
  }

  if (painted > 0) {
    context.putImageData(maskPixels, rect.x, rect.y);
  }
}

function filterTextCandidateMask(mask: Uint8Array, width: number, height: number): Uint8Array {
  const output = new Uint8Array(mask.length);
  const visited = new Uint8Array(mask.length);
  const minPixels = Math.max(2, Math.floor(mask.length * TEXT_MASK_MIN_COMPONENT_RATIO));

  for (let index = 0; index < mask.length; index += 1) {
    if (visited[index] > 0 || mask[index] <= 0) {
      continue;
    }

    const component = collectTextMaskComponent(mask, visited, width, height, index);
    if (component.pixels.length < minPixels || isLikelyNonTextMaskComponent(component, width, height)) {
      continue;
    }

    for (const pixelIndex of component.pixels) {
      output[pixelIndex] = 255;
    }
  }

  return output;
}

function collectTextMaskComponent(
  mask: Uint8Array,
  visited: Uint8Array,
  width: number,
  height: number,
  startIndex: number
): TextMaskComponent {
  const queue = [startIndex];
  const pixels: number[] = [];
  visited[startIndex] = 1;
  let minX = startIndex % width;
  let maxX = minX;
  let minY = Math.floor(startIndex / width);
  let maxY = minY;
  let touchesEdge = minX === 0 || minY === 0 || minX === width - 1 || minY === height - 1;

  for (let cursor = 0; cursor < queue.length; cursor += 1) {
    const index = queue[cursor];
    pixels.push(index);
    const x = index % width;
    const y = Math.floor(index / width);
    minX = Math.min(minX, x);
    minY = Math.min(minY, y);
    maxX = Math.max(maxX, x);
    maxY = Math.max(maxY, y);
    touchesEdge = touchesEdge || x === 0 || y === 0 || x === width - 1 || y === height - 1;

    for (let neighborY = y - 1; neighborY <= y + 1; neighborY += 1) {
      if (neighborY < 0 || neighborY >= height) {
        continue;
      }
      for (let neighborX = x - 1; neighborX <= x + 1; neighborX += 1) {
        if (neighborX < 0 || neighborX >= width || (neighborX === x && neighborY === y)) {
          continue;
        }
        const neighborIndex = neighborY * width + neighborX;
        if (visited[neighborIndex] > 0 || mask[neighborIndex] <= 0) {
          continue;
        }
        visited[neighborIndex] = 1;
        queue.push(neighborIndex);
      }
    }
  }

  return {
    pixels,
    minX,
    minY,
    maxX,
    maxY,
    touchesEdge
  };
}

function isLikelyNonTextMaskComponent(component: TextMaskComponent, width: number, height: number): boolean {
  const componentWidth = component.maxX - component.minX + 1;
  const componentHeight = component.maxY - component.minY + 1;
  const area = componentWidth * componentHeight;
  const density = component.pixels.length / Math.max(1, area);
  const nearEdge = isNearTextMaskEdge(component, width, height);
  const inOuterBand = isInTextMaskOuterBand(component, width, height);
  const inCornerBand = isInTextMaskCornerBand(component, width, height);
  const elongatedLine = isElongatedTextMaskLine(component, width, height);
  const outerStroke = isOuterTextMaskStroke(component, width, height);
  const edgeSized = componentWidth >= width * 0.18 || componentHeight >= height * 0.18 || component.pixels.length >= width * height * TEXT_MASK_EDGE_COMPONENT_RATIO;
  const oversized = componentWidth >= width * 0.78 || componentHeight >= height * 0.78;
  const sparseLargeShape = density < 0.5 && (componentWidth >= width * 0.28 || componentHeight >= height * 0.28);
  const broadOuterShape =
    inOuterBand &&
    (component.touchesEdge || elongatedLine || sparseLargeShape) &&
    (componentWidth >= width * 0.2 || componentHeight >= height * 0.2 || component.pixels.length >= width * height * TEXT_MASK_BROAD_OUTER_RATIO);
  const cornerStroke =
    inCornerBand &&
    (outerStroke || sparseLargeShape || componentWidth >= width * 0.12 || componentHeight >= height * 0.12) &&
    component.pixels.length >= Math.max(6, width * height * TEXT_MASK_CORNER_STROKE_RATIO);

  return (
    cornerStroke ||
    broadOuterShape ||
    (nearEdge && (outerStroke || elongatedLine || oversized || sparseLargeShape || edgeSized)) ||
    (component.touchesEdge && edgeSized)
  );
}

function isNearTextMaskEdge(component: TextMaskComponent, width: number, height: number): boolean {
  const edgeGuard = Math.max(1, Math.min(8, Math.round(Math.min(width, height) * 0.04)));
  return (
    component.minX <= edgeGuard ||
    component.minY <= edgeGuard ||
    component.maxX >= width - 1 - edgeGuard ||
    component.maxY >= height - 1 - edgeGuard
  );
}

function isInTextMaskOuterBand(component: TextMaskComponent, width: number, height: number): boolean {
  const outerBandX = Math.max(2, Math.min(18, Math.round(width * 0.08)));
  const outerBandY = Math.max(2, Math.min(18, Math.round(height * 0.08)));
  return (
    component.minX <= outerBandX ||
    component.minY <= outerBandY ||
    component.maxX >= width - 1 - outerBandX ||
    component.maxY >= height - 1 - outerBandY
  );
}

function isInTextMaskCornerBand(component: TextMaskComponent, width: number, height: number): boolean {
  const cornerBandX = Math.max(3, Math.min(24, Math.round(width * 0.12)));
  const cornerBandY = Math.max(3, Math.min(24, Math.round(height * 0.12)));
  const nearLeft = component.minX <= cornerBandX;
  const nearRight = component.maxX >= width - 1 - cornerBandX;
  const nearTop = component.minY <= cornerBandY;
  const nearBottom = component.maxY >= height - 1 - cornerBandY;
  return (nearLeft || nearRight) && (nearTop || nearBottom);
}

function isElongatedTextMaskLine(component: TextMaskComponent, width: number, height: number): boolean {
  const componentWidth = component.maxX - component.minX + 1;
  const componentHeight = component.maxY - component.minY + 1;
  const longHorizontal = componentWidth >= Math.max(12, width * 0.35) && componentHeight <= Math.max(4, height * 0.08);
  const longVertical = componentHeight >= Math.max(12, height * 0.35) && componentWidth <= Math.max(4, width * 0.08);
  return longHorizontal || longVertical;
}

function isOuterTextMaskStroke(component: TextMaskComponent, width: number, height: number): boolean {
  const componentWidth = component.maxX - component.minX + 1;
  const componentHeight = component.maxY - component.minY + 1;
  const thinVertical = componentHeight >= Math.max(8, height * 0.12) && componentWidth <= Math.max(6, width * 0.1);
  const thinHorizontal = componentWidth >= Math.max(8, width * 0.12) && componentHeight <= Math.max(6, height * 0.1);
  const diagonalStroke =
    componentWidth >= Math.max(7, width * 0.08) &&
    componentHeight >= Math.max(7, height * 0.08) &&
    component.pixels.length / Math.max(1, componentWidth * componentHeight) < 0.58;
  return thinVertical || thinHorizontal || diagonalStroke;
}

function textMaskScanRect(block: TranslationBlock, pageWidth: number, pageHeight: number): { x: number; y: number; width: number; height: number } {
  const rect = bboxToPixels(block.bbox, pageWidth, pageHeight);
  const blockShortSide = Math.max(1, Math.min(rect.w, rect.h));
  const pageShortSide = Math.min(pageWidth, pageHeight);
  const padding = Math.max(2, Math.min(10, Math.round(Math.min(blockShortSide * 0.08, pageShortSide * 0.004))));
  const x = Math.max(0, Math.floor(rect.x - padding));
  const y = Math.max(0, Math.floor(rect.y - padding));
  const right = Math.min(pageWidth, Math.ceil(rect.x + rect.w + padding));
  const bottom = Math.min(pageHeight, Math.ceil(rect.y + rect.h + padding));

  return {
    x,
    y,
    width: Math.max(0, right - x),
    height: Math.max(0, bottom - y)
  };
}

function expandCanvasMask(context: CanvasRenderingContext2D, width: number, height: number, radius: number): void {
  if (radius <= 0) {
    return;
  }

  const imageData = context.getImageData(0, 0, width, height);
  const sourceMask = new Uint8Array(width * height);
  for (let index = 0; index < sourceMask.length; index += 1) {
    const offset = index * 4;
    sourceMask[index] = imageData.data[offset + 3] > 0 ? 255 : 0;
  }

  const expanded = expandMask(sourceMask, width, height, radius);
  for (let index = 0; index < expanded.length; index += 1) {
    if (expanded[index] <= 0) {
      continue;
    }
    const offset = index * 4;
    imageData.data[offset] = 255;
    imageData.data[offset + 1] = 255;
    imageData.data[offset + 2] = 255;
    imageData.data[offset + 3] = 255;
  }
  context.putImageData(imageData, 0, 0);
}

function expandMask(mask: Uint8Array, width: number, height: number, radius: number): Uint8Array {
  const horizontal = new Uint8Array(mask.length);
  const output = new Uint8Array(mask.length);

  for (let y = 0; y < height; y += 1) {
    let activeCount = 0;
    for (let x = -radius; x <= radius; x += 1) {
      if (x >= 0 && x < width && mask[y * width + x] > 0) {
        activeCount += 1;
      }
    }
    for (let x = 0; x < width; x += 1) {
      horizontal[y * width + x] = activeCount > 0 ? 255 : 0;
      const removeX = x - radius;
      const addX = x + radius + 1;
      if (removeX >= 0 && mask[y * width + removeX] > 0) {
        activeCount -= 1;
      }
      if (addX < width && mask[y * width + addX] > 0) {
        activeCount += 1;
      }
    }
  }

  for (let x = 0; x < width; x += 1) {
    let activeCount = 0;
    for (let y = -radius; y <= radius; y += 1) {
      if (y >= 0 && y < height && horizontal[y * width + x] > 0) {
        activeCount += 1;
      }
    }
    for (let y = 0; y < height; y += 1) {
      output[y * width + x] = activeCount > 0 ? 255 : 0;
      const removeY = y - radius;
      const addY = y + radius + 1;
      if (removeY >= 0 && horizontal[removeY * width + x] > 0) {
        activeCount -= 1;
      }
      if (addY < height && horizontal[addY * width + x] > 0) {
        activeCount += 1;
      }
    }
  }

  return output;
}

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("인페인트 레이어 이미지를 불러오지 못했습니다."));
    image.src = src;
  });
}
