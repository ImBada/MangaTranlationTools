import type React from "react";

export function rangeProgressStyle(value: number, min: number, max: number): React.CSSProperties {
  const ratio = max === min ? 0 : (value - min) / (max - min);
  const percent = Math.min(100, Math.max(0, ratio * 100));
  return { "--range-progress": `${percent}%` } as React.CSSProperties;
}
