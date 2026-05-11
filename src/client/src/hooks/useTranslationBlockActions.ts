import React from "react";
import type { ChapterSnapshot, FontPreset, MangaPage, TranslationBlock } from "../../../shared/types";
import { offsetBlockBboxes } from "../../../shared/geometry";
import { DEFAULT_FONT_PRESET } from "../lib/fontPresets";
import {
  applyTranslationBlockFontStyle,
  cloneTranslationBlock,
  extractTranslationBlockFontStyle,
  parseTranslationBlockFontStyleFromClipboard,
  parseTranslationBlockFromClipboard,
  serializeTranslationBlockFontStyleForClipboard,
  serializeTranslationBlockForClipboard,
  splitTextBySelection
} from "../lib/editorUtils";
import type { TranslationBlockFontStylePatch } from "../lib/editorUtils";
import { resolveTranslationBlockGroupsAfterBlockRemoval } from "../lib/blockGroups";
import type { ActiveLayer } from "../lib/layerState";

type UseTranslationBlockActionsOptions = {
  editingFontPreset: FontPreset | null;
  inpaintBusy: boolean;
  pushStatus: (line: string) => void;
  recordTranslationUndoSnapshot: (label: string) => boolean;
  selectLayer: (nextLayer: ActiveLayer) => void;
  selectedBlock: TranslationBlock | null;
  selectedBlockIds: string[];
  selectedPage: MangaPage | null;
  selectedPageEditLocked: boolean;
  setSelectedBlockId: React.Dispatch<React.SetStateAction<string | null>>;
  showOverlayLayer: () => void;
  updateCurrentChapter: (pageId: string | undefined, updater: (chapter: ChapterSnapshot) => ChapterSnapshot) => void;
};

type UseTranslationBlockActionsState = {
  copySelectedBlockFontStyleToClipboard: () => Promise<void>;
  copySelectedBlockToClipboard: () => Promise<void>;
  createEmptyBlock: () => void;
  deleteSelectedBlock: () => void;
  duplicateBlock: (block: TranslationBlock) => void;
  duplicateBlockTextSelection: (block: TranslationBlock, translatedText: string, selectionStart: number, selectionEnd: number) => boolean;
  duplicateSelectedBlock: () => void;
  pasteSelectedBlockFontStyleFromClipboard: () => Promise<boolean>;
  pasteTranslationBlockFromClipboard: () => Promise<void>;
  updateSelectedPageBlockOpacity: (opacity: number) => void;
};

export function useTranslationBlockActions({
  editingFontPreset,
  inpaintBusy,
  pushStatus,
  recordTranslationUndoSnapshot,
  selectLayer,
  selectedBlock,
  selectedBlockIds,
  selectedPage,
  selectedPageEditLocked,
  setSelectedBlockId,
  showOverlayLayer,
  updateCurrentChapter
}: UseTranslationBlockActionsOptions): UseTranslationBlockActionsState {
  const translationBlockClipboardRef = React.useRef<TranslationBlock | null>(null);
  const translationBlockFontStyleClipboardRef = React.useRef<TranslationBlockFontStylePatch | null>(null);

  const deleteSelectedBlock = React.useCallback(() => {
    const selectedIds = selectedBlockIds.length > 1
      ? selectedBlockIds
      : selectedBlock
        ? [selectedBlock.id]
        : [];
    if (!selectedPage || selectedIds.length === 0 || selectedPageEditLocked) {
      return;
    }
    const selectedIdSet = new Set(selectedIds);
    const updatedAt = new Date().toISOString();
    recordTranslationUndoSnapshot(selectedIds.length > 1 ? "번역 블록 여러 개 삭제" : "번역 블록 삭제");
    updateCurrentChapter(selectedPage.id, (current) => ({
      ...current,
      pages: current.pages.map((page) =>
        page.id === selectedPage.id
          ? {
              ...page,
              updatedAt,
              blocks: page.blocks.filter((block) => !selectedIdSet.has(block.id)),
              blockGroups: resolveTranslationBlockGroupsAfterBlockRemoval(page.blockGroups, selectedIds, updatedAt)
            }
          : page
      )
    }));
    setSelectedBlockId(null);
  }, [recordTranslationUndoSnapshot, selectedBlock, selectedBlockIds, selectedPage, selectedPageEditLocked, setSelectedBlockId, updateCurrentChapter]);

  const updateSelectedPageBlockOpacity = React.useCallback((opacity: number) => {
    if (!selectedPage || selectedPageEditLocked) {
      return;
    }

    recordTranslationUndoSnapshot("블록 투명도 변경");
    updateCurrentChapter(selectedPage.id, (current) => ({
      ...current,
      pages: current.pages.map((page) =>
        page.id === selectedPage.id
          ? {
              ...page,
              updatedAt: new Date().toISOString(),
              blocks: page.blocks.map((block) => ({ ...block, opacity }))
            }
          : page
      )
    }));
  }, [recordTranslationUndoSnapshot, selectedPage, selectedPageEditLocked, updateCurrentChapter]);

  const copySelectedBlockToClipboard = React.useCallback(async () => {
    if (!selectedBlock) {
      return;
    }

    const blockCopy = cloneTranslationBlock(selectedBlock);
    translationBlockClipboardRef.current = blockCopy;
    translationBlockFontStyleClipboardRef.current = null;
    try {
      await navigator.clipboard?.writeText(serializeTranslationBlockForClipboard(blockCopy));
      pushStatus("선택한 텍스트 블록을 복사했습니다.");
    } catch {
      pushStatus("선택한 텍스트 블록을 복사했습니다. 시스템 클립보드 접근은 차단되어 앱 안에서만 붙여넣을 수 있습니다.");
    }
  }, [pushStatus, selectedBlock]);

  const copySelectedBlockFontStyleToClipboard = React.useCallback(async () => {
    if (!selectedBlock) {
      return;
    }

    const fontStyle = extractTranslationBlockFontStyle(selectedBlock);
    translationBlockClipboardRef.current = null;
    translationBlockFontStyleClipboardRef.current = { ...fontStyle };
    try {
      await navigator.clipboard?.writeText(serializeTranslationBlockFontStyleForClipboard(fontStyle));
      pushStatus("선택한 블록의 폰트 설정을 복사했습니다.");
    } catch {
      pushStatus("선택한 블록의 폰트 설정을 복사했습니다. 시스템 클립보드 접근은 차단되어 앱 안에서만 붙여넣을 수 있습니다.");
    }
  }, [pushStatus, selectedBlock]);

  const pasteSelectedBlockFontStyleFromClipboard = React.useCallback(async () => {
    if (!selectedPage || !selectedBlock || selectedPageEditLocked) {
      return false;
    }

    let fontStyle = translationBlockFontStyleClipboardRef.current
      ? { ...translationBlockFontStyleClipboardRef.current }
      : null;
    try {
      const clipboardText = await navigator.clipboard?.readText();
      const clipboardFontStyle = clipboardText ? parseTranslationBlockFontStyleFromClipboard(clipboardText) : null;
      if (clipboardFontStyle) {
        fontStyle = clipboardFontStyle;
      }
    } catch {
      // Keep the in-memory style copy path working when clipboard read permission is unavailable.
    }

    if (!fontStyle) {
      return false;
    }

    const appliedFontStyle = fontStyle;
    translationBlockFontStyleClipboardRef.current = { ...appliedFontStyle };
    recordTranslationUndoSnapshot("폰트 설정 붙여넣기");
    updateCurrentChapter(selectedPage.id, (current) => ({
      ...current,
      pages: current.pages.map((page) =>
        page.id === selectedPage.id
          ? {
              ...page,
              updatedAt: new Date().toISOString(),
              blocks: page.blocks.map((block) =>
                block.id === selectedBlock.id ? applyTranslationBlockFontStyle(block, appliedFontStyle) : block
              )
            }
          : page
      )
    }));
    pushStatus("복사한 폰트 설정을 선택한 블록에 적용했습니다.");
    return true;
  }, [pushStatus, recordTranslationUndoSnapshot, selectedBlock, selectedPage, selectedPageEditLocked, updateCurrentChapter]);

  const pasteTranslationBlockFromClipboard = React.useCallback(async () => {
    if (!selectedPage || selectedPageEditLocked) {
      return;
    }

    let sourceBlock = translationBlockClipboardRef.current ? cloneTranslationBlock(translationBlockClipboardRef.current) : null;
    try {
      const clipboardText = await navigator.clipboard?.readText();
      if (clipboardText) {
        sourceBlock = parseTranslationBlockFromClipboard(clipboardText) ?? sourceBlock;
      }
    } catch {
      // Keep the in-memory copy path working when clipboard read permission is unavailable.
    }

    if (!sourceBlock) {
      pushStatus("붙여넣을 텍스트 블록이 없습니다.");
      return;
    }

    const pastedBlock = {
      ...offsetBlockBboxes(sourceBlock, 16, 16),
      id: `${sourceBlock.id}-paste-${Date.now()}`
    };
    translationBlockClipboardRef.current = cloneTranslationBlock(sourceBlock);
    recordTranslationUndoSnapshot("번역 블록 붙여넣기");
    updateCurrentChapter(selectedPage.id, (current) => ({
      ...current,
      pages: current.pages.map((page) =>
        page.id === selectedPage.id
          ? {
              ...page,
              updatedAt: new Date().toISOString(),
              blocks: [...page.blocks, pastedBlock]
            }
          : page
      )
    }));
    showOverlayLayer();
    selectLayer("overlay");
    setSelectedBlockId(pastedBlock.id);
  }, [pushStatus, recordTranslationUndoSnapshot, selectLayer, selectedPage, selectedPageEditLocked, setSelectedBlockId, showOverlayLayer, updateCurrentChapter]);

  const duplicateBlock = React.useCallback((block: TranslationBlock) => {
    if (!selectedPage || selectedPageEditLocked) {
      return;
    }
    const copy = {
      ...offsetBlockBboxes(block, 16, 16),
      id: `${block.id}-copy-${Date.now()}`
    };
    recordTranslationUndoSnapshot("번역 블록 복제");
    updateCurrentChapter(selectedPage.id, (current) => ({
      ...current,
      pages: current.pages.map((page) =>
        page.id === selectedPage.id
          ? {
              ...page,
              updatedAt: new Date().toISOString(),
              blocks: [...page.blocks, copy]
            }
          : page
      )
    }));
    setSelectedBlockId(copy.id);
  }, [recordTranslationUndoSnapshot, selectedPage, selectedPageEditLocked, setSelectedBlockId, updateCurrentChapter]);

  const duplicateBlockTextSelection = React.useCallback((
    block: TranslationBlock,
    translatedText: string,
    selectionStart: number,
    selectionEnd: number
  ) => {
    if (!selectedPage || selectedPageEditLocked || !selectedPage.blocks.some((candidate) => candidate.id === block.id)) {
      return false;
    }

    const split = splitTextBySelection(translatedText, selectionStart, selectionEnd);
    if (!split) {
      return false;
    }

    const copyId = `${block.id}-split-${Date.now()}`;
    recordTranslationUndoSnapshot("번역 블록 나누기");
    updateCurrentChapter(selectedPage.id, (current) => ({
      ...current,
      pages: current.pages.map((page) =>
        page.id === selectedPage.id
          ? {
              ...page,
              updatedAt: new Date().toISOString(),
              blocks: page.blocks.flatMap((candidate) => {
                if (candidate.id !== block.id) {
                  return [candidate];
                }

                const remainingBlock = { ...candidate, translatedText: split.remainingText };
                const selectedTextBlock = {
                  ...offsetBlockBboxes(candidate, 16, 16),
                  id: copyId,
                  translatedText: split.selectedText
                };
                return [remainingBlock, selectedTextBlock];
              })
            }
          : page
      )
    }));
    setSelectedBlockId(copyId);
    return true;
  }, [recordTranslationUndoSnapshot, selectedPage, selectedPageEditLocked, setSelectedBlockId, updateCurrentChapter]);

  const duplicateSelectedBlock = React.useCallback(() => {
    if (!selectedBlock) {
      return;
    }
    duplicateBlock(selectedBlock);
  }, [duplicateBlock, selectedBlock]);

  const createEmptyBlock = React.useCallback(() => {
    if (!selectedPage || selectedPageEditLocked || inpaintBusy) {
      return;
    }
    const sourcePreset = editingFontPreset ?? DEFAULT_FONT_PRESET;
    const blockId = `${selectedPage.id}-block-manual-${Date.now()}`;
    const block: TranslationBlock = {
      id: blockId,
      type: "speech",
      bbox: { x: 350, y: 420, w: 300, h: 140 },
      bboxSpace: "normalized_1000",
      sourceText: "",
      translatedText: "",
      confidence: 1,
      sourceDirection: "vertical",
      renderDirection: "horizontal",
      fontPresetId: editingFontPreset?.id,
      fontSizeLinkedToPreset: editingFontPreset ? true : undefined,
      lineHeightLinkedToPreset: editingFontPreset ? true : undefined,
      letterSpacingLinkedToPreset: editingFontPreset ? true : undefined,
      outlineColorLinkedToPreset: editingFontPreset ? true : undefined,
      outlineWidthLinkedToPreset: editingFontPreset ? true : undefined,
      secondaryOutlineColorLinkedToPreset: editingFontPreset ? true : undefined,
      secondaryOutlineWidthLinkedToPreset: editingFontPreset ? true : undefined,
      shadowEnabledLinkedToPreset: editingFontPreset ? true : undefined,
      shadowColorLinkedToPreset: editingFontPreset ? true : undefined,
      shadowAngleDegLinkedToPreset: editingFontPreset ? true : undefined,
      shadowDistancePxLinkedToPreset: editingFontPreset ? true : undefined,
      autoFitTextLinkedToPreset: editingFontPreset ? true : undefined,
      textColorLinkedToPreset: editingFontPreset ? true : undefined,
      screentoneFillEnabledLinkedToPreset: editingFontPreset ? true : undefined,
      screentoneFillIntensityLinkedToPreset: editingFontPreset ? true : undefined,
      screentoneFillDensityLinkedToPreset: editingFontPreset ? true : undefined,
      screentoneFillAntialiasLinkedToPreset: editingFontPreset ? true : undefined,
      fontWeightLinkedToPreset: editingFontPreset ? true : undefined,
      fontStyleLinkedToPreset: editingFontPreset ? true : undefined,
      textDecorationLinkedToPreset: editingFontPreset ? true : undefined,
      fontFamily: sourcePreset.fontFamily,
      characterFontOverrides: sourcePreset.characterFontOverrides?.map((override) => ({ ...override })),
      fontWeight: sourcePreset.fontWeight,
      fontStyle: sourcePreset.fontStyle,
      textDecoration: sourcePreset.textDecoration,
      fontSizePx: sourcePreset.fontSizePx,
      lineHeight: sourcePreset.lineHeight,
      letterSpacingPx: sourcePreset.letterSpacingPx,
      outlineColor: sourcePreset.outlineColor,
      outlineWidthPx: sourcePreset.outlineWidthPx,
      secondaryOutlineColor: sourcePreset.secondaryOutlineColor,
      secondaryOutlineWidthPx: sourcePreset.secondaryOutlineWidthPx,
      shadowEnabled: sourcePreset.shadowEnabled,
      shadowColor: sourcePreset.shadowColor,
      shadowAngleDeg: sourcePreset.shadowAngleDeg,
      shadowDistancePx: sourcePreset.shadowDistancePx,
      autoFitText: sourcePreset.autoFitText,
      textAlign: "center",
      textPosition: "center",
      textColor: sourcePreset.textColor ?? "#111111",
      screentoneFillEnabled: sourcePreset.screentoneFillEnabled,
      screentoneFillIntensity: sourcePreset.screentoneFillIntensity,
      screentoneFillDensity: sourcePreset.screentoneFillDensity,
      screentoneFillAntialias: sourcePreset.screentoneFillAntialias,
      backgroundColor: "#fffdf5",
      opacity: 0.88
    };

    recordTranslationUndoSnapshot("번역 블록 생성");
    updateCurrentChapter(selectedPage.id, (current) => ({
      ...current,
      pages: current.pages.map((page) =>
        page.id === selectedPage.id
          ? {
              ...page,
              updatedAt: new Date().toISOString(),
              blocks: [...page.blocks, block]
            }
          : page
      )
    }));
    showOverlayLayer();
    selectLayer("overlay");
    setSelectedBlockId(blockId);
  }, [editingFontPreset, inpaintBusy, recordTranslationUndoSnapshot, selectLayer, selectedPage, selectedPageEditLocked, setSelectedBlockId, showOverlayLayer, updateCurrentChapter]);

  return {
    copySelectedBlockFontStyleToClipboard,
    copySelectedBlockToClipboard,
    createEmptyBlock,
    deleteSelectedBlock,
    duplicateBlock,
    duplicateBlockTextSelection,
    duplicateSelectedBlock,
    pasteSelectedBlockFontStyleFromClipboard,
    pasteTranslationBlockFromClipboard,
    updateSelectedPageBlockOpacity
  };
}
