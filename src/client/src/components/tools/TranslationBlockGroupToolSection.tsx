import React from "react";
import type { MangaPage, TranslationBlock, TranslationBlockGroup, TranslationBlockGroupEffect } from "../../../../shared/types";
import { CompactNumberControl } from "../controls/CompactNumberControl";
import {
  resolveTranslationBlockGroupDropShadowEffect,
  resolveTranslationBlockGroupDropShadowSettings,
  setTranslationBlockGroupDropShadowEnabled,
  type TranslationBlockGroupDropShadowSettings,
  updateTranslationBlockGroupDropShadowSettings
} from "../../lib/blockGroupEffects";
import { BLOCK_TYPE_LABELS, resolveBlockPreviewText } from "../../lib/blockDisplay";
import { mouseOnlyColorInputProps, mouseOnlyRangeInputProps } from "../../lib/mouseOnlyCheckbox";
import { rangeProgressStyle } from "../../lib/rangeProgressStyle";

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
      <div className="compact-tool-field font-outline-section font-shadow-section font-effect-section group-effect-section">
        <div className="font-outline-section-header">
          <span>그림자</span>
          <div className="font-outline-mode" aria-label="그룹 그림자 사용">
            <button
              type="button"
              className={!dropShadowActive ? "active" : ""}
              disabled={selectedPageEditLocked}
              onClick={() => setDropShadowEnabled(false)}
              aria-pressed={!dropShadowActive}
            >
              OFF
            </button>
            <button
              type="button"
              className={dropShadowActive ? "active" : ""}
              disabled={selectedPageEditLocked}
              onClick={() => setDropShadowEnabled(true)}
              aria-pressed={dropShadowActive}
            >
              ON
            </button>
          </div>
        </div>
        {dropShadowActive ? (
          <>
            <div className="font-shadow-row font-tool-grid">
              <label className="compact-tool-field font-color-field">
                <span>그림자 색</span>
                <span className="color-picker-shell" style={{ backgroundColor: dropShadowSettings.color }}>
                  <input
                    type="color"
                    {...mouseOnlyColorInputProps}
                    className="outline-color-input"
                    value={dropShadowSettings.color}
                    disabled={selectedPageEditLocked}
                    onChange={(event) => updateDropShadowSettings({ color: event.target.value })}
                  />
                </span>
              </label>
              <label className="compact-tool-field font-range-field">
                <span>
                  <span>불투명도</span>
                  <strong>{Math.round(dropShadowSettings.opacity * 100)}%</strong>
                </span>
                <input
                  type="range"
                  {...mouseOnlyRangeInputProps}
                  min={0.05}
                  max={1}
                  step={0.01}
                  value={dropShadowSettings.opacity}
                  style={rangeProgressStyle(dropShadowSettings.opacity, 0.05, 1)}
                  disabled={selectedPageEditLocked}
                  onChange={(event) => updateDropShadowSettings({ opacity: Number(event.target.value) })}
                />
              </label>
            </div>
            <div className="font-shadow-row font-tool-grid group-effect-offset-row">
              <label className="compact-tool-field font-number-field">
                <span>흐림</span>
                <CompactNumberControl
                  ariaLabel="그룹 그림자 흐림"
                  min={0}
                  max={80}
                  step={0.5}
                  value={dropShadowSettings.blurPx}
                  suffix="px"
                  disabled={selectedPageEditLocked}
                  onChange={(blurPx) => updateDropShadowSettings({ blurPx })}
                />
              </label>
              <label className="compact-tool-field font-number-field">
                <span>X 이동</span>
                <CompactNumberControl
                  ariaLabel="그룹 그림자 X 이동"
                  min={-160}
                  max={160}
                  step={0.5}
                  value={dropShadowSettings.offsetX}
                  suffix="px"
                  disabled={selectedPageEditLocked}
                  onChange={(offsetX) => updateDropShadowSettings({ offsetX })}
                />
              </label>
              <label className="compact-tool-field font-number-field">
                <span>Y 이동</span>
                <CompactNumberControl
                  ariaLabel="그룹 그림자 Y 이동"
                  min={-160}
                  max={160}
                  step={0.5}
                  value={dropShadowSettings.offsetY}
                  suffix="px"
                  disabled={selectedPageEditLocked}
                  onChange={(offsetY) => updateDropShadowSettings({ offsetY })}
                />
              </label>
            </div>
          </>
        ) : null}
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
