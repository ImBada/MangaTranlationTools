import type { TranslationOptions } from "./appSettings";
import { logInfo, logWarn } from "./logger";

export type OpenAICompatibleEndpoint = {
  baseUrl: string;
  child: null;
  startedByScript: false;
  provider: "openai-compatible";
  closed?: boolean;
};

export async function createOpenAICompatibleEndpoint(options: TranslationOptions): Promise<OpenAICompatibleEndpoint> {
  const baseUrl = normalizeBaseUrl(options.openAICompatibleBaseUrl);
  await verifyEndpoint(baseUrl, options);

  logInfo("OpenAI-compatible endpoint ready", {
    label: options.label,
    baseUrl,
    model: options.openAICompatibleModel
  });

  return {
    baseUrl,
    child: null,
    startedByScript: false,
    provider: "openai-compatible"
  };
}

export async function stopOpenAICompatibleEndpoint(endpoint: OpenAICompatibleEndpoint | null | undefined): Promise<void> {
  if (endpoint) {
    endpoint.closed = true;
  }
}

function normalizeBaseUrl(value: string): string {
  return value.trim().replace(/\/+$/, "");
}

async function verifyEndpoint(baseUrl: string, options: TranslationOptions): Promise<void> {
  const verifyUrl = `${baseUrl}/models`;
  let response: Response;
  try {
    response = await fetch(verifyUrl, {
      headers: buildHeaders(options),
      signal: AbortSignal.timeout(30000)
    });
  } catch (error) {
    throw createDetailedError(
      "커스텀 OpenAI 호환 엔드포인트에 연결하지 못했습니다. Base URL을 확인하세요.",
      { baseUrl, verifyUrl, model: options.openAICompatibleModel },
      error
    );
  }

  const rawText = await response.text();
  if (!response.ok) {
    const isProbablyHtml = rawText.trimStart().toLowerCase().startsWith("<!doctype") || rawText.trimStart().toLowerCase().startsWith("<html");
    const isRateLimit = response.status === 429;

    let message = isRateLimit
      ? `커스텀 OpenAI 호환 모델 목록을 확인하지 못했습니다 (429 Too Many Requests). Base URL이 웹사이트 주소인지 확인하세요.`
      : `커스텀 OpenAI 호환 모델 목록을 확인하지 못했습니다.`;

    if (isProbablyHtml) {
      message += " 응답이 HTML 페이지를 반환하고 있습니다. 이는 API 서버가 아닌 웹사이트 주소일 가능성이 높습니다.";
      if (baseUrl.includes("ollama.com") && !baseUrl.includes("11434")) {
        message += " Ollama를 사용 중이라면 로컬 주소인 http://localhost:11434/v1 를 입력하세요.";
      }
    }

    throw createDetailedError(message, {
      baseUrl,
      verifyUrl,
      status: response.status,
      statusText: response.statusText,
      rawTextPreview: truncateText(rawText)
    });
  }

  const availableModels = parseModelIds(rawText);
  if (availableModels.length > 0 && !availableModels.includes(options.openAICompatibleModel)) {
    logWarn("Selected model was not advertised by OpenAI-compatible endpoint", {
      selectedModel: options.openAICompatibleModel,
      availableModels
    });
  }
}

function buildHeaders(options: TranslationOptions): HeadersInit {
  const headers: Record<string, string> = {};
  const apiKey = options.openAICompatibleApiKey.trim();
  if (apiKey) {
    headers.Authorization = `Bearer ${apiKey}`;
  }
  return headers;
}

function parseModelIds(rawText: string): string[] {
  try {
    const parsed = JSON.parse(rawText);
    if (!Array.isArray(parsed?.data)) {
      return [];
    }
    return parsed.data.map((item: { id?: unknown }) => item.id).filter((id: unknown): id is string => typeof id === "string");
  } catch {
    return [];
  }
}

function truncateText(value: string, maxLength = 4000): string {
  return value.length <= maxLength ? value : `${value.slice(0, maxLength)}... [truncated ${value.length - maxLength} chars]`;
}

function createDetailedError(message: string, detail: Record<string, unknown> = {}, cause?: unknown): Error {
  const error = new Error(message);
  if (cause !== undefined) {
    error.cause = cause;
  }
  Object.assign(error, detail);
  return error;
}
