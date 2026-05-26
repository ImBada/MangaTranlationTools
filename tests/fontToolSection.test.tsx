import { describe, expect, it } from "vitest";
import { resolveFontPresetTagActiveId } from "../src/client/src/components/tools/FontToolSection";

describe("font preset tag active state", () => {
  it("does not treat the editing preset as active while multiple blocks are selected", () => {
    expect(resolveFontPresetTagActiveId(null, "preset-editing", true)).toBeNull();
  });

  it("keeps single block and preset editing active states", () => {
    expect(resolveFontPresetTagActiveId({ fontPresetId: "preset-block" }, "preset-editing", false)).toBe("preset-block");
    expect(resolveFontPresetTagActiveId(null, "preset-editing", false)).toBe("preset-editing");
  });
});
