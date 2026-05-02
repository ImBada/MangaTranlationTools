import React from "react";
import type { CodexReasoningEffort } from "../../../../shared/types";
import { CODEX_REASONING_OPTIONS } from "./settingsModalConfig";

type CodexModelSettingsSectionProps = {
  codexModel: string;
  codexOauthPort: string;
  codexReasoningEffort: CodexReasoningEffort;
  controlsBusy: boolean;
  onClearTestState: () => void;
  onCodexModelChange: (value: string) => void;
  onCodexOauthPortChange: (value: string) => void;
  onCodexReasoningEffortChange: (value: CodexReasoningEffort) => void;
  onSubmit: () => void;
};

export function CodexModelSettingsSection({
  codexModel,
  codexOauthPort,
  codexReasoningEffort,
  controlsBusy,
  onClearTestState,
  onCodexModelChange,
  onCodexOauthPortChange,
  onCodexReasoningEffortChange,
  onSubmit
}: CodexModelSettingsSectionProps): React.JSX.Element {
  return (
    <>
      <label>
        Codex 모델
        <input
          value={codexModel}
          disabled={controlsBusy}
          onChange={(event) => {
            onClearTestState();
            onCodexModelChange(event.target.value);
          }}
          placeholder="gpt-5.5"
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              onSubmit();
            }
          }}
        />
      </label>

      <div className="settings-field-stack">
        <span>생각</span>
        <div className="settings-preset-group" role="tablist" aria-label="Codex 생각">
          {CODEX_REASONING_OPTIONS.map((option) => (
            <button
              key={option.id}
              type="button"
              className={`settings-preset-button ${codexReasoningEffort === option.id ? "active" : ""}`}
              onClick={() => {
                onClearTestState();
                onCodexReasoningEffortChange(option.id);
              }}
              disabled={controlsBusy}
              aria-pressed={codexReasoningEffort === option.id}
            >
              {option.label}
            </button>
          ))}
        </div>
        <p className="muted-line modal-note">
          {CODEX_REASONING_OPTIONS.find((option) => option.id === codexReasoningEffort)?.description}
        </p>
      </div>

      <label>
        openai-oauth 포트
        <input
          type="number"
          min={0}
          max={65535}
          step={1}
          value={codexOauthPort}
          disabled={controlsBusy}
          onChange={(event) => {
            onClearTestState();
            onCodexOauthPortChange(event.target.value);
          }}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              onSubmit();
            }
          }}
        />
      </label>
    </>
  );
}
