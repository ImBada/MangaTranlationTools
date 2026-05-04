import React from "react";

type CompactNumberControlProps = {
  ariaLabel: string;
  disabled: boolean;
  max?: number;
  min: number;
  onChange: (value: number) => void;
  step: number;
  suffix: string;
  value: number;
};

const REPEAT_START_DELAY_MS = 350;
const REPEAT_INTERVAL_MS = 70;

export function CompactNumberControl({ ariaLabel, disabled, max, min, onChange, step, suffix, value }: CompactNumberControlProps): React.JSX.Element {
  const latestValueRef = React.useRef(value);
  const repeatDelayRef = React.useRef<number | null>(null);
  const repeatIntervalRef = React.useRef<number | null>(null);
  const suppressNextClickRef = React.useRef(false);
  const precision = Math.max(0, String(step).split(".")[1]?.length ?? 0);

  React.useEffect(() => {
    latestValueRef.current = value;
  }, [value]);

  const clampValue = (nextValue: number) => {
    if (!Number.isFinite(nextValue)) {
      return latestValueRef.current;
    }
    const roundedValue = Math.max(min, Number(nextValue.toFixed(precision)));
    return max === undefined ? roundedValue : Math.min(max, roundedValue);
  };

  const updateValue = (nextValue: number) => {
    const clampedValue = clampValue(nextValue);
    latestValueRef.current = clampedValue;
    onChange(clampedValue);
  };

  const stopRepeat = React.useCallback(() => {
    if (repeatDelayRef.current !== null) {
      window.clearTimeout(repeatDelayRef.current);
      repeatDelayRef.current = null;
    }
    if (repeatIntervalRef.current !== null) {
      window.clearInterval(repeatIntervalRef.current);
      repeatIntervalRef.current = null;
    }
  }, []);

  React.useEffect(() => {
    if (disabled) {
      stopRepeat();
      suppressNextClickRef.current = false;
    }
  }, [disabled, stopRepeat]);

  React.useEffect(() => stopRepeat, [stopRepeat]);

  const stepValue = React.useCallback(
    (direction: -1 | 1) => {
      const currentValue = latestValueRef.current;
      const nextValue = clampValue(currentValue + step * direction);
      if (nextValue === currentValue) {
        return false;
      }
      latestValueRef.current = nextValue;
      onChange(nextValue);
      return true;
    },
    [max, min, onChange, precision, step]
  );

  const startRepeat = React.useCallback(
    (direction: -1 | 1) => {
      if (disabled) {
        return;
      }
      suppressNextClickRef.current = true;
      stopRepeat();
      stepValue(direction);
      repeatDelayRef.current = window.setTimeout(() => {
        repeatIntervalRef.current = window.setInterval(() => {
          if (!stepValue(direction)) {
            stopRepeat();
          }
        }, REPEAT_INTERVAL_MS);
      }, REPEAT_START_DELAY_MS);
    },
    [disabled, stepValue, stopRepeat]
  );

  const handlePointerDown = (event: React.PointerEvent<HTMLButtonElement>, direction: -1 | 1) => {
    if (event.button !== 0) {
      return;
    }
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    startRepeat(direction);
  };

  const handlePointerCancel = () => {
    stopRepeat();
    suppressNextClickRef.current = false;
  };

  const handleClick = (direction: -1 | 1) => {
    if (suppressNextClickRef.current) {
      suppressNextClickRef.current = false;
      return;
    }
    stepValue(direction);
  };

  return (
    <div className="compact-number-control stepped-number-control">
      <button
        type="button"
        aria-label={`${ariaLabel} 감소`}
        disabled={disabled || value <= min}
        onPointerDown={(event) => handlePointerDown(event, -1)}
        onPointerUp={stopRepeat}
        onPointerCancel={handlePointerCancel}
        onLostPointerCapture={stopRepeat}
        onClick={() => handleClick(-1)}
      >
        -
      </button>
      <input
        type="number"
        min={min}
        max={max}
        step={step}
        value={value}
        disabled={disabled}
        aria-label={ariaLabel}
        onChange={(event) => updateValue(Number(event.target.value))}
      />
      <button
        type="button"
        aria-label={`${ariaLabel} 증가`}
        disabled={disabled || (max !== undefined && value >= max)}
        onPointerDown={(event) => handlePointerDown(event, 1)}
        onPointerUp={stopRepeat}
        onPointerCancel={handlePointerCancel}
        onLostPointerCapture={stopRepeat}
        onClick={() => handleClick(1)}
      >
        +
      </button>
      <span>{suffix}</span>
    </div>
  );
}
