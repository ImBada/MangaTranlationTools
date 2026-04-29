import { describe, expect, it } from "vitest";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import sharp from "sharp";
import { runInpaintEngine } from "../src/server/inpaintEngine";

describe("inpaint engine", () => {
  it("creates a PNG result by filling masked pixels from neighboring source colors", async () => {
    const source = rgbaDataUrl(3, 3, [
      [10, 20, 30, 255], [10, 20, 30, 255], [10, 20, 30, 255],
      [10, 20, 30, 255], [240, 0, 0, 255], [10, 20, 30, 255],
      [10, 20, 30, 255], [10, 20, 30, 255], [10, 20, 30, 255]
    ]);
    const mask = rgbaDataUrl(3, 3, [
      [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0],
      [0, 0, 0, 0], [255, 255, 255, 255], [0, 0, 0, 0],
      [0, 0, 0, 0], [0, 0, 0, 0], [0, 0, 0, 0]
    ]);

    const result = await runInpaintEngine(await source, await mask, "local-fill-fallback", {
      settings: {
        engine: "local-fill-fallback",
        paddingPx: 0,
        featherPx: 0,
        tileSize: 1024
      }
    });
    const decoded = await decodeRgba(result);
    const centerOffset = (1 * 3 + 1) * 4;

    expect(result.startsWith("data:image/png;base64,")).toBe(true);
    expect([...decoded.subarray(centerOffset, centerOffset + 4)]).toEqual([10, 20, 30, 255]);
    expect(decoded[3]).toBe(0);
  });

  it("expands the server-side mask by the configured padding", async () => {
    const source = await rgbaDataUrl(5, 1, [
      [0, 0, 0, 255], [0, 0, 0, 255], [0, 0, 0, 255], [0, 0, 0, 255], [0, 0, 0, 255]
    ]);
    const mask = await rgbaDataUrl(5, 1, [
      [0, 0, 0, 0], [0, 0, 0, 0], [255, 255, 255, 255], [0, 0, 0, 0], [0, 0, 0, 0]
    ]);

    const result = await runInpaintEngine(source, mask, "mask-fill-fallback", {
      settings: {
        engine: "mask-fill-fallback",
        paddingPx: 1,
        featherPx: 0,
        tileSize: 1024
      }
    });
    const decoded = await decodeRgba(result);

    expect([0, 1, 2, 3, 4].map((x) => decoded[x * 4])).toEqual([0, 255, 255, 255, 0]);
  });

  it("runs an external LaMa-compatible command with source, mask, and output placeholders", async () => {
    const dir = await mkdtemp(join(tmpdir(), "inpaint-engine-test-"));
    const scriptPath = join(dir, "copy-source.cjs");
    await writeFile(
      scriptPath,
      [
        "const fs = require('node:fs');",
        "const sourceIndex = process.argv.indexOf('--source');",
        "const maskIndex = process.argv.indexOf('--mask');",
        "const outputIndex = process.argv.indexOf('--output');",
        "if (sourceIndex < 0 || maskIndex < 0 || outputIndex < 0) process.exit(2);",
        "if (!fs.existsSync(process.argv[maskIndex + 1])) process.exit(3);",
        "fs.copyFileSync(process.argv[sourceIndex + 1], process.argv[outputIndex + 1]);"
      ].join("\n"),
      "utf8"
    );

    try {
      const source = await rgbaDataUrl(1, 1, [[32, 64, 96, 255]]);
      const mask = await rgbaDataUrl(1, 1, [[255, 255, 255, 255]]);
      const result = await runInpaintEngine(source, mask, "lama", {
        lamaCommand: process.execPath,
        lamaArgs: [scriptPath, "--source", "{source}", "--mask", "{mask}", "--output", "{output}"]
      });
      const decoded = await decodeRgba(result);

      expect([...decoded]).toEqual([32, 64, 96, 255]);
    } finally {
      await rm(dir, { recursive: true, force: true });
    }
  });

  it("passes only the masked crop to an external LaMa-compatible command", async () => {
    const dir = await mkdtemp(join(tmpdir(), "inpaint-engine-crop-test-"));
    const scriptPath = join(dir, "assert-crop.cjs");
    await writeFile(
      scriptPath,
      [
        "const fs = require('node:fs');",
        "const sourceIndex = process.argv.indexOf('--source');",
        "const outputIndex = process.argv.indexOf('--output');",
        "const sourcePath = process.argv[sourceIndex + 1];",
        "const outputPath = process.argv[outputIndex + 1];",
        "const bytes = fs.readFileSync(sourcePath);",
        "const width = bytes.readUInt32BE(16);",
        "if (width >= 100) process.exit(4);",
        "fs.copyFileSync(sourcePath, outputPath);"
      ].join("\n"),
      "utf8"
    );

    try {
      const source = await rgbaDataUrl(100, 1, Array.from({ length: 100 }, () => [12, 34, 56, 255]));
      const mask = await rgbaDataUrl(100, 1, Array.from({ length: 100 }, (_value, index) => index === 50 ? [255, 255, 255, 255] : [0, 0, 0, 0]));
      const result = await runInpaintEngine(source, mask, "lama", {
        lamaCommand: process.execPath,
        lamaArgs: [scriptPath, "--source", "{source}", "--output", "{output}"],
        settings: {
          engine: "lama",
          paddingPx: 0,
          featherPx: 0,
          tileSize: 128
        }
      });
      const metadata = await pngMetadata(result);

      expect(metadata.width).toBe(100);
      expect(metadata.height).toBe(1);
    } finally {
      await rm(dir, { recursive: true, force: true });
    }
  });
});

async function rgbaDataUrl(width: number, height: number, pixels: [number, number, number, number][]): Promise<string> {
  const data = Buffer.from(pixels.flat());
  const buffer = await sharp(data, {
    raw: {
      width,
      height,
      channels: 4
    }
  }).png().toBuffer();
  return `data:image/png;base64,${buffer.toString("base64")}`;
}

async function decodeRgba(dataUrl: string): Promise<Buffer> {
  const match = /^data:image\/png;base64,(.+)$/u.exec(dataUrl);
  if (!match) {
    throw new Error("PNG data URL expected");
  }
  const { data } = await sharp(Buffer.from(match[1], "base64")).ensureAlpha().raw().toBuffer({ resolveWithObject: true });
  return data;
}

async function pngMetadata(dataUrl: string): Promise<sharp.Metadata> {
  const match = /^data:image\/png;base64,(.+)$/u.exec(dataUrl);
  if (!match) {
    throw new Error("PNG data URL expected");
  }
  return sharp(Buffer.from(match[1], "base64")).metadata();
}
