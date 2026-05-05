import { describe, expect, it } from "vitest";
import { isBlockCopyShortcut, isBlockPasteShortcut, isDeleteShortcut } from "../src/client/src/lib/editorShortcuts";

function keyboardEvent(patch: Partial<KeyboardEvent>): KeyboardEvent {
  return {
    key: "",
    code: "",
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    ...patch
  } as KeyboardEvent;
}

describe("editor shortcuts", () => {
  it("recognizes block copy with Command or Control", () => {
    expect(isBlockCopyShortcut(keyboardEvent({ code: "KeyC", key: "c", metaKey: true }))).toBe(true);
    expect(isBlockCopyShortcut(keyboardEvent({ code: "KeyC", key: "c", ctrlKey: true }))).toBe(true);
    expect(isBlockCopyShortcut(keyboardEvent({ code: "KeyC", key: "c" }))).toBe(false);
  });

  it("recognizes block paste with Command or Control", () => {
    expect(isBlockPasteShortcut(keyboardEvent({ code: "KeyV", key: "v", metaKey: true }))).toBe(true);
    expect(isBlockPasteShortcut(keyboardEvent({ code: "KeyV", key: "v", ctrlKey: true }))).toBe(true);
    expect(isBlockPasteShortcut(keyboardEvent({ code: "KeyV", key: "v" }))).toBe(false);
    expect(isBlockPasteShortcut(keyboardEvent({ code: "KeyV", key: "v", shiftKey: true }))).toBe(false);
  });

  it("recognizes Q as delete only in one-hand mode", () => {
    expect(isDeleteShortcut(keyboardEvent({ key: "Backspace" }), false)).toBe(true);
    expect(isDeleteShortcut(keyboardEvent({ key: "Delete" }), false)).toBe(true);
    expect(isDeleteShortcut(keyboardEvent({ code: "KeyQ", key: "q" }), false)).toBe(false);
    expect(isDeleteShortcut(keyboardEvent({ code: "KeyQ", key: "q" }), true)).toBe(true);
    expect(isDeleteShortcut(keyboardEvent({ code: "KeyQ", key: "q", metaKey: true }), true)).toBe(false);
  });
});
