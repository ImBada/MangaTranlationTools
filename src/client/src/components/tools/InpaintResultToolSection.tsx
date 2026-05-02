import React from "react";
import type { ImageRect, MangaPage } from "../../../../shared/types";
import { CompactNumberControl } from "../controls/CompactNumberControl";
import type { InpaintTool } from "../InpaintLayerCanvas";
import type { InpaintResultTool } from "../InpaintResultCanvas";
import { InpaintToolButton } from "../inpaint/InpaintToolButton";
import { INPAINT_TOOL_SHORTCUTS } from "../../lib/editorShortcuts";
import {
  clampInpaintResultBrushSize,
  INPAINT_RESULT_BRUSH_SIZE_MAX,
  INPAINT_RESULT_BRUSH_SIZE_MIN
} from "../../lib/inpaintToolSettings";
import type { LayerVisibility } from "../../lib/layerState";
import { rangeProgressStyle } from "../../lib/rangeProgressStyle";

type InpaintResultToolSectionProps = {
  inpaintBusy: boolean;
  inpaintResultBrushColor: string;
  inpaintResultBrushHardness: number;
  inpaintResultBrushSize: number;
  inpaintResultTool: InpaintResultTool;
  inpaintResultToolStrength: number;
  inpaintSelectionRect: ImageRect | null;
  layerVisibility: LayerVisibility;
  rangeToolActive: boolean;
  selectedPage: MangaPage | null;
  selectedPageEditLocked: boolean;
  onClearInpaintResult: () => void;
  onFillSelectedInpaintSelection: () => void | Promise<void>;
  onInpaintResultBrushColorChange: (value: string) => void;
  onInpaintResultBrushHardnessChange: (value: number) => void;
  onInpaintResultBrushSizeChange: (value: number) => void;
  onInpaintResultToolStrengthChange: (value: number) => void;
  onInpaintSelectionClear: () => void;
  onRerunInpaintForSelection: () => void | Promise<void>;
  onRerunInpaintWithCurrentMask: () => void | Promise<void>;
  onSelectInpaintResultEditTool: (tool: Exclude<InpaintResultTool, "select">) => void;
  onSelectSharedInpaintTool: (tool: InpaintTool) => void;
};

export function InpaintResultToolSection({
  inpaintBusy,
  inpaintResultBrushColor,
  inpaintResultBrushHardness,
  inpaintResultBrushSize,
  inpaintResultTool,
  inpaintResultToolStrength,
  inpaintSelectionRect,
  layerVisibility,
  rangeToolActive,
  selectedPage,
  selectedPageEditLocked,
  onClearInpaintResult,
  onFillSelectedInpaintSelection,
  onInpaintResultBrushColorChange,
  onInpaintResultBrushHardnessChange,
  onInpaintResultBrushSizeChange,
  onInpaintResultToolStrengthChange,
  onInpaintSelectionClear,
  onRerunInpaintForSelection,
  onRerunInpaintWithCurrentMask,
  onSelectInpaintResultEditTool,
  onSelectSharedInpaintTool
}: InpaintResultToolSectionProps): React.JSX.Element {
  const maskDataUrl = selectedPage?.inpaintMaskDataUrl ?? selectedPage?.inpaintLayerDataUrl;
  const resultToolsDisabled = selectedPageEditLocked || !layerVisibility.inpaint || !layerVisibility.inpaintResult;

  return (
    <>
      <div className="segmented-control tool-selector result-tool-grid" role="group" aria-label="인페인트 결과 도구">
        <InpaintToolButton
          active={inpaintResultTool === "brush"}
          icon="brush"
          label="브러시"
          shortcut={INPAINT_TOOL_SHORTCUTS.brush}
          onClick={() => onSelectSharedInpaintTool("brush")}
          disabled={resultToolsDisabled}
        />
        <InpaintToolButton
          active={inpaintResultTool === "eraser"}
          icon="eraser"
          label="지우개"
          shortcut={INPAINT_TOOL_SHORTCUTS.eraser}
          onClick={() => onSelectSharedInpaintTool("eraser")}
          disabled={resultToolsDisabled}
        />
        <InpaintToolButton
          active={inpaintResultTool === "blur"}
          icon="blur"
          label="흐림"
          onClick={() => onSelectInpaintResultEditTool("blur")}
          disabled={resultToolsDisabled}
        />
        <InpaintToolButton
          active={inpaintResultTool === "sharpen"}
          icon="sharpen"
          label="선명"
          onClick={() => onSelectInpaintResultEditTool("sharpen")}
          disabled={resultToolsDisabled}
        />
        <InpaintToolButton
          active={inpaintResultTool === "smudge"}
          icon="smudge"
          label="뭉개기"
          onClick={() => onSelectInpaintResultEditTool("smudge")}
          disabled={resultToolsDisabled}
        />
      </div>
      <div className="result-tool-settings">
        <label className="compact-tool-field result-color-field">
          <span>색상</span>
          <span className="color-picker-shell" style={{ backgroundColor: inpaintResultBrushColor }}>
            <input
              type="color"
              className="outline-color-input"
              value={inpaintResultBrushColor}
              disabled={resultToolsDisabled || inpaintResultTool !== "brush"}
              onChange={(event) => onInpaintResultBrushColorChange(event.target.value)}
            />
          </span>
        </label>
        <label className="compact-tool-field result-size-field">
          <span>크기</span>
          <CompactNumberControl
            ariaLabel="결과 브러시 크기"
            min={INPAINT_RESULT_BRUSH_SIZE_MIN}
            max={INPAINT_RESULT_BRUSH_SIZE_MAX}
            step={1}
            value={inpaintResultBrushSize}
            suffix="px"
            disabled={resultToolsDisabled}
            onChange={(brushSize) => onInpaintResultBrushSizeChange(clampInpaintResultBrushSize(brushSize))}
          />
        </label>
        <label className="compact-tool-field">
          <span>
            <span>가장자리</span>
            <strong>{Math.round(inpaintResultBrushHardness * 100)}%</strong>
          </span>
          <input
            type="range"
            min={0}
            max={1}
            step={0.01}
            value={inpaintResultBrushHardness}
            style={rangeProgressStyle(inpaintResultBrushHardness, 0, 1)}
            disabled={resultToolsDisabled}
            onChange={(event) => onInpaintResultBrushHardnessChange(Number(event.target.value))}
          />
        </label>
        <label className="compact-tool-field">
          <span>
            <span>강도</span>
            <strong>{Math.round(inpaintResultToolStrength * 100)}%</strong>
          </span>
          <input
            type="range"
            min={0.05}
            max={1}
            step={0.01}
            value={inpaintResultToolStrength}
            style={rangeProgressStyle(inpaintResultToolStrength, 0.05, 1)}
            disabled={resultToolsDisabled || inpaintResultTool === "brush" || inpaintResultTool === "eraser"}
            onChange={(event) => onInpaintResultToolStrengthChange(Number(event.target.value))}
          />
        </label>
      </div>
      <div className="result-action-grid">
        <button type="button" onClick={onInpaintSelectionClear} disabled={selectedPageEditLocked || !inpaintSelectionRect}>
          선택 해제
        </button>
        <button
          type="button"
          onClick={() => void onFillSelectedInpaintSelection()}
          disabled={selectedPageEditLocked || !inpaintSelectionRect || !rangeToolActive}
        >
          선택 범위 채우기
        </button>
        <button
          type="button"
          onClick={onClearInpaintResult}
          disabled={selectedPageEditLocked || !selectedPage?.inpaintResultDataUrl}
        >
          인페인트 결과 비우기
        </button>
        <button
          type="button"
          onClick={() => void onRerunInpaintWithCurrentMask()}
          disabled={selectedPageEditLocked || inpaintBusy || !maskDataUrl}
        >
          마스크 유지하고 인페인트 다시하기
        </button>
        <button
          type="button"
          onClick={() => void onRerunInpaintForSelection()}
          disabled={selectedPageEditLocked || inpaintBusy || !inpaintSelectionRect || !maskDataUrl}
        >
          선택 범위만 다시 인페인트
        </button>
      </div>
    </>
  );
}
