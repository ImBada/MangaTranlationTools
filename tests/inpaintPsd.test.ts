import { describe, expect, it } from "vitest";
import { readPsd, writePsdBuffer, type PixelData, type Psd } from "ag-psd";
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

function solidPixel(red: number, green: number, blue: number, alpha = 255): PixelData {
  return {
    data: new Uint8ClampedArray([red, green, blue, alpha]),
    width: 1,
    height: 1
  };
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
    expect([...importedResult.slice(8, 16)]).toEqual([0, 0, 0, 0, 200, 200, 0, 128]);
    expect([...importedMask.slice(8, 16)]).toEqual([0, 0, 0, 0, 255, 255, 255, 255]);
  });

  it("exports translation blocks as a hidden top layer", async () => {
    const source = Buffer.from([10, 20, 30, 255]);
    const result = Buffer.from([200, 0, 0, 255]);
    const translationBlocks = Buffer.from([0, 0, 0, 96]);

    const psd = await exportInpaintPsd({
      chapterId: "chapter",
      pageId: "page",
      pageName: "page.png",
      width: 1,
      height: 1,
      sourceDataUrl: await rgbaToDataUrl(source, 1, 1),
      resultDataUrl: await rgbaToDataUrl(result, 1, 1),
      translationBlocksDataUrl: await rgbaToDataUrl(translationBlocks, 1, 1)
    });
    const exported = readPsd(psd, { useImageData: true, skipThumbnail: true });

    expect(exported.children?.map((layer) => layer.name)).toEqual(["배경 (이름변경금지)", "Inpaint Result", "번역 블록"]);
    expect(exported.children?.[2]?.hidden).toBe(true);
    expect([...(exported.children?.[2]?.imageData?.data ?? [])]).toEqual([0, 0, 0, 96]);

    const imported = await importInpaintPsd(psd, 1, 1);
    expect([...(await dataUrlToRgba(imported.resultDataUrl))]).toEqual([200, 0, 0, 255]);
  });

  it("preserves visible layer order when importing layers inside groups", async () => {
    const psd: Psd = {
      width: 1,
      height: 1,
      children: [
        {
          name: "배경 (이름변경금지)",
          top: 0,
          left: 0,
          bottom: 1,
          right: 1,
          imageData: solidPixel(255, 255, 255)
        },
        {
          name: "paint group",
          children: [
            {
              name: "group bottom red",
              top: 0,
              left: 0,
              bottom: 1,
              right: 1,
              imageData: solidPixel(255, 0, 0)
            },
            {
              name: "group top blue",
              top: 0,
              left: 0,
              bottom: 1,
              right: 1,
              imageData: solidPixel(0, 0, 255)
            }
          ]
        }
      ]
    };

    const imported = await importInpaintPsd(Buffer.from(writePsdBuffer(psd)), 1, 1);
    const importedResult = await dataUrlToRgba(imported.resultDataUrl);
    const importedMask = await dataUrlToRgba(imported.maskDataUrl);

    expect([...importedResult]).toEqual([0, 0, 255, 255]);
    expect([...importedMask]).toEqual([255, 255, 255, 255]);
  });
});
