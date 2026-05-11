import React from "react";
import { useRepeatingStepControl } from "./useRepeatingStepControl";

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

function handleNumberInputKeyDown(event: React.KeyboardEvent<HTMLInputElement>): void {
  if (event.key !== "Enter") {
    return;
  }
  event.preventDefault();
  event.currentTarget.blur();
}

export function CompactNumberControl({ ariaLabel, disabled, max, min, onChange, step, suffix, value }: CompactNumberControlProps): React.JSX.Element {
  const stepControl = useRepeatingStepControl({ disabled, max, min, onChange, step, value });

  return (
    <div className="compact-number-control stepped-number-control">
      <button
        type="button"
        aria-label={`${ariaLabel} 감소`}
        disabled={disabled || value <= min}
        onPointerDown={(event) => stepControl.handlePointerDown(event, -1)}
        onPointerUp={stepControl.stopRepeat}
        onPointerCancel={stepControl.handlePointerCancel}
        onLostPointerCapture={stepControl.stopRepeat}
        onClick={() => stepControl.handleClick(-1)}
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
        onChange={(event) => stepControl.setValue(Number(event.target.value))}
        onKeyDown={handleNumberInputKeyDown}
      />
      <button
        type="button"
        aria-label={`${ariaLabel} 증가`}
        disabled={disabled || (max !== undefined && value >= max)}
        onPointerDown={(event) => stepControl.handlePointerDown(event, 1)}
        onPointerUp={stepControl.stopRepeat}
        onPointerCancel={stepControl.handlePointerCancel}
        onLostPointerCapture={stepControl.stopRepeat}
        onClick={() => stepControl.handleClick(1)}
      >
        +
      </button>
      <span>{suffix}</span>
    </div>
  );
}
