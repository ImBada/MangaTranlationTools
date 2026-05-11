import type { TranslationBlock } from "../../../shared/types";

export const BLOCK_TYPE_LABELS: Record<TranslationBlock["type"], string> = {
  caption: "자막",
  other: "기타",
  sfx: "효과음",
  speech: "말풍선"
};

export function resolveBlockPreviewText(block: TranslationBlock): string {
  const text = (block.translatedText || block.sourceText).replace(/\s+/g, " ").trim();
  return text || "빈 블록";
}
