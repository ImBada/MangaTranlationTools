import React from "react";
import type { FontPreset, TranslationBlock } from "../../../shared/types";
import { resolveBlockRotationDeg } from "../../../shared/geometry";
import {
  DEFAULT_OVERLAY_FONT_FAMILY,
  DEFAULT_OVERLAY_FONT_STYLE,
  DEFAULT_OVERLAY_FONT_WEIGHT,
  DEFAULT_OVERLAY_TEXT_DECORATION,
  buildScreentoneFillCssBackground,
  buildScreentoneFillCssSize,
  hexToRgba,
  resolveBlockTextLayout,
  resolveCharacterFontRuns,
  resolveTextLetterSpacingPx,
  resolveTextPositionFactors,
  resolveWrappedTextLines,
  type ViewportSize
} from "../lib/overlayLayout";
import { isBlockFontPresetValueLinked } from "../lib/fontPresets";
import { useRepeatingStepControl } from "./controls/useRepeatingStepControl";

type OverlayBlockProps = {
  block: TranslationBlock;
  pageSize: ViewportSize;
  stageSize: ViewportSize;
  selected: boolean;
  editingEnabled: boolean;
  widgetsVisible?: boolean;
  inlineEditDraft?: string;
  favoriteFontPresets?: FontPreset[];
  dragPreviewHidden?: boolean;
  visualContentVisible?: boolean;
  onInlineEditCancel?: () => void;
  onInlineEditChange?: (value: string) => void;
  onInlineEditCommit?: () => void;
  inlineEditorRef?: React.Ref<HTMLTextAreaElement>;
  onStartInlineEdit?: (event: React.MouseEvent) => void;
  onFavoriteFontPresetSelect?: (presetId: string) => void;
  onFontStyleCopy?: () => void | Promise<void>;
  onFontSizeChange?: (fontSizePx: number) => void;
  onAutoFitDisable?: () => void;
  onTextAlignChange?: (textAlign: TranslationBlock["textAlign"]) => void;
  onPointerDown: (event: React.PointerEvent) => void;
  onResizePointerDown: (event: React.PointerEvent) => void;
  onRotatePointerDown: (event: React.PointerEvent) => void;
};

const TEXT_ALIGN_OPTIONS: readonly { value: TranslationBlock["textAlign"]; label: string; shortLabel: string }[] = [
  { value: "left", label: "좌측 정렬", shortLabel: "좌" },
  { value: "center", label: "가운데 정렬", shortLabel: "중" },
  { value: "right", label: "우측 정렬", shortLabel: "우" }
];
const OVERLAY_FONT_SIZE_MIN_PX = 8;
const OVERLAY_FONT_SIZE_MAX_PX = 240;
const OVERLAY_FONT_SIZE_STEP_PX = 1;
const noopFontSizeChange = () => undefined;

function stopInlineEditorEvent(event: React.SyntheticEvent): void {
  event.stopPropagation();
}

export function OverlayBlock({
  block,
  pageSize,
  stageSize,
  selected,
  editingEnabled,
  widgetsVisible = true,
  inlineEditDraft,
  favoriteFontPresets = [],
  dragPreviewHidden = false,
  visualContentVisible = true,
  onInlineEditCancel,
  onInlineEditChange,
  onInlineEditCommit,
  inlineEditorRef,
  onStartInlineEdit,
  onFavoriteFontPresetSelect,
  onFontStyleCopy,
  onFontSizeChange,
  onAutoFitDisable,
  onTextAlignChange,
  onPointerDown,
  onResizePointerDown,
  onRotatePointerDown
}: OverlayBlockProps): React.JSX.Element | null {
  const autoFitTextEnabled = block.autoFitText ?? true;
  const selectedHighlightVisible = selected && editingEnabled;
  const selectedControlsVisible = selectedHighlightVisible && widgetsVisible;
  const fontSizeLinkedToPreset = Boolean(block.fontPresetId) && isBlockFontPresetValueLinked(block, "fontSizePx");
  const fontSizeStepControl = useRepeatingStepControl({
    disabled: !selectedControlsVisible || autoFitTextEnabled || !onFontSizeChange,
    max: OVERLAY_FONT_SIZE_MAX_PX,
    min: OVERLAY_FONT_SIZE_MIN_PX,
    onChange: onFontSizeChange ?? noopFontSizeChange,
    step: OVERLAY_FONT_SIZE_STEP_PX,
    value: block.fontSizePx
  });

  if (block.renderDirection === "hidden") {
    return null;
  }

  const displayText = block.translatedText || block.sourceText || "...";
  const layout = resolveBlockTextLayout(block, displayText, pageSize, stageSize);
  const horizontalLines =
    visualContentVisible && block.renderDirection === "horizontal"
      ? resolveWrappedTextLines(block, displayText, layout.fontSizePx, layout.fitInnerWidth)
      : [];
  const renderScale = Math.max(stageSize.width / pageSize.width, stageSize.height / pageSize.height);
  const outlineWidthPx = Math.max(0, block.outlineWidthPx ?? 0) * renderScale;
  const secondaryOutlineWidthPx = Math.max(0, block.secondaryOutlineWidthPx ?? 0) * renderScale;
  const combinedSecondaryOutlineWidthPx = outlineWidthPx + secondaryOutlineWidthPx * 2;
  const shadowDistancePx = Math.max(0, block.shadowDistancePx ?? 0) * renderScale;
  const shadowBlurPx = Math.max(0, block.shadowBlurPx ?? 0) * renderScale;
  const shadowOpacity = Math.max(0, Math.min(1, block.shadowOpacity ?? 1));
  const shadowAngleRad = ((block.shadowAngleDeg ?? 45) * Math.PI) / 180;
  const shadowOffsetX = Math.cos(shadowAngleRad) * shadowDistancePx;
  const shadowOffsetY = Math.sin(shadowAngleRad) * shadowDistancePx;
  const shadowStrokeWidthPx = secondaryOutlineWidthPx > 0 ? combinedSecondaryOutlineWidthPx : outlineWidthPx;
  const shadowEnabled =
    visualContentVisible &&
    (block.shadowEnabled ?? ((block.shadowDistancePx ?? 0) > 0 || (block.shadowBlurPx ?? 0) > 0)) &&
    shadowOpacity > 0 &&
    (shadowDistancePx > 0 || shadowBlurPx > 0);
  const shadowColor = hexToRgba(block.shadowColor ?? "#000000", shadowOpacity);
  const rotationDeg = resolveBlockRotationDeg(block);
  const screentoneFillEnabled = visualContentVisible && (block.screentoneFillEnabled ?? false);
  const horizontalLineAlignSelf = block.textAlign === "left" ? "flex-start" : block.textAlign === "right" ? "flex-end" : "center";
  const textPositionFactors = resolveTextPositionFactors(block.textPosition);
  const textPositionJustifyContent = textPositionFactors.x === 0 ? "flex-start" : textPositionFactors.x === 1 ? "flex-end" : "center";
  const textPositionAlignItems = textPositionFactors.y === 0 ? "flex-start" : textPositionFactors.y === 1 ? "flex-end" : "center";
  const letterSpacingPx = resolveTextLetterSpacingPx(block, layout.fontSizePx);
  const screentoneFillStyle: React.CSSProperties = screentoneFillEnabled
    ? {
        WebkitTextFillColor: "transparent",
        WebkitBackgroundClip: "text",
        backgroundClip: "text",
        backgroundColor: "#ffffff",
        backgroundImage: buildScreentoneFillCssBackground(
          block.textColor,
          block.screentoneFillIntensity,
          block.screentoneFillDensity,
          block.screentoneFillAntialias,
          layout.fontSizePx
        ),
        backgroundSize: buildScreentoneFillCssSize(layout.fontSizePx, block.screentoneFillDensity)
      }
    : {};
  const style: React.CSSProperties = {
    left: layout.rect.left,
    top: layout.rect.top,
    width: layout.rect.width,
    height: layout.rect.height,
    boxSizing: "border-box",
    padding: layout.paddingPx,
    color: visualContentVisible && !screentoneFillEnabled ? block.textColor : "transparent",
    backgroundColor: visualContentVisible ? hexToRgba(block.backgroundColor, editingEnabled ? block.opacity : 0) : "transparent",
    borderColor: editingEnabled ? undefined : "transparent",
    boxShadow: editingEnabled ? undefined : "none",
    fontFamily: block.fontFamily ?? DEFAULT_OVERLAY_FONT_FAMILY,
    fontWeight: block.fontWeight ?? DEFAULT_OVERLAY_FONT_WEIGHT,
    fontStyle: block.fontStyle ?? DEFAULT_OVERLAY_FONT_STYLE,
    textDecoration: block.textDecoration ?? DEFAULT_OVERLAY_TEXT_DECORATION,
    fontSize: `${layout.fontSizePx}px`,
    lineHeight: block.lineHeight,
    letterSpacing: `${letterSpacingPx}px`,
    textAlign: block.textAlign,
    pointerEvents: editingEnabled ? undefined : "none",
    opacity: dragPreviewHidden ? 0 : undefined,
    transform: rotationDeg !== 0 ? `rotate(${rotationDeg}deg)` : undefined,
    transformOrigin: "center center",
    zIndex: inlineEditDraft !== undefined ? 70 : selectedHighlightVisible ? 50 : undefined,
    overflow: inlineEditDraft !== undefined || selectedControlsVisible ? "visible" : "hidden"
  };
  const textWrapStyle: React.CSSProperties = {
    boxSizing: "border-box",
    width: layout.innerWidth,
    maxWidth: "100%",
    height: layout.innerHeight,
    maxHeight: "100%",
    overflow: "hidden",
    justifyContent: textPositionJustifyContent,
    alignItems: textPositionAlignItems
  };
  const contentFrameStyle: React.CSSProperties = {
    position: "relative",
    width: block.renderDirection === "vertical" ? `${layout.fitInnerWidth}px` : "fit-content",
    height: block.renderDirection === "vertical" ? `${layout.fitInnerHeight}px` : undefined,
    maxWidth: "100%",
    maxHeight: "100%"
  };
  const baseContentStyle: React.CSSProperties = {
    boxSizing: "border-box",
    writingMode: block.renderDirection === "vertical" ? "vertical-rl" : "horizontal-tb",
    textOrientation: block.renderDirection === "vertical" ? "upright" : undefined,
    width: "100%",
    height: block.renderDirection === "vertical" ? "100%" : undefined,
    maxWidth: "100%",
    maxHeight: "100%",
  };
  const contentStyle: React.CSSProperties = {
    ...baseContentStyle,
    position: "relative",
    zIndex: 2,
    WebkitTextStroke: outlineWidthPx > 0 ? `${outlineWidthPx}px ${block.outlineColor ?? "#000000"}` : undefined,
    ...screentoneFillStyle
  };
  const secondaryOutlineStyle: React.CSSProperties = {
    ...baseContentStyle,
    position: "absolute",
    inset: 0,
    zIndex: 1,
    pointerEvents: "none",
    color: "transparent",
    WebkitTextFillColor: "transparent",
    WebkitTextStroke:
      combinedSecondaryOutlineWidthPx > 0 ? `${combinedSecondaryOutlineWidthPx}px ${block.secondaryOutlineColor ?? "#ffffff"}` : undefined
  };
  const shadowStyle: React.CSSProperties = {
    ...baseContentStyle,
    position: "absolute",
    inset: 0,
    zIndex: 0,
    pointerEvents: "none",
    color: shadowColor,
    filter: shadowBlurPx > 0 ? `blur(${shadowBlurPx}px)` : undefined,
    WebkitTextFillColor: shadowColor,
    WebkitTextStroke: shadowStrokeWidthPx > 0 ? `${shadowStrokeWidthPx}px ${shadowColor}` : undefined,
    transform: shadowOffsetX !== 0 || shadowOffsetY !== 0 ? `translate(${shadowOffsetX}px, ${shadowOffsetY}px)` : undefined
  };
  const lineStyle: React.CSSProperties = {
    alignSelf: horizontalLineAlignSelf,
    ...screentoneFillStyle
  };
  const secondaryOutlineLineStyle: React.CSSProperties = {
    alignSelf: horizontalLineAlignSelf
  };
  const characterFontRunCache = new Map<string, ReturnType<typeof resolveCharacterFontRuns>>();
  const renderTextWithCharacterFonts = (text: string): React.ReactNode => {
    let runs = characterFontRunCache.get(text);
    if (!runs) {
      runs = resolveCharacterFontRuns(block, text);
      characterFontRunCache.set(text, runs);
    }
    if (runs.length === 1 && !runs[0].fontFamily) {
      return text;
    }
    return runs.map((run, index) => (
      <span key={`${run.text}-${index}`} style={run.fontFamily ? { fontFamily: run.fontFamily } : undefined}>
        {run.text}
      </span>
    ));
  };
  const renderTextContent = (contentLayerStyle: React.CSSProperties, contentLineStyle: React.CSSProperties, ariaHidden = false) => (
    <span className="overlay-text-content" style={contentLayerStyle} aria-hidden={ariaHidden}>
      {block.renderDirection === "horizontal"
        ? horizontalLines.map((line, index) => (
            <span key={`${line}-${index}`} className="overlay-text-line" style={contentLineStyle}>
              {renderTextWithCharacterFonts(line)}
            </span>
          ))
        : renderTextWithCharacterFonts(displayText)}
    </span>
  );
  return (
    <div
      data-testid="translation-block"
      data-block-id={block.id}
      className={`${selectedHighlightVisible ? "overlay-block selected" : "overlay-block"}${layout.overflow && editingEnabled ? " overflowing" : ""}`}
      style={style}
      title={layout.overflow && editingEnabled ? "현재 render box보다 번역문이 길어서 넘칩니다." : undefined}
      onPointerDown={onPointerDown}
      onDoubleClick={editingEnabled ? onStartInlineEdit : undefined}
    >
      {inlineEditDraft !== undefined ? (
        <textarea
          ref={inlineEditorRef}
          className="overlay-inline-editor"
          value={inlineEditDraft}
          aria-label="블록 번역문 바로 편집"
          autoFocus
          onMouseDown={stopInlineEditorEvent}
          onMouseMove={stopInlineEditorEvent}
          onMouseUp={stopInlineEditorEvent}
          onClick={stopInlineEditorEvent}
          onPointerDown={stopInlineEditorEvent}
          onPointerMove={stopInlineEditorEvent}
          onPointerUp={stopInlineEditorEvent}
          onDoubleClick={stopInlineEditorEvent}
          onChange={(event) => onInlineEditChange?.(event.target.value)}
          onBlur={onInlineEditCommit}
          onKeyDown={(event) => {
            if (event.key === "Escape") {
              event.preventDefault();
              onInlineEditCancel?.();
              return;
            }
            if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) {
              event.preventDefault();
              event.currentTarget.blur();
            }
          }}
        />
      ) : null}
      {visualContentVisible ? (
        <div className="overlay-text" style={textWrapStyle}>
          <span className="overlay-text-layer-stack" style={contentFrameStyle}>
            {shadowEnabled ? renderTextContent(shadowStyle, secondaryOutlineLineStyle, true) : null}
            {secondaryOutlineWidthPx > 0 ? renderTextContent(secondaryOutlineStyle, secondaryOutlineLineStyle, true) : null}
            {renderTextContent(contentStyle, lineStyle)}
          </span>
        </div>
      ) : null}
      {selectedControlsVisible && onTextAlignChange ? (
        <div
          className="overlay-align-controls"
          role="group"
          aria-label="텍스트 정렬"
          onPointerDown={stopInlineEditorEvent}
          onClick={stopInlineEditorEvent}
          onDoubleClick={stopInlineEditorEvent}
        >
          {TEXT_ALIGN_OPTIONS.map((option) => (
            <button
              key={option.value}
              type="button"
              className={block.textAlign === option.value ? "active" : ""}
              aria-label={option.label}
              onClick={(event) => {
                event.preventDefault();
                event.stopPropagation();
                onTextAlignChange(option.value);
              }}
            >
              {option.shortLabel}
            </button>
          ))}
        </div>
      ) : null}
      {selectedControlsVisible && favoriteFontPresets.length > 0 ? (
        <div
          className="overlay-favorite-tags"
          role="group"
          aria-label="즐겨찾기 폰트 프리셋"
          onPointerDown={stopInlineEditorEvent}
          onClick={stopInlineEditorEvent}
          onDoubleClick={stopInlineEditorEvent}
        >
          {favoriteFontPresets.map((preset) => (
            <button
              key={preset.id}
              type="button"
              className={block.fontPresetId === preset.id ? "overlay-favorite-tag active" : "overlay-favorite-tag"}
              aria-pressed={block.fontPresetId === preset.id}
              title={`${preset.name} 적용`}
              onClick={(event) => {
                event.preventDefault();
                event.stopPropagation();
                onFavoriteFontPresetSelect?.(preset.id);
              }}
            >
              {preset.name}
            </button>
          ))}
        </div>
      ) : null}
      {selectedControlsVisible && (onAutoFitDisable || onFontSizeChange || onFontStyleCopy) ? (
        <div
          className="overlay-left-controls"
          role="group"
          aria-label="블록 빠른 폰트 설정"
          onPointerDown={stopInlineEditorEvent}
          onClick={stopInlineEditorEvent}
          onDoubleClick={stopInlineEditorEvent}
        >
          {autoFitTextEnabled && onAutoFitDisable ? (
            <button
              type="button"
              className="overlay-auto-fit-off-button"
              aria-label="자동 맞춤 끄기"
              title="자동 맞춤 끄기"
              onClick={(event) => {
                event.preventDefault();
                event.stopPropagation();
                onAutoFitDisable();
              }}
            >
              <span>자동</span>
              <span>맞춤</span>
              <strong>OFF</strong>
            </button>
          ) : onFontSizeChange ? (
            <div className={`overlay-font-size-controls${fontSizeLinkedToPreset ? " linked" : ""}`} role="group" aria-label="개별 폰트 크기">
              <button
                type="button"
                className={`overlay-font-size-step${fontSizeStepControl.pressedDirection === 1 ? " pressed" : ""}`}
                aria-label="폰트 크기 증가"
                disabled={block.fontSizePx >= OVERLAY_FONT_SIZE_MAX_PX}
                onPointerDown={(event) => fontSizeStepControl.handlePointerDown(event, 1)}
                onPointerUp={fontSizeStepControl.stopRepeat}
                onPointerCancel={fontSizeStepControl.handlePointerCancel}
                onLostPointerCapture={fontSizeStepControl.stopRepeat}
                onClick={() => fontSizeStepControl.handleClick(1)}
              >
                +
              </button>
              <div className="overlay-font-size-value">
                <input
                  type="number"
                  min={OVERLAY_FONT_SIZE_MIN_PX}
                  max={OVERLAY_FONT_SIZE_MAX_PX}
                  step={OVERLAY_FONT_SIZE_STEP_PX}
                  value={Math.round(block.fontSizePx)}
                  aria-label="개별 폰트 크기"
                  onChange={(event) => fontSizeStepControl.setValue(Number(event.target.value))}
                />
                <span>px</span>
              </div>
              <button
                type="button"
                className={`overlay-font-size-step${fontSizeStepControl.pressedDirection === -1 ? " pressed" : ""}`}
                aria-label="폰트 크기 감소"
                disabled={block.fontSizePx <= OVERLAY_FONT_SIZE_MIN_PX}
                onPointerDown={(event) => fontSizeStepControl.handlePointerDown(event, -1)}
                onPointerUp={fontSizeStepControl.stopRepeat}
                onPointerCancel={fontSizeStepControl.handlePointerCancel}
                onLostPointerCapture={fontSizeStepControl.stopRepeat}
                onClick={() => fontSizeStepControl.handleClick(-1)}
              >
                -
              </button>
            </div>
          ) : null}
          {onFontStyleCopy ? (
            <button
              type="button"
              className="overlay-font-style-copy-button"
              aria-label="현재 폰트 설정 복사"
              title="현재 폰트 설정 복사"
              onClick={(event) => {
                event.preventDefault();
                event.stopPropagation();
                void onFontStyleCopy();
              }}
            >
              COPY
            </button>
          ) : null}
        </div>
      ) : null}
      {selectedControlsVisible ? <button className="rotate-handle" onPointerDown={onRotatePointerDown} aria-label="Rotate" /> : null}
      {selectedControlsVisible ? <button className="resize-handle" onPointerDown={onResizePointerDown} aria-label="Resize" /> : null}
    </div>
  );
}
