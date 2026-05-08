import type React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { mouseOnlyRangeInputProps } from "../src/client/src/lib/mouseOnlyCheckbox";

describe("mouse-only input props", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("removes focus when a range input is focused by the browser", () => {
    const requestAnimationFrame = vi.fn((callback: FrameRequestCallback) => {
      callback(0);
      return 1;
    });
    const blur = vi.fn();

    vi.stubGlobal("window", { requestAnimationFrame });

    mouseOnlyRangeInputProps.onFocus?.({
      currentTarget: { blur }
    } as unknown as React.FocusEvent<HTMLInputElement>);

    expect(requestAnimationFrame).toHaveBeenCalledTimes(1);
    expect(blur).toHaveBeenCalledTimes(1);
  });

  it("prevents space from activating a focused range input", () => {
    const preventDefault = vi.fn();

    mouseOnlyRangeInputProps.onKeyDown?.({
      code: "Space",
      key: " ",
      preventDefault
    } as unknown as React.KeyboardEvent<HTMLInputElement>);

    expect(preventDefault).toHaveBeenCalledTimes(1);
  });
});
