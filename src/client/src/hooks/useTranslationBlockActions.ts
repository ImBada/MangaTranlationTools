import React from "react";
import type { ChapterSnapshot, FontPreset, MangaPage, TranslationBlock } from "../../../shared/types";
import { offsetBlockBboxes } from "../../../shared/geometry";
import { DEFAULT_FONT_PRESET } from "../lib/fontPresets";
import {
  cloneTranslationBlock,
  parseTranslationBlockFromClipboard,
  serializeTranslationBlockForClipboard
} from "../lib/editorUtils";
import type { ActiveLayer } from "../lib/layerState";

type UseTranslationBlockActionsOptions = {
  editingFontPreset: FontPreset | null;
  inpaintBusy: boolean;
  pushStatus: (line: string) => void;
  recordTranslationUndoSnapshot: (label: string) => boolean;
  selectLayer: (nextLayer: ActiveLayer) => void;
  selectedBlock: TranslationBlock | null;
  selectedPage: MangaPage | null;
  selectedPageEditLocked: boolean;
  setSelectedBlockId: React.Dispatch<React.SetStateAction<string | null>>;
  showOverlayLayer: () => void;
  updateCurrentChapter: (pageId: string | undefined, updater: (chapter: ChapterSnapshot) => ChapterSnapshot) => void;
};

type UseTranslationBlockActionsState = {
  copySelectedBlockToClipboard: () => Promise<void>;
  createEmptyBlock: () => void;
  deleteSelectedBlock: () => void;
  duplicateBlock: (block: TranslationBlock) => void;
  duplicateSelectedBlock: () => void;
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
  selectedPage,
  selectedPageEditLocked,
  setSelectedBlockId,
  showOverlayLayer,
  updateCurrentChapter
}: UseTranslationBlockActionsOptions): UseTranslationBlockActionsState {
  const translationBlockClipboardRef = React.useRef<TranslationBlock | null>(null);

  const deleteSelectedBlock = React.useCallback(() => {
    if (!selectedPage || !selectedBlock || selectedPageEditLocked) {
      return;
    }
    recordTranslationUndoSnapshot("번역 블록 삭제");
    updateCurrentChapter(selectedPage.id, (current) => ({
      ...current,
      pages: current.pages.map((page) =>
        page.id === selectedPage.id
          ? {
              ...page,
              updatedAt: new Date().toISOString(),
              blocks: page.blocks.filter((block) => block.id !== selectedBlock.id)
            }
          : page
      )
    }));
    setSelectedBlockId(null);
  }, [recordTranslationUndoSnapshot, selectedBlock, selectedPage, selectedPageEditLocked, setSelectedBlockId, updateCurrentChapter]);

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
    try {
      await navigator.clipboard?.writeText(serializeTranslationBlockForClipboard(blockCopy));
      pushStatus("선택한 텍스트 블록을 복사했습니다.");
    } catch {
      pushStatus("선택한 텍스트 블록을 복사했습니다. 시스템 클립보드 접근은 차단되어 앱 안에서만 붙여넣을 수 있습니다.");
    }
  }, [pushStatus, selectedBlock]);

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
    copySelectedBlockToClipboard,
    createEmptyBlock,
    deleteSelectedBlock,
    duplicateBlock,
    duplicateSelectedBlock,
    pasteTranslationBlockFromClipboard,
    updateSelectedPageBlockOpacity
  };
}
