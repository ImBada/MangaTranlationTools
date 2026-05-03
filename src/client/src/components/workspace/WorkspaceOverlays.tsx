import React from "react";
import type { JobState } from "../../../../shared/types";
import type { RecoverableFailure, RecoverableFailureId } from "../../hooks/useRecoverableFailures";
import type { StatusToastTone } from "../../hooks/useStatusFeedback";

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
  rangeShortcut: string;
  rangeToolActive: boolean;
  selectedPageEditLocked: boolean;
  zoomToolActive: boolean;
  onSelectPointerTool: () => void;
  onSelectRangeTool: () => void;
  onSelectZoomTool: () => void;
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
  rangeShortcut,
  rangeToolActive,
  selectedPageEditLocked,
  zoomToolActive,
  onSelectPointerTool,
  onSelectRangeTool,
  onSelectZoomTool
}: StageToolOverlayProps): React.JSX.Element {
  return (
    <div className="stage-tool-overlay" aria-label="전역 도구">
      <button
        type="button"
        className={!zoomToolActive && !rangeToolActive ? "active" : ""}
        aria-label="일반 마우스"
        aria-pressed={!zoomToolActive && !rangeToolActive}
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
    </div>
  );
}
