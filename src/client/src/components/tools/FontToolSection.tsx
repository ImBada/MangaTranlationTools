import React from "react";
import type { ChapterSnapshot, FontPreset, TranslationBlock } from "../../../../shared/types";
import { CompactNumberControl } from "../controls/CompactNumberControl";
import { FontFamilyPicker, type FontFamilyOption } from "../font/FontFamilyPicker";
import { FontOutlineControls } from "../font/FontOutlineControls";
import type { BlockFontPatch, LinkableFontPresetKey } from "../../lib/fontPresets";
import {
  DEFAULT_OVERLAY_FONT_FAMILY,
  buildScreentoneFillCssBackground,
  buildScreentoneFillCssSize
} from "../../lib/overlayLayout";
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
  onClearEditingFontPreset: () => void;
  onCreateFontPreset: () => void;
  onDeleteFontPreset: (presetId: string) => void;
  onFontPresetNameChange: (value: string) => void;
  onFontPresetRename: (presetId: string, name: string) => void;
  onFontSettingChange: (patch: BlockFontPatch) => void;
  onSelectFontPreset: (presetId: string) => void;
};

const PRESET_TAG_FONT_SIZE_PX = 12;

function resolvePresetTagTextMetrics(preset: FontPreset): {
  fontSizePx: number;
  outlineWidthPx: number;
  secondaryOutlineWidthPx: number;
  scale: number;
} {
  const sourceFontSizePx = Math.max(1, preset.fontSizePx);
  const sourceOutlineWidthPx = Math.max(0, preset.outlineWidthPx ?? 0);
  const sourceSecondaryOutlineWidthPx = Math.max(0, preset.secondaryOutlineWidthPx ?? 0);
  const sourceOuterStrokeWidthPx = sourceOutlineWidthPx + sourceSecondaryOutlineWidthPx * 2;
  const scale = PRESET_TAG_FONT_SIZE_PX / Math.max(1, sourceFontSizePx + sourceOuterStrokeWidthPx * 2);

  return {
    fontSizePx: sourceFontSizePx,
    outlineWidthPx: sourceOutlineWidthPx,
    secondaryOutlineWidthPx: sourceSecondaryOutlineWidthPx,
    scale
  };
}

function buildPresetTagTextStyles(preset: FontPreset): {
  stack: React.CSSProperties;
  primary: React.CSSProperties;
  secondary: React.CSSProperties;
  hasSecondaryOutline: boolean;
} {
  const { fontSizePx, outlineWidthPx, secondaryOutlineWidthPx, scale } = resolvePresetTagTextMetrics(preset);
  const hasSecondaryOutline = secondaryOutlineWidthPx > 0;
  const combinedSecondaryOutlineWidthPx = outlineWidthPx + secondaryOutlineWidthPx * 2;
  const screentoneFillEnabled = preset.screentoneFillEnabled ?? false;
  const screentoneFillStyle: React.CSSProperties = screentoneFillEnabled
    ? {
        WebkitTextFillColor: "transparent",
        WebkitBackgroundClip: "text",
        backgroundClip: "text",
        backgroundColor: "#ffffff",
        backgroundImage: buildScreentoneFillCssBackground(
          preset.textColor ?? "#111111",
          preset.screentoneFillIntensity,
          preset.screentoneFillDensity,
          preset.screentoneFillAntialias,
          fontSizePx
        ),
        backgroundSize: buildScreentoneFillCssSize(fontSizePx, preset.screentoneFillDensity)
      }
    : {};

  const baseTextStyle: React.CSSProperties = {
    fontFamily: preset.fontFamily ?? DEFAULT_OVERLAY_FONT_FAMILY,
    fontSize: `${fontSizePx}px`,
    lineHeight: preset.lineHeight,
    fontWeight: "inherit"
  };

  return {
    stack: {
      fontFamily: preset.fontFamily ?? DEFAULT_OVERLAY_FONT_FAMILY,
      lineHeight: preset.lineHeight,
      zoom: scale
    },
    primary: {
      ...baseTextStyle,
      color: screentoneFillEnabled ? undefined : preset.textColor ?? "#111111",
      WebkitTextFillColor: screentoneFillEnabled ? "transparent" : preset.textColor ?? "#111111",
      paintOrder: "stroke fill",
      WebkitTextStroke: outlineWidthPx > 0 ? `${outlineWidthPx}px ${preset.outlineColor ?? "#000000"}` : undefined,
      ...screentoneFillStyle
    },
    secondary: {
      ...baseTextStyle,
      color: "transparent",
      WebkitTextFillColor: "transparent",
      WebkitTextStroke:
        combinedSecondaryOutlineWidthPx > 0 ? `${combinedSecondaryOutlineWidthPx}px ${preset.secondaryOutlineColor ?? "#ffffff"}` : undefined
    },
    hasSecondaryOutline
  };
}

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
  onClearEditingFontPreset,
  onCreateFontPreset,
  onDeleteFontPreset,
  onFontPresetNameChange,
  onFontPresetRename,
  onFontSettingChange,
  onSelectFontPreset
}: FontToolSectionProps): React.JSX.Element {
  const activeFontPresetId = selectedBlock?.fontPresetId ?? (!selectedBlock ? editingFontPresetId : null);
  const [renamingFontPresetId, setRenamingFontPresetId] = React.useState<string | null>(null);
  const [renamingFontPresetName, setRenamingFontPresetName] = React.useState("");
  const skipNextRenameCommitRef = React.useRef(false);

  React.useEffect(() => {
    if (!renamingFontPresetId || renamingFontPresetId === activeFontPresetId) {
      return;
    }
    setRenamingFontPresetId(null);
    setRenamingFontPresetName("");
  }, [activeFontPresetId, renamingFontPresetId]);

  const commitFontPresetRename = React.useCallback(() => {
    if (skipNextRenameCommitRef.current) {
      skipNextRenameCommitRef.current = false;
      return;
    }
    if (!renamingFontPresetId) {
      return;
    }
    const currentPreset = fontPresets.find((preset) => preset.id === renamingFontPresetId);
    if (!currentPreset) {
      setRenamingFontPresetId(null);
      setRenamingFontPresetName("");
      return;
    }
    const nextName = renamingFontPresetName.trim();
    if (!nextName || fontPresets.some((preset) => preset.id !== currentPreset.id && preset.name.trim() === nextName)) {
      onFontPresetRename(currentPreset.id, nextName);
      setRenamingFontPresetName(currentPreset.name);
      setRenamingFontPresetId(null);
      return;
    }
    if (nextName !== currentPreset.name) {
      onFontPresetRename(currentPreset.id, nextName);
    }
    setRenamingFontPresetId(null);
    setRenamingFontPresetName("");
  }, [fontPresets, onFontPresetRename, renamingFontPresetId, renamingFontPresetName]);

  const cancelFontPresetRename = React.useCallback(() => {
    skipNextRenameCommitRef.current = true;
    setRenamingFontPresetId(null);
    setRenamingFontPresetName("");
  }, []);

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
          {fontPresets.map((preset) => {
            const tagTextStyles = buildPresetTagTextStyles(preset);
            const active = activeFontPresetId === preset.id;
            const renaming = renamingFontPresetId === preset.id;

            return (
              <span key={preset.id} className={`font-preset-tag ${active ? "active" : ""}`}>
                {renaming ? (
                  <input
                    className="font-preset-tag-name-input"
                    value={renamingFontPresetName}
                    disabled={selectedPageEditLocked}
                    aria-label={`${preset.name} 이름 변경`}
                    autoFocus
                    onBlur={commitFontPresetRename}
                    onChange={(event) => setRenamingFontPresetName(event.target.value)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") {
                        event.currentTarget.blur();
                      }
                      if (event.key === "Escape") {
                        cancelFontPresetRename();
                      }
                    }}
                  />
                ) : (
                  <button
                    type="button"
                    className="font-preset-tag-name"
                    disabled={selectedPageEditLocked}
                    onClick={() => {
                      if (!active) {
                        onSelectFontPreset(preset.id);
                        return;
                      }
                      setRenamingFontPresetId(preset.id);
                      setRenamingFontPresetName(preset.name);
                    }}
                    title={active ? `${preset.name} 이름 변경` : selectedBlock ? `${preset.name} 적용` : `${preset.name} 편집`}
                  >
                    <span className="font-preset-tag-text-stack" style={tagTextStyles.stack}>
                      {tagTextStyles.hasSecondaryOutline ? (
                        <span className="overlay-text-content font-preset-tag-text-secondary" style={tagTextStyles.secondary} aria-hidden>
                          {preset.name}
                        </span>
                      ) : null}
                      <span className="overlay-text-content font-preset-tag-text-primary" style={tagTextStyles.primary}>
                        {preset.name}
                      </span>
                    </span>
                  </button>
                )}
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
            );
          })}
          {selectedBlock?.fontPresetId ? (
            <button type="button" className="font-preset-clear" disabled={selectedPageEditLocked} onClick={onClearSelectedBlockFontPreset}>
              프리셋 해제
            </button>
          ) : null}
          {!selectedBlock && editingFontPresetId ? (
            <button type="button" className="font-preset-clear" disabled={selectedPageEditLocked} onClick={onClearEditingFontPreset}>
              프리셋 선택 해제
            </button>
          ) : null}
        </div>
      </div>
    </>
  );
}
