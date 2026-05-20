import React from "react";
import {
  FILTER_ZIGZAG_INTENSITY_MAX,
  FILTER_ZIGZAG_INTENSITY_MIN,
  FILTER_ZIGZAG_INTENSITY_STEP,
  resolveEnabledFontZigzagIntensity,
  resolveFilterZigzagIntensity
} from "../../lib/fontFilterEffects";
import { mouseOnlyRangeInputProps } from "../../lib/mouseOnlyCheckbox";
import { rangeProgressStyle } from "../../lib/rangeProgressStyle";

export type FontFilterEffectControlValues = {
  enabled: boolean;
  zigzagEnabled: boolean;
  zigzagIntensity: number;
};

export type FontFilterEffectControlKey = keyof FontFilterEffectControlValues;

type FontFilterEffectControlsProps = {
  disabled: boolean;
  enabledAriaLabel: string;
  values: FontFilterEffectControlValues;
  onChange: (patch: Partial<Omit<FontFilterEffectControlValues, "enabled">>) => void;
  onEnabledChange: (enabled: boolean) => void;
  renderLinkButton?: (key: FontFilterEffectControlKey, label: string) => React.ReactNode;
};

export function FontFilterEffectControls({
  disabled,
  enabledAriaLabel,
  values,
  onChange,
  onEnabledChange,
  renderLinkButton
}: FontFilterEffectControlsProps): React.JSX.Element {
  const zigzagIntensity = resolveFilterZigzagIntensity(values.zigzagIntensity);
  const renderOptionalLinkButton = (key: FontFilterEffectControlKey, label: string): React.ReactNode =>
    renderLinkButton?.(key, label) ?? null;

  return (
    <div className="compact-tool-field font-outline-section font-filter-section font-effect-section">
      <div className="font-outline-section-header">
        <span>필터 효과</span>
        <div className="font-filter-header-actions">
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
          {renderOptionalLinkButton("enabled", "필터 효과")}
        </div>
      </div>
      {values.enabled ? (
        <>
          <div className="font-filter-option-row">
            <button
              type="button"
              className={`font-inline-toggle ${values.zigzagEnabled ? "active" : ""}`}
              disabled={disabled}
              onClick={() => {
                const zigzagEnabled = !values.zigzagEnabled;
                onChange({
                  zigzagEnabled,
                  ...(zigzagEnabled ? { zigzagIntensity: resolveEnabledFontZigzagIntensity(values.zigzagIntensity) } : {})
                });
              }}
              aria-pressed={values.zigzagEnabled}
            >
              지그재그
            </button>
            {renderOptionalLinkButton("zigzagEnabled", "지그재그")}
          </div>
          {values.zigzagEnabled ? (
            <div className="font-filter-range-row">
              <label className="compact-tool-field font-range-field">
                <span>
                  <span>강도</span>
                  <strong>{Math.round(zigzagIntensity * 100)}%</strong>
                </span>
                <input
                  type="range"
                  {...mouseOnlyRangeInputProps}
                  min={FILTER_ZIGZAG_INTENSITY_MIN}
                  max={FILTER_ZIGZAG_INTENSITY_MAX}
                  step={FILTER_ZIGZAG_INTENSITY_STEP}
                  value={zigzagIntensity}
                  style={rangeProgressStyle(zigzagIntensity, FILTER_ZIGZAG_INTENSITY_MIN, FILTER_ZIGZAG_INTENSITY_MAX)}
                  disabled={disabled}
                  onChange={(event) => onChange({ zigzagIntensity: Number(event.target.value) })}
                />
                {renderOptionalLinkButton("zigzagIntensity", "지그재그 강도")}
              </label>
            </div>
          ) : null}
        </>
      ) : null}
    </div>
  );
}
