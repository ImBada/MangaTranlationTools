import React from "react";
import type { ModelProvider, TranslationMode } from "../../../../shared/types";
import { MODEL_PROVIDER_OPTIONS, TRANSLATION_MODE_OPTIONS } from "./settingsModalConfig";

type GeneralSettingsSectionProps = {
  controlsBusy: boolean;
  modelProvider: ModelProvider;
  nsfwMode: boolean;
  translationMode: TranslationMode;
  onClearTestState: () => void;
  onModelProviderChange: (value: ModelProvider) => void;
  onNsfwModeChange: React.Dispatch<React.SetStateAction<boolean>>;
  onTranslationModeChange: (value: TranslationMode) => void;
};

export function GeneralSettingsSection({
  controlsBusy,
  modelProvider,
  nsfwMode,
  translationMode,
  onClearTestState,
  onModelProviderChange,
  onNsfwModeChange,
  onTranslationModeChange
}: GeneralSettingsSectionProps): React.JSX.Element {
  return (
    <>
      <p className="muted-line modal-note">다음 번 번역 실행부터 적용됩니다.</p>
      <div className="settings-field-stack">
        <span>번역 모드</span>
        <div className="settings-mode-group" role="tablist" aria-label="번역 모드">
          {TRANSLATION_MODE_OPTIONS.map((option) => (
            <button
              key={option.id}
              type="button"
              className={`settings-preset-button ${translationMode === option.id ? "active" : ""}`}
              onClick={() => {
                onClearTestState();
                onTranslationModeChange(option.id);
              }}
              disabled={controlsBusy}
              aria-pressed={translationMode === option.id}
            >
              {option.label}
            </button>
          ))}
        </div>
        <p className="muted-line modal-note">
          {TRANSLATION_MODE_OPTIONS.find((option) => option.id === translationMode)?.description}
        </p>
      </div>

      <div className="settings-field-stack">
        <span>번역 엔진</span>
        <div className="settings-mode-group" role="tablist" aria-label="번역 엔진">
          {MODEL_PROVIDER_OPTIONS.map((option) => (
            <button
              key={option.id}
              type="button"
              className={`settings-preset-button ${modelProvider === option.id ? "active" : ""}`}
              onClick={() => {
                onClearTestState();
                onModelProviderChange(option.id);
              }}
              disabled={controlsBusy}
              aria-pressed={modelProvider === option.id}
            >
              {option.label}
            </button>
          ))}
        </div>
        <p className="muted-line modal-note">
          {MODEL_PROVIDER_OPTIONS.find((option) => option.id === modelProvider)?.description}
        </p>
      </div>

      <label className="settings-toggle-row">
        NSFW 모드
        <button
          type="button"
          className={`settings-toggle-button ${nsfwMode ? "active" : ""}`}
          onClick={() => {
            onClearTestState();
            onNsfwModeChange((current) => !current);
          }}
          disabled={controlsBusy}
          aria-pressed={nsfwMode}
        >
          {nsfwMode ? "켜짐" : "꺼짐"}
        </button>
      </label>
      <p className="muted-line">켜두면 시스템 프롬프트에 NSFW 허용 지시문을 추가합니다.</p>
    </>
  );
}
