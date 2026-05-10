import React from "react";
import type { ViewportSize } from "../lib/overlayLayout";
import { resolveStageFitSize, resolveStageZoomAnchorPanOffset } from "../lib/stageFit";

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
  zoomOut: boolean;
};

export type StageZoomPanAnchor = {
  clientX: number;
  clientY: number;
  contentX: number;
  contentY: number;
  startScale: number;
};

type UseImageStageViewOptions = {
  onStagePointerDown: (event: React.PointerEvent) => void;
  onStagePointerMove: (event: React.PointerEvent) => void;
  onStagePointerUp: (event: React.PointerEvent) => void;
  pageSize: ViewportSize;
  stagePanDisabled?: boolean;
  temporaryPanActive: boolean;
  viewResetKey: number;
  viewScale: number | null;
  zoomToolActive: boolean;
};

type UseImageStageViewState = {
  applyStageZoomAnchor: (anchor: StageZoomPanAnchor | null, nextScale: number) => void;
  beginStageZoomAnchor: (clientX: number, clientY: number, startScale: number) => StageZoomPanAnchor | null;
  clearZoomCursor: () => void;
  handleStagePointerCancel: (event: React.PointerEvent<HTMLDivElement>) => void;
  handleStagePointerDown: (event: React.PointerEvent<HTMLDivElement>) => void;
  handleStagePointerMove: (event: React.PointerEvent<HTMLDivElement>) => void;
  handleStagePointerUp: (event: React.PointerEvent<HTMLDivElement>) => void;
  panning: boolean;
  stageStyle: React.CSSProperties | undefined;
  updateZoomCursor: (event: React.PointerEvent<HTMLElement>, zoomOut?: boolean) => void;
  wrapRef: React.RefObject<HTMLDivElement | null>;
  zoomCursor: ZoomCursorState | null;
};

export function useImageStageView({
  onStagePointerDown,
  onStagePointerMove,
  onStagePointerUp,
  pageSize,
  stagePanDisabled = false,
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

  React.useLayoutEffect(() => {
    applyPanOffset({ x: 0, y: 0 });
    panRef.current = null;
    setPanning(false);
  }, [applyPanOffset, viewResetKey]);

  React.useEffect(() => {
    if (!zoomToolActive || temporaryPanActive) {
      setZoomCursor(null);
    }
  }, [temporaryPanActive, zoomToolActive]);

  const updateZoomCursor = React.useCallback((event: React.PointerEvent<HTMLElement>, zoomOut = false) => {
    const rect = event.currentTarget.getBoundingClientRect();
    setZoomCursor({
      x: event.clientX - rect.left,
      y: event.clientY - rect.top,
      zoomOut
    });
  }, []);

  const clearZoomCursor = React.useCallback(() => {
    setZoomCursor(null);
  }, []);

  const beginStageZoomAnchor = React.useCallback((clientX: number, clientY: number, startScale: number): StageZoomPanAnchor | null => {
    const wrap = wrapRef.current;
    if (!wrap) {
      return null;
    }

    const rect = wrap.getBoundingClientRect();
    const pan = panOffsetRef.current;
    return {
      clientX,
      clientY,
      contentX: clientX - (rect.left + rect.width / 2) - pan.x,
      contentY: clientY - (rect.top + rect.height / 2) - pan.y,
      startScale
    };
  }, []);

  const applyStageZoomAnchor = React.useCallback((anchor: StageZoomPanAnchor | null, nextScale: number) => {
    const wrap = wrapRef.current;
    if (!wrap || !anchor) {
      return;
    }

    const rect = wrap.getBoundingClientRect();
    const centerX = rect.left + rect.width / 2;
    const centerY = rect.top + rect.height / 2;
    applyPanOffset(resolveStageZoomAnchorPanOffset({
      anchorClientX: anchor.clientX,
      anchorClientY: anchor.clientY,
      centerClientX: centerX,
      centerClientY: centerY,
      contentX: anchor.contentX,
      contentY: anchor.contentY,
      nextScale,
      startScale: anchor.startScale
    }));
  }, [applyPanOffset]);

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
    if (event.button !== 0 || event.defaultPrevented || stagePanDisabled) {
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
  }, [onStagePointerDown, stagePanDisabled]);

  const stageStyle = fitSize ? {
    width: `${fitSize.width}px`,
    height: `${fitSize.height}px`,
    transform: `translate(-50%, -50%) translate(${panOffset.x}px, ${panOffset.y}px)`
  } : undefined;

  return {
    applyStageZoomAnchor,
    beginStageZoomAnchor,
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
