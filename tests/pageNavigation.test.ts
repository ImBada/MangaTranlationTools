import { describe, expect, it } from "vitest";
import { resolveAdjacentPageId, resolveKeyboardPageNavigation } from "../src/client/src/lib/pageNavigation";

type KeyboardPageNavigationOptions = Parameters<typeof resolveKeyboardPageNavigation>[0];

function resolveNavigation(patch: Partial<KeyboardPageNavigationOptions>) {
  return resolveKeyboardPageNavigation({
    key: "",
    code: "",
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    hasPages: true,
    modalOpen: false,
    editableTarget: false,
    centerPanelFocused: true,
    ...patch
  });
}

describe("page navigation helpers", () => {
  const pageIds = ["page-1", "page-2", "page-3"];

  it("moves to the previous and next page around the current selection", () => {
    expect(resolveAdjacentPageId(pageIds, "page-2", "previous")).toBe("page-1");
    expect(resolveAdjacentPageId(pageIds, "page-2", "next")).toBe("page-3");
  });

  it("does not wrap beyond the first or last page", () => {
    expect(resolveAdjacentPageId(pageIds, "page-1", "previous")).toBeNull();
    expect(resolveAdjacentPageId(pageIds, "page-3", "next")).toBeNull();
  });

  it("treats the first page as current when no explicit selection exists", () => {
    expect(resolveAdjacentPageId(pageIds, null, "previous")).toBeNull();
    expect(resolveAdjacentPageId(pageIds, null, "next")).toBe("page-2");
  });

  it("ignores navigation requests when no pages are available", () => {
    expect(resolveAdjacentPageId([], "page-1", "previous")).toBeNull();
    expect(resolveNavigation({
      key: "ArrowRight",
      hasPages: false
    })).toBeNull();
  });

  it("maps left and up to previous, right and down to next", () => {
    expect(resolveNavigation({
      key: "ArrowLeft",
      centerPanelFocused: false
    })).toEqual({
      direction: "previous",
      preventDefault: false
    });

    expect(resolveNavigation({
      key: "ArrowRight",
      centerPanelFocused: false
    })).toEqual({
      direction: "next",
      preventDefault: false
    });

    expect(resolveNavigation({
      key: "ArrowUp"
    })).toEqual({
      direction: "previous",
      preventDefault: true
    });

    expect(resolveNavigation({
      key: "ArrowDown"
    })).toEqual({
      direction: "next",
      preventDefault: true
    });
  });

  it("maps D and F shortcuts to previous and next pages", () => {
    expect(resolveNavigation({
      key: "d",
      code: "KeyD"
    })).toEqual({
      direction: "previous",
      preventDefault: true
    });

    expect(resolveNavigation({
      key: "f",
      code: "KeyF"
    })).toEqual({
      direction: "next",
      preventDefault: true
    });
  });

  it("ignores unrelated keys and up/down outside the center panel", () => {
    expect(resolveNavigation({
      key: "Enter"
    })).toBeNull();

    expect(resolveNavigation({
      key: "ArrowUp",
      centerPanelFocused: false
    })).toBeNull();
  });

  it("ignores navigation when a modal is open or the focus target is editable", () => {
    expect(resolveNavigation({
      key: "ArrowRight",
      modalOpen: true
    })).toBeNull();

    expect(resolveNavigation({
      key: "ArrowRight",
      editableTarget: true
    })).toBeNull();
  });

  it("ignores modified D and F shortcuts", () => {
    expect(resolveNavigation({
      key: "d",
      code: "KeyD",
      metaKey: true
    })).toBeNull();

    expect(resolveNavigation({
      key: "f",
      code: "KeyF",
      ctrlKey: true
    })).toBeNull();
  });
});
