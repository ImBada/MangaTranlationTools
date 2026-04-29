import React from "react";
import type { TranslationBlock } from "../../../shared/types";
import { resolveBlockRectPx, type ViewportSize } from "../lib/overlayLayout";

type InpaintFillProps = {
  block: TranslationBlock;
  pageSize: ViewportSize;
  stageSize: ViewportSize;
  paddingPx?: number;
  fillColor?: string;
};

export function InpaintFill({
  block,
  pageSize,
  stageSize,
  paddingPx = 6,
  fillColor = "#ffffff"
}: InpaintFillProps): React.JSX.Element | null {
  if (!block.inpainted) {
    return null;
  }

  const rect = resolveBlockRectPx(block, pageSize, stageSize);
  const style: React.CSSProperties = {
    position: "absolute",
    left: Math.max(0, rect.left - paddingPx),
    top: Math.max(0, rect.top - paddingPx),
    width: rect.width + paddingPx * 2,
    height: rect.height + paddingPx * 2,
    backgroundColor: fillColor,
    zIndex: 0,
    pointerEvents: "none"
  };

  return <div className="inpaint-fill" style={style} />;
}
