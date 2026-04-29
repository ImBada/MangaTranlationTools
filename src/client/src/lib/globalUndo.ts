export type GlobalUndoAction = {
  id: string;
  label: string;
  canUndo: boolean;
  run: () => void;
};

type UndoShortcutEvent = {
  key: string;
  code?: string;
  altKey: boolean;
  ctrlKey: boolean;
  metaKey: boolean;
  shiftKey: boolean;
};

export function resolveGlobalUndoAction(actions: GlobalUndoAction[]): GlobalUndoAction | null {
  return actions.find((action) => action.canUndo) ?? null;
}

export function isMacLikePlatform(platform: string): boolean {
  return /^(Mac|iPhone|iPad|iPod)/i.test(platform);
}

export function isPlatformUndoShortcut(event: UndoShortcutEvent, platform: string): boolean {
  const undoKey = event.key.toLowerCase() === "z" || event.code === "KeyZ";
  if (!undoKey || event.altKey || event.shiftKey) {
    return false;
  }

  if (isMacLikePlatform(platform)) {
    return event.metaKey && !event.ctrlKey;
  }

  return event.ctrlKey && !event.metaKey;
}
