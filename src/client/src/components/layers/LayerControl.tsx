import React from "react";
import type { ActiveLayer } from "../../lib/layerState";
import { mouseOnlyCheckboxProps, mouseOnlyRangeInputProps } from "../../lib/mouseOnlyCheckbox";
import { rangeProgressStyle } from "../../lib/rangeProgressStyle";

type LayerControlProps = {
  layer: ActiveLayer;
  label: string;
  active: boolean;
  visible: boolean;
  opacity: number;
  viewOnly?: boolean;
  nested?: boolean;
  opacityEditMode?: boolean;
  opacityEditModeLabel?: string;
  onSelect: () => void;
  onVisibleChange: (visible: boolean) => void;
  onOpacityEditModeChange?: (enabled: boolean) => void;
  onOpacityChange: (opacity: number) => void;
};

export function LayerControl({
  layer,
  label,
  active,
  visible,
  opacity,
  viewOnly,
  nested,
  opacityEditMode,
  opacityEditModeLabel,
  onSelect,
  onVisibleChange,
  onOpacityEditModeChange,
  onOpacityChange
}: LayerControlProps): React.JSX.Element {
  return (
    <div
      data-layer-label={label}
      data-active={active ? "true" : "false"}
      data-visible={visible ? "true" : "false"}
      className={`layer-control layer-${layer}${active ? " active" : ""}${nested ? " nested" : ""}`}
      role="button"
      tabIndex={0}
      onClick={onSelect}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onSelect();
        }
      }}
    >
      <div className="layer-toggle">
        <span className="layer-select-grip" aria-hidden="true">::</span>
        {viewOnly ? null : (
          <input
            type="checkbox"
            {...mouseOnlyCheckboxProps}
            checked={visible}
            onClick={(event) => event.stopPropagation()}
            onChange={(event) => onVisibleChange(event.target.checked)}
          />
        )}
        <span className="layer-label-text">{label}</span>
        {viewOnly ? <span className="layer-active-badge">보기</span> : <span className="layer-opacity-value">{Math.round(opacity * 100)}%</span>}
      </div>
      {onOpacityEditModeChange && opacityEditModeLabel ? (
        <label className="layer-edit-toggle" onClick={(event) => event.stopPropagation()}>
          <input
            type="checkbox"
            {...mouseOnlyCheckboxProps}
            checked={Boolean(opacityEditMode)}
            onChange={(event) => onOpacityEditModeChange(event.target.checked)}
          />
          {opacityEditModeLabel}
        </label>
      ) : null}
      {viewOnly ? null : (
        <input
          className="layer-opacity-slider"
          type="range"
          {...mouseOnlyRangeInputProps}
          min={0}
          max={1}
          step={0.01}
          value={opacity}
          style={rangeProgressStyle(opacity, 0, 1)}
          disabled={!visible}
          aria-label={`${label} 투명도`}
          onClick={(event) => event.stopPropagation()}
          onChange={(event) => onOpacityChange(Number(event.target.value))}
        />
      )}
    </div>
  );
}
