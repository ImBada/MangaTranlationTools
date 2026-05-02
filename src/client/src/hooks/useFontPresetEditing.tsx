import React from "react";
import type { ChapterSnapshot, FontPreset, MangaPage, SystemFont, TranslationBlock } from "../../../shared/types";
import { buildFontFamilyOptions } from "../components/font/FontFamilyPicker";
import { FontPresetLinkIcon } from "../components/font/FontPresetLinkIcon";
import {
  applyFontPresetPatchToBlock,
  buildFontPresetLinkPatch,
  clearFontPresetLinkFields,
  createFontPreset,
  DEFAULT_FONT_PRESET,
  isBlockFontPresetValueLinked,
  type BlockFontPatch,
  type FontPresetPatch,
  type LinkableFontPresetKey
} from "../lib/fontPresets";

export type FontControlValues = TranslationBlock | FontPreset | null;

type UseFontPresetEditingOptions = {
  currentChapter: ChapterSnapshot | null;
  editingFontPresetId: string | null;
  recordTranslationUndoSnapshot: (label: string) => boolean;
  selectedBlock: TranslationBlock | null;
  selectedPage: MangaPage | null;
  selectedPageEditLocked: boolean;
  setEditingFontPresetId: React.Dispatch<React.SetStateAction<string | null>>;
  systemFonts: SystemFont[];
  updateCurrentChapter: (pageId: string | undefined, updater: (chapter: ChapterSnapshot) => ChapterSnapshot) => void;
  updateSelectedBlock: (patch: Partial<TranslationBlock>, options?: { recordUndo?: boolean; undoLabel?: string }) => void;
};

type UseFontPresetEditingState = {
  clearSelectedBlockFontPreset: () => void;
  createFontPresetFromSelectedBlock: () => void;
  deleteFontPreset: (presetId: string) => void;
  editingFontPreset: FontPreset | null;
  fontControlValues: FontControlValues;
  fontFamilyOptions: ReturnType<typeof buildFontFamilyOptions>;
  fontPresetName: string;
  fontPresets: FontPreset[];
  renderFontPresetLinkButton: (key: LinkableFontPresetKey, label: string) => React.ReactNode;
  selectFontPreset: (presetId: string) => void;
  selectedFontPreset: FontPreset | null;
  setFontPresetName: React.Dispatch<React.SetStateAction<string>>;
  updateSelectedBlockFontSetting: (patch: BlockFontPatch) => void;
};

export function useFontPresetEditing({
  currentChapter,
  editingFontPresetId,
  recordTranslationUndoSnapshot,
  selectedBlock,
  selectedPage,
  selectedPageEditLocked,
  setEditingFontPresetId,
  systemFonts,
  updateCurrentChapter,
  updateSelectedBlock
}: UseFontPresetEditingOptions): UseFontPresetEditingState {
  const [fontPresetName, setFontPresetName] = React.useState("");
  const fontPresets = currentChapter?.fontPresets ?? [];
  const selectedFontPreset = selectedBlock?.fontPresetId
    ? fontPresets.find((preset) => preset.id === selectedBlock.fontPresetId) ?? null
    : null;
  const editingFontPreset = editingFontPresetId
    ? fontPresets.find((preset) => preset.id === editingFontPresetId) ?? null
    : null;
  const selectedBlockFontControls = selectedBlock && selectedFontPreset
    ? applyFontPresetPatchToBlock(selectedBlock, selectedFontPreset)
    : selectedBlock;
  const fontControlValues = selectedBlockFontControls ?? editingFontPreset;
  const selectedBlockFontPresetLinks = selectedBlock
    ? {
        fontSizePx: isBlockFontPresetValueLinked(selectedBlock, "fontSizePx"),
        lineHeight: isBlockFontPresetValueLinked(selectedBlock, "lineHeight"),
        outlineColor: isBlockFontPresetValueLinked(selectedBlock, "outlineColor"),
        outlineWidthPx: isBlockFontPresetValueLinked(selectedBlock, "outlineWidthPx"),
        secondaryOutlineColor: isBlockFontPresetValueLinked(selectedBlock, "secondaryOutlineColor"),
        secondaryOutlineWidthPx: isBlockFontPresetValueLinked(selectedBlock, "secondaryOutlineWidthPx"),
        autoFitText: isBlockFontPresetValueLinked(selectedBlock, "autoFitText"),
        textColor: isBlockFontPresetValueLinked(selectedBlock, "textColor"),
        screentoneFillEnabled: isBlockFontPresetValueLinked(selectedBlock, "screentoneFillEnabled"),
        screentoneFillIntensity: isBlockFontPresetValueLinked(selectedBlock, "screentoneFillIntensity"),
        screentoneFillDensity: isBlockFontPresetValueLinked(selectedBlock, "screentoneFillDensity"),
        screentoneFillAntialias: isBlockFontPresetValueLinked(selectedBlock, "screentoneFillAntialias")
      }
    : null;
  const fontFamilyOptions = React.useMemo(
    () => buildFontFamilyOptions(systemFonts, fontControlValues?.fontFamily),
    [fontControlValues?.fontFamily, systemFonts]
  );

  const updateAssignedFontPreset = React.useCallback((presetId: string, patch: FontPresetPatch, options: { recordUndo?: boolean; undoLabel?: string } = {}) => {
    if (!currentChapter || selectedPageEditLocked) {
      return;
    }

    if (options.recordUndo !== false) {
      recordTranslationUndoSnapshot(options.undoLabel ?? "폰트 설정 변경");
    }

    updateCurrentChapter(undefined, (current) => ({
      ...current,
      fontPresets: (current.fontPresets ?? []).map((preset) => (preset.id === presetId ? { ...preset, ...patch } : preset)),
      pages: current.pages.map((page) => ({
        ...page,
        updatedAt: page.blocks.some((block) => block.fontPresetId === presetId) ? new Date().toISOString() : page.updatedAt,
        blocks: page.blocks.map((block) => (block.fontPresetId === presetId ? applyFontPresetPatchToBlock(block, patch) : block))
      }))
    }));
  }, [currentChapter, recordTranslationUndoSnapshot, selectedPageEditLocked, updateCurrentChapter]);

  const updateSelectedBlockFontSetting = React.useCallback((patch: BlockFontPatch) => {
    if ("textAlign" in patch) {
      if (patch.textAlign) {
        updateSelectedBlock({ textAlign: patch.textAlign });
      }
      return;
    }
    if (selectedBlock?.fontPresetId) {
      const presetPatch: FontPresetPatch = {};
      const blockPatch: Partial<TranslationBlock> = {};
      for (const key of Object.keys(patch) as (keyof FontPresetPatch)[]) {
        const value = patch[key];
        if (value === undefined) {
          continue;
        }
        if (key === "fontFamily" || isBlockFontPresetValueLinked(selectedBlock, key)) {
          Object.assign(presetPatch, { [key]: value });
        } else {
          Object.assign(blockPatch, { [key]: value });
        }
      }
      if (Object.keys(blockPatch).length > 0) {
        recordTranslationUndoSnapshot("폰트 설정 변경");
        updateSelectedBlock(blockPatch, { recordUndo: false });
        if (Object.keys(presetPatch).length > 0) {
          updateAssignedFontPreset(selectedBlock.fontPresetId, presetPatch, { recordUndo: false });
        }
        return;
      }
      if (Object.keys(presetPatch).length > 0) {
        updateAssignedFontPreset(selectedBlock.fontPresetId, presetPatch);
      }
      return;
    }
    if (!selectedBlock && editingFontPreset) {
      updateAssignedFontPreset(editingFontPreset.id, patch);
      return;
    }
    updateSelectedBlock(patch);
  }, [editingFontPreset, recordTranslationUndoSnapshot, selectedBlock, updateAssignedFontPreset, updateSelectedBlock]);

  const toggleSelectedBlockFontPresetLink = React.useCallback((key: LinkableFontPresetKey) => {
    if (!selectedBlock || !selectedFontPreset) {
      return;
    }

    const nextLinked = !isBlockFontPresetValueLinked(selectedBlock, key);
    updateSelectedBlock({
      ...buildFontPresetLinkPatch(key, nextLinked),
      ...(nextLinked ? { [key]: selectedFontPreset[key] } : {})
    });
  }, [selectedBlock, selectedFontPreset, updateSelectedBlock]);

  const renderFontPresetLinkButton = React.useCallback((key: LinkableFontPresetKey, label: string) => {
    if (!selectedBlock?.fontPresetId || !selectedFontPreset || !selectedBlockFontPresetLinks) {
      return null;
    }

    const linked = selectedBlockFontPresetLinks[key];
    return (
      <button
        type="button"
        className={`font-preset-link-toggle ${linked ? "linked" : "unlinked"}`}
        disabled={selectedPageEditLocked}
        onClick={(event) => {
          event.preventDefault();
          event.stopPropagation();
          toggleSelectedBlockFontPresetLink(key);
        }}
        aria-label={`${label} 프리셋 ${linked ? "연결 해제" : "연결"}`}
        title={`${label} 프리셋 ${linked ? "연결 해제" : "연결"}`}
      >
        <FontPresetLinkIcon linked={linked} />
      </button>
    );
  }, [selectedBlock?.fontPresetId, selectedBlockFontPresetLinks, selectedFontPreset, selectedPageEditLocked, toggleSelectedBlockFontPresetLink]);

  const createFontPresetFromSelectedBlock = React.useCallback(() => {
    if (!currentChapter || selectedPageEditLocked) {
      return;
    }

    const presetName = fontPresetName.trim() || `프리셋 ${(currentChapter?.fontPresets?.length ?? 0) + 1}`;
    const preset = createFontPreset(presetName, selectedBlock ?? DEFAULT_FONT_PRESET);
    recordTranslationUndoSnapshot("폰트 프리셋 생성");
    updateCurrentChapter(selectedPage?.id, (current) => ({
      ...current,
      fontPresets: [...(current.fontPresets ?? []), preset],
      pages: current.pages.map((page) =>
        selectedPage && selectedBlock && page.id === selectedPage.id
          ? {
              ...page,
              updatedAt: new Date().toISOString(),
              blocks: page.blocks.map((block) =>
                block.id === selectedBlock.id
                  ? {
                      ...applyFontPresetPatchToBlock(block, preset, { forceLinkedValues: true }),
                      fontPresetId: preset.id,
                      fontSizeLinkedToPreset: true,
                      lineHeightLinkedToPreset: true,
                      outlineColorLinkedToPreset: true,
                      outlineWidthLinkedToPreset: true,
                      secondaryOutlineColorLinkedToPreset: true,
                      secondaryOutlineWidthLinkedToPreset: true,
                      autoFitTextLinkedToPreset: true,
                      textColorLinkedToPreset: true,
                      screentoneFillEnabledLinkedToPreset: true,
                      screentoneFillIntensityLinkedToPreset: true,
                      screentoneFillDensityLinkedToPreset: true,
                      screentoneFillAntialiasLinkedToPreset: true
                    }
                  : block
              )
            }
          : page
      )
    }));
    setEditingFontPresetId(preset.id);
    setFontPresetName("");
  }, [currentChapter, fontPresetName, recordTranslationUndoSnapshot, selectedBlock, selectedPage, selectedPageEditLocked, setEditingFontPresetId, updateCurrentChapter]);

  const selectFontPreset = React.useCallback((presetId: string) => {
    if (selectedPageEditLocked) {
      return;
    }
    const preset = fontPresets.find((candidate) => candidate.id === presetId);
    if (!preset) {
      return;
    }
    setEditingFontPresetId(presetId);
    if (!selectedPage || !selectedBlock) {
      return;
    }
    updateSelectedBlock({
      ...applyFontPresetPatchToBlock(selectedBlock, preset, { forceLinkedValues: true }),
      fontPresetId: preset.id,
      fontSizeLinkedToPreset: true,
      lineHeightLinkedToPreset: true,
      outlineColorLinkedToPreset: true,
      outlineWidthLinkedToPreset: true,
      secondaryOutlineColorLinkedToPreset: true,
      secondaryOutlineWidthLinkedToPreset: true,
      autoFitTextLinkedToPreset: true,
      textColorLinkedToPreset: true,
      screentoneFillEnabledLinkedToPreset: true,
      screentoneFillIntensityLinkedToPreset: true,
      screentoneFillDensityLinkedToPreset: true,
      screentoneFillAntialiasLinkedToPreset: true
    });
  }, [fontPresets, selectedBlock, selectedPage, selectedPageEditLocked, setEditingFontPresetId, updateSelectedBlock]);

  const clearSelectedBlockFontPreset = React.useCallback(() => {
    if (!selectedBlock) {
      return;
    }
    updateSelectedBlock({
      fontPresetId: undefined,
      fontSizeLinkedToPreset: undefined,
      lineHeightLinkedToPreset: undefined,
      outlineColorLinkedToPreset: undefined,
      outlineWidthLinkedToPreset: undefined,
      secondaryOutlineColorLinkedToPreset: undefined,
      secondaryOutlineWidthLinkedToPreset: undefined,
      autoFitTextLinkedToPreset: undefined,
      textColorLinkedToPreset: undefined,
      screentoneFillEnabledLinkedToPreset: undefined,
      screentoneFillIntensityLinkedToPreset: undefined,
      screentoneFillDensityLinkedToPreset: undefined,
      screentoneFillAntialiasLinkedToPreset: undefined
    });
  }, [selectedBlock, updateSelectedBlock]);

  const deleteFontPreset = React.useCallback((presetId: string) => {
    if (selectedPageEditLocked) {
      return;
    }
    recordTranslationUndoSnapshot("폰트 프리셋 삭제");
    updateCurrentChapter(undefined, (current) => ({
      ...current,
      fontPresets: (current.fontPresets ?? []).filter((preset) => preset.id !== presetId),
      pages: current.pages.map((page) => ({
        ...page,
        blocks: page.blocks.map((block) => {
          if (block.fontPresetId !== presetId) {
            return block;
          }
          const { fontPresetId: _fontPresetId, ...rest } = block;
          return clearFontPresetLinkFields(rest);
        })
      }))
    }));
    setEditingFontPresetId((current) => (current === presetId ? null : current));
  }, [recordTranslationUndoSnapshot, selectedPageEditLocked, setEditingFontPresetId, updateCurrentChapter]);

  return {
    clearSelectedBlockFontPreset,
    createFontPresetFromSelectedBlock,
    deleteFontPreset,
    editingFontPreset,
    fontControlValues,
    fontFamilyOptions,
    fontPresetName,
    fontPresets,
    renderFontPresetLinkButton,
    selectFontPreset,
    selectedFontPreset,
    setFontPresetName,
    updateSelectedBlockFontSetting
  };
}
