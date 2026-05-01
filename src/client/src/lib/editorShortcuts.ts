type InpaintToolKey = "select" | "brush" | "eraser";

export const INPAINT_TOOL_SHORTCUTS: Partial<Record<InpaintToolKey, string>> = {
  select: "T",
  brush: "B",
  eraser: "E"
};

export function resolveInpaintToolShortcut(event: KeyboardEvent): InpaintToolKey | null {
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

export function isZoomToolShortcut(event: KeyboardEvent): boolean {
  return event.code === "KeyZ" || event.key.toLowerCase() === "z";
}

export function isPointerToolShortcut(event: KeyboardEvent): boolean {
  return event.code === "KeyA" || event.key.toLowerCase() === "a";
}

export function isRangeToolShortcut(event: KeyboardEvent): boolean {
  return event.code === "KeyT" || event.key.toLowerCase() === "t";
}
