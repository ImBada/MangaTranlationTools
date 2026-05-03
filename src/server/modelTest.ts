import { randomUUID } from "node:crypto";
import { join } from "node:path";
import { buildBaseTranslationOptions } from "./appSettings";
import type { AppPaths } from "./appPaths";
import { getActiveJob } from "./jobState";
import { createOpenAICompatibleEndpoint, stopOpenAICompatibleEndpoint, type OpenAICompatibleEndpoint } from "./openaiCompatibleEndpoint";
import { startOpenAIOAuthEndpoint, stopOpenAIOAuthEndpoint, type OpenAIOAuthEndpoint } from "./openaiOauthEndpoint";
import type { AppSettings, ModelTestResult } from "../shared/types";

type SimplePageRuntime = {
  testModelReply: (server: { baseUrl: string }, options: Record<string, unknown>) => Promise<{
    outputText: string;
    launchMode: ModelTestResult["launchMode"];
  }>;
};

let cachedSimplePageRuntime: SimplePageRuntime | null = null;

export async function testModelSettings(settings: AppSettings, appPaths: AppPaths): Promise<ModelTestResult> {
  if (getActiveJob()) {
    return { ok: false, message: "번역 작업 중에는 모델 테스트를 실행할 수 없습니다.", launchMode: settings.modelProvider };
  }

  const testId = randomUUID();
  const options = buildBaseTranslationOptions({
    jobId: `settings-test-${testId}`,
    runDir: join(appPaths.dataRoot, "model-tests", testId),
    settings
  });
  let server: OpenAIOAuthEndpoint | OpenAICompatibleEndpoint | null = null;

  try {
    server = options.modelProvider === "openai-compatible"
      ? await createOpenAICompatibleEndpoint(options)
      : await startOpenAIOAuthEndpoint(options);
    const result = await loadSimplePageRuntime(appPaths).testModelReply(server, options);
    return {
      ok: true,
      message: `모델 응답 확인 완료: ${result.outputText}`,
      launchMode: result.launchMode,
      resolvedEndpoint: server.baseUrl
    };
  } catch (error) {
    return {
      ok: false,
      message: error instanceof Error ? error.message : String(error),
      launchMode: options.modelProvider
    };
  } finally {
    if (server) {
      await stopModelTestServer(server);
    }
  }
}

async function stopModelTestServer(
  server: OpenAIOAuthEndpoint | OpenAICompatibleEndpoint
): Promise<void> {
  if (isOpenAICompatibleEndpoint(server)) {
    await stopOpenAICompatibleEndpoint(server);
    return;
  }
  await stopOpenAIOAuthEndpoint(server);
}

function isOpenAICompatibleEndpoint(
  server: OpenAIOAuthEndpoint | OpenAICompatibleEndpoint
): server is OpenAICompatibleEndpoint {
  return "provider" in server && server.provider === "openai-compatible";
}

function loadSimplePageRuntime(appPaths: AppPaths): SimplePageRuntime {
  if (!cachedSimplePageRuntime) {
    cachedSimplePageRuntime = require(join(appPaths.runtimeDir, "simple-page-translate.cjs")) as SimplePageRuntime;
  }
  return cachedSimplePageRuntime;
}
