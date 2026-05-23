import React from "react";
import { createPortal } from "react-dom";

type HsvColor = {
  h: number;
  s: number;
  v: number;
};

type ColorPickerPopoverStyle = {
  left: number;
  top: number;
};

type FastColorPickerProps = {
  ariaLabel: string;
  disabled?: boolean;
  value: string;
  onChange: (value: string) => void;
};

const COLOR_PICKER_POPOVER_GAP_PX = 6;
const COLOR_PICKER_POPOVER_WIDTH_PX = 232;
const COLOR_PICKER_POPOVER_VIEWPORT_MARGIN_PX = 10;
const HEX_COLOR_PATTERN = /^#[\da-f]{6}$/i;
const QUICK_COLORS = [
  "#111111",
  "#ffffff",
  "#ef4444",
  "#f59e0b",
  "#22c55e",
  "#3b82f6",
  "#8b5cf6",
  "#ec4899"
];

export function FastColorPicker({ ariaLabel, disabled = false, value, onChange }: FastColorPickerProps): React.JSX.Element {
  const [open, setOpen] = React.useState(false);
  const [draftHex, setDraftHex] = React.useState(normalizeHexColor(value));
  const [localColor, setLocalColor] = React.useState(normalizeHexColor(value));
  const [popoverStyle, setPopoverStyle] = React.useState<ColorPickerPopoverStyle | null>(null);
  const [draftHsv, setDraftHsv] = React.useState(() => hexToHsv(normalizeHexColor(value)));
  const rootRef = React.useRef<HTMLSpanElement | null>(null);
  const popoverRef = React.useRef<HTMLDivElement | null>(null);
  const hexInputRef = React.useRef<HTMLInputElement | null>(null);
  const draggingRef = React.useRef(false);
  const localColorRef = React.useRef(localColor);
  const normalizedValue = normalizeHexColor(value);
  const lastExternalValueRef = React.useRef(normalizedValue);
  const normalizedValueRef = React.useRef(normalizedValue);
  const pendingCommittedValueRef = React.useRef<string | null>(null);
  const displayColor = open ? localColor : normalizedValue;
  const externalHsv = React.useMemo(() => hexToHsv(normalizedValue), [normalizedValue]);
  const hsv = open ? draftHsv : externalHsv;

  React.useEffect(() => {
    normalizedValueRef.current = normalizedValue;
  }, [normalizedValue]);

  React.useEffect(() => {
    if (lastExternalValueRef.current === normalizedValue) {
      if (!open) {
        syncDraftColor(normalizedValue, setLocalColor, setDraftHex, setDraftHsv);
      }
      return;
    }
    lastExternalValueRef.current = normalizedValue;
    if (pendingCommittedValueRef.current === normalizedValue) {
      pendingCommittedValueRef.current = null;
      if (!open) {
        syncDraftColor(normalizedValue, setLocalColor, setDraftHex, setDraftHsv);
      }
      return;
    }
    pendingCommittedValueRef.current = null;
    draggingRef.current = false;
    localColorRef.current = normalizedValue;
    syncDraftColor(normalizedValue, setLocalColor, setDraftHex, setDraftHsv);
    setOpen(false);
  }, [normalizedValue, open]);

  React.useEffect(() => {
    localColorRef.current = localColor;
  }, [localColor]);

  const updatePopoverBounds = React.useCallback(() => {
    const anchorRect = rootRef.current?.getBoundingClientRect();
    if (!anchorRect) {
      return;
    }
    const nextStyle = resolveColorPickerPopoverStyle(anchorRect);
    setPopoverStyle((currentStyle) =>
      currentStyle && currentStyle.left === nextStyle.left && currentStyle.top === nextStyle.top
        ? currentStyle
        : nextStyle
    );
  }, []);

  const commitColor = React.useCallback((color: string) => {
    const nextColor = normalizeHexColor(color);
    const previousColor = normalizedValueRef.current;
    const changed = nextColor !== previousColor;
    const nextHsv = hexToHsv(nextColor);

    localColorRef.current = nextColor;
    setLocalColor(nextColor);
    setDraftHex(nextColor);
    setDraftHsv((currentHsv) => nextHsv.s > 0 ? nextHsv : { ...nextHsv, h: currentHsv.h });
    if (changed) {
      normalizedValueRef.current = nextColor;
      pendingCommittedValueRef.current = nextColor;
      onChange(nextColor);
    }
  }, [onChange]);

  const commitLocalColor = React.useCallback(() => {
    commitColor(localColorRef.current);
  }, [commitColor]);

  const finishActiveDrag = React.useCallback(() => {
    if (!draggingRef.current) {
      return;
    }
    commitLocalColor();
    draggingRef.current = false;
  }, [commitLocalColor]);

  React.useEffect(() => {
    if (!open) {
      return;
    }

    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (
        !draggingRef.current &&
        !rootRef.current?.contains(target) &&
        !popoverRef.current?.contains(target)
      ) {
        commitLocalColor();
        setOpen(false);
      }
    };
    const handlePointerUp = () => {
      if (draggingRef.current) {
        finishActiveDrag();
      }
    };
    const handlePointerCancel = () => {
      if (draggingRef.current) {
        finishActiveDrag();
      }
    };
    window.addEventListener("pointerdown", handlePointerDown);
    window.addEventListener("pointerup", handlePointerUp);
    window.addEventListener("pointercancel", handlePointerCancel);
    window.setTimeout(() => hexInputRef.current?.select(), 0);
    return () => {
      window.removeEventListener("pointerdown", handlePointerDown);
      window.removeEventListener("pointerup", handlePointerUp);
      window.removeEventListener("pointercancel", handlePointerCancel);
    };
  }, [commitLocalColor, finishActiveDrag, open]);

  React.useLayoutEffect(() => {
    if (!open) {
      return;
    }

    updatePopoverBounds();
    window.addEventListener("resize", updatePopoverBounds);
    window.addEventListener("scroll", updatePopoverBounds, true);
    return () => {
      window.removeEventListener("resize", updatePopoverBounds);
      window.removeEventListener("scroll", updatePopoverBounds, true);
    };
  }, [open, updatePopoverBounds]);

  const commitHsv = React.useCallback((nextHsv: HsvColor) => {
    const nextColor = hsvToHex(nextHsv);
    setDraftHsv(nextHsv);
    localColorRef.current = nextColor;
    setLocalColor(nextColor);
    setDraftHex(nextColor);
  }, []);

  const updateSaturationValue = React.useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    const s = clamp((event.clientX - rect.left) / rect.width, 0, 1);
    const v = clamp(1 - (event.clientY - rect.top) / rect.height, 0, 1);
    commitHsv({ ...hsv, s, v });
  }, [commitHsv, hsv]);

  const updateHueValue = React.useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    const h = clamp((event.clientX - rect.left) / rect.width, 0, 1) * 360;
    commitHsv({ ...hsv, h });
  }, [commitHsv, hsv]);

  const startDrag = React.useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    draggingRef.current = true;
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
  }, []);

  const finishDrag = React.useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    event.stopPropagation();
    finishActiveDrag();
  }, [finishActiveDrag]);

  return (
    <span className="fast-color-picker" ref={rootRef}>
      <button
        type="button"
        className="color-picker-shell"
        style={{ backgroundColor: displayColor }}
        disabled={disabled}
        aria-label={ariaLabel}
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => {
          const nextHsv = hexToHsv(normalizedValue);
          localColorRef.current = normalizedValue;
          setLocalColor(normalizedValue);
          setDraftHex(normalizedValue);
          setDraftHsv((currentHsv) => nextHsv.s > 0 ? nextHsv : { ...nextHsv, h: currentHsv.h });
          setOpen(true);
        }}
      />
      {open ? createPortal(
        <div
          ref={popoverRef}
          className="fast-color-popover"
          style={popoverStyle ?? undefined}
          role="dialog"
          aria-label={ariaLabel}
          onPointerDown={(event) => event.stopPropagation()}
          onClick={(event) => event.stopPropagation()}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              commitLocalColor();
              setOpen(false);
            }
          }}
        >
          <div
            className="fast-color-saturation"
            style={{ backgroundColor: `hsl(${hsv.h} 100% 50%)` }}
            onPointerDown={(event) => {
              startDrag(event);
              updateSaturationValue(event);
            }}
            onPointerMove={(event) => {
              if ((event.buttons & 1) === 1) {
                updateSaturationValue(event);
              }
            }}
            onPointerUp={finishDrag}
            onPointerCancel={finishDrag}
          >
            <span
              className="fast-color-saturation-marker"
              style={{ left: `${hsv.s * 100}%`, top: `${(1 - hsv.v) * 100}%` }}
            />
          </div>
          <div
            className="fast-color-hue-strip"
            role="slider"
            aria-label="색상"
            aria-valuemin={0}
            aria-valuemax={360}
            aria-valuenow={Math.round(hsv.h)}
            tabIndex={0}
            onPointerDown={(event) => {
              startDrag(event);
              updateHueValue(event);
            }}
            onPointerMove={(event) => {
              if ((event.buttons & 1) === 1) {
                updateHueValue(event);
              }
            }}
            onPointerUp={finishDrag}
            onPointerCancel={finishDrag}
            onKeyDown={(event) => {
              if (event.key === "ArrowLeft" || event.key === "ArrowDown") {
                event.preventDefault();
                commitHsv({ ...hsv, h: (hsv.h + 359) % 360 });
              }
              if (event.key === "ArrowRight" || event.key === "ArrowUp") {
                event.preventDefault();
                commitHsv({ ...hsv, h: (hsv.h + 1) % 360 });
              }
            }}
          >
            <span className="fast-color-hue-marker" style={{ left: `${(hsv.h / 360) * 100}%` }} />
          </div>
          <div className="fast-color-bottom-row">
            <input
              ref={hexInputRef}
              className="fast-color-hex-input"
              value={draftHex}
              spellCheck={false}
              onChange={(event) => {
                const nextValue = normalizeHexDraft(event.target.value);
                setDraftHex(nextValue);
                if (HEX_COLOR_PATTERN.test(nextValue)) {
                  commitColor(nextValue);
                }
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  event.stopPropagation();
                  commitLocalColor();
                  setOpen(false);
                }
              }}
            />
            <div className="fast-color-swatches" aria-label="빠른 색상">
              {QUICK_COLORS.map((color) => (
                <button
                  key={color}
                  type="button"
                  className={color.toLowerCase() === displayColor ? "active" : ""}
                  style={{ backgroundColor: color }}
                  aria-label={color}
                  onClick={(event) => {
                    event.stopPropagation();
                    commitColor(color);
                  }}
                />
              ))}
            </div>
          </div>
        </div>,
        document.body
      ) : null}
    </span>
  );
}

function normalizeHexColor(value: string): string {
  const normalized = value.trim().toLowerCase();
  if (HEX_COLOR_PATTERN.test(normalized)) {
    return normalized;
  }
  if (/^#[\da-f]{3}$/i.test(normalized)) {
    return `#${normalized[1]}${normalized[1]}${normalized[2]}${normalized[2]}${normalized[3]}${normalized[3]}`.toLowerCase();
  }
  return "#000000";
}

function normalizeHexDraft(value: string): string {
  const cleanValue = value.trim().replace(/[^\da-f#]/gi, "");
  return cleanValue.startsWith("#") ? `#${cleanValue.slice(1, 7)}` : `#${cleanValue.slice(0, 6)}`;
}

function hexToHsv(hex: string): HsvColor {
  const r = Number.parseInt(hex.slice(1, 3), 16) / 255;
  const g = Number.parseInt(hex.slice(3, 5), 16) / 255;
  const b = Number.parseInt(hex.slice(5, 7), 16) / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const delta = max - min;
  let h = 0;

  if (delta > 0) {
    if (max === r) {
      h = 60 * (((g - b) / delta) % 6);
    } else if (max === g) {
      h = 60 * ((b - r) / delta + 2);
    } else {
      h = 60 * ((r - g) / delta + 4);
    }
  }

  return {
    h: h < 0 ? h + 360 : h,
    s: max === 0 ? 0 : delta / max,
    v: max
  };
}

function hsvToHex({ h, s, v }: HsvColor): string {
  const c = v * s;
  const x = c * (1 - Math.abs(((h / 60) % 2) - 1));
  const m = v - c;
  const [r, g, b] =
    h < 60 ? [c, x, 0]
      : h < 120 ? [x, c, 0]
        : h < 180 ? [0, c, x]
          : h < 240 ? [0, x, c]
            : h < 300 ? [x, 0, c]
              : [c, 0, x];
  return `#${toHexByte((r + m) * 255)}${toHexByte((g + m) * 255)}${toHexByte((b + m) * 255)}`;
}

function toHexByte(value: number): string {
  return Math.round(clamp(value, 0, 255)).toString(16).padStart(2, "0");
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

function resolveColorPickerPopoverStyle(anchorRect: DOMRect): ColorPickerPopoverStyle {
  const viewportWidth = window.innerWidth;
  const viewportHeight = window.innerHeight;
  const left = clamp(
    anchorRect.left,
    COLOR_PICKER_POPOVER_VIEWPORT_MARGIN_PX,
    Math.max(COLOR_PICKER_POPOVER_VIEWPORT_MARGIN_PX, viewportWidth - COLOR_PICKER_POPOVER_WIDTH_PX - COLOR_PICKER_POPOVER_VIEWPORT_MARGIN_PX)
  );
  const belowTop = anchorRect.bottom + COLOR_PICKER_POPOVER_GAP_PX;
  const top = belowTop + 220 < viewportHeight
    ? belowTop
    : Math.max(COLOR_PICKER_POPOVER_VIEWPORT_MARGIN_PX, anchorRect.top - 220 - COLOR_PICKER_POPOVER_GAP_PX);

  return { left, top };
}

function syncDraftColor(
  color: string,
  setLocalColor: React.Dispatch<React.SetStateAction<string>>,
  setDraftHex: React.Dispatch<React.SetStateAction<string>>,
  setDraftHsv: React.Dispatch<React.SetStateAction<HsvColor>>
): void {
  const nextHsv = hexToHsv(color);
  setLocalColor(color);
  setDraftHex(color);
  setDraftHsv((currentHsv) => nextHsv.s > 0 ? nextHsv : { ...nextHsv, h: currentHsv.h });
}
