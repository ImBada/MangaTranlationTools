import React from "react";
import type { FontPresetPatch, LinkableFontPresetKey } from "../../lib/fontPresets";
import { mouseOnlyColorInputProps } from "../../lib/mouseOnlyCheckbox";
import { CompactNumberControl } from "../controls/CompactNumberControl";

type FontOutlineValues = Pick<
  FontPresetPatch,
  "outlineColor" | "outlineWidthPx" | "secondaryOutlineColor" | "secondaryOutlineWidthPx"
>;

type FontOutlineControlsProps = {
  disabled: boolean;
  onChange: (patch: FontPresetPatch) => void;
  renderLinkButton: (key: LinkableFontPresetKey, label: string) => React.ReactNode;
  values: FontOutlineValues;
};

const DEFAULT_SECONDARY_OUTLINE_WIDTH_PX = 2;

export function FontOutlineControls({ disabled, onChange, renderLinkButton, values }: FontOutlineControlsProps): React.JSX.Element {
  const secondaryOutlineActive = (values.secondaryOutlineWidthPx ?? 0) > 0;
  const primaryOutlineLabelPrefix = secondaryOutlineActive ? "1차 " : "";
  const primaryOutlineColorLabel = `${primaryOutlineLabelPrefix}외곽선 색`;
  const primaryOutlineWidthLabel = `${primaryOutlineLabelPrefix}외곽선 두께`;

  const enableSecondaryOutline = () => {
    onChange({
      secondaryOutlineColor: values.secondaryOutlineColor ?? "#ffffff",
      secondaryOutlineWidthPx:
        values.secondaryOutlineWidthPx && values.secondaryOutlineWidthPx > 0
          ? values.secondaryOutlineWidthPx
          : DEFAULT_SECONDARY_OUTLINE_WIDTH_PX
    });
  };

  return (
    <div className="compact-tool-field font-outline-section font-effect-section">
      <div className="font-outline-section-header">
        <span>외곽선</span>
        <div className="font-outline-mode" aria-label="외곽선 개수">
          <button
            type="button"
            className={!secondaryOutlineActive ? "active" : ""}
            disabled={disabled}
            onClick={() => onChange({ secondaryOutlineWidthPx: 0 })}
            aria-pressed={!secondaryOutlineActive}
          >
            1개
          </button>
          <button
            type="button"
            className={secondaryOutlineActive ? "active" : ""}
            disabled={disabled}
            onClick={enableSecondaryOutline}
            aria-pressed={secondaryOutlineActive}
          >
            2개
          </button>
        </div>
      </div>
      <div className="font-outline-row font-tool-grid">
        <label className="compact-tool-field font-color-field">
          <span>{primaryOutlineColorLabel}</span>
          <span className="color-picker-shell" style={{ backgroundColor: values.outlineColor ?? "#000000" }}>
            <input
              type="color"
              {...mouseOnlyColorInputProps}
              className="outline-color-input"
              value={values.outlineColor ?? "#000000"}
              disabled={disabled}
              onChange={(event) => onChange({ outlineColor: event.target.value })}
            />
          </span>
          {renderLinkButton("outlineColor", primaryOutlineColorLabel)}
        </label>
        <label className="compact-tool-field font-number-field">
          <span>{primaryOutlineWidthLabel}</span>
          <CompactNumberControl
            ariaLabel={primaryOutlineWidthLabel}
            min={0}
            max={24}
            step={0.5}
            value={values.outlineWidthPx ?? 0}
            suffix="px"
            disabled={disabled}
            onChange={(outlineWidthPx) => onChange({ outlineWidthPx })}
          />
          {renderLinkButton("outlineWidthPx", primaryOutlineWidthLabel)}
        </label>
      </div>
      {secondaryOutlineActive ? (
        <div className="font-outline-row font-tool-grid secondary-outline-row">
          <label className="compact-tool-field font-color-field">
            <span>2차 외곽선 색</span>
            <span className="color-picker-shell" style={{ backgroundColor: values.secondaryOutlineColor ?? "#ffffff" }}>
              <input
                type="color"
                {...mouseOnlyColorInputProps}
                className="outline-color-input"
                value={values.secondaryOutlineColor ?? "#ffffff"}
                disabled={disabled}
                onChange={(event) => onChange({ secondaryOutlineColor: event.target.value })}
              />
            </span>
            {renderLinkButton("secondaryOutlineColor", "2차 외곽선 색")}
          </label>
          <label className="compact-tool-field font-number-field">
            <span>2차 외곽선 두께</span>
            <CompactNumberControl
              ariaLabel="2차 외곽선 두께"
              min={0}
              max={24}
              step={0.5}
              value={values.secondaryOutlineWidthPx ?? 0}
              suffix="px"
              disabled={disabled}
              onChange={(secondaryOutlineWidthPx) => onChange({ secondaryOutlineWidthPx })}
            />
            {renderLinkButton("secondaryOutlineWidthPx", "2차 외곽선 두께")}
          </label>
        </div>
      ) : null}
    </div>
  );
}
