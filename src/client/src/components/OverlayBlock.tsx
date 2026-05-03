import React from "react";
import type { TranslationBlock } from "../../../shared/types";
import { resolveBlockRotationDeg } from "../../../shared/geometry";
import {
  DEFAULT_OVERLAY_FONT_FAMILY,
  buildScreentoneFillCssBackground,
  buildScreentoneFillCssSize,
  hexToRgba,
  resolveBlockTextLayout,
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
  visualContentVisible?: boolean;
  onInlineEditCancel?: () => void;
  onInlineEditChange?: (value: string) => void;
  onInlineEditCommit?: () => void;
  onStartInlineEdit?: (event: React.MouseEvent) => void;
  onPointerDown: (event: React.PointerEvent) => void;
  onResizePointerDown: (event: React.PointerEvent) => void;
  onRotatePointerDown: (event: React.PointerEvent) => void;
};

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
  visualContentVisible = true,
  onInlineEditCancel,
  onInlineEditChange,
  onInlineEditCommit,
  onStartInlineEdit,
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
  const rotationDeg = resolveBlockRotationDeg(block);
  const screentoneFillEnabled = visualContentVisible && (block.screentoneFillEnabled ?? false);
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
    fontSize: `${layout.fontSizePx}px`,
    lineHeight: block.lineHeight,
    textAlign: block.textAlign,
    pointerEvents: editingEnabled ? undefined : "none",
    transform: rotationDeg !== 0 ? `rotate(${rotationDeg}deg)` : undefined,
    transformOrigin: "center center",
    zIndex: inlineEditDraft !== undefined ? 70 : selected && editingEnabled ? 50 : undefined,
    overflow: inlineEditDraft !== undefined ? "visible" : "hidden"
  };
  const textWrapStyle: React.CSSProperties = {
    boxSizing: "border-box",
    width: layout.innerWidth,
    maxWidth: "100%",
    height: layout.innerHeight,
    maxHeight: "100%",
    overflow: "hidden"
  };
  const contentFrameStyle: React.CSSProperties = {
    position: "relative",
    width: `${block.renderDirection === "vertical" ? layout.fitInnerWidth : layout.innerWidth}px`,
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
    zIndex: 1,
    WebkitTextStroke: outlineWidthPx > 0 ? `${outlineWidthPx}px ${block.outlineColor ?? "#000000"}` : undefined,
    ...screentoneFillStyle
  };
  const secondaryOutlineStyle: React.CSSProperties = {
    ...baseContentStyle,
    position: "absolute",
    inset: 0,
    zIndex: 0,
    pointerEvents: "none",
    color: "transparent",
    WebkitTextFillColor: "transparent",
    WebkitTextStroke:
      combinedSecondaryOutlineWidthPx > 0 ? `${combinedSecondaryOutlineWidthPx}px ${block.secondaryOutlineColor ?? "#ffffff"}` : undefined
  };
  const lineStyle: React.CSSProperties = {
    alignSelf: block.textAlign === "left" ? "flex-start" : block.textAlign === "right" ? "flex-end" : "center",
    ...screentoneFillStyle
  };
  const secondaryOutlineLineStyle: React.CSSProperties = {
    alignSelf: block.textAlign === "left" ? "flex-start" : block.textAlign === "right" ? "flex-end" : "center"
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
            {secondaryOutlineWidthPx > 0 ? renderTextContent(secondaryOutlineStyle, secondaryOutlineLineStyle, true) : null}
            {renderTextContent(contentStyle, lineStyle)}
          </span>
        </div>
      ) : null}
      {selected && editingEnabled ? <button className="rotate-handle" onPointerDown={onRotatePointerDown} aria-label="Rotate" /> : null}
      {selected && editingEnabled ? <button className="resize-handle" onPointerDown={onResizePointerDown} aria-label="Resize" /> : null}
    </div>
  );
}
