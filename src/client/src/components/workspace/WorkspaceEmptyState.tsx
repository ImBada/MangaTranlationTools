import React from "react";
import type { LamaRuntimeStatus } from "../../../../shared/types";
import { LAMA_TEST_PLATFORM_OPTIONS, type LamaNoticePlatform } from "../../lib/lamaRuntimeNotice";
import { EmptyPythonInstallHelp, LamaStatusPill } from "../runtime/RuntimeStatus";

type ImportMode = "images" | "folder" | "zip" | "zip-folder";

type WorkspaceEmptyStateProps = {
  lamaActionBusy: boolean;
  lamaActionMessage: string | null;
  lamaNoticePlatform: LamaNoticePlatform;
  lamaStatus: LamaRuntimeStatus | null;
  showLamaEmptyNotice: boolean;
  showTestPlatformSelector: boolean;
  onDownloadLamaModel: () => void | Promise<unknown>;
  onPrepareLama: () => void | Promise<unknown>;
  onRefreshLamaStatus: () => void | Promise<unknown>;
  onSelectImportFiles: (mode: ImportMode) => void;
  onSetLamaNoticePlatform: (platform: LamaNoticePlatform) => void;
};

export function WorkspaceEmptyState({
  lamaActionBusy,
  lamaActionMessage,
  lamaNoticePlatform,
  lamaStatus,
  showLamaEmptyNotice,
  showTestPlatformSelector,
  onDownloadLamaModel,
  onPrepareLama,
  onRefreshLamaStatus,
  onSelectImportFiles,
  onSetLamaNoticePlatform
}: WorkspaceEmptyStateProps): React.JSX.Element {
  if (showLamaEmptyNotice && lamaStatus) {
    return (
      <div className="empty-state-stack max-w-xl">
        <section className="empty-lama-card">
          <div className="empty-lama-header">
            <h2>LaMa 인페인트 준비</h2>
            <button type="button" onClick={() => void onRefreshLamaStatus()} disabled={lamaActionBusy}>
              새로고침
            </button>
          </div>
          <div className="empty-lama-status-grid">
            <LamaStatusPill label="Python" ready={lamaStatus.pythonAvailable} busy={false} />
            <LamaStatusPill label="런타임" ready={lamaStatus.runtimeReady} busy={lamaStatus.runtimePreparing} />
            <LamaStatusPill label="모델" ready={lamaStatus.modelExists} busy={lamaStatus.modelDownloading} />
          </div>
          {showTestPlatformSelector ? (
            <div className="empty-lama-test-platform" aria-label="테스트 OS">
              {LAMA_TEST_PLATFORM_OPTIONS.map((option) => (
                <button
                  key={option.value}
                  type="button"
                  className={lamaNoticePlatform === option.value ? "active" : ""}
                  onClick={() => onSetLamaNoticePlatform(option.value)}
                >
                  {option.label}
                </button>
              ))}
            </div>
          ) : null}
          {!lamaStatus.pythonAvailable ? <EmptyPythonInstallHelp status={lamaStatus} /> : null}
          <div className="empty-actions flex flex-wrap justify-center gap-2.5">
            <button type="button" onClick={() => void onPrepareLama()} disabled={lamaActionBusy || lamaStatus.runtimePreparing}>
              {lamaStatus.runtimePreparing ? "환경 준비 중" : "환경 준비"}
            </button>
            <button type="button" onClick={() => void onDownloadLamaModel()} disabled={lamaActionBusy || lamaStatus.modelDownloading}>
              {lamaStatus.modelDownloading ? "모델 다운로드 중" : "모델 다운로드"}
            </button>
          </div>
          <p className="muted-line">모델 경로: {lamaStatus.modelPath}</p>
          {lamaActionMessage ? <p className="muted-line">{lamaActionMessage}</p> : null}
        </section>
      </div>
    );
  }

  return (
    <div className="empty-state max-w-xl text-center">
      <h2>보관함에서 화를 열거나 새로 가져오세요.</h2>
      <p>작품과 화 단위로 저장해두고, 이어서 번역하거나 페이지별로 다시 번역할 수 있습니다.</p>
      <div className="empty-actions flex flex-wrap justify-center gap-2.5">
        <button onClick={() => onSelectImportFiles("images")}>이미지 열기</button>
        <button onClick={() => onSelectImportFiles("folder")}>폴더 열기</button>
        <button onClick={() => onSelectImportFiles("zip")}>압축파일 열기</button>
        <button onClick={() => onSelectImportFiles("zip-folder")}>작품 일괄 번역</button>
      </div>
    </div>
  );
}
