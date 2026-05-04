import React from "react";
import type { FontPresetBackupSnapshot, FontPresetBackupSummary } from "../../../../shared/types";

export type FontPresetBackupDialogMode = "backup" | "restore";

type FontPresetBackupModalProps = {
  mode: FontPresetBackupDialogMode;
  onCancel: () => void;
  onCreateBackup: (name: string) => Promise<FontPresetBackupSnapshot | null>;
  onDeleteBackup: (backupId: string) => Promise<FontPresetBackupSummary[]>;
  onListBackups: () => Promise<FontPresetBackupSummary[]>;
  onRestoreBackup: (backupId: string) => Promise<void>;
};

export function FontPresetBackupModal({
  mode,
  onCancel,
  onCreateBackup,
  onDeleteBackup,
  onListBackups,
  onRestoreBackup
}: FontPresetBackupModalProps): React.JSX.Element {
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);
  const [backupName, setBackupName] = React.useState(() => buildDefaultBackupName());
  const [backups, setBackups] = React.useState<FontPresetBackupSummary[]>([]);
  const [selectedBackupId, setSelectedBackupId] = React.useState<string | null>(null);

  const applyBackups = React.useCallback((nextBackups: FontPresetBackupSummary[], preferredBackupId?: string | null) => {
    setBackups(nextBackups);
    setSelectedBackupId((current) => {
      if (preferredBackupId && nextBackups.some((backup) => backup.id === preferredBackupId)) {
        return preferredBackupId;
      }
      if (current && nextBackups.some((backup) => backup.id === current)) {
        return current;
      }
      return nextBackups[0]?.id ?? null;
    });
  }, []);

  const refreshBackups = React.useCallback(async (preferredBackupId?: string | null) => {
    applyBackups(await onListBackups(), preferredBackupId);
  }, [applyBackups, onListBackups]);

  React.useEffect(() => {
    let cancelled = false;
    setBusy(true);
    setError(null);
    onListBackups()
      .then((nextBackups) => {
        if (!cancelled) {
          applyBackups(nextBackups);
        }
      })
      .catch((loadError: unknown) => {
        if (!cancelled) {
          setError(loadError instanceof Error ? loadError.message : "백업 목록을 불러오지 못했습니다.");
        }
      })
      .finally(() => {
        if (!cancelled) {
          setBusy(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [applyBackups, onListBackups]);

  const saveBackup = React.useCallback(async () => {
    const name = backupName.trim();
    if (!name) {
      setError("백업 이름을 입력하세요.");
      return;
    }

    setBusy(true);
    setError(null);
    try {
      const backup = await onCreateBackup(name);
      setBackupName(buildDefaultBackupName());
      await refreshBackups(backup?.id);
    } catch (saveError) {
      setError(saveError instanceof Error ? saveError.message : "폰트 프리셋 백업 저장에 실패했습니다.");
    } finally {
      setBusy(false);
    }
  }, [backupName, onCreateBackup, refreshBackups]);

  const restoreBackup = React.useCallback(async () => {
    if (!selectedBackupId) {
      setError("복원할 백업을 선택하세요.");
      return;
    }

    setBusy(true);
    setError(null);
    try {
      await onRestoreBackup(selectedBackupId);
      setBusy(false);
      onCancel();
    } catch (restoreError) {
      setError(restoreError instanceof Error ? restoreError.message : "폰트 프리셋 백업 복원에 실패했습니다.");
      setBusy(false);
    }
  }, [onCancel, onRestoreBackup, selectedBackupId]);

  const deleteBackup = React.useCallback(async (backup: FontPresetBackupSummary) => {
    const confirmed = await window.mangaApi.confirm(
      "폰트 프리셋 백업 삭제",
      `"${backup.name}" 백업을 삭제할까요?`,
      "삭제한 백업은 복원할 수 없습니다."
    );
    if (!confirmed) {
      return;
    }

    setBusy(true);
    setError(null);
    try {
      applyBackups(await onDeleteBackup(backup.id));
    } catch (deleteError) {
      setError(deleteError instanceof Error ? deleteError.message : "폰트 프리셋 백업 삭제에 실패했습니다.");
    } finally {
      setBusy(false);
    }
  }, [applyBackups, onDeleteBackup]);

  return (
    <div className="modal-backdrop">
      <div className="modal-card font-preset-backup-modal">
        <div className="modal-header">
          <h2>{mode === "backup" ? "폰트 프리셋 백업" : "폰트 프리셋 복원"}</h2>
          <button className="ghost-button" disabled={busy} onClick={onCancel}>
            닫기
          </button>
        </div>
        <section className="modal-section">
          {mode === "backup" ? (
            <label className="font-preset-backup-name-field">
              백업 이름
              <input
                value={backupName}
                disabled={busy}
                autoFocus
                onChange={(event) => setBackupName(event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    void saveBackup();
                  }
                }}
              />
            </label>
          ) : null}
          <div className="font-preset-backup-list" aria-label="폰트 프리셋 백업 목록">
            {backups.length > 0 ? (
              backups.map((backup) => (
                <div
                  key={backup.id}
                  className={`font-preset-backup-list-item ${selectedBackupId === backup.id ? "active" : ""}`}
                >
                  <button
                    type="button"
                    className="font-preset-backup-select"
                    disabled={busy}
                    onClick={() => setSelectedBackupId(backup.id)}
                  >
                    <span>{backup.name}</span>
                    <small>
                      {new Date(backup.createdAt).toLocaleString("ko-KR")} · 폰트 프리셋 {backup.fontPresetCount}개 · 크기 프리셋 {backup.fontSizePresetCount}개
                    </small>
                  </button>
                  <button
                    type="button"
                    className="font-preset-backup-delete"
                    disabled={busy}
                    onClick={() => void deleteBackup(backup)}
                    aria-label={`${backup.name} 백업 삭제`}
                    title="백업 삭제"
                  >
                    삭제
                  </button>
                </div>
              ))
            ) : (
              <p className="muted-line modal-note">저장된 백업이 없습니다.</p>
            )}
          </div>
          {error ? <p className="muted-line modal-note font-preset-backup-error">{error}</p> : null}
        </section>
        <div className="modal-actions">
          <button onClick={onCancel} disabled={busy}>
            취소
          </button>
          {mode === "backup" ? (
            <button className="primary" onClick={() => void saveBackup()} disabled={busy || !backupName.trim()}>
              저장
            </button>
          ) : (
            <button className="primary" onClick={() => void restoreBackup()} disabled={busy || !selectedBackupId}>
              복원
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

function buildDefaultBackupName(): string {
  return `폰트 프리셋 백업 ${new Date().toLocaleString("ko-KR")}`;
}
