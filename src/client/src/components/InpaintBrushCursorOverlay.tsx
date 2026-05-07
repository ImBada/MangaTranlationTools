import React from "react";

type CursorPoint = {
  x: number;
  y: number;
};

type CursorGeometry = {
  height: number;
  width: number;
  wrapLeft: number;
  wrapTop: number;
};

type InpaintBrushCursorOverlayProps = {
  brushSize: number;
  pageSize: {
    width: number;
    height: number;
  };
  stageRef: React.RefObject<HTMLDivElement | null>;
  tool: string;
  wrapRef: React.RefObject<HTMLDivElement | null>;
};

export function InpaintBrushCursorOverlay({
  brushSize,
  pageSize,
  stageRef,
  tool,
  wrapRef
}: InpaintBrushCursorOverlayProps): React.JSX.Element | null {
  const cursorRef = React.useRef<HTMLDivElement | null>(null);
  const geometryRef = React.useRef<CursorGeometry | null>(null);
  const frameRef = React.useRef<number | null>(null);
  const pendingCursorPointRef = React.useRef<CursorPoint | null>(null);

  const flushCursorPoint = React.useCallback(() => {
    frameRef.current = null;
    const cursor = cursorRef.current;
    if (!cursor) {
      return;
    }
    const point = pendingCursorPointRef.current;
    if (!point) {
      cursor.style.visibility = "hidden";
      return;
    }

    cursor.style.visibility = "visible";
    cursor.style.transform = `translate3d(${point.x}px, ${point.y}px, 0) translate(-50%, -50%)`;
  }, []);

  const setCursorPoint = React.useCallback((point: CursorPoint | null) => {
    pendingCursorPointRef.current = point;
    if (frameRef.current !== null) {
      return;
    }
    frameRef.current = window.requestAnimationFrame(flushCursorPoint);
  }, [flushCursorPoint]);

  const syncGeometry = React.useCallback((): CursorGeometry | null => {
    const wrap = wrapRef.current;
    const stage = stageRef.current;
    const cursor = cursorRef.current;
    if (!wrap || !stage || !cursor) {
      geometryRef.current = null;
      setCursorPoint(null);
      return null;
    }

    const wrapRect = wrap.getBoundingClientRect();
    const stageRect = stage.getBoundingClientRect();
    const geometry = {
      wrapLeft: wrapRect.left,
      wrapTop: wrapRect.top,
      width: (Math.max(1, brushSize) / Math.max(1, pageSize.width)) * stageRect.width,
      height: (Math.max(1, brushSize) / Math.max(1, pageSize.height)) * stageRect.height
    };
    geometryRef.current = geometry;
    cursor.style.width = `${geometry.width}px`;
    cursor.style.height = `${geometry.height}px`;
    return geometry;
  }, [brushSize, pageSize.height, pageSize.width, setCursorPoint, stageRef, wrapRef]);

  React.useEffect(() => {
    const wrap = wrapRef.current;
    if (!wrap) {
      setCursorPoint(null);
      return;
    }
    syncGeometry();

    const updateCursor = (event: PointerEvent) => {
      const geometry = geometryRef.current ?? syncGeometry();
      if (!geometry) {
        setCursorPoint(null);
        return;
      }
      setCursorPoint({
        x: event.clientX - geometry.wrapLeft,
        y: event.clientY - geometry.wrapTop
      });
    };
    const clearCursor = () => setCursorPoint(null);
    const syncAndClearCursor = () => {
      syncGeometry();
      clearCursor();
    };
    const observer = new ResizeObserver(syncAndClearCursor);
    observer.observe(wrap);
    const stage = stageRef.current;
    if (stage) {
      observer.observe(stage);
    }

    wrap.addEventListener("pointermove", updateCursor, { capture: true, passive: true });
    wrap.addEventListener("pointerleave", clearCursor);
    wrap.addEventListener("pointercancel", clearCursor);
    window.addEventListener("blur", clearCursor);
    window.addEventListener("resize", syncAndClearCursor);
    return () => {
      wrap.removeEventListener("pointermove", updateCursor, { capture: true });
      wrap.removeEventListener("pointerleave", clearCursor);
      wrap.removeEventListener("pointercancel", clearCursor);
      observer.disconnect();
      window.removeEventListener("blur", clearCursor);
      window.removeEventListener("resize", syncAndClearCursor);
    };
  }, [setCursorPoint, stageRef, syncGeometry, wrapRef]);

  React.useEffect(() => {
    return () => {
      if (frameRef.current !== null) {
        window.cancelAnimationFrame(frameRef.current);
      }
    };
  }, []);

  return (
    <div className="stage-inpaint-cursor-layer" aria-hidden="true">
      <div
        ref={cursorRef}
        className={`inpaint-brush-cursor ${tool}`}
        style={{ visibility: "hidden" }}
      />
    </div>
  );
}
