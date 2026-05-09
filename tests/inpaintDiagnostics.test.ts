import { afterEach, describe, expect, it, vi } from "vitest";
import { isInpaintDebugLogEnabled, summarizeDataUrl, writeInpaintDebugLog } from "../src/client/src/lib/inpaintDiagnostics";

const DEBUG_STORAGE_KEY = "mangaTranslationTools.inpaintDebugLogs";

describe("inpaint diagnostics", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("does not emit logs when debug logging is disabled", () => {
    const writeLog = vi.fn(() => Promise.resolve());
    const detail = vi.fn(() => ({ value: 1 }));
    vi.stubGlobal("window", {
      location: { search: "" },
      localStorage: { getItem: vi.fn(() => null) },
      mangaApi: { writeLog }
    });

    writeInpaintDebugLog("inpaint:test", detail);

    expect(isInpaintDebugLogEnabled()).toBe(false);
    expect(detail).not.toHaveBeenCalled();
    expect(writeLog).not.toHaveBeenCalled();
  });

  it("emits logs when debug logging is enabled through localStorage", () => {
    const writeLog = vi.fn(() => Promise.resolve());
    vi.stubGlobal("window", {
      location: { search: "" },
      localStorage: { getItem: vi.fn((key: string) => (key === DEBUG_STORAGE_KEY ? "1" : null)) },
      mangaApi: { writeLog }
    });

    writeInpaintDebugLog("inpaint:test", { value: 1 });

    expect(isInpaintDebugLogEnabled()).toBe(true);
    expect(writeLog).toHaveBeenCalledWith("debug", "inpaint:test", { value: 1 });
  });

  it("only includes data URL fingerprints while debug logging is enabled", () => {
    const dataUrl = "data:image/png;base64,abc123";
    vi.stubGlobal("window", {
      location: { search: "" },
      localStorage: { getItem: vi.fn(() => null) }
    });

    expect(summarizeDataUrl(dataUrl)).toEqual({
      present: true,
      length: dataUrl.length,
      mime: "image/png"
    });

    vi.stubGlobal("window", {
      location: { search: "?inpaintDebug=1" },
      localStorage: { getItem: vi.fn(() => null) }
    });

    expect(summarizeDataUrl(dataUrl)).toMatchObject({
      present: true,
      length: dataUrl.length,
      mime: "image/png",
      fingerprint: expect.any(String)
    });
  });
});
