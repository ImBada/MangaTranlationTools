import { initializeCanvas, readPsd, writePsdBuffer, type Layer, type PixelData, type Psd } from "ag-psd";
import sharp from "sharp";
import type { ExportInpaintPsdRequest } from "../shared/types";

initializeCanvas(
  () => {
    throw new Error("PSD canvas rendering is not available on the server.");
  },
  (width, height) => ({ data: new Uint8ClampedArray(width * height * 4), width, height }) as ImageData
);

const EXPORT_BACKGROUND_LAYER_NAME = "배경 (이름변경금지)";
const EXPORT_RESULT_LAYER_NAME = "Inpaint Result";

type DecodedRgba = {
  data: Buffer;
};

type FlattenedLayer = {
  opacity: number;
  top: number;
  left: number;
  imageData: PixelData;
  isBackground: boolean;
};

export async function exportInpaintPsd(request: ExportInpaintPsdRequest): Promise<Buffer> {
  const width = assertPositiveInteger(request.width, "PSD 너비");
  const height = assertPositiveInteger(request.height, "PSD 높이");
  const source = await decodeDataUrlToRgba(request.sourceDataUrl, width, height);
  const result = request.resultDataUrl ? await decodeDataUrlToRgba(request.resultDataUrl, width, height) : null;
  const mask = request.maskDataUrl ? await decodeDataUrlToRgba(request.maskDataUrl, width, height) : null;
  const resultLayer = result ? applyMaskToResultLayer(result.data, mask?.data ?? null) : Buffer.alloc(width * height * 4);
  const composite = blendLayerOverOpaqueBackground(source.data, resultLayer);

  const psd: Psd = {
    width,
    height,
    imageData: toImageData(composite, width, height),
    children: [
      {
        name: EXPORT_BACKGROUND_LAYER_NAME,
        top: 0,
        left: 0,
        bottom: height,
        right: width,
        opacity: 1,
        imageData: toImageData(makeOpaque(source.data), width, height)
      },
      {
        name: EXPORT_RESULT_LAYER_NAME,
        top: 0,
        left: 0,
        bottom: height,
        right: width,
        opacity: 1,
        imageData: toImageData(resultLayer, width, height)
      }
    ]
  };

  return writePsdBuffer(psd);
}

export async function importInpaintPsd(buffer: Buffer, width: number, height: number): Promise<{ resultDataUrl: string; maskDataUrl: string }> {
  const psd = readPsd(buffer, {
    useImageData: true,
    skipCompositeImageData: true,
    skipThumbnail: true
  });
  if (psd.width !== width || psd.height !== height) {
    throw new Error(`PSD 크기가 현재 페이지와 다릅니다. PSD: ${psd.width}x${psd.height}, 페이지: ${width}x${height}`);
  }

  const flattened = flattenVisibleLayers(psd.children ?? []);
  const paintLayers = flattened.filter((layer) => !layer.isBackground);
  if (paintLayers.length === 0) {
    throw new Error("배경 레이어를 제외하고 가져올 PSD 레이어가 없습니다.");
  }

  const merged = mergeLayers(paintLayers, width, height);
  let hasPaint = false;
  const result = Buffer.alloc(width * height * 4);
  const mask = Buffer.alloc(width * height * 4);

  for (let offset = 0; offset < merged.length; offset += 4) {
    const alpha = merged[offset + 3];
    if (alpha <= 0) {
      continue;
    }
    hasPaint = true;
    result[offset] = merged[offset];
    result[offset + 1] = merged[offset + 1];
    result[offset + 2] = merged[offset + 2];
    result[offset + 3] = 255;
    mask[offset] = 255;
    mask[offset + 1] = 255;
    mask[offset + 2] = 255;
    mask[offset + 3] = alpha;
  }

  if (!hasPaint) {
    throw new Error("배경 레이어를 제외한 PSD 레이어가 비어 있습니다.");
  }

  return {
    resultDataUrl: await rgbaToPngDataUrl(result, width, height),
    maskDataUrl: await rgbaToPngDataUrl(mask, width, height)
  };
}

export function sanitizePsdFileBasename(value: string, fallback: string): string {
  const base = value.replace(/\.[^.]+$/u, "").replace(/[<>:"/\\|?*\x00-\x1f]/g, "_").trim();
  return base || fallback;
}

function flattenVisibleLayers(layers: Layer[], parentOpacity = 1, parentHidden = false): FlattenedLayer[] {
  const flattened: FlattenedLayer[] = [];
  layers.forEach((layer, index) => {
    const hidden = parentHidden || Boolean(layer.hidden);
    const opacity = parentOpacity * clampOpacity(layer.opacity ?? 1);
    const name = layer.name ?? `Layer ${index + 1}`;

    if (layer.children?.length) {
      flattened.push(...flattenVisibleLayers(layer.children, opacity, hidden));
      return;
    }
    if (hidden || !layer.imageData) {
      return;
    }
    flattened.push({
      opacity,
      top: Math.round(layer.top ?? 0),
      left: Math.round(layer.left ?? 0),
      imageData: layer.imageData,
      isBackground: isBackgroundLayer(name)
    });
  });
  return flattened;
}

function isBackgroundLayer(name: string): boolean {
  return name.trim() === EXPORT_BACKGROUND_LAYER_NAME;
}

function mergeLayers(layersTopToBottom: FlattenedLayer[], width: number, height: number): Buffer {
  const output = Buffer.alloc(width * height * 4);
  for (const layer of [...layersTopToBottom].reverse()) {
    const data = toUint8Array(layer.imageData);
    const layerWidth = layer.imageData.width;
    const layerHeight = layer.imageData.height;
    for (let y = 0; y < layerHeight; y += 1) {
      const targetY = y + layer.top;
      if (targetY < 0 || targetY >= height) {
        continue;
      }
      for (let x = 0; x < layerWidth; x += 1) {
        const targetX = x + layer.left;
        if (targetX < 0 || targetX >= width) {
          continue;
        }
        const sourceOffset = (y * layerWidth + x) * 4;
        const sourceAlpha = (data[sourceOffset + 3] / 255) * layer.opacity;
        if (sourceAlpha <= 0) {
          continue;
        }
        const targetOffset = (targetY * width + targetX) * 4;
        alphaBlendPixel(output, targetOffset, data, sourceOffset, sourceAlpha);
      }
    }
  }
  return output;
}

function alphaBlendPixel(output: Buffer, targetOffset: number, source: Uint8Array | Uint8ClampedArray, sourceOffset: number, sourceAlpha: number): void {
  const targetAlpha = output[targetOffset + 3] / 255;
  const nextAlpha = sourceAlpha + targetAlpha * (1 - sourceAlpha);
  if (nextAlpha <= 0) {
    output[targetOffset] = 0;
    output[targetOffset + 1] = 0;
    output[targetOffset + 2] = 0;
    output[targetOffset + 3] = 0;
    return;
  }
  for (let channel = 0; channel < 3; channel += 1) {
    output[targetOffset + channel] = Math.round(
      (source[sourceOffset + channel] * sourceAlpha + output[targetOffset + channel] * targetAlpha * (1 - sourceAlpha)) / nextAlpha
    );
  }
  output[targetOffset + 3] = Math.round(nextAlpha * 255);
}

function applyMaskToResultLayer(result: Buffer, mask: Buffer | null): Buffer {
  const output = Buffer.from(result);
  if (!mask) {
    return output;
  }
  for (let offset = 0; offset < output.length; offset += 4) {
    const maskAlpha = mask[offset + 3] / 255;
    const maskLuma = (mask[offset] * 0.299 + mask[offset + 1] * 0.587 + mask[offset + 2] * 0.114) / 255;
    output[offset + 3] = Math.round(output[offset + 3] * maskAlpha * maskLuma);
  }
  return output;
}

function blendLayerOverOpaqueBackground(background: Buffer, layer: Buffer): Buffer {
  const output = makeOpaque(background);
  for (let offset = 0; offset < output.length; offset += 4) {
    const alpha = layer[offset + 3] / 255;
    if (alpha <= 0) {
      continue;
    }
    output[offset] = Math.round(layer[offset] * alpha + output[offset] * (1 - alpha));
    output[offset + 1] = Math.round(layer[offset + 1] * alpha + output[offset + 1] * (1 - alpha));
    output[offset + 2] = Math.round(layer[offset + 2] * alpha + output[offset + 2] * (1 - alpha));
    output[offset + 3] = 255;
  }
  return output;
}

function makeOpaque(data: Buffer): Buffer {
  const output = Buffer.from(data);
  for (let offset = 3; offset < output.length; offset += 4) {
    output[offset] = 255;
  }
  return output;
}

function toImageData(data: Buffer, width: number, height: number): PixelData {
  return {
    data: new Uint8ClampedArray(data),
    width,
    height
  };
}

function toUint8Array(imageData: PixelData): Uint8Array | Uint8ClampedArray {
  if (imageData.data instanceof Uint8Array || imageData.data instanceof Uint8ClampedArray) {
    return imageData.data;
  }
  throw new Error("8비트 RGB PSD 레이어만 가져올 수 있습니다.");
}

async function decodeDataUrlToRgba(dataUrl: string, width: number, height: number): Promise<DecodedRgba> {
  const image = sharp(dataUrlToBuffer(dataUrl)).ensureAlpha().resize(width, height, { fit: "fill" });
  const { data } = await image.raw().toBuffer({ resolveWithObject: true });
  return {
    data
  };
}

async function rgbaToPngDataUrl(data: Buffer, width: number, height: number): Promise<string> {
  const buffer = await sharp(data, {
    raw: {
      width,
      height,
      channels: 4
    }
  }).png().toBuffer();
  return `data:image/png;base64,${buffer.toString("base64")}`;
}

function dataUrlToBuffer(dataUrl: string): Buffer {
  const match = /^data:image\/(?:png|jpeg|jpg|webp);base64,([A-Za-z0-9+/=]+)$/u.exec(dataUrl);
  if (!match) {
    throw new Error("이미지 데이터 URL이 올바르지 않습니다.");
  }
  return Buffer.from(match[1], "base64");
}

function assertPositiveInteger(value: number, label: string): number {
  if (!Number.isInteger(value) || value <= 0) {
    throw new Error(`${label} 값이 올바르지 않습니다.`);
  }
  return value;
}

function clampOpacity(value: number): number {
  if (!Number.isFinite(value)) {
    return 1;
  }
  return Math.max(0, Math.min(1, value));
}
