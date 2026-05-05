import React from "react";
import type {
  ChapterSnapshot,
  FontPreset,
  FontPresetBackupSnapshot,
  FontPresetBackupSummary,
  FontSizePreset,
  MangaPage,
  SystemFont,
  TranslationBlock
} from "../../../shared/types";
import {
  clampEditableBbox,
  clampRotationDeg,
  clampTextPaddingPx,
  enforceRenderDirection
} from "../../../shared/geometry";
import {
  type BlockFontPatch,
  type LinkableFontPresetKey
} from "../lib/fontPresets";
import {
  type GlobalUndoHistoryEntry,
  type GlobalUndoKind
} from "../lib/editorUndoHistory";
import type { ActiveLayer } from "../lib/layerState";
import { useTranslationBlockActions } from "./useTranslationBlockActions";
import { useFontPresetEditing, type FontControlValues } from "./useFontPresetEditing";
import { useTranslationUndoHistory } from "./useTranslationUndoHistory";

type UseTranslationEditingOptions = {
  consumeGlobalUndoEntry: (kind: GlobalUndoKind, pageId?: string) => void;
  currentChapter: ChapterSnapshot | null;
  currentChapterRef: React.RefObject<ChapterSnapshot | null>;
  editingFontPresetId: string | null;
  editingFontPresetIdRef: React.RefObject<string | null>;
  inpaintBusy: boolean;
  markDirty: (pageId?: string) => void;
  pushStatus: (line: string, tone?: "failed") => void;
  recordGlobalUndoEntry: (entry: GlobalUndoHistoryEntry) => void;
  selectLayer: (nextLayer: ActiveLayer) => void;
  selectedBlock: TranslationBlock | null;
  selectedBlockIdRef: React.RefObject<string | null>;
  selectedPage: MangaPage | null;
  selectedPageEditLocked: boolean;
  selectedPageIdRef: React.RefObject<string | null>;
  setCurrentChapter: React.Dispatch<React.SetStateAction<ChapterSnapshot | null>>;
  setEditingFontPresetId: React.Dispatch<React.SetStateAction<string | null>>;
  setSelectedBlockId: React.Dispatch<React.SetStateAction<string | null>>;
  setSelectedPageId: React.Dispatch<React.SetStateAction<string | null>>;
  showOverlayLayer: () => void;
  systemFonts: SystemFont[];
  undoVersion: number;
  updateCurrentChapter: (pageId: string | undefined, updater: (chapter: ChapterSnapshot) => ChapterSnapshot) => void;
};

type UseTranslationEditingState = {
  canUndoTranslation: boolean;
  activeFontSizePresetId: string | null;
  clearSelectedBlockFontPreset: () => void;
  clearTranslationUndoStack: () => void;
  copySelectedBlockToClipboard: () => Promise<void>;
  createEmptyBlock: () => void;
  createFontPresetFromSelectedBlock: () => void;
  createFontPresetListBackup: (name: string) => Promise<FontPresetBackupSnapshot | null>;
  createFontSizePresetFromCurrentFontSize: () => void;
  deleteFontPresetBackup: (backupId: string) => Promise<FontPresetBackupSummary[]>;
  deleteFontPreset: (presetId: string) => void;
  deleteFontSizePreset: (presetId: string) => void;
  deleteSelectedBlock: () => void;
  duplicateBlock: (block: TranslationBlock) => void;
  duplicateSelectedBlock: () => void;
  editingFontPreset: FontPreset | null;
  favoriteFontPresetIds: string[];
  favoriteFontPresets: FontPreset[];
  fontControlValues: FontControlValues;
  fontFamilyOptions: ReturnType<typeof useFontPresetEditing>["fontFamilyOptions"];
  fontPresetName: string;
  fontPresets: FontPreset[];
  fontSizePresets: FontSizePreset[];
  pasteTranslationBlockFromClipboard: () => Promise<void>;
  recordTranslationUndoSnapshot: (label: string) => boolean;
  listFontPresetBackups: () => Promise<FontPresetBackupSummary[]>;
  renderFontPresetLinkButton: (key: LinkableFontPresetKey, label: string) => React.ReactNode;
  renderFontPresetLinkGroupButton: (keys: LinkableFontPresetKey[], label: string) => React.ReactNode;
  renameFontPreset: (presetId: string, name: string) => void;
  restoreFontPresetListBackup: (backupId: string) => Promise<void>;
  selectFontSizePreset: (presetId: string | null) => void;
  selectFontPreset: (presetId: string) => void;
  selectedFontPreset: FontPreset | null;
  setFontPresetName: React.Dispatch<React.SetStateAction<string>>;
  toggleFavoriteFontPreset: (presetId: string) => void;
  undoTranslationEdit: () => void;
  updateSelectedBlock: (patch: Partial<TranslationBlock>, options?: { recordUndo?: boolean; undoLabel?: string }) => void;
  updateSelectedBlockFontSetting: (patch: BlockFontPatch) => void;
  updateSelectedPageBlockOpacity: (opacity: number) => void;
};

export function useTranslationEditing({
  consumeGlobalUndoEntry,
  currentChapter,
  currentChapterRef,
  editingFontPresetId,
  editingFontPresetIdRef,
  inpaintBusy,
  markDirty,
  pushStatus,
  recordGlobalUndoEntry,
  selectLayer,
  selectedBlock,
  selectedBlockIdRef,
  selectedPage,
  selectedPageEditLocked,
  selectedPageIdRef,
  setCurrentChapter,
  setEditingFontPresetId,
  setSelectedBlockId,
  setSelectedPageId,
  showOverlayLayer,
  systemFonts,
  undoVersion,
  updateCurrentChapter
}: UseTranslationEditingOptions): UseTranslationEditingState {
  const {
    canUndoTranslation,
    clearTranslationUndoStack,
    recordTranslationUndoSnapshot,
    undoTranslationEdit
  } = useTranslationUndoHistory({
    consumeGlobalUndoEntry,
    currentChapter,
    currentChapterRef,
    editingFontPresetIdRef,
    markDirty,
    recordGlobalUndoEntry,
    selectedBlockIdRef,
    selectedPageEditLocked,
    selectedPageIdRef,
    setCurrentChapter,
    setEditingFontPresetId,
    setSelectedBlockId,
    setSelectedPageId,
    undoVersion
  });

  const updateSelectedBlock = React.useCallback((patch: Partial<TranslationBlock>, options: { recordUndo?: boolean; undoLabel?: string } = {}) => {
    if (!selectedPage || !selectedBlock || selectedPageEditLocked) {
      return;
    }

    if (options.recordUndo !== false) {
      recordTranslationUndoSnapshot(options.undoLabel ?? "번역 블록 변경");
    }

    updateCurrentChapter(selectedPage.id, (current) => ({
      ...current,
      pages: current.pages.map((page) =>
        page.id !== selectedPage.id
          ? page
          : {
              ...page,
              updatedAt: new Date().toISOString(),
              blocks: page.blocks.map((block) => {
                if (block.id !== selectedBlock.id) {
                  return block;
                }

                const nextType = patch.type ?? block.type;
                return {
                  ...block,
                  ...patch,
                  type: nextType,
                  renderDirection: enforceRenderDirection(nextType, patch.renderDirection ?? block.renderDirection),
                  rotationDeg: patch.rotationDeg !== undefined ? clampRotationDeg(patch.rotationDeg) : block.rotationDeg,
                  textPaddingPx:
                    patch.textPaddingPx !== undefined
                      ? clampTextPaddingPx(patch.textPaddingPx)
                      : Object.prototype.hasOwnProperty.call(patch, "textPaddingPx")
                        ? undefined
                        : block.textPaddingPx,
                  bbox: patch.bbox ? clampEditableBbox(patch.bbox) : block.bbox,
                  renderBbox: patch.renderBbox ? clampEditableBbox(patch.renderBbox) : block.renderBbox
                };
              })
            }
      )
    }));
  }, [recordTranslationUndoSnapshot, selectedBlock, selectedPage, selectedPageEditLocked, updateCurrentChapter]);

  const {
    activeFontSizePresetId,
    clearSelectedBlockFontPreset,
    createFontPresetFromSelectedBlock,
    createFontPresetListBackup,
    createFontSizePresetFromCurrentFontSize,
    deleteFontPresetBackup,
    deleteFontPreset,
    deleteFontSizePreset,
    editingFontPreset,
    favoriteFontPresetIds,
    favoriteFontPresets,
    fontControlValues,
    fontFamilyOptions,
    fontPresetName,
    fontPresets,
    fontSizePresets,
    listFontPresetBackups,
    renderFontPresetLinkButton,
    renderFontPresetLinkGroupButton,
    renameFontPreset,
    restoreFontPresetListBackup,
    selectFontSizePreset,
    selectFontPreset,
    selectedFontPreset,
    setFontPresetName,
    toggleFavoriteFontPreset,
    updateSelectedBlockFontSetting
  } = useFontPresetEditing({
    currentChapter,
    editingFontPresetId,
    pushStatus,
    recordTranslationUndoSnapshot,
    selectedBlock,
    selectedPage,
    selectedPageEditLocked,
    setEditingFontPresetId,
    systemFonts,
    updateCurrentChapter,
    updateSelectedBlock
  });

  const {
    copySelectedBlockToClipboard,
    createEmptyBlock,
    deleteSelectedBlock,
    duplicateBlock,
    duplicateSelectedBlock,
    pasteTranslationBlockFromClipboard,
    updateSelectedPageBlockOpacity
  } = useTranslationBlockActions({
    editingFontPreset,
    inpaintBusy,
    pushStatus,
    recordTranslationUndoSnapshot,
    selectLayer,
    selectedBlock,
    selectedPage,
    selectedPageEditLocked,
    setSelectedBlockId,
    showOverlayLayer,
    updateCurrentChapter
  });

  return {
    activeFontSizePresetId,
    canUndoTranslation,
    clearSelectedBlockFontPreset,
    clearTranslationUndoStack,
    copySelectedBlockToClipboard,
    createEmptyBlock,
    createFontPresetFromSelectedBlock,
    createFontPresetListBackup,
    createFontSizePresetFromCurrentFontSize,
    deleteFontPresetBackup,
    deleteFontPreset,
    deleteFontSizePreset,
    deleteSelectedBlock,
    duplicateBlock,
    duplicateSelectedBlock,
    editingFontPreset,
    favoriteFontPresetIds,
    favoriteFontPresets,
    fontControlValues,
    fontFamilyOptions,
    fontPresetName,
    fontPresets,
    fontSizePresets,
    pasteTranslationBlockFromClipboard,
    recordTranslationUndoSnapshot,
    listFontPresetBackups,
    renderFontPresetLinkButton,
    renderFontPresetLinkGroupButton,
    renameFontPreset,
    restoreFontPresetListBackup,
    selectFontSizePreset,
    selectFontPreset,
    selectedFontPreset,
    setFontPresetName,
    toggleFavoriteFontPreset,
    undoTranslationEdit,
    updateSelectedBlock,
    updateSelectedBlockFontSetting,
    updateSelectedPageBlockOpacity
  };
}
