import { existsSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { basename, extname } from "node:path";
import sharp from "sharp";

export type PageImageAsset = {
  buffer: Buffer;
  mime: string;
  updatedAt: string;
};

export function dataUrlToBuffer(dataUrl: string): Buffer {
  const match = /^data:image\/png;base64,(.+)$/u.exec(dataUrl);
  if (!match) {
    throw new Error("PNG 데이터 URL이 아닙니다.");
  }
  return Buffer.from(match[1], "base64");
}

export async function clipOpaqueInpaintResultToMask(resultDataUrl: string, maskDataUrl: string): Promise<string> {
  const result = await sharp(dataUrlToBuffer(resultDataUrl)).ensureAlpha().raw().toBuffer({ resolveWithObject: true });
  const resultData = Buffer.from(result.data);
  let hasTransparentPixel = false;
  for (let offset = 3; offset < resultData.length; offset += 4) {
    if (resultData[offset] < 255) {
      hasTransparentPixel = true;
      break;
    }
  }
  if (hasTransparentPixel) {
    return resultDataUrl;
  }

  const mask = await sharp(dataUrlToBuffer(maskDataUrl))
    .ensureAlpha()
    .resize(result.info.width, result.info.height, { fit: "fill" })
    .raw()
    .toBuffer({ resolveWithObject: true });

  for (let offset = 0; offset < resultData.length; offset += 4) {
    const alpha = mask.data[offset + 3] / 255;
    const luma = (mask.data[offset] * 0.299 + mask.data[offset + 1] * 0.587 + mask.data[offset + 2] * 0.114) / 255;
    resultData[offset + 3] = Math.round(255 * alpha * luma);
  }

  const buffer = await sharp(resultData, {
    raw: {
      width: result.info.width,
      height: result.info.height,
      channels: 4
    }
  }).png().toBuffer();
  return `data:image/png;base64,${buffer.toString("base64")}`;
}

export function sanitizeFileBasename(value: string, fallback: string): string {
  const base = basename(value, extname(value)).replace(/[<>:"/\\|?*\x00-\x1f]/g, "_").trim();
  return base || fallback;
}

export function sanitizeRenderFilename(value: string, fallback: string): string {
  return `${sanitizeFileBasename(value, fallback)}.png`;
}

export function isSupportedImagePath(filePath: string): boolean {
  return [".png", ".jpg", ".jpeg", ".webp"].includes(extname(filePath).toLowerCase());
}

export async function fileToDataUrl(filePath: string): Promise<string> {
  const buffer = await readFile(filePath);
  return `data:${mimeFromPath(filePath)};base64,${buffer.toString("base64")}`;
}

export async function readImageFileAsset(filePath: string, updatedAt: string): Promise<PageImageAsset> {
  if (!existsSync(filePath)) {
    throw new Error("이미지 파일을 찾지 못했습니다.");
  }
  return {
    buffer: await readFile(filePath),
    mime: mimeFromPath(filePath),
    updatedAt
  };
}

export function dataUrlToImageAsset(dataUrl: string, updatedAt: string): PageImageAsset {
  const match = /^data:(image\/(?:png|jpeg|jpg|webp));base64,([A-Za-z0-9+/=]+)$/u.exec(dataUrl);
  if (!match) {
    throw new Error("이미지 데이터 URL이 올바르지 않습니다.");
  }
  return {
    buffer: Buffer.from(match[2], "base64"),
    mime: match[1] === "image/jpg" ? "image/jpeg" : match[1],
    updatedAt
  };
}

function mimeFromPath(filePath: string): string {
  const ext = extname(filePath).toLowerCase();
  if (ext === ".jpg" || ext === ".jpeg") {
    return "image/jpeg";
  }
  if (ext === ".webp") {
    return "image/webp";
  }
  return "image/png";
}
