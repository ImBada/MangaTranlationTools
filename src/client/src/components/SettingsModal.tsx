import React from "react";
import {
  TRANSLATION_PARALLEL_MAX_CONCURRENCY_MAX,
  TRANSLATION_PARALLEL_MAX_CONCURRENCY_MIN,
  type AppSettings,
  type CodexReasoningEffort,
  type LamaRuntimeStatus,
  type ModelProvider,
  type TranslationMode,
  type UpdateStatus
} from "../../../shared/types";
import { CodexModelSettingsSection } from "./settings/CodexModelSettingsSection";
import { GeneralSettingsSection } from "./settings/GeneralSettingsSection";
import { LamaRuntimeSettingsSection } from "./settings/LamaRuntimeSettingsSection";
import { ModelTestSection } from "./settings/ModelTestSection";
import { OpenAICompatibleSettingsSection } from "./settings/OpenAICompatibleSettingsSection";
import { SettingsVersionBar } from "./settings/SettingsVersionBar";
import {
  buildTestDetail,
  type LamaActionState,
  type TestState,
  withSettingsDefaults
} from "./settings/settingsModalUtils";

type SettingsModalProps = {
  initialSettings: AppSettings;
  busy: boolean;
  jobActive: boolean;
  onCancel: () => void;
  onReset: () => void;
  onSubmit: (settings: AppSettings) => void;
};

export function SettingsModal({
  initialSettings,
  busy,
  jobActive,
  onCancel,
  onReset,
  onSubmit
}: SettingsModalProps): React.JSX.Element {
  const safeInitialSettings = React.useMemo(() => withSettingsDefaults(initialSettings), [initialSettings]);
  const [modelProvider, setModelProvider] = React.useState<ModelProvider>(safeInitialSettings.modelProvider);
  const [codexModel, setCodexModel] = React.useState(safeInitialSettings.codex.model);
  const [codexReasoningEffort, setCodexReasoningEffort] = React.useState<CodexReasoningEffort>(
    safeInitialSettings.codex.reasoningEffort
  );
  const [codexOauthPort, setCodexOauthPort] = React.useState(String(safeInitialSettings.codex.oauthPort));
  const [compatibleBaseUrl, setCompatibleBaseUrl] = React.useState(safeInitialSettings.openAICompatible.baseUrl);
  const [compatibleApiKey, setCompatibleApiKey] = React.useState(safeInitialSettings.openAICompatible.apiKey);
  const [compatibleModel, setCompatibleModel] = React.useState(safeInitialSettings.openAICompatible.model);
  const [translationMode, setTranslationMode] = React.useState<TranslationMode>(safeInitialSettings.translationMode);
  const [translationParallelEnabled, setTranslationParallelEnabled] = React.useState(safeInitialSettings.translationParallel.enabled);
  const [translationParallelMaxConcurrency, setTranslationParallelMaxConcurrency] = React.useState(
    safeInitialSettings.translationParallel.maxConcurrency
  );
  const [nsfwMode, setNsfwMode] = React.useState(safeInitialSettings.nsfwMode);
  const [localActionBusy, setLocalActionBusy] = React.useState(false);
  const [testState, setTestState] = React.useState<TestState>({ status: "idle", message: null, detail: null });
  const [lamaStatus, setLamaStatus] = React.useState<LamaRuntimeStatus | null>(null);
  const [lamaActionState, setLamaActionState] = React.useState<LamaActionState>({ status: "idle", message: null });
  const [updateStatus, setUpdateStatus] = React.useState<UpdateStatus | null>(null);
  const [updateBusy, setUpdateBusy] = React.useState(false);

  React.useEffect(() => {
    setModelProvider(safeInitialSettings.modelProvider);
    setCodexModel(safeInitialSettings.codex.model);
    setCodexReasoningEffort(safeInitialSettings.codex.reasoningEffort);
    setCodexOauthPort(String(safeInitialSettings.codex.oauthPort));
    setCompatibleBaseUrl(safeInitialSettings.openAICompatible.baseUrl);
    setCompatibleApiKey(safeInitialSettings.openAICompatible.apiKey);
    setCompatibleModel(safeInitialSettings.openAICompatible.model);
    setTranslationMode(safeInitialSettings.translationMode);
    setTranslationParallelEnabled(safeInitialSettings.translationParallel.enabled);
    setTranslationParallelMaxConcurrency(safeInitialSettings.translationParallel.maxConcurrency);
    setNsfwMode(safeInitialSettings.nsfwMode);
    setTestState({ status: "idle", message: null, detail: null });
  }, [safeInitialSettings]);

  React.useEffect(() => {
    let cancelled = false;
    const loadStatus = async () => {
      try {
        const status = await window.mangaApi.getLamaRuntimeStatus();
        if (!cancelled) {
          setLamaStatus(status);
        }
      } catch {
        if (!cancelled) {
          setLamaStatus(null);
        }
      }
    };
    void loadStatus();
    const interval = window.setInterval(() => {
      if (lamaStatus?.runtimePreparing || lamaStatus?.modelDownloading) {
        void loadStatus();
      }
    }, 2500);
    return () => {
      cancelled = true;
      window.clearInterval(interval);
    };
  }, [lamaStatus?.modelDownloading, lamaStatus?.runtimePreparing]);

  const refreshUpdateStatus = React.useCallback(async (refresh = false) => {
    setUpdateBusy(true);
    try {
      setUpdateStatus(await window.mangaApi.getUpdateStatus(refresh));
    } catch (error) {
      setUpdateStatus({
        currentVersion: __APP_VERSION__,
        latestVersion: null,
        updateAvailable: false,
        checkedAt: new Date().toISOString(),
        releaseUrl: null,
        releaseName: null,
        error: error instanceof Error ? error.message : String(error)
      });
    } finally {
      setUpdateBusy(false);
    }
  }, []);

  React.useEffect(() => {
    void refreshUpdateStatus(false);
  }, [refreshUpdateStatus]);

  const controlsBusy = busy || localActionBusy || testState.status === "running";
  const trimmedCodexModel = codexModel.trim();
  const trimmedCompatibleBaseUrl = compatibleBaseUrl.trim().replace(/\/+$/, "");
  const trimmedCompatibleApiKey = compatibleApiKey.trim();
  const trimmedCompatibleModel = compatibleModel.trim();
  const parsedCodexOauthPort = Number(codexOauthPort);
  const codexOauthPortValid =
    Number.isInteger(parsedCodexOauthPort) && parsedCodexOauthPort >= 0 && parsedCodexOauthPort <= 65535;
  const compatibleBaseUrlValid = /^https?:\/\/.+/i.test(trimmedCompatibleBaseUrl);
  const normalizedTranslationParallelMaxConcurrency = Number.isFinite(translationParallelMaxConcurrency)
    ? Math.min(
        TRANSLATION_PARALLEL_MAX_CONCURRENCY_MAX,
        Math.max(TRANSLATION_PARALLEL_MAX_CONCURRENCY_MIN, Math.floor(translationParallelMaxConcurrency))
      )
    : safeInitialSettings.translationParallel.maxConcurrency;
  const canSubmit = Boolean(
    modelProvider === "openai-codex"
      ? trimmedCodexModel && codexOauthPortValid
      : trimmedCompatibleBaseUrl && compatibleBaseUrlValid && trimmedCompatibleModel
  );

  const buildSettings = React.useCallback((): AppSettings | null => {
    if (modelProvider === "openai-codex") {
      if (!trimmedCodexModel || !codexOauthPortValid) {
        return null;
      }

      return {
        modelProvider,
        codex: {
          model: trimmedCodexModel,
          reasoningEffort: codexReasoningEffort,
          oauthPort: parsedCodexOauthPort
        },
        openAICompatible: {
          baseUrl: trimmedCompatibleBaseUrl || safeInitialSettings.openAICompatible.baseUrl,
          apiKey: trimmedCompatibleApiKey,
          model: trimmedCompatibleModel || safeInitialSettings.openAICompatible.model
        },
        translationMode,
        translationParallel: {
          enabled: translationParallelEnabled,
          maxConcurrency: normalizedTranslationParallelMaxConcurrency
        },
        nsfwMode
      };
    }

    if (!trimmedCompatibleBaseUrl || !compatibleBaseUrlValid || !trimmedCompatibleModel) {
      return null;
    }

    return {
      modelProvider,
      codex: {
        model: trimmedCodexModel || safeInitialSettings.codex.model,
        reasoningEffort: codexReasoningEffort,
        oauthPort: codexOauthPortValid ? parsedCodexOauthPort : safeInitialSettings.codex.oauthPort
      },
      openAICompatible: {
        baseUrl: trimmedCompatibleBaseUrl || safeInitialSettings.openAICompatible.baseUrl,
        apiKey: trimmedCompatibleApiKey,
        model: trimmedCompatibleModel || safeInitialSettings.openAICompatible.model
      },
      translationMode,
      translationParallel: {
        enabled: translationParallelEnabled,
        maxConcurrency: normalizedTranslationParallelMaxConcurrency
      },
      nsfwMode
    };
  }, [
    modelProvider,
    codexOauthPortValid,
    trimmedCodexModel,
    trimmedCompatibleBaseUrl,
    trimmedCompatibleApiKey,
    trimmedCompatibleModel,
    parsedCodexOauthPort,
    compatibleBaseUrlValid,
    codexReasoningEffort,
    safeInitialSettings.codex.model,
    safeInitialSettings.codex.oauthPort,
    safeInitialSettings.openAICompatible.baseUrl,
    safeInitialSettings.openAICompatible.model,
    translationMode,
    translationParallelEnabled,
    normalizedTranslationParallelMaxConcurrency,
    nsfwMode
  ]);

  const clearTestState = React.useCallback(() => {
    setTestState({ status: "idle", message: null, detail: null });
  }, []);

  const submit = React.useCallback(() => {
    const nextSettings = buildSettings();
    if (!nextSettings || !canSubmit) {
      return;
    }
    onSubmit(nextSettings);
  }, [buildSettings, canSubmit, onSubmit]);

  const runModelTest = async () => {
    const nextSettings = buildSettings();
    if (!nextSettings || !canSubmit || jobActive) {
      return;
    }

    setTestState({
      status: "running",
      message: "모델을 불러오고 간단한 텍스트 응답을 확인하는 중입니다...",
      detail: "이 테스트는 모델 로드와 텍스트 응답만 확인합니다."
    });
    try {
      const result = await window.mangaApi.testModelSettings(nextSettings);
      setTestState({
        status: result.ok ? "success" : "error",
        message: result.message,
        detail: buildTestDetail(result.resolvedEndpoint)
      });
    } catch (error) {
      setTestState({
        status: "error",
        message: error instanceof Error ? error.message : String(error),
        detail: null
      });
    }
  };

  const refreshLamaStatus = async () => {
    try {
      setLamaStatus(await window.mangaApi.getLamaRuntimeStatus());
    } catch (error) {
      setLamaActionState({ status: "error", message: error instanceof Error ? error.message : String(error) });
    }
  };

  const prepareLamaRuntime = async () => {
    setLocalActionBusy(true);
    setLamaActionState({ status: "running", message: "LaMa Python 환경을 준비하는 중입니다." });
    try {
      const status = await window.mangaApi.prepareLamaRuntime();
      setLamaStatus(status);
      setLamaActionState({
        status: status.pythonAvailable ? "success" : "error",
        message: status.pythonAvailable ? "LaMa 환경 준비를 시작했습니다." : `Python 설치가 필요합니다: ${status.pythonInstallCommand}`
      });
    } catch (error) {
      setLamaActionState({ status: "error", message: error instanceof Error ? error.message : String(error) });
    } finally {
      setLocalActionBusy(false);
    }
  };

  const downloadLamaModel = async () => {
    setLocalActionBusy(true);
    setLamaActionState({ status: "running", message: "LaMa 모델 다운로드를 시작하는 중입니다." });
    try {
      const status = await window.mangaApi.downloadLamaModel();
      setLamaStatus(status);
      setLamaActionState({
        status: "success",
        message: status.modelExists ? "LaMa 모델이 이미 준비되어 있습니다." : "LaMa 모델 다운로드를 시작했습니다."
      });
    } catch (error) {
      setLamaActionState({ status: "error", message: error instanceof Error ? error.message : String(error) });
    } finally {
      setLocalActionBusy(false);
    }
  };

  return (
    <div className="modal-backdrop">
      <div className="modal-card settings-modal">
        <div className="modal-header">
          <h2>설정</h2>
          <button className="ghost-button" onClick={onCancel} disabled={controlsBusy}>
            닫기
          </button>
        </div>

        <section className="modal-section">
          <GeneralSettingsSection
            controlsBusy={controlsBusy}
            modelProvider={modelProvider}
            nsfwMode={nsfwMode}
            translationParallelEnabled={translationParallelEnabled}
            translationParallelMaxConcurrency={translationParallelMaxConcurrency}
            translationMode={translationMode}
            onClearTestState={clearTestState}
            onModelProviderChange={setModelProvider}
            onNsfwModeChange={setNsfwMode}
            onTranslationParallelEnabledChange={setTranslationParallelEnabled}
            onTranslationParallelMaxConcurrencyChange={setTranslationParallelMaxConcurrency}
            onTranslationModeChange={setTranslationMode}
          />

          <LamaRuntimeSettingsSection
            controlsBusy={controlsBusy}
            lamaActionState={lamaActionState}
            lamaStatus={lamaStatus}
            onDownloadModel={downloadLamaModel}
            onPrepareRuntime={prepareLamaRuntime}
            onRefreshStatus={refreshLamaStatus}
          />

          {modelProvider === "openai-codex" ? (
            <CodexModelSettingsSection
              codexModel={codexModel}
              codexOauthPort={codexOauthPort}
              codexReasoningEffort={codexReasoningEffort}
              controlsBusy={controlsBusy}
              onClearTestState={clearTestState}
              onCodexModelChange={setCodexModel}
              onCodexOauthPortChange={setCodexOauthPort}
              onCodexReasoningEffortChange={setCodexReasoningEffort}
              onSubmit={submit}
            />
          ) : (
            <OpenAICompatibleSettingsSection
              compatibleApiKey={compatibleApiKey}
              compatibleBaseUrl={compatibleBaseUrl}
              compatibleModel={compatibleModel}
              controlsBusy={controlsBusy}
              onClearTestState={clearTestState}
              onCompatibleApiKeyChange={setCompatibleApiKey}
              onCompatibleBaseUrlChange={setCompatibleBaseUrl}
              onCompatibleModelChange={setCompatibleModel}
              onSubmit={submit}
            />
          )}

          <ModelTestSection
            canSubmit={canSubmit}
            codexOauthPortValid={codexOauthPortValid}
            compatibleBaseUrlValid={compatibleBaseUrlValid}
            controlsBusy={controlsBusy}
            jobActive={jobActive}
            modelProvider={modelProvider}
            testState={testState}
            onRunModelTest={runModelTest}
          />
        </section>

        <SettingsVersionBar
          updateBusy={updateBusy}
          updateStatus={updateStatus}
          onRefreshUpdateStatus={() => refreshUpdateStatus(true)}
        />

        <div className="modal-actions settings-actions">
          <button onClick={onReset} disabled={controlsBusy}>
            기본값 복원
          </button>
          <button onClick={onCancel} disabled={controlsBusy}>
            취소
          </button>
          <button className="primary" onClick={submit} disabled={controlsBusy || !canSubmit}>
            저장
          </button>
        </div>
      </div>
    </div>
  );
}
