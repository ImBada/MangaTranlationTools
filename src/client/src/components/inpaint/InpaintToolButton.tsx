import React from "react";
import type { InpaintResultTool } from "../InpaintResultCanvas";

type InpaintToolButtonProps = {
  active: boolean;
  disabled: boolean;
  icon: InpaintResultTool;
  label: string;
  onClick: () => void;
  shortcut?: string;
  text?: string;
};

function InpaintToolIcon({ name }: { name: InpaintResultTool }): React.JSX.Element {
  switch (name) {
    case "select":
      return (
        <svg className="tool-option-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
          <rect x="4" y="4" width="11" height="11" rx="2" strokeDasharray="2.4 2.4" />
          <path d="M13 12l6 6-3 1-1 3-6-6 4-4z" />
        </svg>
      );
    case "brush":
      return (
        <svg className="tool-option-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
          <path d="M14 5l5 5-8.5 8.5a4 4 0 0 1-5.7 0l-.3-.3L14 5z" />
          <path d="M16 3l5 5" />
          <path d="M5 18c-.5 1.6-1.5 2.5-3 2.8 2.4 1.1 5 .7 6.7-1" />
        </svg>
      );
    case "eraser":
      return (
        <svg className="tool-option-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
          <path d="M6 15l8.5-8.5a2.1 2.1 0 0 1 3 0l2 2a2.1 2.1 0 0 1 0 3L12 19H7l-3-3 2-1z" />
          <path d="M10 11l5 5" />
          <path d="M12 19h8" />
        </svg>
      );
    case "blur":
      return (
        <svg className="tool-option-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
          <circle cx="12" cy="12" r="3.2" />
          <circle cx="6" cy="12" r="1.5" />
          <circle cx="18" cy="12" r="1.5" />
          <circle cx="12" cy="6" r="1.5" />
          <circle cx="12" cy="18" r="1.5" />
        </svg>
      );
    case "sharpen":
      return (
        <svg className="tool-option-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
          <path d="M12 3l7 17H5L12 3z" />
          <path d="M12 8v6" />
          <path d="M9.5 17h5" />
        </svg>
      );
    case "smudge":
      return (
        <svg className="tool-option-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">
          <path d="M5 7c3-3 6.5-3 9.2-.5 2.5 2.3 1 5.8-2.2 5.4-2.1-.3-2.5-2.3-1-3.2" />
          <path d="M4 16c3.5-2.5 6.7-2.3 9.5 0 2.1 1.7 4.1 1.7 6.5-.2" />
          <path d="M4 20c4-1.8 7.2-1.6 10 .5" />
        </svg>
      );
  }
}

export function InpaintToolButton({ active, disabled, icon, label, onClick, shortcut, text }: InpaintToolButtonProps): React.JSX.Element {
  const title = shortcut ? `${label} (${shortcut})` : label;
  const displayLabel = text ?? label;
  return (
    <button
      type="button"
      className={`tool-option ${active ? "active" : ""}`}
      aria-pressed={active}
      aria-label={title}
      aria-keyshortcuts={shortcut}
      title={title}
      onClick={onClick}
      disabled={disabled}
    >
      <InpaintToolIcon name={icon} />
      <span className="tool-option-label">{shortcut ? `${displayLabel} (${shortcut})` : displayLabel}</span>
    </button>
  );
}
