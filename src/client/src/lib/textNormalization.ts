export function normalizeKoreanText(value: string): string {
  return value.replace(/\.{3,}/g, (dots) => "…".repeat(Math.floor(dots.length / 3))).replace(/--/g, "—");
}
