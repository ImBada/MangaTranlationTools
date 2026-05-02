import React from "react";
import type { ChapterSnapshot, FontPreset, TranslationBlock } from "../../../../shared/types";
import { CompactNumberControl } from "../controls/CompactNumberControl";
import { FontFamilyPicker, type FontFamilyOption } from "../font/FontFamilyPicker";
import { FontOutlineControls } from "../font/FontOutlineControls";
import type { BlockFontPatch, LinkableFontPresetKey } from "../../lib/fontPresets";
import { DEFAULT_OVERLAY_FONT_FAMILY } from "../../lib/overlayLayout";
import { rangeProgressStyle } from "../../lib/rangeProgressStyle";
import type { LayerToolFontControlValues } from "./LayerToolPanelTypes";

type FontToolSectionProps = {
  currentChapter: ChapterSnapshot | null;
  editingFontPresetId: string | null;
  fontControlValues: LayerToolFontControlValues | null;
  fontFamilyOptions: FontFamilyOption[];
  fontPresetName: string;
  fontPresets: FontPreset[];
  renderFontPresetLinkButton: (key: LinkableFontPresetKey, label: string) => React.ReactNode;
  selectedBlock: TranslationBlock | null;
  selectedPageEditLocked: boolean;
  onClearSelectedBlockFontPreset: () => void;
  onCreateFontPreset: () => void;
  onDeleteFontPreset: (presetId: string) => void;
  onFontPresetNameChange: (value: string) => void;
  onFontSettingChange: (patch: BlockFontPatch) => void;
  onSelectFontPreset: (presetId: string) => void;
};

export function FontToolSection({
  currentChapter,
  editingFontPresetId,
  fontControlValues,
  fontFamilyOptions,
  fontPresetName,
  fontPresets,
  renderFontPresetLinkButton,
  selectedBlock,
  selectedPageEditLocked,
  onClearSelectedBlockFontPreset,
  onCreateFontPreset,
  onDeleteFontPreset,
  onFontPresetNameChange,
  onFontSettingChange,
  onSelectFontPreset
}: FontToolSectionProps): React.JSX.Element {
  return (
    <>
      {fontControlValues ? (
        <>
          <div className="compact-tool-field font-picker-field">
            <span>서체</span>
            <FontFamilyPicker
              options={fontFamilyOptions}
              value={fontControlValues.fontFamily ?? DEFAULT_OVERLAY_FONT_FAMILY}
              disabled={selectedPageEditLocked}
              onChange={(fontFamily) => onFontSettingChange({ fontFamily })}
            />
          </div>
          <div className="font-metrics-row font-tool-grid">
            <label className="compact-tool-field font-number-field">
              <span>폰트 크기</span>
              <CompactNumberControl
                ariaLabel="폰트 크기"
                min={8}
                max={120}
                step={1}
                value={fontControlValues.fontSizePx}
                suffix="px"
                disabled={selectedPageEditLocked}
                onChange={(fontSizePx) => onFontSettingChange({ fontSizePx })}
              />
              {renderFontPresetLinkButton("fontSizePx", "폰트 크기")}
            </label>
            <label className="compact-tool-field font-number-field">
              <span>줄 간격</span>
              <CompactNumberControl
                ariaLabel="줄 간격"
                min={0.8}
                max={2}
                step={0.05}
                value={fontControlValues.lineHeight}
                suffix="배"
                disabled={selectedPageEditLocked}
                onChange={(lineHeight) => onFontSettingChange({ lineHeight })}
              />
              {renderFontPresetLinkButton("lineHeight", "줄 간격")}
            </label>
          </div>
          <FontOutlineControls
            values={fontControlValues}
            disabled={selectedPageEditLocked}
            onChange={onFontSettingChange}
            renderLinkButton={renderFontPresetLinkButton}
          />
          <label className="compact-tool-field font-color-field">
            <span>글자색</span>
            <span className="color-picker-shell" style={{ backgroundColor: fontControlValues.textColor ?? "#111111" }}>
              <input
                type="color"
                className="outline-color-input"
                value={fontControlValues.textColor ?? "#111111"}
                disabled={selectedPageEditLocked}
                onChange={(event) => onFontSettingChange({ textColor: event.target.value })}
              />
            </span>
            {renderFontPresetLinkButton("textColor", "글자색")}
          </label>
          <div className="compact-tool-field font-screentone-field">
            <label className="tool-checkbox font-checkbox-field">
              <input
                type="checkbox"
                checked={fontControlValues.screentoneFillEnabled ?? false}
                disabled={selectedPageEditLocked}
                onChange={(event) => onFontSettingChange({ screentoneFillEnabled: event.target.checked })}
              />
              <span>스크린톤 채우기</span>
              <button
                type="button"
                className={`font-inline-toggle ${fontControlValues.screentoneFillAntialias ?? true ? "active" : ""}`}
                disabled={selectedPageEditLocked}
                onClick={(event) => {
                  event.preventDefault();
                  onFontSettingChange({ screentoneFillAntialias: !(fontControlValues.screentoneFillAntialias ?? true) });
                }}
                aria-pressed={fontControlValues.screentoneFillAntialias ?? true}
                title={`스크린톤 안티 ${(fontControlValues.screentoneFillAntialias ?? true) ? "끄기" : "켜기"}`}
              >
                안티
              </button>
              {renderFontPresetLinkButton("screentoneFillEnabled", "스크린톤 채우기")}
            </label>
            <div className="font-screentone-range-row">
              <label className="compact-tool-field font-range-field">
                <span>
                  <span>강도</span>
                  <strong>{Math.round((fontControlValues.screentoneFillIntensity ?? 0.55) * 100)}%</strong>
                </span>
                <input
                  type="range"
                  min={0.05}
                  max={1}
                  step={0.01}
                  value={fontControlValues.screentoneFillIntensity ?? 0.55}
                  style={rangeProgressStyle(fontControlValues.screentoneFillIntensity ?? 0.55, 0.05, 1)}
                  disabled={selectedPageEditLocked}
                  onChange={(event) => onFontSettingChange({ screentoneFillIntensity: Number(event.target.value) })}
                />
                {renderFontPresetLinkButton("screentoneFillIntensity", "스크린톤 강도")}
              </label>
              <label className="compact-tool-field font-range-field">
                <span>
                  <span>밀도</span>
                  <strong>{Math.round((fontControlValues.screentoneFillDensity ?? 0.55) * 100)}%</strong>
                </span>
                <input
                  type="range"
                  min={0.05}
                  max={1}
                  step={0.01}
                  value={fontControlValues.screentoneFillDensity ?? 0.55}
                  style={rangeProgressStyle(fontControlValues.screentoneFillDensity ?? 0.55, 0.05, 1)}
                  disabled={selectedPageEditLocked}
                  onChange={(event) => onFontSettingChange({ screentoneFillDensity: Number(event.target.value) })}
                />
                {renderFontPresetLinkButton("screentoneFillDensity", "스크린톤 밀도")}
              </label>
            </div>
          </div>
          {selectedBlock ? (
            <div className="compact-tool-field font-align-field">
              <span>정렬</span>
              <div className="text-align-control" role="group" aria-label="텍스트 정렬">
                <button
                  className={selectedBlock.textAlign === "left" ? "active" : ""}
                  disabled={selectedPageEditLocked}
                  onClick={() => onFontSettingChange({ textAlign: "left" })}
                >
                  좌
                </button>
                <button
                  className={selectedBlock.textAlign === "center" ? "active" : ""}
                  disabled={selectedPageEditLocked}
                  onClick={() => onFontSettingChange({ textAlign: "center" })}
                >
                  중앙
                </button>
                <button
                  className={selectedBlock.textAlign === "right" ? "active" : ""}
                  disabled={selectedPageEditLocked}
                  onClick={() => onFontSettingChange({ textAlign: "right" })}
                >
                  우
                </button>
              </div>
            </div>
          ) : null}
          <label className="tool-checkbox compact-tool-field font-checkbox-field">
            <input
              type="checkbox"
              checked={fontControlValues.autoFitText ?? true}
              disabled={selectedPageEditLocked}
              onChange={(event) => onFontSettingChange({ autoFitText: event.target.checked })}
            />
            <span>자동 맞춤</span>
            {renderFontPresetLinkButton("autoFitText", "자동 맞춤")}
          </label>
        </>
      ) : (
        <p className="muted-line">블록이나 프리셋을 선택하면 폰트값을 조정할 수 있습니다.</p>
      )}
      <div className="font-preset-panel">
        <div className="font-preset-create">
          <input
            value={fontPresetName}
            disabled={selectedPageEditLocked || !currentChapter}
            placeholder="새 프리셋 이름"
            onChange={(event) => onFontPresetNameChange(event.target.value)}
          />
          <button type="button" disabled={selectedPageEditLocked || !currentChapter} onClick={onCreateFontPreset}>
            만들기
          </button>
        </div>
        <div className="font-preset-tags" aria-label="폰트 프리셋">
          {fontPresets.map((preset) => (
            <span
              key={preset.id}
              className={`font-preset-tag ${
                selectedBlock?.fontPresetId === preset.id || (!selectedBlock && editingFontPresetId === preset.id) ? "active" : ""
              }`}
            >
              <button
                type="button"
                className="font-preset-tag-name"
                disabled={selectedPageEditLocked}
                onClick={() => onSelectFontPreset(preset.id)}
                title={selectedBlock ? `${preset.name} 적용` : `${preset.name} 편집`}
              >
                {preset.name}
              </button>
              <button
                type="button"
                className="font-preset-tag-remove"
                disabled={selectedPageEditLocked}
                onClick={() => onDeleteFontPreset(preset.id)}
                aria-label={`${preset.name} 삭제`}
              >
                ×
              </button>
            </span>
          ))}
          {selectedBlock?.fontPresetId ? (
            <button type="button" className="font-preset-clear" disabled={selectedPageEditLocked} onClick={onClearSelectedBlockFontPreset}>
              프리셋 해제
            </button>
          ) : null}
        </div>
      </div>
    </>
  );
}
