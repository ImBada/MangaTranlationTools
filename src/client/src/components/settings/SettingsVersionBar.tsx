import React from "react";
import type { UpdateStatus } from "../../../../shared/types";
import { resolveUpdateStatusText } from "./settingsModalUtils";

type SettingsVersionBarProps = {
  updateBusy: boolean;
  updateStatus: UpdateStatus | null;
  onRefreshUpdateStatus: () => void | Promise<unknown>;
};

export function SettingsVersionBar({
  updateBusy,
  updateStatus,
  onRefreshUpdateStatus
}: SettingsVersionBarProps): React.JSX.Element {
  return (
    <div className={`settings-version ${updateStatus?.updateAvailable ? "update-available" : ""}`}>
      <div>
        <strong>MangaTranslationTools v{__APP_VERSION__}</strong>
        <span>{resolveUpdateStatusText(updateStatus, updateBusy)}</span>
      </div>
      {updateStatus?.updateAvailable && updateStatus.releaseUrl ? (
        <a href={updateStatus.releaseUrl} target="_blank" rel="noreferrer">
          업데이트 열기
        </a>
      ) : null}
      <button type="button" onClick={() => void onRefreshUpdateStatus()} disabled={updateBusy}>
        {updateBusy ? "확인 중" : "다시 확인"}
      </button>
    </div>
  );
}
