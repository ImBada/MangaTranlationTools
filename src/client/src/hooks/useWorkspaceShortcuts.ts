import React from "react";
import type { ChapterSnapshot, ImageRect } from "../../../shared/types";
import type { InpaintTool } from "../components/InpaintLayerCanvas";
import {
  isBlockCopyShortcut,
  isBlockPasteShortcut,
  isDeleteShortcut,
  isFindReplaceShortcut,
  isPageProgressToggleShortcut,
  isPointerToolShortcut,
  isRangeToolShortcut,
  isZoomToolShortcut,
  resolveInpaintToolShortcut
} from "../lib/editorShortcuts";
import { isEditableTarget } from "../lib/editorUtils";
import { isPlatformUndoShortcut, resolveGlobalUndoAction, type GlobalUndoAction } from "../lib/globalUndo";
import type { ActiveLayer, LayerVisibility } from "../lib/layerState";
import { resolveAdjacentPageId, resolveKeyboardPageNavigation } from "../lib/pageNavigation";

type WorkspaceShortcutOptions = {
  activeLayer: ActiveLayer;
  clearSelectedInpaintSelection: () => Promise<boolean>;
  copySelectedBlockToClipboard: () => void | Promise<void>;
  currentChapterRef: React.MutableRefObject<ChapterSnapshot | null>;
  deleteSelectedBlock: () => void;
  globalUndoActions: GlobalUndoAction[];
  inpaintSelectionRect: ImageRect | null;
  layerVisibility: LayerVisibility;
  libraryWidgetOpen: boolean;
  modalOpen: boolean;
  oneHandMode: boolean;
  pasteSelectedBlockFontStyleFromClipboard: () => Promise<boolean>;
  pasteTranslationBlockFromClipboard: () => void | Promise<void>;
  pushStatus: (line: string) => void;
  rangeToolActive: boolean;
  openFindReplace: () => void;
  selectLayer: (layer: ActiveLayer) => void;
  selectPageForReading: (pageId: string | null) => void;
  selectPointerTool: () => void;
  selectRangeTool: () => void;
  selectSharedInpaintTool: (tool: InpaintTool) => void;
  selectZoomTool: () => void;
  selectedBlockIdRef: React.MutableRefObject<string | null>;
  selectedPageEditLocked: boolean;
  selectedPageIdRef: React.MutableRefObject<string | null>;
  setLibraryWidgetOpen: React.Dispatch<React.SetStateAction<boolean>>;
  setRangeToolActive: React.Dispatch<React.SetStateAction<boolean>>;
  setTemporaryPanActive: React.Dispatch<React.SetStateAction<boolean>>;
  setZoomToolActive: React.Dispatch<React.SetStateAction<boolean>>;
  temporaryPanHeldRef: React.MutableRefObject<boolean>;
  temporaryPanShortcutEnabled: boolean;
  toggleSelectedPageProgress: (pageId: string, options?: { announce?: boolean }) => void;
  undoShortcutPlatform: string;
  workspacePanelRef: React.RefObject<HTMLElement | null>;
  zoomToolActive: boolean;
};

export function useWorkspaceShortcuts({
  activeLayer,
  clearSelectedInpaintSelection,
  copySelectedBlockToClipboard,
  currentChapterRef,
  deleteSelectedBlock,
  globalUndoActions,
  inpaintSelectionRect,
  layerVisibility,
  libraryWidgetOpen,
  modalOpen,
  oneHandMode,
  pasteSelectedBlockFontStyleFromClipboard,
  pasteTranslationBlockFromClipboard,
  pushStatus,
  rangeToolActive,
  openFindReplace,
  selectLayer,
  selectPageForReading,
  selectPointerTool,
  selectRangeTool,
  selectSharedInpaintTool,
  selectZoomTool,
  selectedBlockIdRef,
  selectedPageEditLocked,
  selectedPageIdRef,
  setLibraryWidgetOpen,
  setRangeToolActive,
  setTemporaryPanActive,
  setZoomToolActive,
  temporaryPanHeldRef,
  temporaryPanShortcutEnabled,
  toggleSelectedPageProgress,
  undoShortcutPlatform,
  workspacePanelRef,
  zoomToolActive
}: WorkspaceShortcutOptions): void {
  React.useEffect(() => {
    if (!temporaryPanShortcutEnabled || modalOpen) {
      temporaryPanHeldRef.current = false;
      setTemporaryPanActive(false);
    }
  }, [modalOpen, setTemporaryPanActive, temporaryPanHeldRef, temporaryPanShortcutEnabled]);

  React.useEffect(() => {
    const shouldHandleSpacePan = (event: KeyboardEvent) =>
      temporaryPanShortcutEnabled &&
      !modalOpen &&
      !event.altKey &&
      !event.ctrlKey &&
      !event.metaKey &&
      !isEditableTarget(event.target) &&
      (event.code === "Space" || event.key === " ");

    const onKeyDown = (event: KeyboardEvent) => {
      if (!shouldHandleSpacePan(event)) {
        return;
      }
      event.preventDefault();
      temporaryPanHeldRef.current = true;
      setTemporaryPanActive(true);
    };

    const onKeyUp = (event: KeyboardEvent) => {
      if ((event.code !== "Space" && event.key !== " ") || !temporaryPanHeldRef.current) {
        return;
      }
      event.preventDefault();
      temporaryPanHeldRef.current = false;
      setTemporaryPanActive(false);
    };

    const resetTemporaryPan = () => {
      temporaryPanHeldRef.current = false;
      setTemporaryPanActive(false);
    };

    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    window.addEventListener("blur", resetTemporaryPan);
    document.addEventListener("visibilitychange", resetTemporaryPan);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      window.removeEventListener("blur", resetTemporaryPan);
      document.removeEventListener("visibilitychange", resetTemporaryPan);
    };
  }, [modalOpen, setTemporaryPanActive, temporaryPanHeldRef, temporaryPanShortcutEnabled]);

  React.useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      const selectTarget = getSelectElementTarget(event.target);
      const selectTargetShortcutOverride = !modalOpen && selectTarget !== null && isPlainSelectOverrideShortcut(event);
      const editableTarget = isEditableTarget(event.target) && !selectTargetShortcutOverride;
      if (selectTargetShortcutOverride && selectTarget) {
        selectTarget.blur();
      }
      if (event.key === "Escape" && modalOpen) {
        return;
      }
      if (event.key === "Escape" && libraryWidgetOpen) {
        setLibraryWidgetOpen(false);
        return;
      }
      if (event.key === "Escape" && zoomToolActive) {
        setZoomToolActive(false);
        return;
      }
      if (event.key === "Escape" && rangeToolActive) {
        setRangeToolActive(false);
        return;
      }

      if (!modalOpen && currentChapterRef.current && isFindReplaceShortcut(event)) {
        event.preventDefault();
        openFindReplace();
        return;
      }

      if (!modalOpen && !editableTarget && isPlatformUndoShortcut(event, undoShortcutPlatform)) {
        event.preventDefault();
        const undoAction = resolveGlobalUndoAction(globalUndoActions);
        if (undoAction) {
          undoAction.run();
        }
        return;
      }

      const blockCopyShortcut =
        !modalOpen && !editableTarget && Boolean(selectedBlockIdRef.current) && isBlockCopyShortcut(event);
      if (blockCopyShortcut) {
        event.preventDefault();
        void copySelectedBlockToClipboard();
        return;
      }

      const blockPasteShortcut =
        !modalOpen && !editableTarget && isBlockPasteShortcut(event);
      if (blockPasteShortcut && !selectedPageEditLocked) {
        event.preventDefault();
        if (selectedBlockIdRef.current) {
          void pasteSelectedBlockFontStyleFromClipboard().then((handled) => {
            if (!handled) {
              void pasteTranslationBlockFromClipboard();
            }
          });
        } else {
          void pasteTranslationBlockFromClipboard();
        }
        return;
      }

      const layerNumberShortcut =
        !modalOpen && !editableTarget && !event.altKey && !event.ctrlKey && !event.metaKey &&
        event.key >= "1" && event.key <= "5";
      if (layerNumberShortcut) {
        event.preventDefault();
        switch (event.key) {
          case "1": selectLayer("output"); break;
          case "2": selectLayer("overlay"); break;
          case "3": selectLayer(activeLayer === "inpaintResult" ? "inpaint" : "inpaintResult"); break;
          case "4": selectLayer(activeLayer === "inpaintMask" ? "inpaint" : "inpaintMask"); break;
          case "5": selectLayer("image"); break;
          default: return;
        }
        workspacePanelRef.current?.focus();
        return;
      }

      const zoomToolShortcut =
        !modalOpen &&
        !editableTarget &&
        !event.altKey &&
        !event.ctrlKey &&
        !event.metaKey &&
        isZoomToolShortcut(event);
      if (zoomToolShortcut) {
        event.preventDefault();
        selectZoomTool();
        workspacePanelRef.current?.focus();
        return;
      }

      const pointerToolShortcut =
        !modalOpen &&
        !editableTarget &&
        !event.altKey &&
        !event.ctrlKey &&
        !event.metaKey &&
        isPointerToolShortcut(event);
      if (pointerToolShortcut) {
        event.preventDefault();
        selectPointerTool();
        workspacePanelRef.current?.focus();
        return;
      }

      const rangeToolShortcut =
        !modalOpen &&
        !editableTarget &&
        !event.altKey &&
        !event.ctrlKey &&
        !event.metaKey &&
        isRangeToolShortcut(event);
      if (rangeToolShortcut && !selectedPageEditLocked) {
        event.preventDefault();
        selectRangeTool();
        workspacePanelRef.current?.focus();
        return;
      }

      const inpaintToolShortcut = !modalOpen && !editableTarget && !event.altKey && !event.ctrlKey && !event.metaKey
        ? resolveInpaintToolShortcut(event)
        : null;
      const inpaintToolShortcutEnabled =
        !selectedPageEditLocked &&
        ((activeLayer === "inpaintMask" && layerVisibility.inpaint && layerVisibility.inpaintMask) ||
          (activeLayer === "inpaintResult" && layerVisibility.inpaint && layerVisibility.inpaintResult));
      if (inpaintToolShortcut && inpaintToolShortcutEnabled) {
        event.preventDefault();
        selectSharedInpaintTool(inpaintToolShortcut);
        return;
      }

      const pageProgressToggleShortcut =
        !modalOpen &&
        !editableTarget &&
        isPageProgressToggleShortcut(event) &&
        Boolean(selectedPageIdRef.current);
      if (pageProgressToggleShortcut) {
        event.preventDefault();
        const selectedPageId = selectedPageIdRef.current;
        if (selectedPageId) {
          toggleSelectedPageProgress(selectedPageId, { announce: true });
        }
        return;
      }

      const selectionClearShortcut =
        isDeleteShortcut(event, oneHandMode) &&
        !modalOpen &&
        !editableTarget &&
        Boolean(inpaintSelectionRect) &&
        rangeToolActive &&
        (activeLayer === "inpaintMask" || activeLayer === "inpaintResult");
      if (selectionClearShortcut) {
        event.preventDefault();
        void clearSelectedInpaintSelection().then((handled) => {
          if (!handled) {
            pushStatus("선택 범위에서 지울 레이어 내용이 없습니다.");
          }
        });
        return;
      }

      const blockDeleteShortcut =
        isDeleteShortcut(event, oneHandMode) &&
        !modalOpen &&
        !editableTarget &&
        Boolean(selectedBlockIdRef.current);
      if (blockDeleteShortcut) {
        event.preventDefault();
        deleteSelectedBlock();
        return;
      }

      if (isDeleteShortcut(event, oneHandMode) && !modalOpen && !editableTarget) {
        if (inpaintSelectionRect) {
          return;
        }
      }

      const chapter = currentChapterRef.current;
      const pageIds = chapter?.pages.map((page) => page.id) ?? [];
      const activeElement = typeof document !== "undefined" ? document.activeElement : null;
      const navigation = resolveKeyboardPageNavigation({
        key: event.key,
        code: event.code,
        altKey: event.altKey,
        ctrlKey: event.ctrlKey,
        metaKey: event.metaKey,
        hasPages: pageIds.length > 0,
        modalOpen,
        editableTarget,
        centerPanelFocused: Boolean(workspacePanelRef.current && activeElement && workspacePanelRef.current.contains(activeElement))
      });

      if (!navigation) {
        return;
      }

      const nextPageId = resolveAdjacentPageId(pageIds, selectedPageIdRef.current, navigation.direction);
      if (!nextPageId) {
        return;
      }

      if (navigation.preventDefault) {
        event.preventDefault();
      }

      selectPageForReading(nextPageId);
    };

    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [
    activeLayer,
    clearSelectedInpaintSelection,
    copySelectedBlockToClipboard,
    currentChapterRef,
    deleteSelectedBlock,
    globalUndoActions,
    inpaintSelectionRect,
    layerVisibility,
    libraryWidgetOpen,
    modalOpen,
    oneHandMode,
    openFindReplace,
    pasteSelectedBlockFontStyleFromClipboard,
    pasteTranslationBlockFromClipboard,
    pushStatus,
    rangeToolActive,
    selectLayer,
    selectPageForReading,
    selectPointerTool,
    selectRangeTool,
    selectSharedInpaintTool,
    selectZoomTool,
    selectedBlockIdRef,
    selectedPageEditLocked,
    selectedPageIdRef,
    setLibraryWidgetOpen,
    setRangeToolActive,
    setZoomToolActive,
    toggleSelectedPageProgress,
    undoShortcutPlatform,
    workspacePanelRef,
    zoomToolActive
  ]);
}

function getSelectElementTarget(target: EventTarget | null): HTMLSelectElement | null {
  if (typeof HTMLSelectElement === "undefined" || !(target instanceof HTMLSelectElement)) {
    return null;
  }
  return target;
}

function isPlainSelectOverrideShortcut(event: KeyboardEvent): boolean {
  if (event.altKey || event.ctrlKey || event.metaKey) {
    return false;
  }

  return (
    event.key >= "1" && event.key <= "5" ||
    isZoomToolShortcut(event) ||
    isPointerToolShortcut(event) ||
    isRangeToolShortcut(event) ||
    isPageProgressToggleShortcut(event) ||
    Boolean(resolveInpaintToolShortcut(event)) ||
    event.code === "KeyD" ||
    event.code === "KeyF" ||
    event.key.toLowerCase() === "d" ||
    event.key.toLowerCase() === "f"
  );
}
