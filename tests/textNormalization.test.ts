import { describe, expect, it } from "vitest";
import { normalizeKoreanText } from "../src/client/src/lib/textNormalization";

describe("text normalization", () => {
  it("normalizes three or more dots to a single ellipsis", () => {
    expect(normalizeKoreanText("아... 그렇군....")).toBe("아… 그렇군…");
  });
});
