import React from "react";
import type { LamaRuntimeStatus } from "../../../../shared/types";

export function LamaStatusPill({ label, ready, busy }: { label: string; ready: boolean; busy: boolean }): React.JSX.Element {
  return (
    <div className={`empty-lama-status ${ready ? "ready" : busy ? "busy" : "missing"}`}>
      <span>{label}</span>
      <strong>{ready ? "준비됨" : busy ? "진행 중" : "필요"}</strong>
    </div>
  );
}

export function EmptyPythonInstallHelp({ status }: { status: LamaRuntimeStatus }): React.JSX.Element {
  return (
    <div className="empty-lama-python-help">
      <strong>Python 설치가 필요합니다.</strong>
      <code>{status.pythonInstallCommand}</code>
      {status.pythonInstallHelp.map((line) => (
        <p key={line}>{line}</p>
      ))}
    </div>
  );
}
