import type { CodexReasoningEffort, ModelProvider, ModelSource, TranslationMode } from "../../../../shared/types";

export const MAX_GPU_LAYERS = 30;
export const DEFAULT_GEMMA_MODEL_REPO = "unsloth/gemma-4-26B-A4B-it-GGUF";
export const DEFAULT_OPENAI_COMPATIBLE_BASE_URL = "http://127.0.0.1:11434/v1";
export const DEFAULT_OPENAI_COMPATIBLE_MODEL = "gemma4:31b";

export const MODEL_PRESETS = {
  q3: {
    label: "Q3_K_XL",
    modelRepo: DEFAULT_GEMMA_MODEL_REPO,
    modelFile: "gemma-4-26B-A4B-it-UD-Q3_K_XL.gguf"
  },
  q4: {
    label: "Q4_K_XL",
    modelRepo: DEFAULT_GEMMA_MODEL_REPO,
    modelFile: "gemma-4-26B-A4B-it-UD-Q4_K_XL.gguf"
  },
  q6: {
    label: "Q6_K_XL",
    modelRepo: DEFAULT_GEMMA_MODEL_REPO,
    modelFile: "gemma-4-26B-A4B-it-UD-Q6_K_XL.gguf"
  }
} as const;

export type ModelPresetId = keyof typeof MODEL_PRESETS | "custom";

type TranslationModeOption = {
  id: TranslationMode;
  label: string;
  description: string;
};

type ModelSourceOption = {
  id: ModelSource;
  label: string;
  description: string;
};

type ModelProviderOption = {
  id: ModelProvider;
  label: string;
  description: string;
};

type CodexReasoningOption = {
  id: CodexReasoningEffort;
  label: string;
  description: string;
};

export const TRANSLATION_MODE_OPTIONS: TranslationModeOption[] = [
  {
    id: "fast",
    label: "빠름",
    description: "원본 이미지만 보내고 토큰 예산을 줄여 더 빠르게 처리합니다."
  },
  {
    id: "accuracy",
    label: "정확성",
    description: "고대비 보조 이미지를 함께 보내고 더 넉넉한 토큰 예산을 사용합니다."
  }
];

export const MODEL_SOURCE_OPTIONS: ModelSourceOption[] = [
  {
    id: "huggingface",
    label: "HF repo",
    description: "기본 프리셋이나 Hugging Face repo/GGUF 파일명을 사용합니다."
  },
  {
    id: "local",
    label: "로컬 파일",
    description: "이미 가지고 있는 GGUF 모델과 mmproj를 직접 지정합니다."
  }
];

export const MODEL_PROVIDER_OPTIONS: ModelProviderOption[] = [
  {
    id: "openai-codex",
    label: "OpenAI API",
    description: "Codex 로그인 토큰을 쓰는 openai-oauth 엔드포인트로 요청합니다."
  },
  {
    id: "openai-compatible",
    label: "커스텀",
    description: "OpenAI 호환 /v1/chat/completions 엔드포인트로 요청합니다."
  }
];

export const CODEX_REASONING_OPTIONS: CodexReasoningOption[] = [
  {
    id: "none",
    label: "없음",
    description: "생각 예산을 쓰지 않고 가장 빠르게 응답합니다."
  },
  {
    id: "low",
    label: "낮음",
    description: "가벼운 추론으로 처리합니다."
  },
  {
    id: "medium",
    label: "보통",
    description: "기본 균형 설정입니다."
  },
  {
    id: "high",
    label: "높음",
    description: "더 오래 생각해서 까다로운 페이지를 처리합니다."
  },
  {
    id: "xhigh",
    label: "최고",
    description: "가장 넉넉한 생각 예산을 사용합니다."
  }
];
