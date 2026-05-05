import React from "react";
import {
  clearCanvasToSize,
  drawImageToCanvas,
  loadCanvasImage,
  resizeCanvasToSize,
  type CanvasImageDrawSize
} from "../lib/canvasImageDrawing";

export type CanvasImageSyncState = CanvasImageDrawSize & {
  dataUrl: string | undefined;
};

type UseCanvasImageSyncOptions = {
  dataUrl?: string;
  loadErrorMessage: string;
  pageSize: CanvasImageDrawSize;
  willReadFrequently?: boolean;
};

type UseCanvasImageSyncState = {
  canvasRef: React.RefObject<HTMLCanvasElement | null>;
  drawingRef: React.MutableRefObject<boolean>;
  markCanvasCommitted: (dataUrl: string | undefined) => void;
  markCanvasEdited: () => void;
};

export function createCanvasImageSyncState(
  dataUrl: string | undefined,
  size: CanvasImageDrawSize
): CanvasImageSyncState {
  return {
    dataUrl,
    width: size.width,
    height: size.height
  };
}

export function isCanvasImageSyncStateCurrent(
  state: CanvasImageSyncState | null,
  dataUrl: string | undefined,
  size: CanvasImageDrawSize
): boolean {
  return state !== null && state.dataUrl === dataUrl && state.width === size.width && state.height === size.height;
}

export function useCanvasImageSync({
  dataUrl,
  loadErrorMessage,
  pageSize,
  willReadFrequently = false
}: UseCanvasImageSyncOptions): UseCanvasImageSyncState {
  const canvasRef = React.useRef<HTMLCanvasElement | null>(null);
  const drawingRef = React.useRef(false);
  const appliedCanvasStateRef = React.useRef<CanvasImageSyncState | null>(null);
  const canvasEditRevisionRef = React.useRef(0);

  React.useEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d", willReadFrequently ? { willReadFrequently: true } : undefined);
    if (!canvas || !context) {
      return;
    }

    if (isCanvasImageSyncStateCurrent(appliedCanvasStateRef.current, dataUrl, pageSize) || drawingRef.current) {
      return;
    }

    resizeCanvasToSize(canvas, pageSize);

    if (!dataUrl) {
      clearCanvasToSize(canvas, context, pageSize);
      appliedCanvasStateRef.current = createCanvasImageSyncState(undefined, pageSize);
      return;
    }

    let cancelled = false;
    const loadRevision = canvasEditRevisionRef.current;
    void loadCanvasImage(dataUrl, loadErrorMessage)
      .then((image) => {
        if (!cancelled && loadRevision === canvasEditRevisionRef.current && !drawingRef.current) {
          drawImageToCanvas(canvas, context, image, pageSize);
          appliedCanvasStateRef.current = createCanvasImageSyncState(dataUrl, pageSize);
        }
      })
      .catch((error) => {
        if (!cancelled) {
          console.error(error);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [dataUrl, loadErrorMessage, pageSize.height, pageSize.width, willReadFrequently]);

  const markCanvasEdited = React.useCallback(() => {
    canvasEditRevisionRef.current += 1;
  }, []);

  const markCanvasCommitted = React.useCallback((nextDataUrl: string | undefined) => {
    canvasEditRevisionRef.current += 1;
    appliedCanvasStateRef.current = createCanvasImageSyncState(nextDataUrl, pageSize);
  }, [pageSize.height, pageSize.width]);

  return {
    canvasRef,
    drawingRef,
    markCanvasCommitted,
    markCanvasEdited
  };
}
