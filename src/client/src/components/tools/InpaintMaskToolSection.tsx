import React from "react";
import type { ImageRect, MangaPage } from "../../../../shared/types";
import { CompactNumberControl } from "../controls/CompactNumberControl";
import type { InpaintTool } from "../InpaintLayerCanvas";
import { InpaintToolButton } from "../inpaint/InpaintToolButton";
import { INPAINT_TOOL_SHORTCUTS } from "../../lib/editorShortcuts";
import {
  clampInpaintMaskBrushSize,
  INPAINT_MASK_BRUSH_SIZE_MAX,
  INPAINT_MASK_BRUSH_SIZE_MIN
} from "../../lib/inpaintToolSettings";
import type { LayerVisibility } from "../../lib/layerState";

type InpaintMaskToolSectionProps = {
  inpaintBrushSize: number;
  inpaintBusy: boolean;
  inpaintSelectionRect: ImageRect | null;
  inpaintTool: InpaintTool;
  layerVisibility: LayerVisibility;
  rangeToolActive: boolean;
  selectedPage: MangaPage | null;
  selectedPageEditLocked: boolean;
  onClearInpaintMask: () => void;
  onFillSelectedInpaintSelection: () => void | Promise<void>;
  onInpaintBrushSizeChange: (value: number) => void;
  onInpaintSelectionClear: () => void;
  onRerunInpaintForSelection: () => void | Promise<void>;
  onSelectSharedInpaintTool: (tool: InpaintTool) => void;
};

export function InpaintMaskToolSection({
  inpaintBrushSize,
  inpaintBusy,
  inpaintSelectionRect,
  inpaintTool,
  layerVisibility,
  rangeToolActive,
  selectedPage,
  selectedPageEditLocked,
  onClearInpaintMask,
  onFillSelectedInpaintSelection,
  onInpaintBrushSizeChange,
  onInpaintSelectionClear,
  onRerunInpaintForSelection,
  onSelectSharedInpaintTool
}: InpaintMaskToolSectionProps): React.JSX.Element {
  const maskDataUrl = selectedPage?.inpaintMaskDataUrl ?? selectedPage?.inpaintLayerDataUrl;
  const maskToolsDisabled = selectedPageEditLocked || !layerVisibility.inpaint || !layerVisibility.inpaintMask;

  return (
    <>
      <div className="segmented-control tool-selector mask-tool-selector" role="group" aria-label="인페인트 도구">
        <InpaintToolButton
          active={inpaintTool === "brush"}
          icon="brush"
          label="브러시"
          shortcut={INPAINT_TOOL_SHORTCUTS.brush}
          onClick={() => onSelectSharedInpaintTool("brush")}
          disabled={maskToolsDisabled}
        />
        <InpaintToolButton
          active={inpaintTool === "eraser"}
          icon="eraser"
          label="지우개"
          shortcut={INPAINT_TOOL_SHORTCUTS.eraser}
          onClick={() => onSelectSharedInpaintTool("eraser")}
          disabled={maskToolsDisabled}
        />
      </div>
      <div className="result-tool-settings mask-tool-settings">
        <label className="compact-tool-field result-size-field">
          <span>브러시 크기</span>
          <CompactNumberControl
            ariaLabel="마스크 브러시 크기"
            min={INPAINT_MASK_BRUSH_SIZE_MIN}
            max={INPAINT_MASK_BRUSH_SIZE_MAX}
            step={1}
            value={inpaintBrushSize}
            suffix="px"
            disabled={maskToolsDisabled}
            onChange={(brushSize) => onInpaintBrushSizeChange(clampInpaintMaskBrushSize(brushSize))}
          />
        </label>
      </div>
      <div className="result-action-grid mask-action-grid">
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
          onClick={onClearInpaintMask}
          disabled={selectedPageEditLocked || !maskDataUrl}
        >
          인페인트 마스크 비우기
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
