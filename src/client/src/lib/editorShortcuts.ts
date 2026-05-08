import { isMacLikePlatform } from "./globalUndo";
import type { ActiveLayer, LayerVisibility } from "./layerState";

type InpaintToolKey = "select" | "brush" | "smartBrush" | "eraser" | "autoEraser" | "colorPicker";
type ModifierKeyEvent = Pick<KeyboardEvent | PointerEvent, "ctrlKey" | "metaKey">;
type ResolvedInpaintToolShortcut = Exclude<InpaintToolKey, "select">;
type InpaintMaskShortcutTool = Extract<ResolvedInpaintToolShortcut, "brush" | "eraser" | "autoEraser">;
type InpaintResultShortcutTool = Extract<ResolvedInpaintToolShortcut, "brush" | "smartBrush" | "eraser" | "colorPicker">;

export type InpaintToolShortcutAction =
  | {
    layer: "inpaintMask";
    selectLayer: false;
    tool: InpaintMaskShortcutTool;
  }
  | {
    layer: "inpaintResult";
    selectLayer: boolean;
    tool: InpaintResultShortcutTool;
  };

export const INPAINT_TOOL_SHORTCUTS: Partial<Record<InpaintToolKey, string>> = {
  select: "T",
  brush: "B",
  smartBrush: "Alt+B",
  eraser: "E",
  autoEraser: "Alt+E",
  colorPicker: "I"
};
export const BLOCK_INLINE_EDIT_SHORTCUT = "E";

export function resolveInpaintToolShortcut(event: KeyboardEvent): InpaintToolKey | null {
  if (event.altKey) {
    if (event.ctrlKey || event.metaKey || event.shiftKey) {
      return null;
    }
    if (isKeyB(event)) {
      return "smartBrush";
    }
    return isKeyE(event) ? "autoEraser" : null;
  }

  if (event.ctrlKey || event.metaKey) {
    return null;
  }

  switch (event.code) {
    case "KeyB":
      return "brush";
    case "KeyE":
      return "eraser";
    case "KeyI":
      return "colorPicker";
  }

  switch (event.key.toLowerCase()) {
    case "b":
      return "brush";
    case "e":
      return "eraser";
    case "i":
      return "colorPicker";
    default:
      return null;
  }
}

export function resolveInpaintToolShortcutAction(options: {
  activeLayer: ActiveLayer;
  layerVisibility: LayerVisibility;
  selectedPageEditLocked: boolean;
  shortcut: InpaintToolKey | null;
}): InpaintToolShortcutAction | null {
  const {
    activeLayer,
    layerVisibility,
    selectedPageEditLocked,
    shortcut
  } = options;

  if (!shortcut || selectedPageEditLocked) {
    return null;
  }

  if (
    shortcut === "colorPicker" ||
    shortcut === "smartBrush" ||
    (shortcut === "brush" && activeLayer !== "inpaintMask")
  ) {
    return {
      layer: "inpaintResult",
      selectLayer: activeLayer !== "inpaintResult",
      tool: shortcut
    };
  }

  if (
    activeLayer === "inpaintMask" &&
    isInpaintMaskShortcutTool(shortcut) &&
    layerVisibility.inpaint &&
    layerVisibility.inpaintMask
  ) {
    return {
      layer: "inpaintMask",
      selectLayer: false,
      tool: shortcut
    };
  }

  if (
    activeLayer === "inpaintResult" &&
    isInpaintResultShortcutTool(shortcut) &&
    layerVisibility.inpaint &&
    layerVisibility.inpaintResult
  ) {
    return {
      layer: "inpaintResult",
      selectLayer: false,
      tool: shortcut
    };
  }

  return null;
}

function isInpaintMaskShortcutTool(shortcut: InpaintToolKey): shortcut is InpaintMaskShortcutTool {
  return shortcut === "brush" || shortcut === "eraser" || shortcut === "autoEraser";
}

function isInpaintResultShortcutTool(shortcut: InpaintToolKey): shortcut is InpaintResultShortcutTool {
  return shortcut === "brush" || shortcut === "smartBrush" || shortcut === "eraser" || shortcut === "colorPicker";
}

function isKeyB(event: KeyboardEvent): boolean {
  return event.code === "KeyB" || event.key.toLowerCase() === "b";
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
