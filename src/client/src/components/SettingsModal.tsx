import React from "react";
import type {
  AppSettings,
  CodexReasoningEffort,
  LamaRuntimeStatus,
  ModelProvider,
  ModelSource,
  TranslationMode,
  UpdateStatus
} from "../../../shared/types";
import { CodexModelSettingsSection } from "./settings/CodexModelSettingsSection";
import { GemmaModelSettingsSection } from "./settings/GemmaModelSettingsSection";
import { GeneralSettingsSection } from "./settings/GeneralSettingsSection";
import { LamaRuntimeSettingsSection } from "./settings/LamaRuntimeSettingsSection";
import { ModelTestSection } from "./settings/ModelTestSection";
import { OpenAICompatibleSettingsSection } from "./settings/OpenAICompatibleSettingsSection";
import { SettingsVersionBar } from "./settings/SettingsVersionBar";
import {
  DEFAULT_GEMMA_MODEL_REPO,
  MAX_GPU_LAYERS,
  MODEL_PRESETS,
  type ModelPresetId
} from "./settings/settingsModalConfig";
import {
  buildTestDetail,
  clampGpuLayers,
  type LamaActionState,
  resolveModelPreset,
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
  const [modelSource, setModelSource] = React.useState<ModelSource>(safeInitialSettings.gemma.modelSource);
  const [selectedPreset, setSelectedPreset] = React.useState<ModelPresetId>(() =>
    resolveModelPreset(safeInitialSettings.gemma.modelRepo, safeInitialSettings.gemma.modelFile)
  );
  const [customModelRepo, setCustomModelRepo] = React.useState(safeInitialSettings.gemma.modelRepo);
  const [customModelFile, setCustomModelFile] = React.useState(safeInitialSettings.gemma.modelFile);
  const [localModelPath, setLocalModelPath] = React.useState(safeInitialSettings.gemma.localModelPath ?? "");
  const [localMmprojPath, setLocalMmprojPath] = React.useState(safeInitialSettings.gemma.localMmprojPath ?? "");
  const [gpuLayers, setGpuLayers] = React.useState(String(clampGpuLayers(safeInitialSettings.gemma.gpuLayers)));
  const [codexModel, setCodexModel] = React.useState(safeInitialSettings.codex.model);
  const [codexReasoningEffort, setCodexReasoningEffort] = React.useState<CodexReasoningEffort>(
    safeInitialSettings.codex.reasoningEffort
  );
  const [codexOauthPort, setCodexOauthPort] = React.useState(String(safeInitialSettings.codex.oauthPort));
  const [compatibleBaseUrl, setCompatibleBaseUrl] = React.useState(safeInitialSettings.openAICompatible.baseUrl);
  const [compatibleApiKey, setCompatibleApiKey] = React.useState(safeInitialSettings.openAICompatible.apiKey);
  const [compatibleModel, setCompatibleModel] = React.useState(safeInitialSettings.openAICompatible.model);
  const [translationMode, setTranslationMode] = React.useState<TranslationMode>(safeInitialSettings.translationMode);
  const [nsfwMode, setNsfwMode] = React.useState(safeInitialSettings.nsfwMode);
  const [localActionBusy, setLocalActionBusy] = React.useState(false);
  const [testState, setTestState] = React.useState<TestState>({ status: "idle", message: null, detail: null });
  const [lamaStatus, setLamaStatus] = React.useState<LamaRuntimeStatus | null>(null);
  const [lamaActionState, setLamaActionState] = React.useState<LamaActionState>({ status: "idle", message: null });
  const [updateStatus, setUpdateStatus] = React.useState<UpdateStatus | null>(null);
  const [updateBusy, setUpdateBusy] = React.useState(false);
  const modelRepoInputRef = React.useRef<HTMLInputElement | null>(null);
  const localModelInputRef = React.useRef<HTMLInputElement | null>(null);
  const gpuSliderRef = React.useRef<HTMLInputElement | null>(null);

  React.useEffect(() => {
    setModelProvider(safeInitialSettings.modelProvider);
    setModelSource(safeInitialSettings.gemma.modelSource);
    setSelectedPreset(resolveModelPreset(safeInitialSettings.gemma.modelRepo, safeInitialSettings.gemma.modelFile));
    setCustomModelRepo(safeInitialSettings.gemma.modelRepo);
    setCustomModelFile(safeInitialSettings.gemma.modelFile);
    setLocalModelPath(safeInitialSettings.gemma.localModelPath ?? "");
    setLocalMmprojPath(safeInitialSettings.gemma.localMmprojPath ?? "");
    setGpuLayers(String(clampGpuLayers(safeInitialSettings.gemma.gpuLayers)));
    setCodexModel(safeInitialSettings.codex.model);
    setCodexReasoningEffort(safeInitialSettings.codex.reasoningEffort);
    setCodexOauthPort(String(safeInitialSettings.codex.oauthPort));
    setCompatibleBaseUrl(safeInitialSettings.openAICompatible.baseUrl);
    setCompatibleApiKey(safeInitialSettings.openAICompatible.apiKey);
    setCompatibleModel(safeInitialSettings.openAICompatible.model);
    setTranslationMode(safeInitialSettings.translationMode);
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

  React.useEffect(() => {
    if (modelProvider === "openai-codex") {
      return;
    }
    if (modelSource === "local") {
      localModelInputRef.current?.focus();
      localModelInputRef.current?.select();
      return;
    }
    if (selectedPreset === "custom") {
      modelRepoInputRef.current?.focus();
      modelRepoInputRef.current?.select();
      return;
    }
    gpuSliderRef.current?.focus();
  }, [modelProvider, modelSource, selectedPreset]);

  const controlsBusy = busy || localActionBusy || testState.status === "running";
  const activePreset = modelSource === "huggingface" && selectedPreset !== "custom" ? MODEL_PRESETS[selectedPreset] : null;
  const trimmedModelRepo = (activePreset?.modelRepo ?? customModelRepo).trim();
  const trimmedModelFile = (activePreset?.modelFile ?? customModelFile).trim();
  const trimmedLocalModelPath = localModelPath.trim();
  const trimmedLocalMmprojPath = localMmprojPath.trim();
  const trimmedCodexModel = codexModel.trim();
  const trimmedCompatibleBaseUrl = compatibleBaseUrl.trim().replace(/\/+$/, "");
  const trimmedCompatibleApiKey = compatibleApiKey.trim();
  const trimmedCompatibleModel = compatibleModel.trim();
  const parsedGpuLayers = Number(gpuLayers);
  const parsedCodexOauthPort = Number(codexOauthPort);
  const gpuLayersValid =
    Number.isInteger(parsedGpuLayers) && parsedGpuLayers >= 0 && parsedGpuLayers <= MAX_GPU_LAYERS;
  const codexOauthPortValid =
    Number.isInteger(parsedCodexOauthPort) && parsedCodexOauthPort >= 0 && parsedCodexOauthPort <= 65535;
  const compatibleBaseUrlValid = /^https?:\/\/.+/i.test(trimmedCompatibleBaseUrl);
  const canSubmit = Boolean(
    modelProvider === "openai-codex"
      ? trimmedCodexModel && codexOauthPortValid
      : modelProvider === "openai-compatible"
        ? trimmedCompatibleBaseUrl && compatibleBaseUrlValid && trimmedCompatibleModel
        : gpuLayersValid && (modelSource === "local" ? trimmedLocalModelPath : trimmedModelRepo && trimmedModelFile)
  );
  const sliderValue =
    Number.isFinite(parsedGpuLayers) ? clampGpuLayers(Math.trunc(parsedGpuLayers)) : 0;

  const buildSettings = React.useCallback((): AppSettings | null => {
    if (modelProvider === "openai-codex") {
      if (!trimmedCodexModel || !codexOauthPortValid) {
        return null;
      }

      return {
        modelProvider,
        gemma: {
          modelSource,
          modelRepo: trimmedModelRepo || DEFAULT_GEMMA_MODEL_REPO,
          modelFile: trimmedModelFile || MODEL_PRESETS.q4.modelFile,
          ...(trimmedLocalModelPath ? { localModelPath: trimmedLocalModelPath } : {}),
          ...(trimmedLocalMmprojPath ? { localMmprojPath: trimmedLocalMmprojPath } : {}),
          gpuLayers: gpuLayersValid ? parsedGpuLayers : safeInitialSettings.gemma.gpuLayers
        },
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
        nsfwMode
      };
    }

    if (modelProvider === "openai-compatible") {
      if (!trimmedCompatibleBaseUrl || !compatibleBaseUrlValid || !trimmedCompatibleModel) {
        return null;
      }

      return {
        modelProvider,
        gemma: {
          modelSource,
          modelRepo: trimmedModelRepo || DEFAULT_GEMMA_MODEL_REPO,
          modelFile: trimmedModelFile || MODEL_PRESETS.q4.modelFile,
          ...(trimmedLocalModelPath ? { localModelPath: trimmedLocalModelPath } : {}),
          ...(trimmedLocalMmprojPath ? { localMmprojPath: trimmedLocalMmprojPath } : {}),
          gpuLayers: gpuLayersValid ? parsedGpuLayers : safeInitialSettings.gemma.gpuLayers
        },
        codex: {
          model: trimmedCodexModel || safeInitialSettings.codex.model,
          reasoningEffort: codexReasoningEffort,
          oauthPort: codexOauthPortValid ? parsedCodexOauthPort : safeInitialSettings.codex.oauthPort
        },
        openAICompatible: {
          baseUrl: trimmedCompatibleBaseUrl,
          apiKey: trimmedCompatibleApiKey,
          model: trimmedCompatibleModel
        },
        translationMode,
        nsfwMode
      };
    }

    if (!gpuLayersValid) {
      return null;
    }

    return {
      modelProvider,
      gemma: {
        modelSource,
        modelRepo: trimmedModelRepo || DEFAULT_GEMMA_MODEL_REPO,
        modelFile: trimmedModelFile || MODEL_PRESETS.q4.modelFile,
        ...(trimmedLocalModelPath ? { localModelPath: trimmedLocalModelPath } : {}),
        ...(trimmedLocalMmprojPath ? { localMmprojPath: trimmedLocalMmprojPath } : {}),
        gpuLayers: parsedGpuLayers
      },
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
      nsfwMode
    };
  }, [
    modelProvider,
    gpuLayersValid,
    codexOauthPortValid,
    modelSource,
    trimmedModelRepo,
    trimmedModelFile,
    trimmedLocalModelPath,
    trimmedLocalMmprojPath,
    trimmedCodexModel,
    trimmedCompatibleBaseUrl,
    trimmedCompatibleApiKey,
    trimmedCompatibleModel,
    parsedGpuLayers,
    parsedCodexOauthPort,
    compatibleBaseUrlValid,
    codexReasoningEffort,
    safeInitialSettings.gemma.gpuLayers,
    safeInitialSettings.codex.model,
    safeInitialSettings.codex.oauthPort,
    safeInitialSettings.openAICompatible.baseUrl,
    safeInitialSettings.openAICompatible.model,
    translationMode,
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

  const handleGpuLayersInputChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    clearTestState();
    const nextValue = event.target.value;
    if (!nextValue) {
      setGpuLayers("");
      return;
    }

    const parsed = Number(nextValue);
    if (!Number.isFinite(parsed)) {
      setGpuLayers(nextValue);
      return;
    }

    if (parsed < 0) {
      setGpuLayers("0");
      return;
    }

    if (parsed > MAX_GPU_LAYERS) {
      setGpuLayers(String(MAX_GPU_LAYERS));
      return;
    }

    setGpuLayers(nextValue);
  };

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
        detail: buildTestDetail(result.resolvedModelPath, result.resolvedMmprojPath, result.resolvedEndpoint)
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
            translationMode={translationMode}
            onClearTestState={clearTestState}
            onModelProviderChange={setModelProvider}
            onNsfwModeChange={setNsfwMode}
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

          {modelProvider === "gemma" ? (
            <GemmaModelSettingsSection
              controlsBusy={controlsBusy}
              customModelFile={customModelFile}
              customModelRepo={customModelRepo}
              gpuLayers={gpuLayers}
              gpuSliderRef={gpuSliderRef}
              localMmprojPath={localMmprojPath}
              localModelInputRef={localModelInputRef}
              localModelPath={localModelPath}
              modelRepoInputRef={modelRepoInputRef}
              modelSource={modelSource}
              selectedPreset={selectedPreset}
              sliderValue={sliderValue}
              onClearTestState={clearTestState}
              onCustomModelFileChange={setCustomModelFile}
              onCustomModelRepoChange={setCustomModelRepo}
              onGpuLayersChange={setGpuLayers}
              onGpuLayersInputChange={handleGpuLayersInputChange}
              onLocalMmprojPathChange={setLocalMmprojPath}
              onLocalModelPathChange={setLocalModelPath}
              onModelSourceChange={setModelSource}
              onSelectedPresetChange={setSelectedPreset}
              onSubmit={submit}
            />
          ) : modelProvider === "openai-codex" ? (
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
            gpuLayersValid={gpuLayersValid}
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
