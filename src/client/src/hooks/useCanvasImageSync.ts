import React from "react";
import {
  clearCanvasToSize,
  drawImageToCanvas,
  loadCanvasImage,
  resizeCanvasToSize,
  type CanvasImageDrawSize
} from "../lib/canvasImageDrawing";
import {
  summarizeCanvasSyncState,
  summarizeDataUrl,
  summarizeError,
  writeInpaintDebugLog
} from "../lib/inpaintDiagnostics";

export type CanvasImageSyncState = CanvasImageDrawSize & {
  dataUrl: string | undefined;
};

type UseCanvasImageSyncOptions = {
  afterDraw?: (canvas: HTMLCanvasElement, context: CanvasRenderingContext2D) => void;
  dataUrl?: string;
  loadErrorMessage: string;
  pageSize: CanvasImageDrawSize;
  willReadFrequently?: boolean;
};

type UseCanvasImageSyncState = {
  canvasRef: React.RefObject<HTMLCanvasElement | null>;
  drawingRef: React.MutableRefObject<boolean>;
  markCanvasCommitted: (dataUrl: string | undefined, expectedSourceState?: CanvasImageSyncState) => boolean;
  markCanvasEdited: () => void;
  readCanvasSourceState: () => CanvasImageSyncState;
  readCommittedCanvasState: () => CanvasImageSyncState | null;
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
  afterDraw,
  dataUrl,
  loadErrorMessage,
  pageSize,
  willReadFrequently = false
}: UseCanvasImageSyncOptions): UseCanvasImageSyncState {
  const canvasRef = React.useRef<HTMLCanvasElement | null>(null);
  const drawingRef = React.useRef(false);
  const appliedCanvasStateRef = React.useRef<CanvasImageSyncState | null>(null);
  const canvasEditRevisionRef = React.useRef(0);
  const canvasSourceStateRef = React.useRef(createCanvasImageSyncState(dataUrl, pageSize));
  canvasSourceStateRef.current = createCanvasImageSyncState(dataUrl, pageSize);

  React.useEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d", willReadFrequently ? { willReadFrequently: true } : undefined);
    if (!canvas || !context) {
      return;
    }

    if (isCanvasImageSyncStateCurrent(appliedCanvasStateRef.current, dataUrl, pageSize) || drawingRef.current) {
      writeInpaintDebugLog("inpaint-canvas-sync:effect-skip", {
        appliedState: summarizeCanvasSyncState(appliedCanvasStateRef.current),
        dataUrl: summarizeDataUrl(dataUrl),
        drawing: drawingRef.current,
        loadErrorMessage,
        pageSize,
        reason: drawingRef.current ? "drawing" : "already-current"
      });
      return;
    }

    resizeCanvasToSize(canvas, pageSize);

    if (!dataUrl) {
      clearCanvasToSize(canvas, context, pageSize);
      appliedCanvasStateRef.current = createCanvasImageSyncState(undefined, pageSize);
      writeInpaintDebugLog("inpaint-canvas-sync:clear", {
        loadErrorMessage,
        pageSize
      });
      return;
    }

    let cancelled = false;
    const loadRevision = canvasEditRevisionRef.current;
    writeInpaintDebugLog("inpaint-canvas-sync:load-start", {
      dataUrl: summarizeDataUrl(dataUrl),
      loadErrorMessage,
      loadRevision,
      pageSize
    });
    void loadCanvasImage(dataUrl, loadErrorMessage)
      .then((image) => {
        if (!cancelled && loadRevision === canvasEditRevisionRef.current && !drawingRef.current) {
          drawImageToCanvas(canvas, context, image, pageSize);
          afterDraw?.(canvas, context);
          appliedCanvasStateRef.current = createCanvasImageSyncState(dataUrl, pageSize);
          writeInpaintDebugLog("inpaint-canvas-sync:load-apply", {
            dataUrl: summarizeDataUrl(dataUrl),
            loadErrorMessage,
            loadRevision,
            pageSize
          });
          return;
        }
        writeInpaintDebugLog("inpaint-canvas-sync:load-skip", {
          cancelled,
          dataUrl: summarizeDataUrl(dataUrl),
          drawing: drawingRef.current,
          loadErrorMessage,
          loadRevision,
          currentRevision: canvasEditRevisionRef.current,
          reason: cancelled
            ? "cancelled"
            : loadRevision !== canvasEditRevisionRef.current
              ? "revision-mismatch"
              : "drawing"
        });
      })
      .catch((error) => {
        if (!cancelled) {
          console.error(error);
          writeInpaintDebugLog("inpaint-canvas-sync:load-error", {
            dataUrl: summarizeDataUrl(dataUrl),
            error: summarizeError(error),
            loadErrorMessage,
            loadRevision
          });
        }
      });

    return () => {
      cancelled = true;
    };
  }, [afterDraw, dataUrl, loadErrorMessage, pageSize.height, pageSize.width, willReadFrequently]);

  const markCanvasEdited = React.useCallback(() => {
    canvasEditRevisionRef.current += 1;
    writeInpaintDebugLog("inpaint-canvas-sync:mark-edited", {
      nextRevision: canvasEditRevisionRef.current
    });
  }, []);

  const markCanvasCommitted = React.useCallback((nextDataUrl: string | undefined, expectedSourceState?: CanvasImageSyncState) => {
    const sourceState = canvasSourceStateRef.current;
    if (
      expectedSourceState &&
      !isCanvasImageSyncStateCurrent(expectedSourceState, sourceState.dataUrl, sourceState)
    ) {
      writeInpaintDebugLog("inpaint-canvas-sync:mark-committed-skip", {
        expectedSourceState: summarizeCanvasSyncState(expectedSourceState),
        nextDataUrl: summarizeDataUrl(nextDataUrl),
        reason: "source-mismatch",
        sourceState: summarizeCanvasSyncState(sourceState)
      });
      return false;
    }
    canvasEditRevisionRef.current += 1;
    appliedCanvasStateRef.current = createCanvasImageSyncState(nextDataUrl, sourceState);
    writeInpaintDebugLog("inpaint-canvas-sync:mark-committed", {
      nextDataUrl: summarizeDataUrl(nextDataUrl),
      nextRevision: canvasEditRevisionRef.current,
      sourceState: summarizeCanvasSyncState(sourceState)
    });
    return true;
  }, []);

  const readCanvasSourceState = React.useCallback(() => canvasSourceStateRef.current, []);

  const readCommittedCanvasState = React.useCallback(() => {
    const state = appliedCanvasStateRef.current;
    if (!state || state.width !== pageSize.width || state.height !== pageSize.height) {
      return null;
    }
    return state;
  }, [pageSize.height, pageSize.width]);

  return {
    canvasRef,
    drawingRef,
    markCanvasCommitted,
    markCanvasEdited,
    readCanvasSourceState,
    readCommittedCanvasState
  };
}
