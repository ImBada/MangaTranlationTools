import { describe, expect, it } from "vitest";
import { readPsd } from "ag-psd";
import sharp from "sharp";
import { exportInpaintPsd, importInpaintPsd } from "../src/server/inpaintPsd";

async function rgbaToDataUrl(data: Buffer, width: number, height: number): Promise<string> {
  const png = await sharp(data, {
    raw: {
      width,
      height,
      channels: 4
    }
  }).png().toBuffer();
  return `data:image/png;base64,${png.toString("base64")}`;
}

async function dataUrlToRgba(dataUrl: string): Promise<Buffer> {
  const encoded = dataUrl.replace(/^data:image\/png;base64,/u, "");
  const { data } = await sharp(Buffer.from(encoded, "base64")).ensureAlpha().raw().toBuffer({ resolveWithObject: true });
  return data;
}

describe("inpaint PSD import/export", () => {
  it("exports the source as background and imports only painted layers as result and mask", async () => {
    const width = 2;
    const height = 2;
    const source = Buffer.from([
      10, 20, 30, 255, 40, 50, 60, 255,
      70, 80, 90, 255, 100, 110, 120, 255
    ]);
    const result = Buffer.from([
      200, 0, 0, 255, 0, 200, 0, 255,
      0, 0, 200, 255, 200, 200, 0, 255
    ]);
    const mask = Buffer.from([
      255, 255, 255, 255, 0, 0, 0, 0,
      0, 0, 0, 0, 255, 255, 255, 128
    ]);

    const psd = await exportInpaintPsd({
      chapterId: "chapter",
      pageId: "page",
      pageName: "page.png",
      width,
      height,
      sourceDataUrl: await rgbaToDataUrl(source, width, height),
      resultDataUrl: await rgbaToDataUrl(result, width, height),
      maskDataUrl: await rgbaToDataUrl(mask, width, height)
    });
    const exported = readPsd(psd, { useImageData: true, skipThumbnail: true });
    expect(exported.children?.map((layer) => layer.name)).toEqual(["배경 (이름변경금지)", "Inpaint Result"]);
    const imported = await importInpaintPsd(psd, width, height);
    const importedResult = await dataUrlToRgba(imported.resultDataUrl);
    const importedMask = await dataUrlToRgba(imported.maskDataUrl);

    expect([...importedResult.slice(0, 8)]).toEqual([200, 0, 0, 255, 0, 0, 0, 0]);
    expect([...importedMask.slice(0, 8)]).toEqual([255, 255, 255, 255, 0, 0, 0, 0]);
    expect([...importedResult.slice(8, 16)]).toEqual([0, 0, 0, 0, 200, 200, 0, 255]);
    expect([...importedMask.slice(8, 16)]).toEqual([0, 0, 0, 0, 255, 255, 255, 128]);
  });
});
