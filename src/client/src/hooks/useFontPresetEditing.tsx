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

function renderFontPresetLinkToggle(
  linked: boolean,
  label: string,
  disabled: boolean,
  onToggle: () => void
): React.ReactNode {
  return (
    <button
      type="button"
      className={`font-preset-link-toggle ${linked ? "linked" : "unlinked"}`}
      disabled={disabled}
      onClick={(event) => {
        event.preventDefault();
        event.stopPropagation();
        onToggle();
      }}
      aria-label={`${label} 프리셋 ${linked ? "연결 해제" : "연결"}`}
      title={`${label} 프리셋 ${linked ? "연결 해제" : "연결"}`}
    >
      <FontPresetLinkIcon linked={linked} />
    </button>
  );
}

function isFontPresetNameTaken(fontPresets: FontPreset[], name: string, excludePresetId?: string): boolean {
  const normalizedName = name.trim();
  return fontPresets.some((preset) => preset.id !== excludePresetId && preset.name.trim() === normalizedName);
}

type UseFontPresetEditingOptions = {
  currentChapter: ChapterSnapshot | null;
  editingFontPresetId: string | null;
  pushStatus: (line: string, tone?: "failed") => void;
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
  renderFontPresetLinkGroupButton: (keys: LinkableFontPresetKey[], label: string) => React.ReactNode;
  renameFontPreset: (presetId: string, name: string) => void;
  selectFontPreset: (presetId: string) => void;
  selectedFontPreset: FontPreset | null;
  setFontPresetName: React.Dispatch<React.SetStateAction<string>>;
  updateSelectedBlockFontSetting: (patch: BlockFontPatch) => void;
};

export function useFontPresetEditing({
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
        screentoneFillAntialias: isBlockFontPresetValueLinked(selectedBlock, "screentoneFillAntialias"),
        fontWeight: isBlockFontPresetValueLinked(selectedBlock, "fontWeight"),
        fontStyle: isBlockFontPresetValueLinked(selectedBlock, "fontStyle"),
        textDecoration: isBlockFontPresetValueLinked(selectedBlock, "textDecoration")
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

  const toggleSelectedBlockFontPresetLinkGroup = React.useCallback((keys: LinkableFontPresetKey[], linked: boolean) => {
    if (!selectedBlock || !selectedFontPreset) {
      return;
    }

    const patch: Partial<TranslationBlock> = {};
    for (const key of keys) {
      Object.assign(patch, buildFontPresetLinkPatch(key, linked));
      if (linked) {
        Object.assign(patch, { [key]: selectedFontPreset[key] });
      }
    }
    updateSelectedBlock(patch);
  }, [selectedBlock, selectedFontPreset, updateSelectedBlock]);

  const renderFontPresetLinkButton = React.useCallback((key: LinkableFontPresetKey, label: string) => {
    if (!selectedBlock?.fontPresetId || !selectedFontPreset || !selectedBlockFontPresetLinks) {
      return null;
    }

    const linked = selectedBlockFontPresetLinks[key];
    return renderFontPresetLinkToggle(linked, label, selectedPageEditLocked, () => toggleSelectedBlockFontPresetLink(key));
  }, [selectedBlock?.fontPresetId, selectedBlockFontPresetLinks, selectedFontPreset, selectedPageEditLocked, toggleSelectedBlockFontPresetLink]);

  const renderFontPresetLinkGroupButton = React.useCallback((keys: LinkableFontPresetKey[], label: string) => {
    if (!selectedBlock?.fontPresetId || !selectedFontPreset || !selectedBlockFontPresetLinks) {
      return null;
    }

    const linked = keys.every((key) => selectedBlockFontPresetLinks[key]);
    return renderFontPresetLinkToggle(linked, label, selectedPageEditLocked, () => toggleSelectedBlockFontPresetLinkGroup(keys, !linked));
  }, [selectedBlock?.fontPresetId, selectedBlockFontPresetLinks, selectedFontPreset, selectedPageEditLocked, toggleSelectedBlockFontPresetLinkGroup]);

  const createFontPresetFromSelectedBlock = React.useCallback(() => {
    if (!currentChapter || selectedPageEditLocked) {
      return;
    }

    const presetName = fontPresetName.trim() || `프리셋 ${(currentChapter?.fontPresets?.length ?? 0) + 1}`;
    if (isFontPresetNameTaken(fontPresets, presetName)) {
      pushStatus("이미 있는 프리셋 이름입니다.", "failed");
      return;
    }
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
                      screentoneFillAntialiasLinkedToPreset: true,
                      fontWeightLinkedToPreset: true,
                      fontStyleLinkedToPreset: true,
                      textDecorationLinkedToPreset: true
                    }
                  : block
              )
            }
          : page
      )
    }));
    setEditingFontPresetId(preset.id);
    setFontPresetName("");
  }, [currentChapter, fontPresetName, fontPresets, pushStatus, recordTranslationUndoSnapshot, selectedBlock, selectedPage, selectedPageEditLocked, setEditingFontPresetId, updateCurrentChapter]);

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
      screentoneFillAntialiasLinkedToPreset: true,
      fontWeightLinkedToPreset: true,
      fontStyleLinkedToPreset: true,
      textDecorationLinkedToPreset: true
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
      screentoneFillAntialiasLinkedToPreset: undefined,
      fontWeightLinkedToPreset: undefined,
      fontStyleLinkedToPreset: undefined,
      textDecorationLinkedToPreset: undefined
    });
  }, [selectedBlock, updateSelectedBlock]);

  const renameFontPreset = React.useCallback((presetId: string, name: string) => {
    const nextName = name.trim();
    if (!currentChapter || selectedPageEditLocked || !nextName) {
      if (currentChapter && !selectedPageEditLocked) {
        pushStatus("프리셋 이름을 입력하세요.", "failed");
      }
      return;
    }
    const currentPreset = fontPresets.find((preset) => preset.id === presetId);
    if (!currentPreset || currentPreset.name === nextName) {
      return;
    }
    if (isFontPresetNameTaken(fontPresets, nextName, presetId)) {
      pushStatus("이미 있는 프리셋 이름입니다.", "failed");
      return;
    }

    recordTranslationUndoSnapshot("폰트 프리셋 이름 변경");
    updateCurrentChapter(undefined, (current) => ({
      ...current,
      fontPresets: (current.fontPresets ?? []).map((preset) => (preset.id === presetId ? { ...preset, name: nextName } : preset))
    }));
  }, [currentChapter, fontPresets, pushStatus, recordTranslationUndoSnapshot, selectedPageEditLocked, updateCurrentChapter]);

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
    renderFontPresetLinkGroupButton,
    renameFontPreset,
    selectFontPreset,
    selectedFontPreset,
    setFontPresetName,
    updateSelectedBlockFontSetting
  };
}
