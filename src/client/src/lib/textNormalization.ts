export function normalizeKoreanText(value: string): string {
  return value.replace(/\.{3,}/g, "…");
}
