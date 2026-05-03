import React from "react";
import type { ChapterSnapshot, JobState, LibraryIndex, MangaPage } from "../../../shared/types";
import type { ProgressSnapshot } from "../lib/jobProgress";
import { LibraryTree } from "./LibraryTree";

type ImportMode = "images" | "folder" | "zip" | "zip-folder";
type AnalysisRunMode = "pending" | "all" | "single-page";
type RenderProgress = {
  mode: "page" | "all";
  current: number;
  total: number;
};

type ContextBarProps = {
  currentChapter: ChapterSnapshot | null;
  currentChapterId: string | null;
  jobActive: boolean;
  jobState: JobState;
  library: LibraryIndex;
  libraryAnchorRef: React.RefObject<HTMLDivElement | null>;
  libraryWidgetOpen: boolean;
  progressSnapshot: ProgressSnapshot | null;
  renderBusy: boolean;
  renderProgress: RenderProgress | null;
  selectedPage: MangaPage | null;
  settingsBusy: boolean;
  settingsOpen: boolean;
  showProgressBar: boolean;
  statusIndicatorLabel: string;
  statusWidgetTone: string;
  inpaintBusy: boolean;
  onApplyInpaintAllPages: () => void | Promise<void>;
  onOpenChapter: (chapterId: string) => void | Promise<void>;
  onOpenSettings: () => void | Promise<void>;
  onRenameChapter: (chapterId: string) => void | Promise<void>;
  onRenameWork: (workId: string) => void | Promise<void>;
  onRenderAllPages: () => void | Promise<void>;
  onRenderSelectedPage: () => void | Promise<void>;
  onReorderChapter: (workId: string, sourceChapterId: string, targetChapterId: string) => void | Promise<void>;
  onRunAnalysis: (runMode: AnalysisRunMode) => void | Promise<void>;
  onSelectImportFiles: (mode: ImportMode) => void;
  onSetLibraryWidgetOpen: React.Dispatch<React.SetStateAction<boolean>>;
};

export function ContextBar({
  currentChapter,
  currentChapterId,
  jobActive,
  jobState,
  library,
  libraryAnchorRef,
  libraryWidgetOpen,
  progressSnapshot,
  renderBusy,
  renderProgress,
  selectedPage,
  settingsBusy,
  settingsOpen,
  showProgressBar,
  statusIndicatorLabel,
  statusWidgetTone,
  inpaintBusy,
  onApplyInpaintAllPages,
  onOpenChapter,
  onOpenSettings,
  onRenameChapter,
  onRenameWork,
  onRenderAllPages,
  onRenderSelectedPage,
  onReorderChapter,
  onRunAnalysis,
  onSelectImportFiles,
  onSetLibraryWidgetOpen
}: ContextBarProps): React.JSX.Element {
  const libraryChapterCount = React.useMemo(() => library.works.reduce((total, work) => total + work.chapters.length, 0), [library.works]);
  const batchInpaintDisabled =
    !currentChapter ||
    currentChapter.pages.every((page) => page.blocks.length === 0 || page.progressCompleted) ||
    jobActive ||
    inpaintBusy;

  return (
    <header className="context-bar">
      <div className="context-bar-left">
        <div className="context-library-anchor" ref={libraryAnchorRef}>
          <button
            type="button"
            className={libraryWidgetOpen ? "context-button active" : "context-button"}
            onClick={() => onSetLibraryWidgetOpen((current) => !current)}
            aria-expanded={libraryWidgetOpen}
            aria-controls="library-widget"
          >
            보관함
            <span className="panel-count">{libraryChapterCount}</span>
          </button>
          {libraryWidgetOpen ? (
            <div id="library-widget" className="library-widget">
              <LibraryTree
                library={library}
                currentChapterId={currentChapterId}
                jobActive={jobActive}
                collapsed={false}
                onToggleCollapsed={() => onSetLibraryWidgetOpen(false)}
                onOpenChapter={(chapterId) => {
                  onSetLibraryWidgetOpen(false);
                  void onOpenChapter(chapterId);
                }}
                onRenameWork={(workId) => {
                  onSetLibraryWidgetOpen(false);
                  void onRenameWork(workId);
                }}
                onRenameChapter={(chapterId) => {
                  onSetLibraryWidgetOpen(false);
                  void onRenameChapter(chapterId);
                }}
                onReorderChapter={(workId, sourceChapterId, targetChapterId) => {
                  void onReorderChapter(workId, sourceChapterId, targetChapterId);
                }}
              />
            </div>
          ) : null}
        </div>
        <div className="context-import-group">
          <span className="context-label">가져오기</span>
          <div className="import-actions grid grid-cols-4 gap-1.5">
            <button onClick={() => onSelectImportFiles("images")} disabled={jobActive}>
              이미지
            </button>
            <button onClick={() => onSelectImportFiles("folder")} disabled={jobActive}>
              폴더
            </button>
            <button onClick={() => onSelectImportFiles("zip")} disabled={jobActive}>
              압축파일
            </button>
            <button onClick={() => onSelectImportFiles("zip-folder")} disabled={jobActive}>
              일괄 번역
            </button>
          </div>
        </div>
      </div>
      <div className="context-bar-right">
        <div className="context-chapter-chip" title={currentChapter?.title ?? "현재 화 없음"}>
          <strong>{currentChapter?.title ?? "현재 화 없음"}</strong>
          <span>{currentChapter ? `${currentChapter.pages.length}p` : "대기"}</span>
        </div>
        <div className="context-run-group" aria-label="번역 실행">
          <div className="context-run-actions">
            <button className="primary" onClick={() => void onRunAnalysis("pending")} disabled={!currentChapter || jobActive}>
              계속 번역 (AI)
            </button>
            <button onClick={() => void onRunAnalysis("all")} disabled={!currentChapter || jobActive}>
              전체 번역 (AI)
            </button>
            <button onClick={() => void onApplyInpaintAllPages()} disabled={batchInpaintDisabled}>
              {inpaintBusy ? "전체 인페인트 중" : "전체 인페인트"}
            </button>
            <button onClick={() => void onRenderSelectedPage()} disabled={!currentChapter || !selectedPage || jobActive || renderBusy}>
              {renderProgress?.mode === "page" ? "출력 중" : "페이지 출력"}
            </button>
            <button onClick={() => void onRenderAllPages()} disabled={!currentChapter || currentChapter.pages.length === 0 || jobActive || renderBusy}>
              {renderProgress?.mode === "all" ? `전체 출력 ${renderProgress.current}/${renderProgress.total}` : "전체 페이지 출력"}
            </button>
            {jobActive ? (
              <button className="danger" onClick={() => void window.mangaApi.cancelJob()}>
                취소
              </button>
            ) : null}
          </div>
          {showProgressBar && progressSnapshot ? (
            <div className="context-progress">
              <div className="context-progress-meta">
                <span>{jobState.progressText}</span>
                {progressSnapshot.mode === "determinate" ? (
                  <strong>
                    {progressSnapshot.current} / {progressSnapshot.total}
                  </strong>
                ) : (
                  <strong>준비 중</strong>
                )}
              </div>
              <div className={`progress-track ${progressSnapshot.mode === "indeterminate" ? "indeterminate" : ""}`} aria-hidden="true">
                <div
                  className={`progress-fill ${progressSnapshot.mode === "indeterminate" ? "indeterminate" : ""}`}
                  style={
                    progressSnapshot.mode === "determinate"
                      ? { width: `${Math.round(progressSnapshot.ratio * 100)}%` }
                      : undefined
                  }
                />
              </div>
            </div>
          ) : null}
        </div>
        <span className={`context-status-indicator ${statusWidgetTone}`} aria-label={`상태: ${statusIndicatorLabel}`} title={statusIndicatorLabel} />
        <button className="ghost-button context-settings" onClick={() => void onOpenSettings()} disabled={settingsBusy && !settingsOpen}>
          설정
        </button>
      </div>
    </header>
  );
}
