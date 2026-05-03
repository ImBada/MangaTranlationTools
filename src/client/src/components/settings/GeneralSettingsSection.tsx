import React from "react";
import {
  TRANSLATION_PARALLEL_MAX_CONCURRENCY_MAX,
  TRANSLATION_PARALLEL_MAX_CONCURRENCY_MIN,
  type ModelProvider,
  type TranslationMode
} from "../../../../shared/types";
import {
  MODEL_PROVIDER_OPTIONS,
  TRANSLATION_MODE_OPTIONS
} from "./settingsModalConfig";

type GeneralSettingsSectionProps = {
  controlsBusy: boolean;
  modelProvider: ModelProvider;
  nsfwMode: boolean;
  translationParallelEnabled: boolean;
  translationParallelMaxConcurrency: number;
  translationMode: TranslationMode;
  onClearTestState: () => void;
  onModelProviderChange: (value: ModelProvider) => void;
  onNsfwModeChange: React.Dispatch<React.SetStateAction<boolean>>;
  onTranslationParallelEnabledChange: React.Dispatch<React.SetStateAction<boolean>>;
  onTranslationParallelMaxConcurrencyChange: (value: number) => void;
  onTranslationModeChange: (value: TranslationMode) => void;
};

export function GeneralSettingsSection({
  controlsBusy,
  modelProvider,
  nsfwMode,
  translationParallelEnabled,
  translationParallelMaxConcurrency,
  translationMode,
  onClearTestState,
  onModelProviderChange,
  onNsfwModeChange,
  onTranslationParallelEnabledChange,
  onTranslationParallelMaxConcurrencyChange,
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
        AI 번역 병렬 처리
        <button
          type="button"
          className={`settings-toggle-button ${translationParallelEnabled ? "active" : ""}`}
          onClick={() => {
            onClearTestState();
            onTranslationParallelEnabledChange((current) => !current);
          }}
          disabled={controlsBusy}
          aria-pressed={translationParallelEnabled}
        >
          {translationParallelEnabled ? "켜짐" : "꺼짐"}
        </button>
      </label>
      <div className="settings-inline-number-row">
        <label htmlFor="translation-parallel-max">최대 동시 번역 수</label>
        <input
          id="translation-parallel-max"
          type="number"
          min={TRANSLATION_PARALLEL_MAX_CONCURRENCY_MIN}
          max={TRANSLATION_PARALLEL_MAX_CONCURRENCY_MAX}
          step={1}
          value={translationParallelMaxConcurrency}
          disabled={controlsBusy || !translationParallelEnabled}
          onChange={(event) => {
            onClearTestState();
            onTranslationParallelMaxConcurrencyChange(Number(event.target.value));
          }}
        />
      </div>
      <p className="muted-line">켜두면 여러 페이지를 동시에 AI 번역합니다. API/로컬 모델 한도에 맞춰 조절하세요.</p>

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
