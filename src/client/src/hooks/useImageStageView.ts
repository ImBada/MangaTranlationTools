import React from "react";
import type { ViewportSize } from "../lib/overlayLayout";
import { resolveStageFitSize } from "../lib/stageFit";

type PanOffset = {
  x: number;
  y: number;
};

type StagePanState = {
  pointerId: number;
  startX: number;
  startY: number;
  startPan: PanOffset;
};

type ZoomCursorState = {
  x: number;
  y: number;
  altKey: boolean;
};

type UseImageStageViewOptions = {
  onStagePointerDown: (event: React.PointerEvent) => void;
  onStagePointerMove: (event: React.PointerEvent) => void;
  onStagePointerUp: (event: React.PointerEvent) => void;
  pageId: string;
  pageSize: ViewportSize;
  temporaryPanActive: boolean;
  viewResetKey: number;
  viewScale: number | null;
  zoomToolActive: boolean;
};

type UseImageStageViewState = {
  clearZoomCursor: () => void;
  handleStagePointerCancel: (event: React.PointerEvent<HTMLDivElement>) => void;
  handleStagePointerDown: (event: React.PointerEvent<HTMLDivElement>) => void;
  handleStagePointerMove: (event: React.PointerEvent<HTMLDivElement>) => void;
  handleStagePointerUp: (event: React.PointerEvent<HTMLDivElement>) => void;
  panning: boolean;
  stageStyle: React.CSSProperties | undefined;
  updateZoomCursor: (event: React.PointerEvent<HTMLElement>) => void;
  wrapRef: React.RefObject<HTMLDivElement | null>;
  zoomCursor: ZoomCursorState | null;
};

export function useImageStageView({
  onStagePointerDown,
  onStagePointerMove,
  onStagePointerUp,
  pageId,
  pageSize,
  temporaryPanActive,
  viewResetKey,
  viewScale,
  zoomToolActive
}: UseImageStageViewOptions): UseImageStageViewState {
  const wrapRef = React.useRef<HTMLDivElement | null>(null);
  const panRef = React.useRef<StagePanState | null>(null);
  const panOffsetRef = React.useRef<PanOffset>({ x: 0, y: 0 });
  const [fitSize, setFitSize] = React.useState<ViewportSize | null>(null);
  const [panOffset, setPanOffset] = React.useState<PanOffset>({ x: 0, y: 0 });
  const [panning, setPanning] = React.useState(false);
  const [zoomCursor, setZoomCursor] = React.useState<ZoomCursorState | null>(null);

  const applyPanOffset = React.useCallback((offset: PanOffset) => {
    panOffsetRef.current = offset;
    setPanOffset(offset);
  }, []);

  React.useLayoutEffect(() => {
    const wrap = wrapRef.current;
    if (!wrap) {
      return;
    }

    let frameId = 0;
    const syncFitSize = () => {
      const next = resolveStageFitSize(pageSize, resolveStageFitBounds(wrap), { viewScale });
      setPanOffset((current) => {
        const nextPan = viewScale === null ? { x: 0, y: 0 } : current;
        panOffsetRef.current = nextPan;
        return nextPan;
      });
      setFitSize((current) => {
        if (
          current &&
          Math.abs(current.width - next.width) < 0.5 &&
          Math.abs(current.height - next.height) < 0.5
        ) {
          return current;
        }
        return next;
      });
    };
    const scheduleSync = () => {
      if (frameId) {
        cancelAnimationFrame(frameId);
      }
      frameId = requestAnimationFrame(() => {
        frameId = 0;
        syncFitSize();
      });
    };

    syncFitSize();
    const observer = new ResizeObserver(scheduleSync);
    const clipElement = wrap.closest(".workspace") as HTMLElement | null;
    observer.observe(wrap);
    if (clipElement) {
      observer.observe(clipElement);
    }
    window.addEventListener("resize", scheduleSync);

    return () => {
      if (frameId) {
        cancelAnimationFrame(frameId);
      }
      observer.disconnect();
      window.removeEventListener("resize", scheduleSync);
    };
  }, [pageSize, viewScale]);

  React.useEffect(() => {
    applyPanOffset({ x: 0, y: 0 });
    panRef.current = null;
    setPanning(false);
  }, [applyPanOffset, pageId, viewResetKey]);

  React.useEffect(() => {
    if (!zoomToolActive || temporaryPanActive) {
      setZoomCursor(null);
      return;
    }

    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Alt") {
        return;
      }
      setZoomCursor((current) => current ? { ...current, altKey: true } : current);
    };
    const onKeyUp = (event: KeyboardEvent) => {
      if (event.key !== "Alt") {
        return;
      }
      setZoomCursor((current) => current ? { ...current, altKey: false } : current);
    };
    const resetAlt = () => {
      setZoomCursor((current) => current ? { ...current, altKey: false } : current);
    };

    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("keyup", onKeyUp);
    window.addEventListener("blur", resetAlt);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("keyup", onKeyUp);
      window.removeEventListener("blur", resetAlt);
    };
  }, [temporaryPanActive, zoomToolActive]);

  const updateZoomCursor = React.useCallback((event: React.PointerEvent<HTMLElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    setZoomCursor({
      x: event.clientX - rect.left,
      y: event.clientY - rect.top,
      altKey: event.altKey
    });
  }, []);

  const clearZoomCursor = React.useCallback(() => {
    setZoomCursor(null);
  }, []);

  const handleStagePointerMove = React.useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    const pan = panRef.current;
    if (pan && pan.pointerId === event.pointerId) {
      event.preventDefault();
      applyPanOffset({
        x: pan.startPan.x + event.clientX - pan.startX,
        y: pan.startPan.y + event.clientY - pan.startY
      });
      return;
    }
    onStagePointerMove(event);
  }, [applyPanOffset, onStagePointerMove]);

  const handleStagePointerUp = React.useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    if (panRef.current?.pointerId === event.pointerId) {
      event.preventDefault();
      event.currentTarget.releasePointerCapture(event.pointerId);
      panRef.current = null;
      setPanning(false);
    }
    onStagePointerUp(event);
  }, [onStagePointerUp]);

  const handleStagePointerCancel = React.useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    if (panRef.current?.pointerId === event.pointerId) {
      event.currentTarget.releasePointerCapture(event.pointerId);
      panRef.current = null;
      setPanning(false);
    }
    onStagePointerUp(event);
  }, [onStagePointerUp]);

  const handleStagePointerDown = React.useCallback((event: React.PointerEvent<HTMLDivElement>) => {
    onStagePointerDown(event);
    if (event.button !== 0 || event.defaultPrevented) {
      return;
    }
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    panRef.current = {
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      startPan: panOffsetRef.current
    };
    setPanning(true);
  }, [onStagePointerDown]);

  const stageStyle = fitSize ? {
    width: `${fitSize.width}px`,
    height: `${fitSize.height}px`,
    transform: `translate(${panOffset.x}px, ${panOffset.y}px)`
  } : undefined;

  return {
    clearZoomCursor,
    handleStagePointerCancel,
    handleStagePointerDown,
    handleStagePointerMove,
    handleStagePointerUp,
    panning,
    stageStyle,
    updateZoomCursor,
    wrapRef,
    zoomCursor
  };
}

function resolveStageFitBounds(wrap: HTMLDivElement): ViewportSize {
  const clipElement = wrap.closest(".workspace") as HTMLElement | null;
  if (!clipElement) {
    return {
      width: wrap.clientWidth,
      height: wrap.clientHeight
    };
  }

  const style = window.getComputedStyle(clipElement);
  const paddingX = Number.parseFloat(style.paddingLeft) + Number.parseFloat(style.paddingRight);
  const paddingY = Number.parseFloat(style.paddingTop) + Number.parseFloat(style.paddingBottom);
  return {
    width: Math.max(1, clipElement.clientWidth - paddingX),
    height: Math.max(1, clipElement.clientHeight - paddingY)
  };
}
