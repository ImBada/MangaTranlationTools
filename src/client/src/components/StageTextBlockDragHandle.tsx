import React from "react";

export function StageTextBlockDragHandle(): React.JSX.Element {
  return (
    <span className="stage-text-block-list-drag-handle" aria-hidden="true">
      <svg viewBox="0 0 12 18" focusable="false">
        <circle cx="4" cy="4" r="1.2" />
        <circle cx="8" cy="4" r="1.2" />
        <circle cx="4" cy="9" r="1.2" />
        <circle cx="8" cy="9" r="1.2" />
        <circle cx="4" cy="14" r="1.2" />
        <circle cx="8" cy="14" r="1.2" />
      </svg>
    </span>
  );
}
