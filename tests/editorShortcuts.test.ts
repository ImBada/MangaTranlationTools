import { describe, expect, it } from "vitest";
import {
  isBlockCopyShortcut,
  isBlockInlineEditShortcut,
  isBlockPasteShortcut,
  isDeleteShortcut,
  isFindReplaceShortcut,
  isPageProgressToggleShortcut,
  resolveInpaintToolShortcut
} from "../src/client/src/lib/editorShortcuts";

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

  it("recognizes find replace with Command or Control F", () => {
    expect(isFindReplaceShortcut(keyboardEvent({ code: "KeyF", key: "f", metaKey: true }))).toBe(true);
    expect(isFindReplaceShortcut(keyboardEvent({ code: "KeyF", key: "f", ctrlKey: true }))).toBe(true);
    expect(isFindReplaceShortcut(keyboardEvent({ code: "KeyF", key: "f" }))).toBe(false);
    expect(isFindReplaceShortcut(keyboardEvent({ code: "KeyF", key: "f", ctrlKey: true, shiftKey: true }))).toBe(false);
  });

  it("recognizes the won key as page progress toggle without modifiers", () => {
    expect(isPageProgressToggleShortcut(keyboardEvent({ key: "₩" }))).toBe(true);
    expect(isPageProgressToggleShortcut(keyboardEvent({ key: "\\", code: "Backslash" }))).toBe(true);
    expect(isPageProgressToggleShortcut(keyboardEvent({ key: "", code: "IntlYen" }))).toBe(true);
    expect(isPageProgressToggleShortcut(keyboardEvent({ key: "`", code: "Backquote" }))).toBe(true);
    expect(isPageProgressToggleShortcut(keyboardEvent({ key: "₩", metaKey: true }))).toBe(false);
    expect(isPageProgressToggleShortcut(keyboardEvent({ key: "₩", shiftKey: true }))).toBe(false);
  });

  it("recognizes E as selected block inline edit without command modifiers", () => {
    expect(isBlockInlineEditShortcut(keyboardEvent({ code: "KeyE", key: "e" }))).toBe(true);
    expect(isBlockInlineEditShortcut(keyboardEvent({ code: "KeyE", key: "E", shiftKey: true }))).toBe(true);
    expect(isBlockInlineEditShortcut(keyboardEvent({ code: "KeyE", key: "e", altKey: true }))).toBe(false);
    expect(isBlockInlineEditShortcut(keyboardEvent({ code: "KeyE", key: "e", metaKey: true }))).toBe(false);
    expect(isBlockInlineEditShortcut(keyboardEvent({ code: "KeyE", key: "e", ctrlKey: true }))).toBe(false);
    expect(isBlockInlineEditShortcut(keyboardEvent({ code: "KeyB", key: "b" }))).toBe(false);
  });

  it("recognizes Alt E as the inpaint auto eraser shortcut", () => {
    expect(resolveInpaintToolShortcut(keyboardEvent({ code: "KeyE", key: "e", altKey: true }))).toBe("autoEraser");
    expect(resolveInpaintToolShortcut(keyboardEvent({ code: "KeyE", key: "e" }))).toBe("eraser");
    expect(resolveInpaintToolShortcut(keyboardEvent({ code: "KeyE", key: "e", altKey: true, shiftKey: true }))).toBe(null);
    expect(resolveInpaintToolShortcut(keyboardEvent({ code: "KeyE", key: "e", altKey: true, metaKey: true }))).toBe(null);
  });

  it("recognizes Alt B as the inpaint result smart brush shortcut", () => {
    expect(resolveInpaintToolShortcut(keyboardEvent({ code: "KeyB", key: "b", altKey: true }))).toBe("smartBrush");
    expect(resolveInpaintToolShortcut(keyboardEvent({ code: "KeyB", key: "b" }))).toBe("brush");
    expect(resolveInpaintToolShortcut(keyboardEvent({ code: "KeyB", key: "b", altKey: true, shiftKey: true }))).toBe(null);
    expect(resolveInpaintToolShortcut(keyboardEvent({ code: "KeyB", key: "b", altKey: true, ctrlKey: true }))).toBe(null);
  });

  it("recognizes Q as delete only in one-hand mode", () => {
    expect(isDeleteShortcut(keyboardEvent({ key: "Backspace" }), false)).toBe(true);
    expect(isDeleteShortcut(keyboardEvent({ key: "Delete" }), false)).toBe(true);
    expect(isDeleteShortcut(keyboardEvent({ code: "KeyQ", key: "q" }), false)).toBe(false);
    expect(isDeleteShortcut(keyboardEvent({ code: "KeyQ", key: "q" }), true)).toBe(true);
    expect(isDeleteShortcut(keyboardEvent({ code: "KeyQ", key: "q", metaKey: true }), true)).toBe(false);
  });
});
