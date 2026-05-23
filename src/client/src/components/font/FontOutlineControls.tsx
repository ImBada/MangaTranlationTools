import React from "react";
import type { FontPresetPatch, LinkableFontPresetKey } from "../../lib/fontPresets";
import { resolveSecondaryOutlineEnabled } from "../../lib/overlayLayout";
import { CompactNumberControl } from "../controls/CompactNumberControl";
import { FastColorPicker } from "../controls/FastColorPicker";

type FontOutlineValues = Pick<
  FontPresetPatch,
  "outlineColor" | "outlineWidthPx" | "secondaryOutlineEnabled" | "secondaryOutlineColor" | "secondaryOutlineWidthPx"
>;

type FontOutlineControlsProps = {
  disabled: boolean;
  onChange: (patch: FontPresetPatch) => void;
  renderLinkButton: (key: LinkableFontPresetKey, label: string) => React.ReactNode;
  values: FontOutlineValues;
};

const DEFAULT_SECONDARY_OUTLINE_WIDTH_PX = 2;

export function FontOutlineControls({ disabled, onChange, renderLinkButton, values }: FontOutlineControlsProps): React.JSX.Element {
  const secondaryOutlineActive = resolveSecondaryOutlineEnabled(values);
  const primaryOutlineLabelPrefix = secondaryOutlineActive ? "1차 " : "";
  const primaryOutlineColorLabel = `${primaryOutlineLabelPrefix}외곽선 색`;
  const primaryOutlineWidthLabel = `${primaryOutlineLabelPrefix}외곽선 두께`;

  const enableSecondaryOutline = () => {
    onChange({
      secondaryOutlineEnabled: true,
      secondaryOutlineColor: values.secondaryOutlineColor ?? "#ffffff",
      secondaryOutlineWidthPx: secondaryOutlineActive
        ? values.secondaryOutlineWidthPx ?? 0
        : values.secondaryOutlineWidthPx && values.secondaryOutlineWidthPx > 0
          ? values.secondaryOutlineWidthPx
          : DEFAULT_SECONDARY_OUTLINE_WIDTH_PX
    });
  };

  return (
    <div className="compact-tool-field font-outline-section font-effect-section">
      <div className="font-outline-section-header">
        <span>외곽선</span>
        <div className="font-outline-header-actions">
          <div className="font-outline-mode" aria-label="외곽선 개수">
            <button
              type="button"
              className={!secondaryOutlineActive ? "active" : ""}
              disabled={disabled}
              onClick={() => onChange({ secondaryOutlineEnabled: false })}
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
          {renderLinkButton("secondaryOutlineEnabled", "외곽선 개수")}
        </div>
      </div>
      <div className="font-outline-row font-tool-grid">
        <div className="compact-tool-field font-color-field">
          <span>{primaryOutlineColorLabel}</span>
          <FastColorPicker
            ariaLabel={primaryOutlineColorLabel}
            value={values.outlineColor ?? "#000000"}
            disabled={disabled}
            onChange={(outlineColor) => onChange({ outlineColor })}
          />
          {renderLinkButton("outlineColor", primaryOutlineColorLabel)}
        </div>
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
          <div className="compact-tool-field font-color-field">
            <span>2차 외곽선 색</span>
            <FastColorPicker
              ariaLabel="2차 외곽선 색"
              value={values.secondaryOutlineColor ?? "#ffffff"}
              disabled={disabled}
              onChange={(secondaryOutlineColor) => onChange({ secondaryOutlineColor })}
            />
            {renderLinkButton("secondaryOutlineColor", "2차 외곽선 색")}
          </div>
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
