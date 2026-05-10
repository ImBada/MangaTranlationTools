import React from "react";
import type { ChapterSnapshot } from "../../../../shared/types";
import type { ResultReportProgress } from "../../hooks/useResultReport";

type OutputToolSectionProps = {
  canOpenLastResultReport: boolean;
  currentChapter: ChapterSnapshot | null;
  jobActive: boolean;
  reportBusy: boolean;
  reportProgress: ResultReportProgress | null;
  renderBusy: boolean;
  onGenerateResultReport: () => void | Promise<void>;
  onOpenLastResultReport: () => void;
};

export function OutputToolSection({
  canOpenLastResultReport,
  currentChapter,
  jobActive,
  reportBusy,
  reportProgress,
  renderBusy,
  onGenerateResultReport,
  onOpenLastResultReport
}: OutputToolSectionProps): React.JSX.Element {
  const blockCount = currentChapter?.pages.reduce((total, page) => total + page.blocks.length, 0) ?? 0;
  const disabled = !currentChapter || blockCount === 0 || jobActive || renderBusy || reportBusy;
  const progressRatio = reportProgress && reportProgress.total > 0
    ? Math.min(1, Math.max(0, reportProgress.current / reportProgress.total))
    : 0;

  return (
    <>
      <div className="result-action-grid output-action-grid">
        <button type="button" onClick={() => void onGenerateResultReport()} disabled={disabled}>
          {reportBusy ? "보고서 생성 중" : "결과 보고서 생성"}
        </button>
        <button type="button" onClick={onOpenLastResultReport} disabled={!canOpenLastResultReport || reportBusy}>
          마지막 보고서 확인
        </button>
      </div>
      {reportProgress ? (
        <div className="output-report-progress" aria-label="결과 보고서 생성 진행도">
          <div className="output-report-progress-meta">
            <span>{reportProgress.label}</span>
            <strong>
              {reportProgress.current} / {reportProgress.total}
            </strong>
          </div>
          <div className="progress-track" aria-hidden="true">
            <div className="progress-fill" style={{ width: `${Math.round(progressRatio * 100)}%` }} />
          </div>
        </div>
      ) : null}
      <p className="tool-helper-line">
        현재 화의 텍스트 블록별 원본 범위, 최종 아웃풋, 폰트 프리셋과 설정을 정렬 가능한 보고서 페이지로 엽니다.
      </p>
    </>
  );
}
