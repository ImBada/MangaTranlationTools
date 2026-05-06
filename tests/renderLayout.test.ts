import { afterEach, describe, expect, it } from "vitest";
import {
  buildOverlayCanvasFont,
  hasNativeBoldFontWeight,
  resolveOverlayCanvasFontWeight,
  resolveBlockPaddingPx,
  resolveBlockTextLayout,
  resolveSyntheticBoldStrokeWidthPx,
  resolveSyntheticItalicSkewX,
  resolveTextPositionFactors,
  resolveWrappedTextLines
} from "../src/client/src/lib/overlayLayout";
import type { TranslationBlock } from "../src/shared/types";

const originalDocument = globalThis.document;

describe("render layout padding", () => {
  afterEach(() => {
    Object.defineProperty(globalThis, "document", {
      value: originalDocument,
      configurable: true,
      writable: true
    });
  });

  it("reduces padding for small blocks and keeps large block padding capped", () => {
    expect(resolveBlockPaddingPx({ left: 0, top: 0, width: 40, height: 40 })).toBe(0);
    expect(resolveBlockPaddingPx({ left: 0, top: 0, width: 64, height: 64 })).toBe(1);
    expect(resolveBlockPaddingPx({ left: 0, top: 0, width: 90, height: 90 })).toBe(2);
    expect(resolveBlockPaddingPx({ left: 0, top: 0, width: 240, height: 240 })).toBe(14);
  });

  it("uses block text padding when explicitly set", () => {
    expect(
      resolveBlockPaddingPx(
        { textPaddingPx: 12 },
        { left: 0, top: 0, width: 240, height: 240 },
        { width: 1000, height: 1000 },
        { width: 500, height: 500 }
      )
    ).toBe(6);
  });

  it("resolves text position anchors with center as the default", () => {
    expect(resolveTextPositionFactors(undefined)).toEqual({ x: 0.5, y: 0.5 });
    expect(resolveTextPositionFactors("top-left")).toEqual({ x: 0, y: 0 });
    expect(resolveTextPositionFactors("bottom-right")).toEqual({ x: 1, y: 1 });
  });

  it("shrinks horizontal single-character text to fit narrow block width", () => {
    installCanvasMeasureMock();

    const block: TranslationBlock = {
      id: "block-1",
      type: "speech",
      bbox: { x: 0, y: 0, w: 40, h: 300 },
      sourceText: "가",
      translatedText: "가",
      confidence: 1,
      sourceDirection: "vertical",
      renderDirection: "horizontal",
      fontSizePx: 96,
      lineHeight: 1.18,
      textAlign: "center",
      textColor: "#111111",
      backgroundColor: "#fffdf5",
      opacity: 1,
      autoFitText: true
    };

    const layout = resolveBlockTextLayout(block, block.translatedText, { width: 1000, height: 1000 }, { width: 1000, height: 1000 });

    expect(layout.fontSizePx).toBeLessThanOrEqual(18);
    expect(layout.overflow).toBe(false);
  });

  it("allows auto-fit text to grow beyond the previous 256px cap", () => {
    installCanvasMeasureMock();

    const block: TranslationBlock = {
      id: "block-1",
      type: "sfx",
      bbox: { x: 0, y: 0, w: 1000, h: 1000 },
      sourceText: "가",
      translatedText: "가",
      confidence: 1,
      sourceDirection: "vertical",
      renderDirection: "horizontal",
      fontSizePx: 600,
      lineHeight: 1,
      textAlign: "center",
      textColor: "#111111",
      backgroundColor: "#fffdf5",
      opacity: 1,
      autoFitText: true
    };

    const layout = resolveBlockTextLayout(block, block.translatedText, { width: 1000, height: 1000 }, { width: 1000, height: 1000 });

    expect(layout.fontSizePx).toBeGreaterThan(256);
    expect(layout.overflow).toBe(false);
  });

  it("wraps attached trailing marks without rendering an inserted space", () => {
    installCanvasMeasureMock();

    const block = {
      fontFamily: "Arial",
      fontWeight: 700,
      fontStyle: "normal"
    } as const;

    expect(resolveWrappedTextLines(block, "장난이 심하면 곤란해……", 10, 40)).toEqual(["장난이", "심하면", "곤란해", "……"]);
    expect(resolveWrappedTextLines(block, "곤란해……", 10, 120)).toEqual(["곤란해……"]);
    expect(resolveWrappedTextLines(block, "곤란해...", 10, 40)).toEqual(["곤란해", "..."]);
    expect(resolveWrappedTextLines(block, "지금…!!", 10, 40)).toEqual(["지금", "…!!"]);
    expect(resolveWrappedTextLines(block, "지금...?!", 10, 40)).toEqual(["지금", "...?!"]);
    expect(resolveWrappedTextLines(block, "지금…!!", 10, 120)).toEqual(["지금…!!"]);
    expect(resolveWrappedTextLines(block, "다음은—…", 10, 40)).toEqual(["다음은", "—…"]);
    expect(resolveWrappedTextLines(block, "다음은—…!!", 10, 40)).toEqual(["다음은", "—…!!"]);
    expect(resolveWrappedTextLines(block, "다음은—…", 10, 120)).toEqual(["다음은—…"]);
    expect(resolveWrappedTextLines(block, "곤란해~", 10, 35)).toEqual(["곤란해", "~"]);
    expect(resolveWrappedTextLines(block, "곤란해～", 10, 35)).toEqual(["곤란해", "～"]);
    expect(resolveWrappedTextLines(block, "곤란해~", 10, 120)).toEqual(["곤란해~"]);
    expect(resolveWrappedTextLines(block, "곤란해~!!", 10, 40)).toEqual(["곤란해", "~!!"]);
    expect(resolveWrappedTextLines(block, "곤란해～?", 10, 40)).toEqual(["곤란해", "～?"]);
    expect(resolveWrappedTextLines(block, "곤란해ー", 10, 35)).toEqual(["곤란해", "ー"]);
    expect(resolveWrappedTextLines(block, "곤란해ー!!", 10, 40)).toEqual(["곤란해", "ー!!"]);
    expect(resolveWrappedTextLines(block, "곤란해ー.", 10, 40)).toEqual(["곤란해", "ー."]);
    expect(resolveWrappedTextLines(block, "곤란해ーー", 10, 120)).toEqual(["곤란해ーー"]);
  });

  it("adds synthetic bold stroke only for bold font weights", () => {
    expect(resolveSyntheticBoldStrokeWidthPx({ fontWeight: 400 }, 24)).toBe(0);

    const semiBoldStrokeWidth = resolveSyntheticBoldStrokeWidthPx({ fontWeight: 600 }, 24);
    const boldStrokeWidth = resolveSyntheticBoldStrokeWidthPx({ fontWeight: 700 }, 24);
    const blackStrokeWidth = resolveSyntheticBoldStrokeWidthPx({ fontWeight: 900 }, 240);

    expect(semiBoldStrokeWidth).toBeGreaterThan(0);
    expect(boldStrokeWidth).toBeGreaterThan(semiBoldStrokeWidth);
    expect(blackStrokeWidth).toBeLessThanOrEqual(8);
  });

  it("skips synthetic bold when the selected font family has a native bold face", () => {
    const fontWeightAvailability = [
      { cssFamily: "\"Regular Only\", sans-serif", weights: [400] },
      { cssFamily: "\"Native Bold\", sans-serif", weights: [400, 700] }
    ];

    expect(hasNativeBoldFontWeight({ fontFamily: "\"Native Bold\", serif", fontWeight: 700 }, fontWeightAvailability)).toBe(true);
    expect(resolveSyntheticBoldStrokeWidthPx({ fontFamily: "\"Native Bold\", serif", fontWeight: 700 }, 24, fontWeightAvailability)).toBe(0);
    expect(resolveSyntheticBoldStrokeWidthPx({ fontFamily: "\"Regular Only\", serif", fontWeight: 700 }, 24, fontWeightAvailability)).toBeGreaterThan(0);
  });

  it("uses a regular canvas font weight when manual synthetic bold is applied", () => {
    const fontWeightAvailability = [
      { cssFamily: "\"Regular Only\", sans-serif", weights: [400] },
      { cssFamily: "\"Medium Only\", sans-serif", weights: [300, 500] },
      { cssFamily: "\"Native Bold\", sans-serif", weights: [400, 700] }
    ];

    expect(resolveOverlayCanvasFontWeight({ fontFamily: "\"Regular Only\", serif", fontWeight: 700 }, fontWeightAvailability)).toBe(400);
    expect(resolveOverlayCanvasFontWeight({ fontFamily: "\"Medium Only\", serif", fontWeight: 700 }, fontWeightAvailability)).toBe(500);
    expect(resolveOverlayCanvasFontWeight({ fontFamily: "\"Native Bold\", serif", fontWeight: 700 }, fontWeightAvailability)).toBe(700);
    expect(
      buildOverlayCanvasFont(
        24,
        { fontFamily: "\"Regular Only\", serif", fontWeight: 700, fontStyle: "normal" },
        fontWeightAvailability
      )
    ).toBe("normal 400 24px \"Regular Only\", serif");
  });

  it("uses synthetic italic skew instead of relying on a native italic face", () => {
    expect(resolveSyntheticItalicSkewX({ fontStyle: "normal" })).toBe(0);
    expect(resolveSyntheticItalicSkewX({ fontStyle: "italic" })).toBeLessThan(0);
    expect(buildOverlayCanvasFont(24, { fontFamily: "Arial", fontWeight: 400, fontStyle: "italic" })).toBe("normal 400 24px Arial");
  });
});

function installCanvasMeasureMock(): void {
  const context = {
    font: "",
    measureText(text: string) {
      const match = /(\d+)px/.exec(this.font);
      const fontSize = Number(match?.[1] ?? 16);
      return { width: [...text].length * fontSize * 0.95 } as TextMetrics;
    }
  };

  Object.defineProperty(globalThis, "document", {
    value: {
      createElement: () => ({
        getContext: () => context
      })
    },
    configurable: true,
    writable: true
  });
}
