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

const GROUP_BLOCK_DRAG_DATA_TYPE = "application/x-manga-translation-group-block-id";

type TranslationBlockGroupToolSectionProps = {
  selectedBlockGroup: TranslationBlockGroup;
  selectedPage: MangaPage | null;
  selectedPageEditLocked: boolean;
  onEffectsChange: (effects: TranslationBlockGroupEffect[]) => void;
  onBlockOrderChange: (blockIds: string[]) => void;
  onSelectBlock: (blockId: string) => void;
};

export function TranslationBlockGroupToolSection({
  selectedBlockGroup,
  selectedPage,
  selectedPageEditLocked,
  onEffectsChange,
  onBlockOrderChange,
  onSelectBlock
}: TranslationBlockGroupToolSectionProps): React.JSX.Element {
  const dropShadowEffect = resolveTranslationBlockGroupDropShadowEffect(selectedBlockGroup);
  const dropShadowSettings = resolveTranslationBlockGroupDropShadowSettings(dropShadowEffect);
  const dropShadowActive = Boolean(dropShadowEffect?.enabled);
  const groupBlocks = React.useMemo(() => resolveGroupBlocks(selectedPage, selectedBlockGroup), [selectedBlockGroup, selectedPage]);
  const [draggingBlockId, setDraggingBlockId] = React.useState<string | null>(null);
  const [dragOverTarget, setDragOverTarget] = React.useState<{ blockId: string; position: "before" | "after" } | null>(null);
  const draggingBlockIdRef = React.useRef<string | null>(null);
  const suppressClickAfterDragRef = React.useRef(false);
  const groupBlockIds = React.useMemo(() => groupBlocks.map(({ block }) => block.id), [groupBlocks]);
  const reorderEnabled = !selectedPageEditLocked && groupBlocks.length > 1;

  const setDropShadowEnabled = React.useCallback((enabled: boolean) => {
    if (dropShadowActive === enabled) {
      return;
    }
    onEffectsChange(setTranslationBlockGroupDropShadowEnabled(selectedBlockGroup.effects, enabled));
  }, [dropShadowActive, onEffectsChange, selectedBlockGroup.effects]);

  const updateDropShadowSettings = React.useCallback((patch: Partial<TranslationBlockGroupDropShadowSettings>) => {
    onEffectsChange(updateTranslationBlockGroupDropShadowSettings(selectedBlockGroup.effects, patch));
  }, [onEffectsChange, selectedBlockGroup.effects]);

  const clearDragState = React.useCallback(() => {
    draggingBlockIdRef.current = null;
    setDraggingBlockId(null);
    setDragOverTarget(null);
  }, []);

  const handleBlockClick = React.useCallback((blockId: string) => {
    if (suppressClickAfterDragRef.current) {
      suppressClickAfterDragRef.current = false;
      return;
    }
    onSelectBlock(blockId);
  }, [onSelectBlock]);

  const handleDragStart = React.useCallback((event: React.DragEvent<HTMLButtonElement>, blockId: string) => {
    if (!reorderEnabled) {
      event.preventDefault();
      return;
    }

    draggingBlockIdRef.current = blockId;
    setDraggingBlockId(blockId);
    event.dataTransfer.effectAllowed = "move";
    event.dataTransfer.setData(GROUP_BLOCK_DRAG_DATA_TYPE, blockId);
  }, [reorderEnabled]);

  const handleDragOver = React.useCallback((event: React.DragEvent<HTMLButtonElement>, blockId: string) => {
    const sourceBlockId = draggingBlockIdRef.current || event.dataTransfer.getData(GROUP_BLOCK_DRAG_DATA_TYPE);
    if (!reorderEnabled || !sourceBlockId || sourceBlockId === blockId) {
      return;
    }

    event.preventDefault();
    event.dataTransfer.dropEffect = "move";
    const rect = event.currentTarget.getBoundingClientRect();
    const position = event.clientY > rect.top + rect.height / 2 ? "after" : "before";
    setDragOverTarget((current) =>
      current?.blockId === blockId && current.position === position ? current : { blockId, position }
    );
  }, [reorderEnabled]);

  const handleDrop = React.useCallback((event: React.DragEvent<HTMLButtonElement>, targetBlockId: string) => {
    const sourceBlockId = draggingBlockIdRef.current || event.dataTransfer.getData(GROUP_BLOCK_DRAG_DATA_TYPE);
    const dropPosition = dragOverTarget?.blockId === targetBlockId ? dragOverTarget.position : null;
    if (!reorderEnabled || !sourceBlockId || sourceBlockId === targetBlockId || !dropPosition) {
      clearDragState();
      return;
    }

    event.preventDefault();
    const nextBlockIds = reorderBlockIds(groupBlockIds, sourceBlockId, targetBlockId, dropPosition);
    clearDragState();
    suppressClickAfterDragRef.current = true;
    window.setTimeout(() => {
      suppressClickAfterDragRef.current = false;
    }, 0);
    if (nextBlockIds.some((blockId, index) => blockId !== groupBlockIds[index])) {
      onBlockOrderChange(nextBlockIds);
    }
  }, [clearDragState, dragOverTarget, groupBlockIds, onBlockOrderChange, reorderEnabled]);

  const handleDragEnd = React.useCallback(() => {
    clearDragState();
    suppressClickAfterDragRef.current = true;
    window.setTimeout(() => {
      suppressClickAfterDragRef.current = false;
    }, 0);
  }, [clearDragState]);

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
            {groupBlocks.map(({ block, blockIndex }, orderIndex) => (
              <button
                key={block.id}
                type="button"
                className={`stage-text-block-list-row grouped${reorderEnabled ? " reorderable" : ""}${block.renderDirection === "hidden" ? " hidden" : ""}${draggingBlockId === block.id ? " dragging" : ""}${dragOverTarget?.blockId === block.id ? ` drag-over-${dragOverTarget.position}` : ""}`}
                draggable={reorderEnabled}
                onClick={() => handleBlockClick(block.id)}
                onDragEnd={handleDragEnd}
                onDragOver={(event) => handleDragOver(event, block.id)}
                onDragStart={(event) => handleDragStart(event, block.id)}
                onDrop={(event) => handleDrop(event, block.id)}
                aria-label={`그룹 순서 ${orderIndex + 1}, 블록 ${blockIndex + 1} 개별 설정`}
                title={reorderEnabled ? "드래그해서 순서 변경, 클릭해서 개별 설정" : "개별 설정"}
              >
                <span className="stage-text-block-list-drag-handle" aria-hidden="true">
                  <svg viewBox="0 0 12 18" focusable="false">
                    <circle cx="4" cy="4" r="1.2" />
                    <circle cx="8" cy="4" r="1.2" />
                    <circle cx="4" cy="9" r="1.2" />
                    <circle cx="8" cy="9" r="1.2" />
                    <circle cx="4" cy="14" r="1.2" />
                    <circle cx="8" cy="14" r="1.2" />
                  </svg>
                </span>
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

function reorderBlockIds(
  blockIds: readonly string[],
  sourceBlockId: string,
  targetBlockId: string,
  position: "before" | "after"
): string[] {
  const nextBlockIds = blockIds.filter((blockId) => blockId !== sourceBlockId);
  const targetIndex = nextBlockIds.indexOf(targetBlockId);
  if (targetIndex < 0) {
    return [...blockIds];
  }

  nextBlockIds.splice(position === "after" ? targetIndex + 1 : targetIndex, 0, sourceBlockId);
  return nextBlockIds;
}
