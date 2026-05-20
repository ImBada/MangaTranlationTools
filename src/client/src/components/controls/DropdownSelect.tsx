import React from "react";

export type DropdownSelectOption = {
  value: string;
  label: React.ReactNode;
  disabled?: boolean;
};

type DropdownSelectProps = {
  value: string;
  options: DropdownSelectOption[];
  disabled?: boolean;
  ariaLabel?: string;
  className?: string;
  buttonClassName?: string;
  onChange: (value: string) => void;
};

type DropdownPopoverStyle = {
  left: number;
  top: number;
  width: number;
  maxHeight: number;
};

const DROPDOWN_POPOVER_GAP_PX = 6;
const DROPDOWN_POPOVER_MIN_WIDTH_PX = 160;
const DROPDOWN_POPOVER_VIEWPORT_MARGIN_PX = 12;

export function DropdownSelect({
  value,
  options,
  disabled = false,
  ariaLabel,
  className,
  buttonClassName,
  onChange
}: DropdownSelectProps): React.JSX.Element {
  const [open, setOpen] = React.useState(false);
  const [popoverStyle, setPopoverStyle] = React.useState<DropdownPopoverStyle | null>(null);
  const rootRef = React.useRef<HTMLDivElement | null>(null);
  const triggerRef = React.useRef<HTMLButtonElement | null>(null);
  const selectedOption = options.find((option) => option.value === value) ?? options[0];

  const updatePopoverBounds = React.useCallback(() => {
    const anchorRect = rootRef.current?.getBoundingClientRect();
    if (!anchorRect) {
      return;
    }

    const nextStyle = resolveDropdownPopoverStyle(anchorRect);
    setPopoverStyle((currentStyle) =>
      currentStyle &&
      currentStyle.left === nextStyle.left &&
      currentStyle.top === nextStyle.top &&
      currentStyle.width === nextStyle.width &&
      currentStyle.maxHeight === nextStyle.maxHeight
        ? currentStyle
        : nextStyle
    );
  }, []);

  const closeAndReleaseFocus = React.useCallback(() => {
    setOpen(false);
    setPopoverStyle(null);
    window.setTimeout(() => triggerRef.current?.blur(), 0);
  }, []);

  const selectValue = React.useCallback((nextValue: string) => {
    if (nextValue !== value) {
      onChange(nextValue);
    }
    closeAndReleaseFocus();
  }, [closeAndReleaseFocus, onChange, value]);

  React.useEffect(() => {
    if (!open) {
      return;
    }

    const handlePointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) {
        closeAndReleaseFocus();
      }
    };
    window.addEventListener("pointerdown", handlePointerDown);
    return () => window.removeEventListener("pointerdown", handlePointerDown);
  }, [closeAndReleaseFocus, open]);

  React.useLayoutEffect(() => {
    if (!open) {
      return;
    }

    updatePopoverBounds();
    window.addEventListener("resize", updatePopoverBounds);
    window.addEventListener("scroll", updatePopoverBounds, true);

    const resizeObserver = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(updatePopoverBounds);
    if (rootRef.current) {
      resizeObserver?.observe(rootRef.current);
    }

    return () => {
      window.removeEventListener("resize", updatePopoverBounds);
      window.removeEventListener("scroll", updatePopoverBounds, true);
      resizeObserver?.disconnect();
    };
  }, [open, updatePopoverBounds]);

  const rootClassName = [
    "dropdown-select",
    open ? "open" : "",
    className ?? ""
  ].filter(Boolean).join(" ");

  return (
    <div className={rootClassName} ref={rootRef}>
      <button
        ref={triggerRef}
        type="button"
        className={buttonClassName ? `font-picker-trigger dropdown-select-trigger ${buttonClassName}` : "font-picker-trigger dropdown-select-trigger"}
        disabled={disabled}
        aria-label={ariaLabel}
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen((current) => !current)}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            closeAndReleaseFocus();
          }
        }}
      >
        <span className="dropdown-select-current">{selectedOption?.label ?? value}</span>
      </button>
      {open ? (
        <div className="font-picker-popover dropdown-select-popover" style={popoverStyle ?? undefined} role="listbox" aria-label={ariaLabel}>
          {options.map((option) => (
            <button
              key={option.value}
              type="button"
              className={option.value === value ? "dropdown-select-option active" : "dropdown-select-option"}
              role="option"
              aria-selected={option.value === value}
              disabled={option.disabled}
              onClick={() => selectValue(option.value)}
            >
              <span className="dropdown-select-option-label">{option.label}</span>
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function resolveDropdownPopoverStyle(anchorRect: DOMRect): DropdownPopoverStyle {
  const viewportWidth = window.innerWidth;
  const viewportHeight = window.innerHeight;
  const availableBelow = viewportHeight - anchorRect.bottom - DROPDOWN_POPOVER_VIEWPORT_MARGIN_PX - DROPDOWN_POPOVER_GAP_PX;
  const availableAbove = anchorRect.top - DROPDOWN_POPOVER_VIEWPORT_MARGIN_PX - DROPDOWN_POPOVER_GAP_PX;
  const openAbove = availableBelow < 180 && availableAbove > availableBelow;
  const maxHeight = Math.max(120, Math.floor(openAbove ? availableAbove : availableBelow));
  const width = Math.min(
    Math.max(anchorRect.width, DROPDOWN_POPOVER_MIN_WIDTH_PX),
    viewportWidth - DROPDOWN_POPOVER_VIEWPORT_MARGIN_PX * 2
  );
  const left = Math.min(
    Math.max(DROPDOWN_POPOVER_VIEWPORT_MARGIN_PX, anchorRect.left),
    viewportWidth - DROPDOWN_POPOVER_VIEWPORT_MARGIN_PX - width
  );
  const top = openAbove
    ? Math.max(DROPDOWN_POPOVER_VIEWPORT_MARGIN_PX, anchorRect.top - DROPDOWN_POPOVER_GAP_PX - maxHeight)
    : Math.min(anchorRect.bottom + DROPDOWN_POPOVER_GAP_PX, viewportHeight - DROPDOWN_POPOVER_VIEWPORT_MARGIN_PX - maxHeight);

  return { left, top, width, maxHeight };
}
