import React from "react";
import type { ChapterSnapshot } from "../../../shared/types";
import { collectFindReplaceMatches, replaceTextLiteral } from "../lib/findReplace";
import type { ActiveLayer } from "../lib/layerState";

type UseFindReplaceEditingOptions = {
  currentChapter: ChapterSnapshot | null;
  jobActive: boolean;
  pushStatus: (line: string) => void;
  recordTranslationUndoSnapshot: (label: string) => boolean;
  selectLayer: (nextLayer: ActiveLayer) => void;
  setSelectedBlockId: React.Dispatch<React.SetStateAction<string | null>>;
  setSelectedPageId: React.Dispatch<React.SetStateAction<string | null>>;
  updateCurrentChapter: (pageId: string | undefined, updater: (chapter: ChapterSnapshot) => ChapterSnapshot) => void;
};

type UseFindReplaceEditingState = {
  findReplaceOpen: boolean;
  focusFindReplaceMatch: (pageId: string, blockId: string) => void;
  openFindReplace: () => void;
  replaceAllMatches: (keyword: string, replacement: string) => void;
  replaceSingleMatch: (pageId: string, blockId: string, keyword: string, replacement: string) => void;
  setFindReplaceOpen: React.Dispatch<React.SetStateAction<boolean>>;
};

export function useFindReplaceEditing({
  currentChapter,
  jobActive,
  pushStatus,
  recordTranslationUndoSnapshot,
  selectLayer,
  setSelectedBlockId,
  setSelectedPageId,
  updateCurrentChapter
}: UseFindReplaceEditingOptions): UseFindReplaceEditingState {
  const [findReplaceOpen, setFindReplaceOpen] = React.useState(false);

  const focusFindReplaceMatch = React.useCallback((pageId: string, blockId: string) => {
    selectLayer("overlay");
    setSelectedPageId(pageId);
    setSelectedBlockId(blockId);
  }, [selectLayer, setSelectedBlockId, setSelectedPageId]);

  const openFindReplace = React.useCallback(() => {
    setFindReplaceOpen(true);
  }, []);

  const replaceAllMatches = React.useCallback((keyword: string, replacement: string) => {
    if (!currentChapter || jobActive || !keyword) {
      return;
    }

    const matches = collectFindReplaceMatches(currentChapter.pages, keyword, replacement)
      .filter((match) => match.before !== match.after);
    if (matches.length === 0) {
      pushStatus("바꿀 문장이 없습니다.");
      return;
    }

    recordTranslationUndoSnapshot("찾아바꾸기 일괄 변경");
    const updatedAt = new Date().toISOString();
    updateCurrentChapter(undefined, (chapter) => ({
      ...chapter,
      pages: chapter.pages.map((page) => {
        let pageChanged = false;
        const blocks = page.blocks.map((block) => {
          const translatedText = replaceTextLiteral(block.translatedText, keyword, replacement);
          if (translatedText === block.translatedText) {
            return block;
          }
          pageChanged = true;
          return { ...block, translatedText };
        });
        return pageChanged ? { ...page, updatedAt, blocks } : page;
      })
    }));
    pushStatus(`${matches.length}개 문장을 바꿨습니다.`);
  }, [currentChapter, jobActive, pushStatus, recordTranslationUndoSnapshot, updateCurrentChapter]);

  const replaceSingleMatch = React.useCallback((pageId: string, blockId: string, keyword: string, replacement: string) => {
    if (!currentChapter || jobActive || !keyword) {
      return;
    }

    const page = currentChapter.pages.find((candidate) => candidate.id === pageId);
    const block = page?.blocks.find((candidate) => candidate.id === blockId);
    if (!page || !block) {
      pushStatus("바꿀 문장을 찾을 수 없습니다.");
      return;
    }

    const translatedText = replaceTextLiteral(block.translatedText, keyword, replacement);
    if (translatedText === block.translatedText) {
      pushStatus("변경할 내용이 없습니다.");
      return;
    }

    recordTranslationUndoSnapshot("찾아바꾸기 변경");
    focusFindReplaceMatch(pageId, blockId);
    const updatedAt = new Date().toISOString();
    updateCurrentChapter(pageId, (chapter) => ({
      ...chapter,
      pages: chapter.pages.map((candidatePage) =>
        candidatePage.id !== pageId
          ? candidatePage
          : {
              ...candidatePage,
              updatedAt,
              blocks: candidatePage.blocks.map((candidateBlock) =>
                candidateBlock.id === blockId
                  ? { ...candidateBlock, translatedText }
                  : candidateBlock
              )
            }
      )
    }));
    pushStatus("1개 문장을 바꿨습니다.");
  }, [currentChapter, focusFindReplaceMatch, jobActive, pushStatus, recordTranslationUndoSnapshot, updateCurrentChapter]);

  React.useEffect(() => {
    if (!currentChapter) {
      setFindReplaceOpen(false);
    }
  }, [currentChapter]);

  return {
    findReplaceOpen,
    focusFindReplaceMatch,
    openFindReplace,
    replaceAllMatches,
    replaceSingleMatch,
    setFindReplaceOpen
  };
}
