import React from "react";

type OpenAICompatibleSettingsSectionProps = {
  compatibleApiKey: string;
  compatibleBaseUrl: string;
  compatibleModel: string;
  controlsBusy: boolean;
  onClearTestState: () => void;
  onCompatibleApiKeyChange: (value: string) => void;
  onCompatibleBaseUrlChange: (value: string) => void;
  onCompatibleModelChange: (value: string) => void;
  onSubmit: () => void;
};

export function OpenAICompatibleSettingsSection({
  compatibleApiKey,
  compatibleBaseUrl,
  compatibleModel,
  controlsBusy,
  onClearTestState,
  onCompatibleApiKeyChange,
  onCompatibleBaseUrlChange,
  onCompatibleModelChange,
  onSubmit
}: OpenAICompatibleSettingsSectionProps): React.JSX.Element {
  return (
    <>
      <label>
        Base URL
        <input
          value={compatibleBaseUrl}
          disabled={controlsBusy}
          onChange={(event) => {
            onClearTestState();
            onCompatibleBaseUrlChange(event.target.value);
          }}
          placeholder="http://127.0.0.1:1234/v1"
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              onSubmit();
            }
          }}
        />
      </label>

      <label>
        API Key
        <input
          type="password"
          value={compatibleApiKey}
          disabled={controlsBusy}
          onChange={(event) => {
            onClearTestState();
            onCompatibleApiKeyChange(event.target.value);
          }}
          placeholder="로컬 서버면 비워둘 수 있습니다"
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              onSubmit();
            }
          }}
        />
      </label>

      <label>
        모델
        <input
          value={compatibleModel}
          disabled={controlsBusy}
          onChange={(event) => {
            onClearTestState();
            onCompatibleModelChange(event.target.value);
          }}
          placeholder="gpt-4o-mini"
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
