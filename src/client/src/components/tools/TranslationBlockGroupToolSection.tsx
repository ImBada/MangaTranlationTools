import React from "react";
import type { MangaPage, TranslationBlock, TranslationBlockGroup, TranslationBlockGroupEffect } from "../../../../shared/types";
import { FontShadowControls } from "../font/FontShadowControls";
import {
  resolveTranslationBlockGroupDropShadowEffect,
  resolveTranslationBlockGroupDropShadowSettings,
  setTranslationBlockGroupDropShadowEnabled,
  type TranslationBlockGroupDropShadowSettings,
  updateTranslationBlockGroupDropShadowSettings
} from "../../lib/blockGroupEffects";
import { BLOCK_TYPE_LABELS, resolveBlockPreviewText } from "../../lib/blockDisplay";

type TranslationBlockGroupToolSectionProps = {
  selectedBlockGroup: TranslationBlockGroup;
  selectedPage: MangaPage | null;
  selectedPageEditLocked: boolean;
  onEffectsChange: (effects: TranslationBlockGroupEffect[]) => void;
  onSelectBlock: (blockId: string) => void;
};

export function TranslationBlockGroupToolSection({
  selectedBlockGroup,
  selectedPage,
  selectedPageEditLocked,
  onEffectsChange,
  onSelectBlock
}: TranslationBlockGroupToolSectionProps): React.JSX.Element {
  const dropShadowEffect = resolveTranslationBlockGroupDropShadowEffect(selectedBlockGroup);
  const dropShadowSettings = resolveTranslationBlockGroupDropShadowSettings(dropShadowEffect);
  const dropShadowActive = Boolean(dropShadowEffect?.enabled);
  const groupBlocks = React.useMemo(() => resolveGroupBlocks(selectedPage, selectedBlockGroup), [selectedBlockGroup, selectedPage]);

  const setDropShadowEnabled = React.useCallback((enabled: boolean) => {
    if (dropShadowActive === enabled) {
      return;
    }
    onEffectsChange(setTranslationBlockGroupDropShadowEnabled(selectedBlockGroup.effects, enabled));
  }, [dropShadowActive, onEffectsChange, selectedBlockGroup.effects]);

  const updateDropShadowSettings = React.useCallback((patch: Partial<TranslationBlockGroupDropShadowSettings>) => {
    onEffectsChange(updateTranslationBlockGroupDropShadowSettings(selectedBlockGroup.effects, patch));
  }, [onEffectsChange, selectedBlockGroup.effects]);

  return (
    <div className="group-effect-control-stack">
      <div className="compact-tool-field group-effect-summary-field">
        <span>선택 그룹</span>
        <strong>{selectedBlockGroup.blockIds.length}개 블록</strong>
      </div>
      {groupBlocks.length > 0 ? (
        <div className="compact-tool-field group-block-list-field">
          <span>
            <span>그룹 블록</span>
            <strong>{groupBlocks.length}</strong>
          </span>
          <div className="group-block-list">
            {groupBlocks.map(({ block, blockIndex }) => (
              <button
                key={block.id}
                type="button"
                className={`stage-text-block-list-row grouped${block.renderDirection === "hidden" ? " hidden" : ""}`}
                onClick={() => onSelectBlock(block.id)}
                aria-label={`블록 ${blockIndex + 1} 개별 설정`}
                title="개별 설정"
              >
                <span className="stage-text-block-list-index">{blockIndex + 1}</span>
                <span className="stage-text-block-list-copy">
                  <span className="stage-text-block-list-meta">
                    <span>{BLOCK_TYPE_LABELS[block.type]}</span>
                    {block.renderDirection === "hidden" ? <span>숨김</span> : null}
                  </span>
                  <span className="stage-text-block-list-text">{resolveBlockPreviewText(block)}</span>
                </span>
              </button>
            ))}
          </div>
        </div>
      ) : null}
      <div className="group-effect-section">
        <FontShadowControls
          disabled={selectedPageEditLocked}
          enabledAriaLabel="그룹 그림자 사용"
          values={{
            angleDeg: dropShadowSettings.angleDeg,
            blurPx: dropShadowSettings.blurPx,
            color: dropShadowSettings.color,
            distancePx: dropShadowSettings.distancePx,
            enabled: dropShadowActive,
            opacity: dropShadowSettings.opacity
          }}
          onEnabledChange={setDropShadowEnabled}
          onChange={updateDropShadowSettings}
        />
      </div>
    </div>
  );
}

function resolveGroupBlocks(
  selectedPage: MangaPage | null,
  selectedBlockGroup: TranslationBlockGroup
): { block: TranslationBlock; blockIndex: number }[] {
  if (!selectedPage) {
    return [];
  }

  const blocksById = new Map(selectedPage.blocks.map((block, index) => [block.id, { block, blockIndex: index }]));
  return selectedBlockGroup.blockIds.flatMap((blockId) => {
    const item = blocksById.get(blockId);
    return item ? [item] : [];
  });
}
