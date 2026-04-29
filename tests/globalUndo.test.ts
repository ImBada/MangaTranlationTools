import { describe, expect, it, vi } from "vitest";
import { isMacLikePlatform, isPlatformUndoShortcut, resolveGlobalUndoAction, type GlobalUndoAction } from "../src/client/src/lib/globalUndo";

function shortcutEvent(patch: Partial<Parameters<typeof isPlatformUndoShortcut>[0]>): Parameters<typeof isPlatformUndoShortcut>[0] {
  return {
    key: "z",
    code: "KeyZ",
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    ...patch
  };
}

describe("global undo helpers", () => {
  it("recognizes Mac-like platforms", () => {
    expect(isMacLikePlatform("MacIntel")).toBe(true);
    expect(isMacLikePlatform("iPad")).toBe(true);
    expect(isMacLikePlatform("Win32")).toBe(false);
  });

  it("uses Command+Z on macOS", () => {
    expect(isPlatformUndoShortcut(shortcutEvent({ metaKey: true }), "MacIntel")).toBe(true);
    expect(isPlatformUndoShortcut(shortcutEvent({ ctrlKey: true }), "MacIntel")).toBe(false);
  });

  it("uses Control+Z outside macOS", () => {
    expect(isPlatformUndoShortcut(shortcutEvent({ ctrlKey: true }), "Win32")).toBe(true);
    expect(isPlatformUndoShortcut(shortcutEvent({ metaKey: true }), "Win32")).toBe(false);
  });

  it("does not treat modified redo-like shortcuts as undo", () => {
    expect(isPlatformUndoShortcut(shortcutEvent({ metaKey: true, shiftKey: true }), "MacIntel")).toBe(false);
    expect(isPlatformUndoShortcut(shortcutEvent({ ctrlKey: true, altKey: true }), "Win32")).toBe(false);
  });

  it("selects the first available undo action", () => {
    const actions: GlobalUndoAction[] = [
      { id: "first", label: "First", canUndo: false, run: vi.fn() },
      { id: "second", label: "Second", canUndo: true, run: vi.fn() }
    ];

    expect(resolveGlobalUndoAction(actions)?.id).toBe("second");
  });
});
