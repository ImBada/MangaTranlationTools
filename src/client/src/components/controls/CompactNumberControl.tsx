import React from "react";

type CompactNumberControlProps = {
  ariaLabel: string;
  disabled: boolean;
  max: number;
  min: number;
  onChange: (value: number) => void;
  step: number;
  suffix: string;
  value: number;
};

export function CompactNumberControl({ ariaLabel, disabled, max, min, onChange, step, suffix, value }: CompactNumberControlProps): React.JSX.Element {
  const precision = Math.max(0, String(step).split(".")[1]?.length ?? 0);
  const clampValue = (nextValue: number) => {
    if (!Number.isFinite(nextValue)) {
      return value;
    }
    return Math.min(max, Math.max(min, Number(nextValue.toFixed(precision))));
  };
  const updateValue = (nextValue: number) => onChange(clampValue(nextValue));

  return (
    <div className="compact-number-control stepped-number-control">
      <button type="button" aria-label={`${ariaLabel} 감소`} disabled={disabled || value <= min} onClick={() => updateValue(value - step)}>
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
      <button type="button" aria-label={`${ariaLabel} 증가`} disabled={disabled || value >= max} onClick={() => updateValue(value + step)}>
        +
      </button>
      <span>{suffix}</span>
    </div>
  );
}
