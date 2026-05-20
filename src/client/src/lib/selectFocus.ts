import type React from "react";

export function blurSelectAfterChange<T extends HTMLSelectElement>(event: React.ChangeEvent<T>): string {
  const { value } = event.currentTarget;
  event.currentTarget.blur();
  return value;
}
