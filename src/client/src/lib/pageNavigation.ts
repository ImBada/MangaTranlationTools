export type PageNavigationDirection = "previous" | "next";

type KeyboardPageNavigationOptions = {
  key: string;
  code: string;
  altKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  hasPages: boolean;
  modalOpen: boolean;
  editableTarget: boolean;
  centerPanelFocused: boolean;
};

type KeyboardPageNavigation = {
  direction: PageNavigationDirection;
  preventDefault: boolean;
};

export function resolveAdjacentPageId(
  pageIds: string[],
  selectedPageId: string | null,
  direction: PageNavigationDirection
): string | null {
  if (!pageIds.length) {
    return null;
  }

  const currentIndex = Math.max(0, pageIds.indexOf(selectedPageId ?? ""));
  const targetIndex = direction === "previous" ? currentIndex - 1 : currentIndex + 1;

  if (targetIndex < 0 || targetIndex >= pageIds.length) {
    return null;
  }

  return pageIds[targetIndex] ?? null;
}

export function resolveKeyboardPageNavigation({
  key,
  code,
  altKey,
  ctrlKey,
  metaKey,
  hasPages,
  modalOpen,
  editableTarget,
  centerPanelFocused
}: KeyboardPageNavigationOptions): KeyboardPageNavigation | null {
  if (!hasPages || modalOpen || editableTarget) {
    return null;
  }

  switch (key) {
    case "ArrowLeft":
      return { direction: "previous", preventDefault: false };
    case "ArrowRight":
      return { direction: "next", preventDefault: false };
    case "ArrowUp":
      return centerPanelFocused ? { direction: "previous", preventDefault: true } : null;
    case "ArrowDown":
      return centerPanelFocused ? { direction: "next", preventDefault: true } : null;
    default:
      break;
  }

  if (altKey || ctrlKey || metaKey) {
    return null;
  }

  switch (code || key.toLowerCase()) {
    case "KeyD":
    case "d":
      return { direction: "previous", preventDefault: true };
    case "KeyF":
    case "f":
      return { direction: "next", preventDefault: true };
    default:
      return null;
  }
}
