import { describe, expect, it } from "vitest";
import { normalizeKoreanText } from "../src/client/src/lib/textNormalization";

describe("text normalization", () => {
  it("normalizes each group of three dots to an ellipsis", () => {
    expect(normalizeKoreanText("아... 그렇군.... 기다려...... 설마.........")).toBe("아… 그렇군… 기다려…… 설마………");
  });

  it("normalizes double hyphens to an em dash", () => {
    expect(normalizeKoreanText("잠깐-- 뭐라고--")).toBe("잠깐— 뭐라고—");
  });
});
