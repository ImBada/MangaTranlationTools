import React from "react";

export type RepeatingStepDirection = -1 | 1;

type UseRepeatingStepControlOptions = {
  disabled?: boolean;
  max?: number;
  min: number;
  onChange: (value: number) => void;
  repeatIntervalMs?: number;
  repeatStartDelayMs?: number;
  step: number;
  value: number;
};

type UseRepeatingStepControlState = {
  handleClick: (direction: RepeatingStepDirection) => void;
  handlePointerCancel: () => void;
  handlePointerDown: (event: React.PointerEvent<HTMLButtonElement>, direction: RepeatingStepDirection) => void;
  pressedDirection: RepeatingStepDirection | null;
  setValue: (value: number) => void;
  stopRepeat: () => void;
};

const DEFAULT_REPEAT_START_DELAY_MS = 350;
const DEFAULT_REPEAT_INTERVAL_MS = 70;

export function useRepeatingStepControl({
  disabled = false,
  max,
  min,
  onChange,
  repeatIntervalMs = DEFAULT_REPEAT_INTERVAL_MS,
  repeatStartDelayMs = DEFAULT_REPEAT_START_DELAY_MS,
  step,
  value
}: UseRepeatingStepControlOptions): UseRepeatingStepControlState {
  const latestValueRef = React.useRef(value);
  const repeatDelayRef = React.useRef<number | null>(null);
  const repeatIntervalRef = React.useRef<number | null>(null);
  const suppressNextClickRef = React.useRef(false);
  const [pressedDirection, setPressedDirection] = React.useState<RepeatingStepDirection | null>(null);
  const precision = Math.max(0, String(step).split(".")[1]?.length ?? 0);

  React.useEffect(() => {
    latestValueRef.current = value;
  }, [value]);

  const clampValue = React.useCallback(
    (nextValue: number) => {
      if (!Number.isFinite(nextValue)) {
        return latestValueRef.current;
      }
      const roundedValue = Math.max(min, Number(nextValue.toFixed(precision)));
      return max === undefined ? roundedValue : Math.min(max, roundedValue);
    },
    [max, min, precision]
  );

  const setValue = React.useCallback(
    (nextValue: number) => {
      const clampedValue = clampValue(nextValue);
      latestValueRef.current = clampedValue;
      onChange(clampedValue);
    },
    [clampValue, onChange]
  );

  const clearRepeatTimers = React.useCallback(() => {
    if (repeatDelayRef.current !== null) {
      window.clearTimeout(repeatDelayRef.current);
      repeatDelayRef.current = null;
    }
    if (repeatIntervalRef.current !== null) {
      window.clearInterval(repeatIntervalRef.current);
      repeatIntervalRef.current = null;
    }
  }, []);

  const stopRepeat = React.useCallback(() => {
    clearRepeatTimers();
    setPressedDirection(null);
  }, [clearRepeatTimers]);

  React.useEffect(() => {
    if (disabled) {
      stopRepeat();
      suppressNextClickRef.current = false;
    }
  }, [disabled, stopRepeat]);

  React.useEffect(() => clearRepeatTimers, [clearRepeatTimers]);

  const stepValue = React.useCallback(
    (direction: RepeatingStepDirection) => {
      const currentValue = latestValueRef.current;
      const nextValue = clampValue(currentValue + step * direction);
      if (nextValue === currentValue) {
        return false;
      }
      latestValueRef.current = nextValue;
      onChange(nextValue);
      return true;
    },
    [clampValue, onChange, step]
  );

  const startRepeat = React.useCallback(
    (direction: RepeatingStepDirection) => {
      if (disabled) {
        return;
      }
      suppressNextClickRef.current = true;
      stopRepeat();
      setPressedDirection(direction);
      stepValue(direction);
      repeatDelayRef.current = window.setTimeout(() => {
        repeatIntervalRef.current = window.setInterval(() => {
          if (!stepValue(direction)) {
            stopRepeat();
          }
        }, repeatIntervalMs);
      }, repeatStartDelayMs);
    },
    [disabled, repeatIntervalMs, repeatStartDelayMs, stepValue, stopRepeat]
  );

  const handlePointerDown = React.useCallback(
    (event: React.PointerEvent<HTMLButtonElement>, direction: RepeatingStepDirection) => {
      if (event.button !== 0) {
        return;
      }
      event.preventDefault();
      event.currentTarget.setPointerCapture(event.pointerId);
      startRepeat(direction);
    },
    [startRepeat]
  );

  const handlePointerCancel = React.useCallback(() => {
    stopRepeat();
    suppressNextClickRef.current = false;
  }, [stopRepeat]);

  const handleClick = React.useCallback(
    (direction: RepeatingStepDirection) => {
      if (suppressNextClickRef.current) {
        suppressNextClickRef.current = false;
        return;
      }
      stepValue(direction);
    },
    [stepValue]
  );

  return {
    handleClick,
    handlePointerCancel,
    handlePointerDown,
    pressedDirection,
    setValue,
    stopRepeat
  };
}
