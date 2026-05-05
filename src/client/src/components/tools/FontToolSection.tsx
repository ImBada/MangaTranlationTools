import React from "react";
import type {
  ChapterSnapshot,
  FontPreset,
  FontPresetBackupSnapshot,
  FontPresetBackupSummary,
  FontSizePreset,
  TextPosition,
  TranslationBlock
} from "../../../../shared/types";
import { CompactNumberControl } from "../controls/CompactNumberControl";
import { FontFamilyPicker, type FontFamilyOption } from "../font/FontFamilyPicker";
import { FontOutlineControls } from "../font/FontOutlineControls";
import { FontPresetBackupModal, type FontPresetBackupDialogMode } from "./FontPresetBackupModal";
import type { BlockFontPatch, LinkableFontPresetKey } from "../../lib/fontPresets";
import {
  DEFAULT_OVERLAY_FONT_FAMILY,
  DEFAULT_OVERLAY_FONT_STYLE,
  DEFAULT_OVERLAY_FONT_WEIGHT,
  DEFAULT_OVERLAY_TEXT_DECORATION,
  buildScreentoneFillCssBackground,
  buildScreentoneFillCssSize,
  resolveTextPosition
} from "../../lib/overlayLayout";
import { mouseOnlyCheckboxProps } from "../../lib/mouseOnlyCheckbox";
import { rangeProgressStyle } from "../../lib/rangeProgressStyle";
import type { LayerToolFontControlValues } from "./LayerToolPanelTypes";

type FontToolSectionProps = {
  activeFontSizePresetId: string | null;
  currentChapter: ChapterSnapshot | null;
  editingFontPresetId: string | null;
  fontControlValues: LayerToolFontControlValues | null;
  fontFamilyOptions: FontFamilyOption[];
  fontPresetName: string;
  fontPresets: FontPreset[];
  fontSizePresets: FontSizePreset[];
  renderFontPresetLinkButton: (key: LinkableFontPresetKey, label: string) => React.ReactNode;
  renderFontPresetLinkGroupButton: (keys: LinkableFontPresetKey[], label: string) => React.ReactNode;
  selectedBlock: TranslationBlock | null;
  selectedPageEditLocked: boolean;
  onClearSelectedBlockFontPreset: () => void;
  onClearEditingFontPreset: () => void;
  onCreateFontPreset: () => void;
  onCreateFontPresetListBackup: (name: string) => Promise<FontPresetBackupSnapshot | null>;
  onCreateFontSizePreset: () => void;
  onDeleteFontPresetBackup: (backupId: string) => Promise<FontPresetBackupSummary[]>;
  onDeleteFontPreset: (presetId: string) => void;
  onDeleteFontSizePreset: (presetId: string) => void;
  onFontPresetNameChange: (value: string) => void;
  onFontPresetRename: (presetId: string, name: string) => void;
  onFontSettingChange: (patch: BlockFontPatch) => void;
  onListFontPresetBackups: () => Promise<FontPresetBackupSummary[]>;
  onRestoreFontPresetListBackup: (backupId: string) => Promise<void>;
  onSelectFontPreset: (presetId: string) => void;
  onSelectFontSizePreset: (presetId: string | null) => void;
};

const PRESET_TAG_FONT_SIZE_PX = 18;
const PRESET_TAG_LINE_HEIGHT = 1;
const FONT_WEIGHT_LABELS: Record<number, string> = {
  100: "Thin 100",
  200: "Extra Light 200",
  300: "Light 300",
  400: "Regular 400",
  500: "Medium 500",
  600: "Semi Bold 600",
  700: "Bold 700",
  800: "Extra Bold 800",
  900: "Black 900"
};

const TEXT_POSITION_OPTIONS: { value: TextPosition; label: string }[] = [
  { value: "top-left", label: "상단 좌측" },
  { value: "top", label: "상단 중앙" },
  { value: "top-right", label: "상단 우측" },
  { value: "left", label: "중앙 좌측" },
  { value: "center", label: "중앙" },
  { value: "right", label: "중앙 우측" },
  { value: "bottom-left", label: "하단 좌측" },
  { value: "bottom", label: "하단 중앙" },
  { value: "bottom-right", label: "하단 우측" }
];
const DEFAULT_SHADOW_DISTANCE_PX = 4;

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
    lineHeight: PRESET_TAG_LINE_HEIGHT,
    letterSpacing: `${preset.letterSpacingPx ?? 0}px`,
    fontWeight: preset.fontWeight ?? DEFAULT_OVERLAY_FONT_WEIGHT,
    fontStyle: preset.fontStyle ?? DEFAULT_OVERLAY_FONT_STYLE,
    textDecoration: preset.textDecoration ?? DEFAULT_OVERLAY_TEXT_DECORATION
  };

  return {
    stack: {
      fontFamily: preset.fontFamily ?? DEFAULT_OVERLAY_FONT_FAMILY,
      lineHeight: PRESET_TAG_LINE_HEIGHT,
      letterSpacing: `${preset.letterSpacingPx ?? 0}px`,
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
  activeFontSizePresetId,
  currentChapter,
  editingFontPresetId,
  fontControlValues,
  fontFamilyOptions,
  fontPresetName,
  fontPresets,
  fontSizePresets,
  renderFontPresetLinkButton,
  renderFontPresetLinkGroupButton,
  selectedBlock,
  selectedPageEditLocked,
  onClearSelectedBlockFontPreset,
  onClearEditingFontPreset,
  onCreateFontPreset,
  onCreateFontPresetListBackup,
  onCreateFontSizePreset,
  onDeleteFontPresetBackup,
  onDeleteFontPreset,
  onDeleteFontSizePreset,
  onFontPresetNameChange,
  onFontPresetRename,
  onFontSettingChange,
  onListFontPresetBackups,
  onRestoreFontPresetListBackup,
  onSelectFontPreset,
  onSelectFontSizePreset
}: FontToolSectionProps): React.JSX.Element {
  const activeFontPresetId = selectedBlock?.fontPresetId ?? (!selectedBlock ? editingFontPresetId : null);
  const [renamingFontPresetId, setRenamingFontPresetId] = React.useState<string | null>(null);
  const [renamingFontPresetName, setRenamingFontPresetName] = React.useState("");
  const [backupDialogMode, setBackupDialogMode] = React.useState<FontPresetBackupDialogMode | null>(null);
  const skipNextRenameCommitRef = React.useRef(false);
  const selectedFontFamilyOption = fontFamilyOptions.find((option) => option.value === (fontControlValues?.fontFamily ?? DEFAULT_OVERLAY_FONT_FAMILY));
  const selectedFontWeights = selectedFontFamilyOption?.weights ?? [];
  const currentFontWeight = fontControlValues?.fontWeight ?? DEFAULT_OVERLAY_FONT_WEIGHT;
  const selectedTextPosition = resolveTextPosition(selectedBlock?.textPosition);
  const selectedFontSizePresetId = fontSizePresets.some((preset) => preset.id === activeFontSizePresetId)
    ? activeFontSizePresetId
    : null;
  const fontSizePresetControlsDisabled =
    selectedPageEditLocked ||
    !fontControlValues ||
    (selectedBlock !== null && (!selectedBlock.fontPresetId || selectedBlock.fontSizeLinkedToPreset === false));
  const fontWeightOptions = selectedFontWeights.length > 1
    ? selectedFontWeights.includes(currentFontWeight)
      ? selectedFontWeights
      : [...selectedFontWeights, currentFontWeight].sort((a, b) => a - b)
    : [];
  const shadowActive = fontControlValues?.shadowEnabled ?? ((fontControlValues?.shadowDistancePx ?? 0) > 0);
  const screentoneFillActive = fontControlValues?.screentoneFillEnabled ?? false;

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

  const confirmDeleteFontPreset = React.useCallback(async (preset: FontPreset) => {
    const confirmed = await window.mangaApi.confirm(
      "폰트 프리셋 삭제",
      `"${preset.name}" 프리셋을 삭제할까요?`,
      "이 프리셋이 적용된 텍스트 블록에서는 프리셋 연결만 해제됩니다."
    );
    if (!confirmed) {
      return;
    }
    onDeleteFontPreset(preset.id);
  }, [onDeleteFontPreset]);

  const confirmDeleteFontSizePreset = React.useCallback(async (presetId: string) => {
    const preset = fontSizePresets.find((candidate) => candidate.id === presetId);
    const confirmed = await window.mangaApi.confirm(
      "폰트 크기 프리셋 삭제",
      preset ? `"${preset.name}" 크기 프리셋을 삭제할까요?` : "선택한 크기 프리셋을 삭제할까요?",
      "이 크기 프리셋을 참조하는 폰트 프리셋에서는 크기 프리셋 연결만 해제됩니다."
    );
    if (!confirmed) {
      return;
    }
    onDeleteFontSizePreset(presetId);
  }, [fontSizePresets, onDeleteFontSizePreset]);

  return (
    <>
      {fontControlValues ? (
        <div className="font-control-stack">
          <div className="compact-tool-field font-picker-field">
            <span>서체</span>
            <FontFamilyPicker
              options={fontFamilyOptions}
              value={fontControlValues.fontFamily ?? DEFAULT_OVERLAY_FONT_FAMILY}
              disabled={selectedPageEditLocked}
              onChange={(fontFamily) => onFontSettingChange({ fontFamily })}
            />
          </div>
          <div className="compact-tool-field font-style-field">
            <span>스타일</span>
            <div className="font-style-control">
              <div className="font-style-basic" role="group" aria-label="기본 텍스트 스타일">
                <button
                  type="button"
                  className={currentFontWeight >= 600 ? "active" : ""}
                  disabled={selectedPageEditLocked}
                  onClick={() => onFontSettingChange({ fontWeight: currentFontWeight >= 600 ? 400 : DEFAULT_OVERLAY_FONT_WEIGHT })}
                  aria-pressed={currentFontWeight >= 600}
                  title="볼드"
                >
                  B
                </button>
                <button
                  type="button"
                  className={(fontControlValues.fontStyle ?? DEFAULT_OVERLAY_FONT_STYLE) === "italic" ? "active" : ""}
                  disabled={selectedPageEditLocked}
                  onClick={() => onFontSettingChange({ fontStyle: (fontControlValues.fontStyle ?? DEFAULT_OVERLAY_FONT_STYLE) === "italic" ? DEFAULT_OVERLAY_FONT_STYLE : "italic" })}
                  aria-pressed={(fontControlValues.fontStyle ?? DEFAULT_OVERLAY_FONT_STYLE) === "italic"}
                  title="이탤릭"
                >
                  I
                </button>
                <button
                  type="button"
                  className={(fontControlValues.textDecoration ?? DEFAULT_OVERLAY_TEXT_DECORATION) === "underline" ? "active" : ""}
                  disabled={selectedPageEditLocked}
                  onClick={() => onFontSettingChange({ textDecoration: (fontControlValues.textDecoration ?? DEFAULT_OVERLAY_TEXT_DECORATION) === "underline" ? DEFAULT_OVERLAY_TEXT_DECORATION : "underline" })}
                  aria-pressed={(fontControlValues.textDecoration ?? DEFAULT_OVERLAY_TEXT_DECORATION) === "underline"}
                  title="언더라인"
                >
                  U
                </button>
              </div>
              {fontWeightOptions.length > 1 ? (
                <select
                  className="font-weight-select"
                  value={currentFontWeight}
                  disabled={selectedPageEditLocked}
                  aria-label="폰트 자체 굵기"
                  onChange={(event) => onFontSettingChange({ fontWeight: Number(event.target.value) })}
                >
                  {fontWeightOptions.map((weight) => (
                    <option key={weight} value={weight}>
                      {FONT_WEIGHT_LABELS[weight] ?? `Weight ${weight}`}
                    </option>
                  ))}
                </select>
              ) : null}
              <span className="font-style-link-group">
                {renderFontPresetLinkGroupButton(["fontWeight", "fontStyle", "textDecoration"], "스타일")}
              </span>
            </div>
          </div>
          <div className="compact-tool-field font-size-row-field">
            <span className="font-size-row-label">크기</span>
            <CompactNumberControl
              ariaLabel="폰트 크기"
              min={8}
              step={1}
              value={fontControlValues.fontSizePx}
              suffix="px"
              disabled={selectedPageEditLocked}
              onChange={(fontSizePx) => onFontSettingChange({ fontSizePx })}
            />
            <span className="font-size-row-link">{renderFontPresetLinkButton("fontSizePx", "폰트 크기")}</span>
            <select
              className="font-size-preset-select"
              value={selectedFontSizePresetId ?? ""}
              disabled={fontSizePresetControlsDisabled}
              aria-label="폰트 크기 프리셋"
              onChange={(event) => onSelectFontSizePreset(event.target.value || null)}
            >
              <option value="">개별 지정</option>
              {fontSizePresets.map((preset) => (
                <option key={preset.id} value={preset.id}>
                  {preset.name} · {preset.fontSizePx}px
                </option>
              ))}
            </select>
            <button
              type="button"
              className="font-size-preset-save"
              disabled={fontSizePresetControlsDisabled || !currentChapter}
              onClick={() => {
                if (selectedFontSizePresetId) {
                  void confirmDeleteFontSizePreset(selectedFontSizePresetId);
                  return;
                }
                onCreateFontSizePreset();
              }}
              aria-label={selectedFontSizePresetId ? "선택한 폰트 크기 프리셋 삭제" : "현재 폰트 크기를 크기 프리셋으로 저장"}
              title={selectedFontSizePresetId ? "선택한 크기 프리셋 삭제" : "현재 크기 저장"}
            >
              {selectedFontSizePresetId ? "-" : "+"}
            </button>
          </div>
          <div className="font-metrics-row font-tool-grid">
            <div className="compact-tool-field font-number-field">
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
            </div>
            <div className="compact-tool-field font-number-field">
              <span>글자 간격</span>
              <CompactNumberControl
                ariaLabel="글자 간격"
                min={-10}
                max={30}
                step={0.5}
                value={fontControlValues.letterSpacingPx ?? 0}
                suffix="px"
                disabled={selectedPageEditLocked}
                onChange={(letterSpacingPx) => onFontSettingChange({ letterSpacingPx })}
              />
              {renderFontPresetLinkButton("letterSpacingPx", "글자 간격")}
            </div>
          </div>
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
          <FontOutlineControls
            values={fontControlValues}
            disabled={selectedPageEditLocked}
            onChange={onFontSettingChange}
            renderLinkButton={renderFontPresetLinkButton}
          />
          {selectedBlock ? (
            <div className="font-align-position-row">
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
              <div className="compact-tool-field font-position-field">
                <span>위치</span>
                <div className="text-position-control" role="group" aria-label="텍스트 위치">
                  {TEXT_POSITION_OPTIONS.map((option) => (
                    <button
                      key={option.value}
                      className={selectedTextPosition === option.value ? "active" : ""}
                      disabled={selectedPageEditLocked}
                      title={option.label}
                      aria-label={option.label}
                      aria-pressed={selectedTextPosition === option.value}
                      onClick={() => onFontSettingChange({ textPosition: option.value })}
                    >
                      <span className="text-position-dot" aria-hidden />
                    </button>
                  ))}
                </div>
              </div>
            </div>
          ) : null}
          <label className="tool-checkbox compact-tool-field font-checkbox-field">
            <input
              type="checkbox"
              {...mouseOnlyCheckboxProps}
              checked={fontControlValues.autoFitText ?? true}
              disabled={selectedPageEditLocked}
              onChange={(event) => onFontSettingChange({ autoFitText: event.target.checked })}
            />
            <span>자동 맞춤</span>
            {renderFontPresetLinkButton("autoFitText", "자동 맞춤")}
          </label>
          <div className="compact-tool-field font-outline-section font-shadow-section font-effect-section">
            <div className="font-outline-section-header">
              <span>그림자</span>
              <div className="font-shadow-header-actions">
                <div className="font-outline-mode" aria-label="그림자 사용">
                  <button
                    type="button"
                    className={!shadowActive ? "active" : ""}
                    disabled={selectedPageEditLocked}
                    onClick={() => onFontSettingChange({ shadowEnabled: false })}
                    aria-pressed={!shadowActive}
                  >
                    OFF
                  </button>
                  <button
                    type="button"
                    className={shadowActive ? "active" : ""}
                    disabled={selectedPageEditLocked}
                    onClick={() =>
                      onFontSettingChange({
                        shadowEnabled: true,
                        shadowDistancePx: (fontControlValues.shadowDistancePx ?? 0) > 0
                          ? fontControlValues.shadowDistancePx
                          : DEFAULT_SHADOW_DISTANCE_PX
                      })
                    }
                    aria-pressed={shadowActive}
                  >
                    ON
                  </button>
                </div>
                {renderFontPresetLinkButton("shadowEnabled", "그림자")}
              </div>
            </div>
            {shadowActive ? (
              <div className="font-shadow-row font-tool-grid">
                <label className="compact-tool-field font-color-field">
                  <span>그림자 색</span>
                  <span className="color-picker-shell" style={{ backgroundColor: fontControlValues.shadowColor ?? "#000000" }}>
                    <input
                      type="color"
                      className="outline-color-input"
                      value={fontControlValues.shadowColor ?? "#000000"}
                      disabled={selectedPageEditLocked}
                      onChange={(event) => onFontSettingChange({ shadowColor: event.target.value })}
                    />
                  </span>
                  {renderFontPresetLinkButton("shadowColor", "그림자 색")}
                </label>
                <label className="compact-tool-field font-number-field">
                  <span>그림자 각도</span>
                  <CompactNumberControl
                    ariaLabel="그림자 각도"
                    min={-360}
                    max={360}
                    step={1}
                    value={fontControlValues.shadowAngleDeg ?? 45}
                    suffix="도"
                    disabled={selectedPageEditLocked}
                    onChange={(shadowAngleDeg) => onFontSettingChange({ shadowAngleDeg })}
                  />
                  {renderFontPresetLinkButton("shadowAngleDeg", "그림자 각도")}
                </label>
                <label className="compact-tool-field font-number-field">
                  <span>그림자 거리</span>
                  <CompactNumberControl
                    ariaLabel="그림자 거리"
                    min={0}
                    max={80}
                    step={0.5}
                    value={fontControlValues.shadowDistancePx ?? 0}
                    suffix="px"
                    disabled={selectedPageEditLocked}
                    onChange={(shadowDistancePx) => onFontSettingChange({ shadowDistancePx })}
                  />
                  {renderFontPresetLinkButton("shadowDistancePx", "그림자 거리")}
                </label>
              </div>
            ) : null}
          </div>
          <div className="compact-tool-field font-screentone-field">
            <div className="font-screentone-header">
              <div className="font-screentone-title">
                <span>스크린톤 채우기</span>
              </div>
              <div className="font-screentone-header-actions">
                <div className="font-outline-mode" aria-label="스크린톤 채우기 사용">
                  <button
                    type="button"
                    className={!screentoneFillActive ? "active" : ""}
                    disabled={selectedPageEditLocked}
                    onClick={() => onFontSettingChange({ screentoneFillEnabled: false })}
                    aria-pressed={!screentoneFillActive}
                  >
                    OFF
                  </button>
                  <button
                    type="button"
                    className={screentoneFillActive ? "active" : ""}
                    disabled={selectedPageEditLocked}
                    onClick={() => onFontSettingChange({ screentoneFillEnabled: true })}
                    aria-pressed={screentoneFillActive}
                  >
                    ON
                  </button>
                </div>
                {renderFontPresetLinkButton("screentoneFillEnabled", "스크린톤 채우기")}
              </div>
            </div>
            {screentoneFillActive ? (
              <>
                <div className="font-screentone-option-row">
                  <button
                    type="button"
                    className={`font-inline-toggle ${fontControlValues.screentoneFillAntialias ?? true ? "active" : ""}`}
                    disabled={selectedPageEditLocked}
                    onClick={() => {
                      onFontSettingChange({ screentoneFillAntialias: !(fontControlValues.screentoneFillAntialias ?? true) });
                    }}
                    aria-pressed={fontControlValues.screentoneFillAntialias ?? true}
                    title={`스크린톤 안티 ${(fontControlValues.screentoneFillAntialias ?? true) ? "끄기" : "켜기"}`}
                  >
                    안티
                  </button>
                </div>
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
              </>
            ) : null}
          </div>
        </div>
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
                  onClick={() => void confirmDeleteFontPreset(preset)}
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
        <div className="font-preset-backup-actions">
          <button
            type="button"
            disabled={!currentChapter}
            onClick={() => setBackupDialogMode("backup")}
          >
            백업
          </button>
          <button
            type="button"
            disabled={!currentChapter || selectedPageEditLocked}
            onClick={() => setBackupDialogMode("restore")}
          >
            복원
          </button>
        </div>
      </div>
      {backupDialogMode ? (
        <FontPresetBackupModal
          mode={backupDialogMode}
          onCancel={() => setBackupDialogMode(null)}
          onCreateBackup={onCreateFontPresetListBackup}
          onDeleteBackup={onDeleteFontPresetBackup}
          onListBackups={onListFontPresetBackups}
          onRestoreBackup={onRestoreFontPresetListBackup}
        />
      ) : null}
    </>
  );
}
