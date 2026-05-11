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
import { buildFontFamilyOptions } from "../components/font/FontFamilyPicker";
import { FontPresetLinkIcon } from "../components/font/FontPresetLinkIcon";
import {
  applyFontPresetPatchToBlock,
  buildNextFontSizePresetName,
  buildFontPresetLinkPatch,
  clearFontPresetLinkFields,
  createFontPreset,
  createFontSizePreset,
  DEFAULT_FONT_PRESET,
  isBlockFontPresetValueLinked,
  normalizeCharacterFontOverrides,
  resolveFontPreset,
  type BlockFontPatch,
  type FontPresetPatch,
  type LinkableFontPresetKey
} from "../lib/fontPresets";
import type { UpdateCurrentChapter } from "./useChapterSession";

export type FontControlValues = TranslationBlock | FontPreset | null;
const MAX_FAVORITE_FONT_PRESETS = 5;

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

function resolveOptionalFontPreset(preset: FontPreset | null | undefined, fontSizePresets: FontSizePreset[]): FontPreset | null {
  return preset ? resolveFontPreset(preset, fontSizePresets) : null;
}

function resolveFavoriteFontPresetIds(favoriteFontPresetIds: readonly string[] | undefined, fontPresets: FontPreset[]): string[] {
  const validPresetIds = new Set(fontPresets.map((preset) => preset.id));
  const resolvedIds: string[] = [];

  for (const presetId of favoriteFontPresetIds ?? []) {
    if (!validPresetIds.has(presetId) || resolvedIds.includes(presetId)) {
      continue;
    }
    resolvedIds.push(presetId);
    if (resolvedIds.length === MAX_FAVORITE_FONT_PRESETS) {
      break;
    }
  }

  return resolvedIds;
}

function applyFontPresetListBackupToChapter(chapter: ChapterSnapshot, backup: FontPresetBackupSnapshot): ChapterSnapshot {
  const nextFontPresets = backup.fontPresets.map(cloneFontPreset);
  const nextFontSizePresets = backup.fontSizePresets.map((preset) => ({ ...preset }));
  const presetById = new Map(nextFontPresets.map((preset) => [preset.id, resolveFontPreset(preset, nextFontSizePresets)]));
  const now = new Date().toISOString();

  return {
    ...chapter,
    favoriteFontPresetIds: resolveFavoriteFontPresetIds(chapter.favoriteFontPresetIds, nextFontPresets),
    fontPresets: nextFontPresets,
    fontSizePresets: nextFontSizePresets,
    updatedAt: now,
    pages: chapter.pages.map((page) => {
      let pageChanged = false;
      const blocks = page.blocks.map((block) => {
        if (!block.fontPresetId) {
          return block;
        }
        const preset = presetById.get(block.fontPresetId);
        pageChanged = true;
        if (!preset) {
          const { fontPresetId: _fontPresetId, ...rest } = block;
          return clearFontPresetLinkFields(rest);
        }
        return applyFontPresetPatchToBlock(block, preset);
      });

      return pageChanged
        ? {
            ...page,
            blocks,
            updatedAt: now
          }
        : page;
    })
  };
}

function cloneFontPreset(preset: FontPreset): FontPreset {
  return {
    ...preset,
    characterFontOverrides: preset.characterFontOverrides?.map((override) => ({ ...override }))
  };
}

type AssignedFontPresetPatch = FontPresetPatch & Partial<Pick<FontPreset, "fontSizePresetId">>;

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
  updateCurrentChapter: UpdateCurrentChapter;
  updateSelectedBlock: (patch: Partial<TranslationBlock>, options?: { recordUndo?: boolean; undoLabel?: string }) => void;
};

type UseFontPresetEditingState = {
  clearSelectedBlockFontPreset: () => void;
  createFontPresetFromSelectedBlock: () => void;
  createFontPresetListBackup: (name: string) => Promise<FontPresetBackupSnapshot | null>;
  createFontSizePresetFromCurrentFontSize: () => void;
  deleteFontPresetBackup: (backupId: string) => Promise<FontPresetBackupSummary[]>;
  deleteFontPreset: (presetId: string) => void;
  deleteFontSizePreset: (presetId: string) => void;
  editingFontPreset: FontPreset | null;
  favoriteFontPresetIds: string[];
  favoriteFontPresets: FontPreset[];
  fontControlValues: FontControlValues;
  fontFamilyOptions: ReturnType<typeof buildFontFamilyOptions>;
  fontPresetName: string;
  fontPresets: FontPreset[];
  fontSizePresets: FontSizePreset[];
  renderFontPresetLinkButton: (key: LinkableFontPresetKey, label: string) => React.ReactNode;
  renderFontPresetLinkGroupButton: (keys: LinkableFontPresetKey[], label: string) => React.ReactNode;
  activeFontSizePresetId: string | null;
  listFontPresetBackups: () => Promise<FontPresetBackupSummary[]>;
  renameFontPreset: (presetId: string, name: string) => void;
  restoreFontPresetListBackup: (backupId: string) => Promise<void>;
  selectFontSizePreset: (presetId: string | null) => void;
  selectFontPreset: (presetId: string) => void;
  selectedFontPreset: FontPreset | null;
  setFontPresetName: React.Dispatch<React.SetStateAction<string>>;
  toggleFavoriteFontPreset: (presetId: string) => void;
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
  const fontSizePresets = currentChapter?.fontSizePresets ?? [];
  const favoriteFontPresetIds = React.useMemo(
    () => resolveFavoriteFontPresetIds(currentChapter?.favoriteFontPresetIds, fontPresets),
    [currentChapter?.favoriteFontPresetIds, fontPresets]
  );
  const favoriteFontPresets = React.useMemo(() => {
    const presetById = new Map(fontPresets.map((preset) => [preset.id, preset]));
    return favoriteFontPresetIds
      .map((presetId) => presetById.get(presetId))
      .filter((preset): preset is FontPreset => Boolean(preset));
  }, [favoriteFontPresetIds, fontPresets]);
  const selectedFontPreset = selectedBlock?.fontPresetId
    ? resolveOptionalFontPreset(fontPresets.find((preset) => preset.id === selectedBlock.fontPresetId), fontSizePresets)
    : null;
  const rawEditingFontPreset = editingFontPresetId
    ? fontPresets.find((preset) => preset.id === editingFontPresetId) ?? null
    : null;
  const editingFontPreset = resolveOptionalFontPreset(rawEditingFontPreset, fontSizePresets);
  const selectedBlockFontControls = selectedBlock && selectedFontPreset
    ? applyFontPresetPatchToBlock(selectedBlock, selectedFontPreset)
    : selectedBlock;
  const fontControlValues = selectedBlockFontControls ?? editingFontPreset;
  const selectedBlockFontSizeLinked = selectedBlock ? isBlockFontPresetValueLinked(selectedBlock, "fontSizePx") : true;
  const activeFontSizePresetId = selectedBlock
    ? selectedBlockFontSizeLinked
      ? selectedFontPreset?.fontSizePresetId ?? null
      : null
    : rawEditingFontPreset?.fontSizePresetId ?? null;
  const selectedBlockFontPresetLinks = selectedBlock
    ? {
        fontSizePx: isBlockFontPresetValueLinked(selectedBlock, "fontSizePx"),
        lineHeight: isBlockFontPresetValueLinked(selectedBlock, "lineHeight"),
        letterSpacingPx: isBlockFontPresetValueLinked(selectedBlock, "letterSpacingPx"),
        outlineColor: isBlockFontPresetValueLinked(selectedBlock, "outlineColor"),
        outlineWidthPx: isBlockFontPresetValueLinked(selectedBlock, "outlineWidthPx"),
        secondaryOutlineColor: isBlockFontPresetValueLinked(selectedBlock, "secondaryOutlineColor"),
        secondaryOutlineWidthPx: isBlockFontPresetValueLinked(selectedBlock, "secondaryOutlineWidthPx"),
        shadowEnabled: isBlockFontPresetValueLinked(selectedBlock, "shadowEnabled"),
        shadowColor: isBlockFontPresetValueLinked(selectedBlock, "shadowColor"),
        shadowOpacity: isBlockFontPresetValueLinked(selectedBlock, "shadowOpacity"),
        shadowBlurPx: isBlockFontPresetValueLinked(selectedBlock, "shadowBlurPx"),
        shadowAngleDeg: isBlockFontPresetValueLinked(selectedBlock, "shadowAngleDeg"),
        shadowDistancePx: isBlockFontPresetValueLinked(selectedBlock, "shadowDistancePx"),
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

  const updateAssignedFontPreset = React.useCallback((presetId: string, patch: AssignedFontPresetPatch, options: { recordUndo?: boolean; undoLabel?: string } = {}) => {
    if (!currentChapter || selectedPageEditLocked) {
      return;
    }

    if (options.recordUndo !== false) {
      recordTranslationUndoSnapshot(options.undoLabel ?? "폰트 설정 변경");
    }

    updateCurrentChapter(undefined, (current) => {
      const nextFontSizePresets = current.fontSizePresets ?? [];
      const normalizedPatch: AssignedFontPresetPatch =
        patch.characterFontOverrides !== undefined
          ? { ...patch, characterFontOverrides: normalizeCharacterFontOverrides(patch.characterFontOverrides) }
          : patch;
      const nextFontPresets = (current.fontPresets ?? []).map((preset) =>
        preset.id === presetId ? { ...preset, ...normalizedPatch } : preset
      );
      const nextPreset = nextFontPresets.find((preset) => preset.id === presetId);
      const blockPatch = nextPreset ? resolveFontPreset(nextPreset, nextFontSizePresets) : normalizedPatch;
      return {
        ...current,
        fontPresets: nextFontPresets,
        pages: current.pages.map((page) => ({
          ...page,
          updatedAt: page.blocks.some((block) => block.fontPresetId === presetId) ? new Date().toISOString() : page.updatedAt,
          blocks: page.blocks.map((block) => (block.fontPresetId === presetId ? applyFontPresetPatchToBlock(block, blockPatch) : block))
        }))
      };
    });
  }, [currentChapter, recordTranslationUndoSnapshot, selectedPageEditLocked, updateCurrentChapter]);

  const updateFontSizePresetValue = React.useCallback((presetId: string, fontSizePx: number, options: { recordUndo?: boolean } = {}) => {
    if (!currentChapter || selectedPageEditLocked) {
      return;
    }

    if (options.recordUndo !== false) {
      recordTranslationUndoSnapshot("폰트 크기 프리셋 변경");
    }

    updateCurrentChapter(undefined, (current) => {
      const linkedFontPresetIds = new Set(
        (current.fontPresets ?? [])
          .filter((preset) => preset.fontSizePresetId === presetId)
          .map((preset) => preset.id)
      );

      return {
        ...current,
        fontSizePresets: (current.fontSizePresets ?? []).map((preset) =>
          preset.id === presetId ? { ...preset, fontSizePx } : preset
        ),
        fontPresets: (current.fontPresets ?? []).map((preset) =>
          preset.fontSizePresetId === presetId ? { ...preset, fontSizePx } : preset
        ),
        pages: current.pages.map((page) => ({
          ...page,
          updatedAt: page.blocks.some((block) =>
            block.fontPresetId &&
            linkedFontPresetIds.has(block.fontPresetId) &&
            isBlockFontPresetValueLinked(block, "fontSizePx")
          )
            ? new Date().toISOString()
            : page.updatedAt,
          blocks: page.blocks.map((block) =>
            block.fontPresetId && linkedFontPresetIds.has(block.fontPresetId) && isBlockFontPresetValueLinked(block, "fontSizePx")
              ? { ...block, fontSizePx }
              : block
          )
        }))
      };
    });
  }, [currentChapter, recordTranslationUndoSnapshot, selectedPageEditLocked, updateCurrentChapter]);

  const updateSelectedBlockFontSetting = React.useCallback((patch: BlockFontPatch) => {
    if ("textAlign" in patch || "textPosition" in patch) {
      const blockPatch: Partial<Pick<TranslationBlock, "textAlign" | "textPosition">> = {};
      if (patch.textAlign) {
        blockPatch.textAlign = patch.textAlign;
      }
      if (patch.textPosition) {
        blockPatch.textPosition = patch.textPosition;
      }
      if (Object.keys(blockPatch).length > 0) {
        updateSelectedBlock(blockPatch);
      }
      return;
    }
    if (selectedBlock?.fontPresetId) {
      const presetPatch: AssignedFontPresetPatch = {};
      const blockPatch: Partial<TranslationBlock> = {};
      for (const key of Object.keys(patch) as (keyof FontPresetPatch)[]) {
        const value = patch[key];
        if (value === undefined) {
          continue;
        }
        if (
          key === "fontFamily" ||
          key === "characterFontOverrides" ||
          isBlockFontPresetValueLinked(selectedBlock, key as LinkableFontPresetKey)
        ) {
          if (key === "fontSizePx" && selectedFontPreset?.fontSizePresetId) {
            updateFontSizePresetValue(selectedFontPreset.fontSizePresetId, value as number);
            continue;
          }
          Object.assign(presetPatch, { [key]: value });
          if (key === "fontSizePx") {
            presetPatch.fontSizePresetId = undefined;
          }
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
    if (!selectedBlock && rawEditingFontPreset) {
      if ("fontSizePx" in patch && patch.fontSizePx !== undefined && rawEditingFontPreset.fontSizePresetId) {
        updateFontSizePresetValue(rawEditingFontPreset.fontSizePresetId, patch.fontSizePx);
        const { fontSizePx: _fontSizePx, ...restPatch } = patch;
        if (Object.keys(restPatch).length > 0) {
          updateAssignedFontPreset(rawEditingFontPreset.id, restPatch, { recordUndo: false });
        }
        return;
      }
      updateAssignedFontPreset(rawEditingFontPreset.id, "fontSizePx" in patch ? { ...patch, fontSizePresetId: undefined } : patch);
      return;
    }
    updateSelectedBlock(patch);
  }, [rawEditingFontPreset, recordTranslationUndoSnapshot, selectedBlock, selectedFontPreset?.fontSizePresetId, updateAssignedFontPreset, updateFontSizePresetValue, updateSelectedBlock]);

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
    const preset = {
      ...createFontPreset(presetName, fontControlValues ?? selectedBlock ?? DEFAULT_FONT_PRESET),
      fontSizePresetId: activeFontSizePresetId ?? undefined
    };
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
                      letterSpacingLinkedToPreset: true,
                      outlineColorLinkedToPreset: true,
                      outlineWidthLinkedToPreset: true,
                      secondaryOutlineColorLinkedToPreset: true,
                      secondaryOutlineWidthLinkedToPreset: true,
                      shadowEnabledLinkedToPreset: true,
                      shadowColorLinkedToPreset: true,
                      shadowOpacityLinkedToPreset: true,
                      shadowBlurPxLinkedToPreset: true,
                      shadowAngleDegLinkedToPreset: true,
                      shadowDistancePxLinkedToPreset: true,
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
  }, [activeFontSizePresetId, currentChapter, fontControlValues, fontPresetName, fontPresets, pushStatus, recordTranslationUndoSnapshot, selectedBlock, selectedPage, selectedPageEditLocked, setEditingFontPresetId, updateCurrentChapter]);

  const createFontSizePresetFromCurrentFontSize = React.useCallback(() => {
    if (!currentChapter || selectedPageEditLocked || !fontControlValues) {
      return;
    }

    const presetName = buildNextFontSizePresetName(fontSizePresets);
    const preset = createFontSizePreset(presetName, fontControlValues.fontSizePx);
    const activePresetId = selectedBlock?.fontPresetId ?? (!selectedBlock ? rawEditingFontPreset?.id : undefined);
    recordTranslationUndoSnapshot("폰트 크기 프리셋 생성");
    updateCurrentChapter(undefined, (current) => ({
      ...current,
      fontSizePresets: [...(current.fontSizePresets ?? []), preset],
      fontPresets: activePresetId
        ? (current.fontPresets ?? []).map((fontPreset) =>
            fontPreset.id === activePresetId
              ? { ...fontPreset, fontSizePresetId: preset.id, fontSizePx: preset.fontSizePx }
              : fontPreset
          )
        : current.fontPresets
    }));
  }, [currentChapter, fontControlValues, fontSizePresets, rawEditingFontPreset?.id, recordTranslationUndoSnapshot, selectedBlock, selectedPageEditLocked, updateCurrentChapter]);

  const createFontPresetListBackup = React.useCallback(async (name: string) => {
    if (!currentChapter) {
      pushStatus("백업할 화를 먼저 여세요.", "failed");
      return null;
    }

    const backup = await window.mangaApi.createFontPresetBackup({
      name,
      fontPresets: currentChapter.fontPresets ?? [],
      fontSizePresets: currentChapter.fontSizePresets ?? []
    });
    pushStatus("폰트 프리셋 백업을 저장했습니다.");
    return backup;
  }, [currentChapter, pushStatus]);

  const listFontPresetBackups = React.useCallback(() => window.mangaApi.listFontPresetBackups(), []);

  const deleteFontPresetBackup = React.useCallback((backupId: string) => window.mangaApi.deleteFontPresetBackup(backupId), []);

  const restoreFontPresetListBackup = React.useCallback(async (backupId: string) => {
    if (!currentChapter || selectedPageEditLocked) {
      return;
    }

    const backup = await window.mangaApi.getFontPresetBackup(backupId);
    recordTranslationUndoSnapshot("폰트 프리셋 백업 복원");
    updateCurrentChapter(undefined, (current) => applyFontPresetListBackupToChapter(current, backup));
    setEditingFontPresetId((current) => (current && backup.fontPresets.some((preset) => preset.id === current) ? current : null));
    pushStatus("폰트 프리셋 백업을 복원했습니다.");
  }, [currentChapter, pushStatus, recordTranslationUndoSnapshot, selectedPageEditLocked, setEditingFontPresetId, updateCurrentChapter]);

  const toggleFavoriteFontPreset = React.useCallback((presetId: string) => {
    if (!currentChapter || selectedPageEditLocked || !fontPresets.some((preset) => preset.id === presetId)) {
      return;
    }

    if (!favoriteFontPresetIds.includes(presetId) && favoriteFontPresetIds.length >= MAX_FAVORITE_FONT_PRESETS) {
      pushStatus("즐겨찾기 태그는 최대 5개까지 지정할 수 있습니다.", "failed");
      return;
    }

    const nextFavoriteFontPresetIds = favoriteFontPresetIds.includes(presetId)
      ? favoriteFontPresetIds.filter((favoritePresetId) => favoritePresetId !== presetId)
      : [...favoriteFontPresetIds, presetId];
    const updatedAt = new Date().toISOString();
    updateCurrentChapter(
      undefined,
      (current) => ({
        ...current,
        favoriteFontPresetIds: nextFavoriteFontPresetIds,
        updatedAt
      }),
      {
        immediateMetadataSave: {
          failureMessage: "즐겨찾기 태그 저장에 실패했습니다."
        }
      }
    );
  }, [currentChapter, favoriteFontPresetIds, fontPresets, pushStatus, selectedPageEditLocked, updateCurrentChapter]);

  const selectFontPreset = React.useCallback((presetId: string) => {
    if (selectedPageEditLocked) {
      return;
    }
    const preset = fontPresets.find((candidate) => candidate.id === presetId);
    if (!preset) {
      return;
    }
    const resolvedPreset = resolveFontPreset(preset, fontSizePresets);
    setEditingFontPresetId(presetId);
    if (!selectedPage || !selectedBlock) {
      return;
    }
    updateSelectedBlock({
      ...applyFontPresetPatchToBlock(selectedBlock, resolvedPreset, { forceLinkedValues: true }),
      fontPresetId: preset.id,
      fontSizeLinkedToPreset: true,
      lineHeightLinkedToPreset: true,
      letterSpacingLinkedToPreset: true,
      outlineColorLinkedToPreset: true,
      outlineWidthLinkedToPreset: true,
      secondaryOutlineColorLinkedToPreset: true,
      secondaryOutlineWidthLinkedToPreset: true,
      shadowEnabledLinkedToPreset: true,
      shadowColorLinkedToPreset: true,
      shadowOpacityLinkedToPreset: true,
      shadowBlurPxLinkedToPreset: true,
      shadowAngleDegLinkedToPreset: true,
      shadowDistancePxLinkedToPreset: true,
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
  }, [fontPresets, fontSizePresets, selectedBlock, selectedPage, selectedPageEditLocked, setEditingFontPresetId, updateSelectedBlock]);

  const clearSelectedBlockFontPreset = React.useCallback(() => {
    if (!selectedBlock) {
      return;
    }
    updateSelectedBlock({
      fontPresetId: undefined,
      fontSizeLinkedToPreset: undefined,
      lineHeightLinkedToPreset: undefined,
      letterSpacingLinkedToPreset: undefined,
      outlineColorLinkedToPreset: undefined,
      outlineWidthLinkedToPreset: undefined,
      secondaryOutlineColorLinkedToPreset: undefined,
      secondaryOutlineWidthLinkedToPreset: undefined,
      shadowEnabledLinkedToPreset: undefined,
      shadowColorLinkedToPreset: undefined,
      shadowOpacityLinkedToPreset: undefined,
      shadowBlurPxLinkedToPreset: undefined,
      shadowAngleDegLinkedToPreset: undefined,
      shadowDistancePxLinkedToPreset: undefined,
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

  const selectFontSizePreset = React.useCallback((presetId: string | null) => {
    if (!currentChapter || selectedPageEditLocked) {
      return;
    }
    if (selectedBlock && (!selectedBlock.fontPresetId || !isBlockFontPresetValueLinked(selectedBlock, "fontSizePx"))) {
      return;
    }
    const preset = presetId ? fontSizePresets.find((candidate) => candidate.id === presetId) ?? null : null;
    if (presetId && !preset) {
      return;
    }
    const nextFontSizePx = preset?.fontSizePx ?? fontControlValues?.fontSizePx;
    if (nextFontSizePx === undefined) {
      return;
    }

    if (selectedBlock?.fontPresetId) {
      updateAssignedFontPreset(selectedBlock.fontPresetId, {
        fontSizePresetId: preset?.id,
        fontSizePx: nextFontSizePx
      }, { undoLabel: preset ? "폰트 크기 프리셋 적용" : "폰트 크기 프리셋 해제" });
      return;
    }
    if (!selectedBlock && rawEditingFontPreset) {
      updateAssignedFontPreset(rawEditingFontPreset.id, {
        fontSizePresetId: preset?.id,
        fontSizePx: nextFontSizePx
      }, { undoLabel: preset ? "폰트 크기 프리셋 적용" : "폰트 크기 프리셋 해제" });
      return;
    }
    if (selectedBlock && preset) {
      updateSelectedBlock({ fontSizePx: preset.fontSizePx }, { undoLabel: "폰트 크기 프리셋 적용" });
    }
  }, [currentChapter, fontControlValues?.fontSizePx, fontSizePresets, rawEditingFontPreset, selectedBlock, selectedPageEditLocked, updateAssignedFontPreset, updateSelectedBlock]);

  const deleteFontSizePreset = React.useCallback((presetId: string) => {
    if (!currentChapter || selectedPageEditLocked) {
      return;
    }

    recordTranslationUndoSnapshot("폰트 크기 프리셋 삭제");
    updateCurrentChapter(undefined, (current) => ({
      ...current,
      fontSizePresets: (current.fontSizePresets ?? []).filter((preset) => preset.id !== presetId),
      fontPresets: (current.fontPresets ?? []).map((preset) =>
        preset.fontSizePresetId === presetId ? { ...preset, fontSizePresetId: undefined } : preset
      )
    }));
  }, [currentChapter, recordTranslationUndoSnapshot, selectedPageEditLocked, updateCurrentChapter]);

  const deleteFontPreset = React.useCallback((presetId: string) => {
    if (selectedPageEditLocked) {
      return;
    }
    recordTranslationUndoSnapshot("폰트 프리셋 삭제");
    updateCurrentChapter(undefined, (current) => ({
      ...current,
      favoriteFontPresetIds: resolveFavoriteFontPresetIds(
        current.favoriteFontPresetIds?.filter((favoritePresetId) => favoritePresetId !== presetId),
        (current.fontPresets ?? []).filter((preset) => preset.id !== presetId)
      ),
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
  };
}
