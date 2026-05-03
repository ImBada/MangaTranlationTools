import { randomUUID } from "node:crypto";
import { join } from "node:path";
import { buildBaseTranslationOptions, type TranslationOptions } from "./appSettings";
import type { AppPaths } from "./appPaths";
import { getActiveJob } from "./jobState";
import { createOpenAICompatibleEndpoint, stopOpenAICompatibleEndpoint, type OpenAICompatibleEndpoint } from "./openaiCompatibleEndpoint";
import { startOpenAIOAuthEndpoint, stopOpenAIOAuthEndpoint, type OpenAIOAuthEndpoint } from "./openaiOauthEndpoint";
import type { AppSettings, ModelTestResult } from "../shared/types";

type SimplePageRuntime = {
  startServer: (options: Record<string, unknown>) => Promise<{ baseUrl: string; child: unknown; startedByScript: boolean }>;
  stopServer: (server: { baseUrl: string; child: unknown; startedByScript: boolean } | null | undefined) => Promise<void>;
  testModelReply: (server: { baseUrl: string }, options: Record<string, unknown>) => Promise<{
    outputText: string;
    launchTarget: { launchMode: string; modelPath?: string | null; mmprojPath?: string | null };
  }>;
};

let cachedSimplePageRuntime: SimplePageRuntime | null = null;

export async function testModelSettings(settings: AppSettings, appPaths: AppPaths): Promise<ModelTestResult> {
  if (getActiveJob()) {
    return { ok: false, message: "번역 작업 중에는 모델 테스트를 실행할 수 없습니다.", launchMode: modelProviderToTestLaunchMode(settings.modelProvider) };
  }

  const testId = randomUUID();
  const options = buildBaseTranslationOptions({
    jobId: `settings-test-${testId}`,
    runDir: join(appPaths.dataRoot, "model-tests", testId),
    paths: appPaths,
    settings
  });
  let server: OpenAIOAuthEndpoint | OpenAICompatibleEndpoint | { baseUrl: string; child: unknown; startedByScript: boolean } | null = null;

  try {
    server = options.modelProvider === "openai-compatible"
      ? await createOpenAICompatibleEndpoint(options)
      : options.modelProvider === "openai-codex"
        ? await startOpenAIOAuthEndpoint(options)
        : await loadSimplePageRuntime(appPaths).startServer(options);
    const result = await loadSimplePageRuntime(appPaths).testModelReply(server, options);
    return {
      ok: true,
      message: `모델 응답 확인 완료: ${result.outputText}`,
      launchMode: result.launchTarget.launchMode as ModelTestResult["launchMode"],
      resolvedEndpoint: server.baseUrl
    };
  } catch (error) {
    return {
      ok: false,
      message: error instanceof Error ? error.message : String(error),
      launchMode: modelProviderToTestLaunchMode(options.modelProvider)
    };
  } finally {
    if (server) {
      await stopModelTestServer(server, options, appPaths);
    }
  }
}

async function stopModelTestServer(
  server: OpenAIOAuthEndpoint | OpenAICompatibleEndpoint | { baseUrl: string; child: unknown; startedByScript: boolean },
  _options: TranslationOptions,
  appPaths: AppPaths
): Promise<void> {
  if (isOpenAICompatibleEndpoint(server)) {
    await stopOpenAICompatibleEndpoint(server);
  } else if (isOpenAIOAuthEndpoint(server)) {
    await stopOpenAIOAuthEndpoint(server);
  } else {
    await loadSimplePageRuntime(appPaths).stopServer(server);
  }
}

function isOpenAICompatibleEndpoint(
  server: OpenAIOAuthEndpoint | OpenAICompatibleEndpoint | { baseUrl: string; child: unknown; startedByScript: boolean }
): server is OpenAICompatibleEndpoint {
  return "provider" in server && server.provider === "openai-compatible";
}

function isOpenAIOAuthEndpoint(
  server: OpenAIOAuthEndpoint | OpenAICompatibleEndpoint | { baseUrl: string; child: unknown; startedByScript: boolean }
): server is OpenAIOAuthEndpoint {
  return "provider" in server && server.provider === "openai-codex";
}

function modelProviderToTestLaunchMode(modelProvider: AppSettings["modelProvider"]): ModelTestResult["launchMode"] {
  if (modelProvider === "openai-compatible" || modelProvider === "openai-codex") {
    return modelProvider;
  }
  return "huggingface";
}

function loadSimplePageRuntime(appPaths: AppPaths): SimplePageRuntime {
  if (!cachedSimplePageRuntime) {
    cachedSimplePageRuntime = require(join(appPaths.runtimeDir, "simple-page-translate.cjs")) as SimplePageRuntime;
  }
  return cachedSimplePageRuntime;
}
