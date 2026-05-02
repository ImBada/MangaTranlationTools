import type { AppSettings, UpdateStatus } from "../../../../shared/types";
import {
  DEFAULT_GEMMA_MODEL_REPO,
  DEFAULT_OPENAI_COMPATIBLE_BASE_URL,
  DEFAULT_OPENAI_COMPATIBLE_MODEL,
  MAX_GPU_LAYERS,
  MODEL_PRESETS,
  type ModelPresetId
} from "./settingsModalConfig";

export function resolveUpdateStatusText(status: UpdateStatus | null, busy: boolean): string {
  if (busy && !status) {
    return "업데이트 확인 중...";
  }
  if (!status) {
    return "업데이트 확인 대기 중";
  }
  if (status.error) {
    return `업데이트 확인 실패: ${status.error}`;
  }
  if (status.updateAvailable && status.latestVersion) {
    return `새 버전 v${status.latestVersion} 사용 가능`;
  }
  if (status.latestVersion) {
    return "최신 버전입니다.";
  }
  return "최신 릴리스 정보를 찾지 못했습니다.";
}

export function resolveModelPreset(modelRepo: string, modelFile: string): ModelPresetId {
  const trimmedModelRepo = modelRepo.trim();
  const trimmedModelFile = modelFile.trim();

  if (matchesPreset(MODEL_PRESETS.q4, trimmedModelRepo, trimmedModelFile)) {
    return "q4";
  }

  if (matchesPreset(MODEL_PRESETS.q3, trimmedModelRepo, trimmedModelFile)) {
    return "q3";
  }

  if (matchesPreset(MODEL_PRESETS.q6, trimmedModelRepo, trimmedModelFile)) {
    return "q6";
  }

  return "custom";
}

function matchesPreset(
  preset: (typeof MODEL_PRESETS)[keyof typeof MODEL_PRESETS],
  modelRepo: string,
  modelFile: string
): boolean {
  return preset.modelRepo === modelRepo && preset.modelFile === modelFile;
}

export function clampGpuLayers(value: number): number {
  return Math.min(MAX_GPU_LAYERS, Math.max(0, value));
}

export function withSettingsDefaults(settings: AppSettings): AppSettings {
  return {
    ...settings,
    openAICompatible: {
      baseUrl: settings.openAICompatible?.baseUrl || DEFAULT_OPENAI_COMPATIBLE_BASE_URL,
      apiKey: settings.openAICompatible?.apiKey || "",
      model: settings.openAICompatible?.model || DEFAULT_OPENAI_COMPATIBLE_MODEL
    }
  };
}

export function buildTestDetail(
  modelPath: string | null | undefined,
  mmprojPath: string | null | undefined,
  endpoint: string | null | undefined
): string | null {
  const lines = [
    modelPath ? `모델: ${modelPath}` : null,
    mmprojPath ? `mmproj: ${mmprojPath}` : null,
    endpoint ? `엔드포인트: ${endpoint}` : null
  ].filter(Boolean);

  return lines.length > 0 ? lines.join("\n") : null;
}

export type TestState =
  | {
      status: "idle";
      message: null;
      detail: null;
    }
  | {
      status: "running" | "success" | "error";
      message: string;
      detail: string | null;
    };

export type LamaActionState = {
  status: "idle" | "running" | "success" | "error";
  message: string | null;
};
