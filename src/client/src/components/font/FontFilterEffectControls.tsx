import React from "react";
import {
  FONT_FILTER_EFFECT_DEFINITIONS,
  FILTER_EFFECT_INTENSITY_MAX,
  FILTER_EFFECT_INTENSITY_MIN,
  FILTER_EFFECT_INTENSITY_STEP,
  type FontFilterEffectControlEnabledKey,
  type FontFilterEffectControlIntensityKey,
  type FontFilterEffectDefinition
} from "../../lib/fontFilterEffectMetadata";
import { mouseOnlyRangeInputProps } from "../../lib/mouseOnlyCheckbox";
import { rangeProgressStyle } from "../../lib/rangeProgressStyle";

export type FontFilterEffectControlValues = {
  enabled: boolean;
} & Record<FontFilterEffectControlEnabledKey, boolean> & Record<FontFilterEffectControlIntensityKey, number>;

export type FontFilterEffectControlKey = keyof FontFilterEffectControlValues;
type FontFilterEffectControlPatch = Partial<Omit<FontFilterEffectControlValues, "enabled">>;

type FontFilterEffectControlsProps = {
  disabled: boolean;
  enabledAriaLabel: string;
  values: FontFilterEffectControlValues;
  onChange: (patch: FontFilterEffectControlPatch) => void;
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
  const renderOptionalLinkButton = (key: FontFilterEffectControlKey, label: string): React.ReactNode =>
    renderLinkButton?.(key, label) ?? null;

  const renderFilterControl = (definition: FontFilterEffectDefinition): React.ReactNode => {
    const filterEnabled = Boolean(values[definition.enabledControlKey]);
    const intensity = definition.resolveIntensity(values[definition.intensityControlKey]);
    return (
      <React.Fragment key={definition.enabledControlKey}>
        <div className="font-filter-option-row">
          <button
            type="button"
            className={`font-inline-toggle ${filterEnabled ? "active" : ""}`}
            disabled={disabled}
            onClick={() => {
              const nextEnabled = !filterEnabled;
              const patch = { [definition.enabledControlKey]: nextEnabled } as FontFilterEffectControlPatch;
              if (nextEnabled) {
                Object.assign(patch, {
                  [definition.intensityControlKey]: definition.resolveEnabledIntensity(values[definition.intensityControlKey])
                });
              }
              onChange(patch);
            }}
            aria-pressed={filterEnabled}
          >
            {definition.label}
          </button>
          {renderOptionalLinkButton(definition.enabledControlKey, definition.label)}
        </div>
        {filterEnabled ? (
          <div className="font-filter-range-row">
            <label className="compact-tool-field font-range-field">
              <span>
                <span>강도</span>
                <strong>{Math.round(intensity * 100)}%</strong>
              </span>
              <input
                type="range"
                {...mouseOnlyRangeInputProps}
                min={FILTER_EFFECT_INTENSITY_MIN}
                max={FILTER_EFFECT_INTENSITY_MAX}
                step={FILTER_EFFECT_INTENSITY_STEP}
                value={intensity}
                style={rangeProgressStyle(intensity, FILTER_EFFECT_INTENSITY_MIN, FILTER_EFFECT_INTENSITY_MAX)}
                aria-label={`${definition.label} 강도`}
                disabled={disabled}
                onChange={(event) => onChange({ [definition.intensityControlKey]: Number(event.target.value) } as FontFilterEffectControlPatch)}
              />
              {renderOptionalLinkButton(definition.intensityControlKey, `${definition.label} 강도`)}
            </label>
          </div>
        ) : null}
      </React.Fragment>
    );
  };

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
      {values.enabled ? FONT_FILTER_EFFECT_DEFINITIONS.map(renderFilterControl) : null}
    </div>
  );
}
