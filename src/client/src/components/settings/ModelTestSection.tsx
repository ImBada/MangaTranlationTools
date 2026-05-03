import React from "react";
import type { ModelProvider } from "../../../../shared/types";
import type { TestState } from "./settingsModalUtils";

type ModelTestSectionProps = {
  canSubmit: boolean;
  codexOauthPortValid: boolean;
  compatibleBaseUrlValid: boolean;
  controlsBusy: boolean;
  jobActive: boolean;
  modelProvider: ModelProvider;
  testState: TestState;
  onRunModelTest: () => void | Promise<unknown>;
};

export function ModelTestSection({
  canSubmit,
  codexOauthPortValid,
  compatibleBaseUrlValid,
  controlsBusy,
  jobActive,
  modelProvider,
  testState,
  onRunModelTest
}: ModelTestSectionProps): React.JSX.Element {
  return (
    <>
      <div className="settings-field-stack">
        <span>모델 테스트</span>
        <div className="settings-inline-actions">
          <button
            type="button"
            onClick={() => void onRunModelTest()}
            disabled={controlsBusy || !canSubmit || jobActive}
          >
            {testState.status === "running" ? "테스트 중..." : "잘 작동되나 확인"}
          </button>
        </div>
        <p className="muted-line modal-note">
          서버가 뜨고 간단한 텍스트 요청에 응답하는지만 확인합니다. 실제 이미지 번역 가능 여부와는 다를 수 있습니다.
        </p>
        {jobActive ? <p className="muted-line">번역 작업 중에는 모델 테스트를 실행할 수 없습니다.</p> : null}
        {testState.status !== "idle" ? (
          <div className={`settings-test-result ${testState.status}`}>
            <strong>{testState.message}</strong>
            {testState.detail ? <p>{testState.detail}</p> : null}
          </div>
        ) : null}
      </div>

      {modelProvider === "openai-codex" && !codexOauthPortValid ? (
        <p className="muted-line">openai-oauth 포트는 0 이상 65535 이하의 정수여야 합니다.</p>
      ) : null}
      {modelProvider === "openai-compatible" && !compatibleBaseUrlValid ? (
        <p className="muted-line">Base URL은 http:// 또는 https://로 시작해야 합니다.</p>
      ) : null}
    </>
  );
}
