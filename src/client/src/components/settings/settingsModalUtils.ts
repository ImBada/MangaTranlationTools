import type { AppSettings, UpdateStatus } from "../../../../shared/types";
import {
  DEFAULT_OPENAI_COMPATIBLE_BASE_URL,
  DEFAULT_OPENAI_COMPATIBLE_MODEL
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

export function buildTestDetail(endpoint: string | null | undefined): string | null {
  const lines = [endpoint ? `엔드포인트: ${endpoint}` : null].filter(Boolean);

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
