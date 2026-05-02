import { describe, expect, it } from "vitest";
import { isBlockCopyShortcut, isBlockPasteShortcut } from "../src/client/src/lib/editorShortcuts";

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

  it("recognizes block paste with Command, Control, or plain V", () => {
    expect(isBlockPasteShortcut(keyboardEvent({ code: "KeyV", key: "v", metaKey: true }))).toBe(true);
    expect(isBlockPasteShortcut(keyboardEvent({ code: "KeyV", key: "v", ctrlKey: true }))).toBe(true);
    expect(isBlockPasteShortcut(keyboardEvent({ code: "KeyV", key: "v" }))).toBe(true);
    expect(isBlockPasteShortcut(keyboardEvent({ code: "KeyV", key: "v", shiftKey: true }))).toBe(false);
  });
});
