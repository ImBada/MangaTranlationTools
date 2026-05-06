import { isMacLikePlatform } from "./globalUndo";

type InpaintToolKey = "select" | "brush" | "eraser" | "autoEraser";
type ModifierKeyEvent = Pick<KeyboardEvent | PointerEvent, "ctrlKey" | "metaKey">;

export const INPAINT_TOOL_SHORTCUTS: Partial<Record<InpaintToolKey, string>> = {
  select: "T",
  brush: "B",
  eraser: "E",
  autoEraser: "Alt+E"
};
export const BLOCK_INLINE_EDIT_SHORTCUT = "E";

export function resolveInpaintToolShortcut(event: KeyboardEvent): InpaintToolKey | null {
  if (event.altKey) {
    return !event.ctrlKey && !event.metaKey && !event.shiftKey && isKeyE(event) ? "autoEraser" : null;
  }

  if (event.ctrlKey || event.metaKey) {
    return null;
  }

  switch (event.code) {
    case "KeyB":
      return "brush";
    case "KeyE":
      return "eraser";
  }

  switch (event.key.toLowerCase()) {
    case "b":
      return "brush";
    case "e":
      return "eraser";
    default:
      return null;
  }
}

function isKeyE(event: KeyboardEvent): boolean {
  return event.code === "KeyE" || event.key.toLowerCase() === "e";
}

export function isZoomToolShortcut(event: KeyboardEvent): boolean {
  return event.code === "KeyZ" || event.key.toLowerCase() === "z";
}

export function isPointerToolShortcut(event: KeyboardEvent): boolean {
  return event.code === "KeyA" || event.key.toLowerCase() === "a";
}

export function isRangeToolShortcut(event: KeyboardEvent): boolean {
  return event.code === "KeyT" || event.key.toLowerCase() === "t";
}

export function isBlockInlineEditShortcut(event: KeyboardEvent): boolean {
  return !event.altKey && !event.ctrlKey && !event.metaKey && (event.code === "KeyE" || event.key.toLowerCase() === "e");
}

export function isBlockCopyShortcut(event: KeyboardEvent): boolean {
  return !event.altKey && (event.metaKey || event.ctrlKey) && (event.code === "KeyC" || event.key.toLowerCase() === "c");
}

export function isBlockPasteShortcut(event: KeyboardEvent): boolean {
  const hasPasteModifier = event.metaKey || event.ctrlKey;
  return !event.altKey && !event.shiftKey && hasPasteModifier && (event.code === "KeyV" || event.key.toLowerCase() === "v");
}

export function isFindReplaceShortcut(event: KeyboardEvent): boolean {
  const hasFindModifier = event.metaKey || event.ctrlKey;
  return !event.altKey && !event.shiftKey && hasFindModifier && (event.code === "KeyF" || event.key.toLowerCase() === "f");
}

export function isPageProgressToggleShortcut(event: KeyboardEvent): boolean {
  if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) {
    return false;
  }
  return (
    event.key === "₩" ||
    event.key === "\\" ||
    event.key === "¥" ||
    event.key === "`" ||
    event.code === "IntlYen" ||
    event.code === "Backslash" ||
    event.code === "Backquote"
  );
}

export function isBlockDuplicateModifier(event: ModifierKeyEvent, platform: string): boolean {
  return isMacLikePlatform(platform) ? event.metaKey : event.ctrlKey;
}

export function isDeleteShortcut(event: KeyboardEvent, oneHandMode: boolean): boolean {
  if (event.key === "Delete" || event.key === "Backspace") {
    return true;
  }
  return (
    oneHandMode &&
    !event.altKey &&
    !event.ctrlKey &&
    !event.metaKey &&
    (event.code === "KeyQ" || event.key.toLowerCase() === "q")
  );
}
