import React from "react";

export function FontPresetLinkIcon({ linked }: { linked: boolean }): React.JSX.Element {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
      <path d="M10 13a5 5 0 0 0 7.5.5l2.1-2.1a5 5 0 0 0-7.1-7.1l-1.2 1.2" />
      <path d="M14 11a5 5 0 0 0-7.5-.5l-2.1 2.1a5 5 0 0 0 7.1 7.1l1.2-1.2" />
      {linked ? null : <path d="M3 3l18 18" />}
    </svg>
  );
}
