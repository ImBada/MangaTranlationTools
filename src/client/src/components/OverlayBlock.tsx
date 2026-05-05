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
  resolveTextLetterSpacingPx,
  resolveTextPositionFactors,
  resolveWrappedTextLines,
  type ViewportSize
} from "../lib/overlayLayout";

type OverlayBlockProps = {
  block: TranslationBlock;
  pageSize: ViewportSize;
  stageSize: ViewportSize;
  selected: boolean;
  editingEnabled: boolean;
  inlineEditDraft?: string;
  favoriteFontPresets?: FontPreset[];
  visualContentVisible?: boolean;
  onInlineEditCancel?: () => void;
  onInlineEditChange?: (value: string) => void;
  onInlineEditCommit?: () => void;
  onStartInlineEdit?: (event: React.MouseEvent) => void;
  onFavoriteFontPresetSelect?: (presetId: string) => void;
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

function stopInlineEditorEvent(event: React.SyntheticEvent): void {
  event.stopPropagation();
}

export function OverlayBlock({
  block,
  pageSize,
  stageSize,
  selected,
  editingEnabled,
  inlineEditDraft,
  favoriteFontPresets = [],
  visualContentVisible = true,
  onInlineEditCancel,
  onInlineEditChange,
  onInlineEditCommit,
  onStartInlineEdit,
  onFavoriteFontPresetSelect,
  onTextAlignChange,
  onPointerDown,
  onResizePointerDown,
  onRotatePointerDown
}: OverlayBlockProps): React.JSX.Element | null {
  if (block.renderDirection === "hidden") {
    return null;
  }

  const displayText = block.translatedText || block.sourceText || "...";
  const layout = resolveBlockTextLayout(block, displayText, pageSize, stageSize);
  const horizontalLines =
    visualContentVisible && block.renderDirection === "horizontal"
      ? resolveWrappedTextLines(block, displayText, layout.fontSizePx, layout.fitInnerWidth)
      : [];
  const outlineWidthPx = Math.max(0, block.outlineWidthPx ?? 0) * Math.max(stageSize.width / pageSize.width, stageSize.height / pageSize.height);
  const secondaryOutlineWidthPx = Math.max(0, block.secondaryOutlineWidthPx ?? 0) * Math.max(stageSize.width / pageSize.width, stageSize.height / pageSize.height);
  const combinedSecondaryOutlineWidthPx = outlineWidthPx + secondaryOutlineWidthPx * 2;
  const shadowDistancePx = Math.max(0, block.shadowDistancePx ?? 0) * Math.max(stageSize.width / pageSize.width, stageSize.height / pageSize.height);
  const shadowAngleRad = ((block.shadowAngleDeg ?? 45) * Math.PI) / 180;
  const shadowOffsetX = Math.cos(shadowAngleRad) * shadowDistancePx;
  const shadowOffsetY = Math.sin(shadowAngleRad) * shadowDistancePx;
  const shadowStrokeWidthPx = secondaryOutlineWidthPx > 0 ? combinedSecondaryOutlineWidthPx : outlineWidthPx;
  const shadowEnabled = visualContentVisible && (block.shadowEnabled ?? ((block.shadowDistancePx ?? 0) > 0)) && shadowDistancePx > 0;
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
    transform: rotationDeg !== 0 ? `rotate(${rotationDeg}deg)` : undefined,
    transformOrigin: "center center",
    zIndex: inlineEditDraft !== undefined ? 70 : selected && editingEnabled ? 50 : undefined,
    overflow: inlineEditDraft !== undefined || (selected && editingEnabled) ? "visible" : "hidden"
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
    color: block.shadowColor ?? "#000000",
    WebkitTextFillColor: block.shadowColor ?? "#000000",
    WebkitTextStroke: shadowStrokeWidthPx > 0 ? `${shadowStrokeWidthPx}px ${block.shadowColor ?? "#000000"}` : undefined,
    transform: `translate(${shadowOffsetX}px, ${shadowOffsetY}px)`
  };
  const lineStyle: React.CSSProperties = {
    alignSelf: horizontalLineAlignSelf,
    ...screentoneFillStyle
  };
  const secondaryOutlineLineStyle: React.CSSProperties = {
    alignSelf: horizontalLineAlignSelf
  };
  const renderTextContent = (contentLayerStyle: React.CSSProperties, contentLineStyle: React.CSSProperties, ariaHidden = false) => (
    <span className="overlay-text-content" style={contentLayerStyle} aria-hidden={ariaHidden}>
      {block.renderDirection === "horizontal"
        ? horizontalLines.map((line, index) => (
            <span key={`${line}-${index}`} className="overlay-text-line" style={contentLineStyle}>
              {line}
            </span>
          ))
        : displayText}
    </span>
  );

  return (
    <div
      data-testid="translation-block"
      data-block-id={block.id}
      className={`${selected && editingEnabled ? "overlay-block selected" : "overlay-block"}${layout.overflow && editingEnabled ? " overflowing" : ""}`}
      style={style}
      title={layout.overflow && editingEnabled ? "현재 render box보다 번역문이 길어서 넘칩니다." : undefined}
      onPointerDown={onPointerDown}
      onDoubleClick={editingEnabled ? onStartInlineEdit : undefined}
    >
      {inlineEditDraft !== undefined ? (
        <textarea
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
      {selected && editingEnabled && onTextAlignChange ? (
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
      {selected && editingEnabled && favoriteFontPresets.length > 0 ? (
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
      {selected && editingEnabled ? <button className="rotate-handle" onPointerDown={onRotatePointerDown} aria-label="Rotate" /> : null}
      {selected && editingEnabled ? <button className="resize-handle" onPointerDown={onResizePointerDown} aria-label="Resize" /> : null}
    </div>
  );
}
