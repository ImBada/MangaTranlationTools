import React from "react";
import type { LamaRuntimeStatus } from "../../../../shared/types";
import type { LamaActionState } from "./settingsModalUtils";

type LamaRuntimeSettingsSectionProps = {
  controlsBusy: boolean;
  lamaActionState: LamaActionState;
  lamaStatus: LamaRuntimeStatus | null;
  onDownloadModel: () => void | Promise<unknown>;
  onPrepareRuntime: () => void | Promise<unknown>;
  onRefreshStatus: () => void | Promise<unknown>;
};

export function LamaRuntimeSettingsSection({
  controlsBusy,
  lamaActionState,
  lamaStatus,
  onDownloadModel,
  onPrepareRuntime,
  onRefreshStatus
}: LamaRuntimeSettingsSectionProps): React.JSX.Element {
  return (
    <div className="settings-field-stack">
      <span>LaMa 인페인트</span>
      <div className="settings-runtime-grid">
        <RuntimeState label="Python" ready={Boolean(lamaStatus?.pythonAvailable)} busy={false} />
        <RuntimeState label="런타임" ready={Boolean(lamaStatus?.runtimeReady)} busy={Boolean(lamaStatus?.runtimePreparing)} />
        <RuntimeState label="모델" ready={Boolean(lamaStatus?.modelExists)} busy={Boolean(lamaStatus?.modelDownloading)} />
      </div>
      <div className="settings-inline-actions">
        <button type="button" onClick={() => void onPrepareRuntime()} disabled={controlsBusy || Boolean(lamaStatus?.runtimePreparing)}>
          {lamaStatus?.runtimePreparing ? "준비 중..." : "환경 준비"}
        </button>
        <button type="button" onClick={() => void onDownloadModel()} disabled={controlsBusy || Boolean(lamaStatus?.modelDownloading)}>
          {lamaStatus?.modelDownloading ? "다운로드 중..." : "모델 다운로드"}
        </button>
        <button type="button" onClick={() => void onRefreshStatus()} disabled={controlsBusy}>
          새로고침
        </button>
      </div>
      {lamaStatus && !lamaStatus.pythonAvailable ? <PythonInstallHelp status={lamaStatus} compact /> : null}
      {lamaStatus ? <p className="muted-line modal-note">모델 경로: {lamaStatus.modelPath}</p> : null}
      {lamaStatus?.lastError ? <p className="muted-line modal-note">최근 오류: {lamaStatus.lastError}</p> : null}
      {lamaActionState.message ? (
        <div className={`settings-test-result ${lamaActionState.status === "error" ? "error" : lamaActionState.status === "success" ? "success" : ""}`}>
          <strong>{lamaActionState.message}</strong>
        </div>
      ) : null}
    </div>
  );
}

function RuntimeState({ label, ready, busy }: { label: string; ready: boolean; busy: boolean }): React.JSX.Element {
  return (
    <div className={`settings-runtime-state ${ready ? "ready" : busy ? "busy" : "missing"}`}>
      <span>{label}</span>
      <strong>{ready ? "준비됨" : busy ? "진행 중" : "필요"}</strong>
    </div>
  );
}

function PythonInstallHelp({ status, compact = false }: { status: LamaRuntimeStatus; compact?: boolean }): React.JSX.Element {
  return (
    <div className={compact ? "settings-python-help compact" : "settings-python-help"}>
      <strong>Python 설치가 필요합니다.</strong>
      <code>{status.pythonInstallCommand}</code>
      {status.pythonInstallHelp.map((line) => (
        <p key={line}>{line}</p>
      ))}
    </div>
  );
}
