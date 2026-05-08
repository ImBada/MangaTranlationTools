import React, { useEffect, useRef } from "react";
import type { MangaPage } from "../../../shared/types";
import { drawOverlayBlocks } from "../lib/pageRender";
import type { FontWeightAvailability, ViewportSize } from "../lib/overlayLayout";

type OverlayRenderCanvasProps = {
  canvasRef?: React.RefObject<HTMLCanvasElement | null>;
  page: MangaPage;
  stageSize: ViewportSize;
  editingEnabled: boolean;
  fontWeightAvailability: readonly FontWeightAvailability[];
};

export function OverlayRenderCanvas({
  canvasRef: externalCanvasRef,
  page,
  stageSize,
  editingEnabled,
  fontWeightAvailability
}: OverlayRenderCanvasProps): React.JSX.Element {
  const internalCanvasRef = useRef<HTMLCanvasElement | null>(null);
  const canvasRef = externalCanvasRef ?? internalCanvasRef;

  useEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d", { willReadFrequently: true });
    if (!canvas || !context) {
      return;
    }

    const width = Math.max(1, page.width);
    const height = Math.max(1, page.height);
    canvas.width = width;
    canvas.height = height;
    canvas.style.width = `${Math.max(1, Math.round(stageSize.width))}px`;
    canvas.style.height = `${Math.max(1, Math.round(stageSize.height))}px`;

    context.setTransform(1, 0, 0, 1, 0, 0);
    context.clearRect(0, 0, width, height);
    drawOverlayBlocks(context, page, {
      renderSize: { width: page.width, height: page.height },
      editingEnabled,
      fontWeightAvailability
    });
  }, [editingEnabled, fontWeightAvailability, page, stageSize.height, stageSize.width]);

  return <canvas ref={canvasRef} className="overlay-render-canvas" aria-hidden="true" />;
}
