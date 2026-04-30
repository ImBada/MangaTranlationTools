import { describe, expect, it, vi } from "vitest";
import { mkdtemp, rm } from "node:fs/promises";
import { join } from "node:path";
import { tmpdir } from "node:os";
import sharp from "sharp";

const runtimeHelpers = require("../src/server/runtime/simple-page-translate.cjs") as {
  enhanceBitmapBuffer: (bitmap: Buffer, contrast?: number, grayscale?: boolean) => Buffer;
  getScaledSize: (width: number, height: number, maxLongSide: number) => { width: number; height: number };
  prepareImageVariants: (options: {
    imagePath: string;
    outputDir: string;
    includeEnhancedVariant: boolean;
    enhancedMaxLongSide: number;
    enhancedContrast: number;
    label: string;
  }) => Promise<{
    diagnostics: unknown[];
    imageVariants: Array<{ role: string; path: string; mime: string; dataUrl: string }>;
  }>;
};
const { enhanceBitmapBuffer, getScaledSize, prepareImageVariants } = runtimeHelpers;

describe("runtime image enhancement helpers", () => {
  it("scales images down while preserving aspect ratio", () => {
    expect(getScaledSize(3000, 1500, 1900)).toEqual({
      width: 1900,
      height: 950
    });

    expect(getScaledSize(1000, 1400, 1900)).toEqual({
      width: 1000,
      height: 1400
    });
  });

  it("applies grayscale contrast while preserving alpha", () => {
    const input = Buffer.from([
      10, 20, 30, 255,
      200, 150, 100, 128
    ]);

    const output = enhanceBitmapBuffer(input, 1.35, true);

    expect(output).not.toBe(input);
    expect(output[0]).toBe(output[1]);
    expect(output[1]).toBe(output[2]);
    expect(output[4]).toBe(output[5]);
    expect(output[5]).toBe(output[6]);
    expect(output[3]).toBe(255);
    expect(output[7]).toBe(128);
  });

  it("leaves colors untouched when contrast is neutral and grayscale is disabled", () => {
    const input = Buffer.from([
      11, 22, 33, 44,
      55, 66, 77, 88
    ]);

    const output = enhanceBitmapBuffer(input, 1, false);

    expect([...output]).toEqual([...input]);
  });

  it("builds an enhanced PNG variant without PowerShell", async () => {
    const tempDir = await mkdtemp(join(tmpdir(), "manga-runtime-enhance-"));
    try {
      const imagePath = join(tempDir, "input.png");
      const outputDir = join(tempDir, "out");
      await sharp({
        create: {
          width: 20,
          height: 10,
          channels: 4,
          background: { r: 180, g: 120, b: 60, alpha: 1 }
        }
      }).png().toFile(imagePath);

      let stderrOutput = "";
      const stderrWrite = vi.spyOn(process.stderr, "write").mockImplementation((chunk: string | Uint8Array) => {
        stderrOutput += String(chunk);
        return true;
      });
      let result: Awaited<ReturnType<typeof prepareImageVariants>>;
      try {
        result = await prepareImageVariants({
          imagePath,
          outputDir,
          includeEnhancedVariant: true,
          enhancedMaxLongSide: 8,
          enhancedContrast: 1.35,
          label: "test-page"
        });
      } finally {
        stderrWrite.mockRestore();
      }

      const enhanced = result.imageVariants.find((variant) => variant.role === "enhanced");
      expect(result.diagnostics).toEqual([]);
      expect(enhanced?.mime).toBe("image/png");
      expect(enhanced?.path).toBe(join(outputDir, "input-enhanced.png"));
      expect(enhanced?.dataUrl.startsWith("data:image/png;base64,")).toBe(true);
      expect(stderrOutput).toContain("[runtime:test-page:info] enhanced PNG variant ready");
      await expect(sharp(enhanced?.path).metadata()).resolves.toMatchObject({
        width: 8,
        height: 4,
        format: "png"
      });
    } finally {
      await rm(tempDir, { recursive: true, force: true });
    }
  });
});
