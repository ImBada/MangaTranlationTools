import React from "react";
import type { JobState, MangaPage, TranslationBlock } from "../../../../shared/types";
import type { RecoverableFailure, RecoverableFailureId } from "../../hooks/useRecoverableFailures";
import {
  resolveCurrentTranslationBlockSelection,
  resolveToggledTranslationBlockIds,
  resolveTranslationBlockRangeSelection
} from "../../lib/blockSelection";
import {
  resolveTranslationBlockGroupBlockIds,
  resolveTranslationBlockListItems
} from "../../lib/blockGroups";
import type { StatusToastTone } from "../../hooks/useStatusFeedback";
import type { ActiveLayer } from "../../lib/layerState";

export type PageInpaintNotice = {
  actionLabel?: string;
  message: string;
  onAction?: () => void | Promise<void>;
  title: string;
  tone: string;
};

type NotificationDockProps = {
  inpaintNotice: PageInpaintNotice | null;
  onDismissRecoverableFailure: (id: RecoverableFailureId) => void;
  onRetryRecoverableFailure: (id: RecoverableFailureId) => void | Promise<void>;
  recoverableFailures: RecoverableFailure[];
  statusToastLine: string | null;
  statusToastTone: StatusToastTone;
  statusWidgetTone: string;
};

type StatusHistoryPanelProps = {
  jobState: JobState;
  onClose: () => void;
  statusLines: string[];
  statusWidgetTone: string;
};

type StageZoomOverlayProps = {
  label: string;
  onFit: () => void;
  onOriginalSize: () => void;
  onZoomIn: () => void;
  onZoomOut: () => void;
};

type StageToolOverlayProps = {
  activeLayer: ActiveLayer;
  blockInlineEditShortcutActive: boolean;
  blockInlineEditShortcut: string;
  blockInlineEditShortcutVisible: boolean;
  colorPickerActive: boolean;
  colorPickerShortcut: string;
  colorPickerVisible: boolean;
  pointerToolActive: boolean;
  rangeShortcut: string;
  rangeToolActive: boolean;
  selectedPageEditLocked: boolean;
  zoomToolActive: boolean;
  onSelectPointerTool: () => void;
  onSelectResultColorPicker: () => void;
  onSelectRangeTool: () => void;
  onSelectZoomTool: () => void;
};

type StageTextBlockListProps = {
  collapsed: boolean;
  groupAction: "group" | "ungroup" | null;
  groupDisabled: boolean;
  onBlockSelectionChange: (blockIds: string[]) => void;
  onGroupSelectedBlocks: () => void;
  onSelectBlock: (blockId: string) => void;
  onToggleCollapsed: () => void;
  onUngroupSelectedBlocks: () => void;
  page: MangaPage;
  selectedBlockId: string | null;
  selectedBlockIds: string[];
};

const BLOCK_TYPE_LABELS: Record<TranslationBlock["type"], string> = {
  caption: "자막",
  other: "기타",
  sfx: "효과음",
  speech: "말풍선"
};

export function NotificationDock({
  inpaintNotice,
  onDismissRecoverableFailure,
  onRetryRecoverableFailure,
  recoverableFailures,
  statusToastLine,
  statusToastTone,
  statusWidgetTone
}: NotificationDockProps): React.JSX.Element | null {
  if (!inpaintNotice && recoverableFailures.length === 0 && !statusToastLine) {
    return null;
  }

  return (
    <aside className="notification-dock" aria-label="알림" aria-live="polite">
      {inpaintNotice ? (
        <section className={`notification-card ${inpaintNotice.tone}`}>
          <div className="notification-copy">
            <strong>{inpaintNotice.title}</strong>
            <span>{inpaintNotice.message}</span>
          </div>
          {inpaintNotice.onAction ? (
            <button type="button" className="notification-action" onClick={() => void inpaintNotice.onAction?.()}>
              {inpaintNotice.actionLabel ?? "재시도"}
            </button>
          ) : null}
        </section>
      ) : null}
      {recoverableFailures.map((failure) => (
        <section className="notification-card failed" key={failure.id}>
          <div className="notification-copy">
            <strong>{failure.title}</strong>
            <span>{failure.message}</span>
          </div>
          <div className="notification-actions">
            <button type="button" className="notification-action" onClick={() => void onRetryRecoverableFailure(failure.id)}>
              재시도
            </button>
            <button type="button" className="notification-dismiss" onClick={() => onDismissRecoverableFailure(failure.id)} aria-label={`${failure.title} 닫기`}>
              닫기
            </button>
          </div>
        </section>
      ))}
      {statusToastLine ? (
        <section className={`notification-card ${statusToastTone === "default" ? statusWidgetTone : statusToastTone}`}>
          <div className="notification-copy">
            <strong>알림</strong>
            <span>{statusToastLine}</span>
          </div>
        </section>
      ) : null}
    </aside>
  );
}

export function StageTextBlockList({
  collapsed,
  groupAction,
  groupDisabled,
  onBlockSelectionChange,
  onGroupSelectedBlocks,
  onSelectBlock,
  onToggleCollapsed,
  onUngroupSelectedBlocks,
  page,
  selectedBlockId,
  selectedBlockIds
}: StageTextBlockListProps): React.JSX.Element {
  const selectedBlockIdSet = React.useMemo(() => new Set(selectedBlockIds), [selectedBlockIds]);
  const blockListItems = React.useMemo(() => resolveTranslationBlockListItems(page), [page]);
  const activeRowRef = React.useRef<HTMLButtonElement | null>(null);
  const listId = React.useId();
  const selectionAnchorIdRef = React.useRef<string | null>(null);

  React.useEffect(() => {
    if (collapsed) {
      return;
    }
    activeRowRef.current?.scrollIntoView({ block: "nearest" });
  }, [collapsed, page.id, selectedBlockId, selectedBlockIds]);

  React.useEffect(() => {
    if (selectedBlockIds.length > 1) {
      return;
    }
    selectionAnchorIdRef.current = selectedBlockId;
  }, [selectedBlockId, selectedBlockIds.length]);

  const selectBlock = React.useCallback((event: React.MouseEvent<HTMLButtonElement>, blockId: string) => {
    const blockIdsInOrder = page.blocks.map((block) => block.id);
    const groupBlockIds = resolveTranslationBlockGroupBlockIds(page, blockId);
    const selectedUnitBlockIds = groupBlockIds ?? [blockId];
    const anchorBlockId =
      selectionAnchorIdRef.current && blockIdsInOrder.includes(selectionAnchorIdRef.current)
        ? selectionAnchorIdRef.current
        : selectedBlockId && blockIdsInOrder.includes(selectedBlockId)
          ? selectedBlockId
          : null;

    if (event.shiftKey) {
      const rangeBlockIds = resolveTranslationBlockRangeSelection(blockIdsInOrder, anchorBlockId, blockId);
      if (rangeBlockIds.length === 0) {
        return;
      }
      const nextBlockIds = event.ctrlKey || event.metaKey
        ? Array.from(new Set([...resolveCurrentTranslationBlockSelection(selectedBlockId, selectedBlockIds), ...rangeBlockIds]))
        : rangeBlockIds;
      selectionAnchorIdRef.current = anchorBlockId ?? blockId;
      onBlockSelectionChange(nextBlockIds);
      return;
    }

    if (event.ctrlKey || event.metaKey) {
      selectionAnchorIdRef.current = blockId;
      onBlockSelectionChange(resolveToggledTranslationBlockIds(selectedBlockId, selectedBlockIds, selectedUnitBlockIds));
      return;
    }

    selectionAnchorIdRef.current = blockId;
    onSelectBlock(blockId);
  }, [onBlockSelectionChange, onSelectBlock, page, selectedBlockId, selectedBlockIds]);

  const renderBlockRow = React.useCallback((block: TranslationBlock, index: number, grouped = false) => {
    const selected = block.id === selectedBlockId || selectedBlockIdSet.has(block.id);
    return (
      <button
        key={block.id}
        ref={block.id === selectedBlockId ? activeRowRef : undefined}
        type="button"
        className={`stage-text-block-list-row${grouped ? " grouped" : ""}${selected ? " selected" : ""}${block.renderDirection === "hidden" ? " hidden" : ""}`}
        aria-pressed={selected}
        onClick={(event) => selectBlock(event, block.id)}
      >
        <span className="stage-text-block-list-index">{index + 1}</span>
        <span className="stage-text-block-list-copy">
          <span className="stage-text-block-list-meta">
            <span>{BLOCK_TYPE_LABELS[block.type]}</span>
            {block.renderDirection === "hidden" ? <span>숨김</span> : null}
          </span>
          <span className="stage-text-block-list-text">{resolveBlockPreviewText(block)}</span>
        </span>
      </button>
    );
  }, [selectBlock, selectedBlockId, selectedBlockIdSet]);
  const groupActionConfig = groupAction === "ungroup"
    ? {
        ariaLabel: "선택한 텍스트 블록 그룹 해제",
        disabledTitle: "잠긴 페이지에서는 그룹을 해제할 수 없음",
        icon: "ungroup" as const,
        onClick: onUngroupSelectedBlocks,
        title: "선택한 텍스트 블록 그룹 해제"
      }
    : groupAction === "group"
      ? {
          ariaLabel: "선택한 텍스트 블록 그룹화",
          disabledTitle: "잠긴 페이지에서는 그룹화할 수 없음",
          icon: "group" as const,
          onClick: onGroupSelectedBlocks,
          title: "선택한 텍스트 블록 그룹화"
        }
      : null;

  return (
    <>
      {groupActionConfig ? (
        <button
          type="button"
          className="stage-text-block-group-button"
          aria-label={groupActionConfig.ariaLabel}
          title={groupDisabled ? groupActionConfig.disabledTitle : groupActionConfig.title}
          disabled={groupDisabled}
          onClick={groupActionConfig.onClick}
        >
          <svg className="stage-text-block-group-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
            <rect x="4" y="5" width="7" height="7" rx="1.5" />
            <rect x="13" y="12" width="7" height="7" rx="1.5" />
            {groupActionConfig.icon === "group" ? (
              <>
                <path d="M11 8.5h2.5a3 3 0 0 1 3 3v.5" />
                <path d="M13 15.5h-2.5a3 3 0 0 1-3-3V12" />
              </>
            ) : (
              <>
                <path d="M12 8.5h1.5a3 3 0 0 1 2.2.96" />
                <path d="M12 15.5h-1.5a3 3 0 0 1-2.2-.96" />
                <path d="M6 18l12-12" />
              </>
            )}
          </svg>
        </button>
      ) : null}
      <section className={`stage-text-block-list${collapsed ? " collapsed" : ""}`} aria-label={`${page.name} 텍스트 블록 목록`}>
        <div className="stage-text-block-list-header">
          <h2>텍스트 블록</h2>
          <div className="stage-text-block-list-header-actions">
            <span>{page.blocks.length}</span>
            <button
              type="button"
              className="stage-text-block-list-toggle"
              aria-controls={listId}
              aria-expanded={!collapsed}
              aria-label={`텍스트 블록 목록 ${collapsed ? "펼치기" : "접기"}`}
              title={collapsed ? "펼치기" : "접기"}
              onClick={onToggleCollapsed}
            >
              <svg className="stage-text-block-list-toggle-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
                <path d="M6 15l6-6 6 6" />
              </svg>
            </button>
          </div>
        </div>
        <div id={listId} className="stage-text-block-list-scroll" hidden={collapsed}>
          {blockListItems.length ? (
            blockListItems.map((item) => {
              if (item.kind === "block") {
                return renderBlockRow(item.block, item.blockIndex);
              }

              const groupSelected = item.blocks.some(({ block }) => block.id === selectedBlockId || selectedBlockIdSet.has(block.id));
              const firstBlockId = item.blocks[0]?.block.id;
              return (
                <div key={item.group.id} className={`stage-text-block-list-group${groupSelected ? " selected" : ""}`}>
                  <button
                    type="button"
                    className="stage-text-block-list-group-header"
                    aria-pressed={groupSelected}
                    onClick={(event) => firstBlockId ? selectBlock(event, firstBlockId) : undefined}
                  >
                    <span className="stage-text-block-list-group-title">그룹</span>
                    <span className="stage-text-block-list-group-count">{item.blocks.length}</span>
                  </button>
                  <div className="stage-text-block-list-group-items">
                    {item.blocks.map(({ block, blockIndex }) => renderBlockRow(block, blockIndex, true))}
                  </div>
                </div>
              );
            })
          ) : (
            <p className="stage-text-block-list-empty">텍스트 블록 없음</p>
          )}
        </div>
      </section>
    </>
  );
}

export function StatusHistoryPanel({
  jobState,
  onClose,
  statusLines,
  statusWidgetTone
}: StatusHistoryPanelProps): React.JSX.Element {
  return (
    <section className={`status-history-panel ${statusWidgetTone}`}>
      <div className="notification-panel-header">
        <h2>상태 기록</h2>
        <button type="button" className="notification-panel-close" onClick={onClose} aria-label="상태 기록 닫기">
          ×
        </button>
      </div>
      <div className={`job-pill ${jobState.status}`}>{jobState.progressText}</div>
      <div className="status-log-scroll">
        {statusLines.length ? (
          statusLines.map((line, index) => <p key={`${line}-${index}`}>{line}</p>)
        ) : (
          <p className="muted-line">아직 표시할 상태가 없습니다.</p>
        )}
      </div>
    </section>
  );
}

function resolveBlockPreviewText(block: TranslationBlock): string {
  const text = (block.translatedText || block.sourceText).replace(/\s+/g, " ").trim();
  return text || "빈 블록";
}

export function StageZoomOverlay({
  label,
  onFit,
  onOriginalSize,
  onZoomIn,
  onZoomOut
}: StageZoomOverlayProps): React.JSX.Element {
  return (
    <div className="stage-zoom-overlay" aria-label="만화 확대/축소">
      <button type="button" onClick={onZoomOut} aria-label="축소" title="축소">
        -
      </button>
      <span className="stage-zoom-value" aria-live="polite">{label}</span>
      <button type="button" onClick={onZoomIn} aria-label="확대" title="확대">
        +
      </button>
      <button type="button" onClick={onOriginalSize} title="원본 사이즈 보기">
        원본
      </button>
      <button type="button" onClick={onFit} title="화면에 맞춤">
        맞춤
      </button>
    </div>
  );
}

export function StageToolOverlay({
  activeLayer,
  blockInlineEditShortcutActive,
  blockInlineEditShortcut,
  blockInlineEditShortcutVisible,
  colorPickerActive,
  colorPickerShortcut,
  colorPickerVisible,
  pointerToolActive,
  rangeShortcut,
  rangeToolActive,
  selectedPageEditLocked,
  zoomToolActive,
  onSelectPointerTool,
  onSelectResultColorPicker,
  onSelectRangeTool,
  onSelectZoomTool
}: StageToolOverlayProps): React.JSX.Element {
  const showPointerToolHints = activeLayer === "overlay" && pointerToolActive;

  return (
    <>
      <div className="stage-tool-overlay" aria-label="전역 도구">
        <button
          type="button"
          className={pointerToolActive ? "active" : ""}
          aria-label="일반 마우스"
          aria-pressed={pointerToolActive}
          aria-keyshortcuts="A"
          title="일반 마우스 (A)"
          onClick={onSelectPointerTool}
        >
          <svg className="stage-tool-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
            <path d="M6 3l12 11-6.2 1.1L8.3 21 6 3z" />
          </svg>
          <span className="stage-tool-shortcut" aria-hidden="true">A</span>
        </button>
        <button
          type="button"
          className={rangeToolActive ? "active" : ""}
          aria-label="범위 선택"
          aria-pressed={rangeToolActive}
          aria-keyshortcuts={rangeShortcut}
          title={`범위 선택 (${rangeShortcut})`}
          onClick={onSelectRangeTool}
          disabled={selectedPageEditLocked}
        >
          <svg className="stage-tool-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
            <rect x="4" y="4" width="11" height="11" rx="2" strokeDasharray="2.4 2.4" />
            <path d="M13 12l6 6-3 1-1 3-6-6 4-4z" />
          </svg>
          <span className="stage-tool-shortcut" aria-hidden="true">{rangeShortcut}</span>
        </button>
        <button
          type="button"
          className={zoomToolActive ? "active" : ""}
          aria-label="줌 도구"
          aria-pressed={zoomToolActive}
          aria-keyshortcuts="Z"
          title="줌 도구 (Z)"
          onClick={onSelectZoomTool}
        >
          <svg className="stage-tool-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
            <circle cx="10.5" cy="10.5" r="5.8" />
            <path d="M15 15l5 5" />
            <path d="M10.5 7.5v6" />
            <path d="M7.5 10.5h6" />
          </svg>
          <span className="stage-tool-shortcut" aria-hidden="true">Z</span>
        </button>
        {blockInlineEditShortcutVisible ? (
          <div
            className={`stage-tool-shortcut-guide${blockInlineEditShortcutActive ? " active" : ""}`}
            aria-label={`선택 블록 텍스트 수정 (${blockInlineEditShortcut})`}
            aria-keyshortcuts={blockInlineEditShortcut}
            title={`선택 블록 텍스트 수정 (${blockInlineEditShortcut})`}
          >
            <svg className="stage-tool-icon stage-tool-text-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
              <path d="M5 5h14" />
              <path d="M12 5v14" />
              <path d="M8.5 19h7" />
            </svg>
            <span className="stage-tool-shortcut" aria-hidden="true">{blockInlineEditShortcut}</span>
          </div>
        ) : null}
        {colorPickerVisible ? (
          <button
            type="button"
            className={`stage-layer-tool-button${colorPickerActive ? " active" : ""}`}
            aria-label="결과 레이어 컬러 피커"
            aria-pressed={colorPickerActive}
            aria-keyshortcuts={colorPickerShortcut}
            title={`결과 레이어 컬러 피커 (${colorPickerShortcut})`}
            onClick={onSelectResultColorPicker}
            disabled={selectedPageEditLocked}
          >
            <svg className="stage-tool-icon stage-tool-eyedropper-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
              <path d="M20.7 5.6 18.4 3.3c-.4-.4-1-.4-1.4 0l-3.1 3.1-1.9-1.9-1.4 1.4 1.4 1.4L3 16.3V21h4.7l8.9-8.9 1.4 1.4 1.4-1.4-1.9-1.9 3.1-3.1c.5-.5.5-1.1.1-1.5zM6.9 19H5v-1.9L13.1 9l1.9 1.9L6.9 19z" />
            </svg>
            <span className="stage-tool-shortcut" aria-hidden="true">{colorPickerShortcut}</span>
          </button>
        ) : null}
      </div>
      {showPointerToolHints ? (
        <div className="stage-tool-hints" aria-label="선택 도구 보조키 안내">
          <span>Alt+드래그: 선택 블록 범위 변경</span>
          <span>Ctrl(맥 Cmd)+클릭: 블록 복제</span>
        </div>
      ) : null}
    </>
  );
}
