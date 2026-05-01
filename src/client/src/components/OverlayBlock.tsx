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
  visualContentVisible?: boolean;
  onPointerDown: (event: React.PointerEvent) => void;
  onResizePointerDown: (event: React.PointerEvent) => void;
  onRotatePointerDown: (event: React.PointerEvent) => void;
};

export function OverlayBlock({
  block,
  pageSize,
  stageSize,
  selected,
  editingEnabled,
  visualContentVisible = true,
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
    overflow: "hidden",
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
    zIndex: selected && editingEnabled ? 50 : undefined
  };
  const textWrapStyle: React.CSSProperties = {
    boxSizing: "border-box",
    width: layout.innerWidth,
    maxWidth: "100%",
    height: layout.innerHeight,
    maxHeight: "100%",
    overflow: "hidden"
  };
  const contentStyle: React.CSSProperties = {
    boxSizing: "border-box",
    writingMode: block.renderDirection === "vertical" ? "vertical-rl" : "horizontal-tb",
    textOrientation: block.renderDirection === "vertical" ? "upright" : undefined,
    width: `${block.renderDirection === "vertical" ? layout.fitInnerWidth : layout.innerWidth}px`,
    height: block.renderDirection === "vertical" ? `${layout.fitInnerHeight}px` : undefined,
    maxWidth: "100%",
    maxHeight: "100%",
    WebkitTextStroke: outlineWidthPx > 0 ? `${outlineWidthPx}px ${block.outlineColor ?? "#000000"}` : undefined,
    ...screentoneFillStyle
  };
  const lineStyle: React.CSSProperties = {
    alignSelf: block.textAlign === "left" ? "flex-start" : block.textAlign === "right" ? "flex-end" : "center",
    ...screentoneFillStyle
  };

  return (
    <div
      className={`${selected && editingEnabled ? "overlay-block selected" : "overlay-block"}${layout.overflow && editingEnabled ? " overflowing" : ""}`}
      style={style}
      title={layout.overflow && editingEnabled ? "현재 render box보다 번역문이 길어서 넘칩니다." : undefined}
      onPointerDown={onPointerDown}
    >
      {visualContentVisible ? (
        <div className="overlay-text" style={textWrapStyle}>
          <span className="overlay-text-content" style={contentStyle}>
            {block.renderDirection === "horizontal"
                ? horizontalLines.map((line, index) => (
                  <span key={`${line}-${index}`} className="overlay-text-line" style={lineStyle}>
                    {line}
                  </span>
                ))
              : displayText}
          </span>
        </div>
      ) : null}
      {selected && editingEnabled ? <button className="rotate-handle" onPointerDown={onRotatePointerDown} aria-label="Rotate" /> : null}
      {selected && editingEnabled ? <button className="resize-handle" onPointerDown={onResizePointerDown} aria-label="Resize" /> : null}
    </div>
  );
}
