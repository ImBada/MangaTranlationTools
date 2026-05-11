import React from "react";
import { CompactNumberControl } from "../controls/CompactNumberControl";
import { mouseOnlyColorInputProps, mouseOnlyRangeInputProps } from "../../lib/mouseOnlyCheckbox";
import { rangeProgressStyle } from "../../lib/rangeProgressStyle";

export type FontShadowControlValues = {
  angleDeg: number;
  blurPx: number;
  color: string;
  distancePx: number;
  enabled: boolean;
  opacity: number;
};

export type FontShadowControlKey = keyof FontShadowControlValues;

type FontShadowControlsProps = {
  disabled: boolean;
  enabledAriaLabel: string;
  values: FontShadowControlValues;
  onChange: (patch: Partial<Omit<FontShadowControlValues, "enabled">>) => void;
  onEnabledChange: (enabled: boolean) => void;
  renderLinkButton?: (key: FontShadowControlKey, label: string) => React.ReactNode;
};

const SHADOW_OPACITY_MIN = 0;
const SHADOW_OPACITY_MAX = 1;
const SHADOW_ANGLE_STEP = 1;
const SHADOW_DISTANCE_STEP = 0.5;
const SHADOW_BLUR_STEP = 0.5;

function roundToStep(value: number, step: number): number {
  if (!Number.isFinite(value)) {
    return 0;
  }
  const decimalPlaces = step.toString().split(".")[1]?.length ?? 0;
  return Number((Math.round(value / step) * step).toFixed(decimalPlaces));
}

export function FontShadowControls({
  disabled,
  enabledAriaLabel,
  values,
  onChange,
  onEnabledChange,
  renderLinkButton
}: FontShadowControlsProps): React.JSX.Element {
  const renderOptionalLinkButton = (key: FontShadowControlKey, label: string): React.ReactNode =>
    renderLinkButton?.(key, label) ?? null;

  return (
    <div className="compact-tool-field font-outline-section font-shadow-section font-effect-section">
      <div className="font-outline-section-header">
        <span>그림자</span>
        <div className="font-shadow-header-actions">
          <div className="font-outline-mode" aria-label={enabledAriaLabel}>
            <button
              type="button"
              className={!values.enabled ? "active" : ""}
              disabled={disabled}
              onClick={() => onEnabledChange(false)}
              aria-pressed={!values.enabled}
            >
              OFF
            </button>
            <button
              type="button"
              className={values.enabled ? "active" : ""}
              disabled={disabled}
              onClick={() => onEnabledChange(true)}
              aria-pressed={values.enabled}
            >
              ON
            </button>
          </div>
          {renderOptionalLinkButton("enabled", "그림자")}
        </div>
      </div>
      {values.enabled ? (
        <>
          <div className="font-shadow-row font-tool-grid">
            <label className="compact-tool-field font-color-field">
              <span>그림자 색</span>
              <span className="color-picker-shell" style={{ backgroundColor: values.color }}>
                <input
                  type="color"
                  {...mouseOnlyColorInputProps}
                  className="outline-color-input"
                  value={values.color}
                  disabled={disabled}
                  onChange={(event) => onChange({ color: event.target.value })}
                />
              </span>
              {renderOptionalLinkButton("color", "그림자 색")}
            </label>
          </div>
          <div className="font-shadow-row font-tool-grid">
            <label className="compact-tool-field font-range-field">
              <span>
                <span>불투명도</span>
                <strong>{Math.round(values.opacity * 100)}%</strong>
              </span>
              <input
                type="range"
                {...mouseOnlyRangeInputProps}
                min={SHADOW_OPACITY_MIN}
                max={SHADOW_OPACITY_MAX}
                step={0.01}
                value={values.opacity}
                style={rangeProgressStyle(values.opacity, SHADOW_OPACITY_MIN, SHADOW_OPACITY_MAX)}
                disabled={disabled}
                onChange={(event) => onChange({ opacity: Number(event.target.value) })}
              />
              {renderOptionalLinkButton("opacity", "그림자 불투명도")}
            </label>
            <label className="compact-tool-field font-number-field">
              <span>흐림</span>
              <CompactNumberControl
                ariaLabel="그림자 흐림"
                min={0}
                max={80}
                step={SHADOW_BLUR_STEP}
                value={roundToStep(values.blurPx, SHADOW_BLUR_STEP)}
                suffix="px"
                disabled={disabled}
                onChange={(blurPx) => onChange({ blurPx })}
              />
              {renderOptionalLinkButton("blurPx", "그림자 흐림")}
            </label>
          </div>
          <div className="font-shadow-row font-tool-grid">
            <label className="compact-tool-field font-number-field">
              <span>각도</span>
              <CompactNumberControl
                ariaLabel="그림자 각도"
                min={-360}
                max={360}
                step={SHADOW_ANGLE_STEP}
                value={roundToStep(values.angleDeg, SHADOW_ANGLE_STEP)}
                suffix="도"
                disabled={disabled}
                onChange={(angleDeg) => onChange({ angleDeg })}
              />
              {renderOptionalLinkButton("angleDeg", "그림자 각도")}
            </label>
            <label className="compact-tool-field font-number-field">
              <span>거리</span>
              <CompactNumberControl
                ariaLabel="그림자 거리"
                min={0}
                max={80}
                step={SHADOW_DISTANCE_STEP}
                value={roundToStep(values.distancePx, SHADOW_DISTANCE_STEP)}
                suffix="px"
                disabled={disabled}
                onChange={(distancePx) => onChange({ distancePx })}
              />
              {renderOptionalLinkButton("distancePx", "그림자 거리")}
            </label>
          </div>
        </>
      ) : null}
    </div>
  );
}
