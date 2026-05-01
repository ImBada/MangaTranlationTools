import React from "react";
import type { ImageRect, MangaPage, TranslationBlock } from "../../../shared/types";
import type { ViewportSize } from "../lib/overlayLayout";
import { resolveStageFitSize, resolveStagePanRange } from "../lib/stageFit";
import { InpaintLayerCanvas, type InpaintTool } from "./InpaintLayerCanvas";
import { InpaintResultCanvas, type InpaintResultTool } from "./InpaintResultCanvas";
import { OverlayBlock } from "./OverlayBlock";
import { OverlayRenderCanvas } from "./OverlayRenderCanvas";

type ImageStageProps = {
  page: MangaPage;
  imageRef: React.RefObject<HTMLImageElement | null>;
  stageRef: React.RefObject<HTMLDivElement | null>;
  stageSize: ViewportSize | null;
  viewScale: number | null;
  viewResetKey: number;
  zoomToolActive: boolean;
  rangeToolActive: boolean;
  selectedBlockId: string | null;
  layerVisibility: {
    image: boolean;
    inpaint: boolean;
    inpaintResult: boolean;
    inpaintMask: boolean;
    overlay: boolean;
  };
  layerOpacity: {
    image: number;
    inpaint: number;
    inpaintResult: number;
    inpaintMask: number;
    overlay: number;
  };
  activeLayer: "output" | "image" | "inpaint" | "inpaintResult" | "inpaintMask" | "overlay";
  inpaintTool: InpaintTool;
  inpaintBrushSize: number;
  inpaintResultTool: InpaintResultTool;
  inpaintResultBrushSize: number;
  inpaintResultBrushColor: string;
  inpaintResultBrushHardness: number;
  inpaintResultToolStrength: number;
  inpaintDisabled: boolean;
  inpaintResultDisabled: boolean;
  rangeSelectionDisabled: boolean;
  temporaryPanActive: boolean;
  inpaintSelectionRect: ImageRect | null;
  onInpaintLayerChange: (dataUrl: string | undefined) => void;
  onInpaintSelectionChange: (rect: ImageRect | null) => void;
  onInpaintResultLayerChange: (dataUrl: string | undefined) => void;
  onZoomToolClick: (direction: "in" | "out") => void;
  onStagePointerMove: (event: React.PointerEvent) => void;
  onStagePointerUp: (event: React.PointerEvent) => void;
  onStagePointerDown: (event: React.PointerEvent) => void;
  onBlockPointerDown: (event: React.PointerEvent, block: TranslationBlock, mode: "move" | "resize" | "rotate") => void;
};

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

export function ImageStage({
  page,
  imageRef,
  stageRef,
  stageSize,
  viewScale,
  viewResetKey,
  zoomToolActive,
  rangeToolActive,
  selectedBlockId,
  layerVisibility,
  layerOpacity,
  activeLayer,
  inpaintTool,
  inpaintBrushSize,
  inpaintResultTool,
  inpaintResultBrushSize,
  inpaintResultBrushColor,
  inpaintResultBrushHardness,
  inpaintResultToolStrength,
  inpaintDisabled,
  inpaintResultDisabled,
  rangeSelectionDisabled,
  temporaryPanActive,
  inpaintSelectionRect,
  onInpaintLayerChange,
  onInpaintSelectionChange,
  onInpaintResultLayerChange,
  onZoomToolClick,
  onStagePointerMove,
  onStagePointerUp,
  onStagePointerDown,
  onBlockPointerDown
}: ImageStageProps): React.JSX.Element {
  const wrapRef = React.useRef<HTMLDivElement | null>(null);
  const panRef = React.useRef<StagePanState | null>(null);
  const panOffsetRef = React.useRef<PanOffset>({ x: 0, y: 0 });
  const [fitSize, setFitSize] = React.useState<ViewportSize | null>(null);
  const [panOffset, setPanOffset] = React.useState<PanOffset>({ x: 0, y: 0 });
  const [panning, setPanning] = React.useState(false);
  const [zoomCursor, setZoomCursor] = React.useState<ZoomCursorState | null>(null);
  const inpaintMaskDataUrl = page.inpaintMaskDataUrl ?? page.inpaintLayerDataUrl;
  const pageSize = React.useMemo(() => ({ width: page.width, height: page.height }), [page.height, page.width]);

  const applyPanOffset = React.useCallback((offset: PanOffset) => {
    panOffsetRef.current = offset;
    setPanOffset(offset);
  }, []);

  const clampPanOffset = React.useCallback((offset: PanOffset, size?: ViewportSize | null): PanOffset => {
    const wrap = wrapRef.current;
    if (!wrap) {
      return offset;
    }

    const clipElement = wrap.closest(".workspace") as HTMLElement | null;
    const clipRect = (clipElement ?? wrap).getBoundingClientRect();
    const imageRect = size ? null : imageRef.current?.getBoundingClientRect();
    const currentPan = panOffsetRef.current;
    const wrapRect = wrap.getBoundingClientRect();
    const stageRect = imageRect
      ? {
          left: imageRect.left - currentPan.x,
          top: imageRect.top - currentPan.y,
          right: imageRect.right - currentPan.x,
          bottom: imageRect.bottom - currentPan.y,
          width: imageRect.width,
          height: imageRect.height
        }
      : size
        ? {
            left: wrapRect.left + (wrapRect.width - size.width) / 2,
            top: wrapRect.top + (wrapRect.height - size.height) / 2,
            right: wrapRect.left + (wrapRect.width + size.width) / 2,
            bottom: wrapRect.top + (wrapRect.height + size.height) / 2,
            width: size.width,
            height: size.height
          }
        : null;
    if (!stageRect) {
      return offset;
    }
    const range = resolveStagePanRange(stageRect, {
      left: clipRect.left,
      top: clipRect.top,
      right: clipRect.right,
      bottom: clipRect.bottom,
      width: clipRect.width || wrap.clientWidth,
      height: clipRect.height || wrap.clientHeight
    });
    return {
      x: Math.min(range.maxX, Math.max(range.minX, offset.x)),
      y: Math.min(range.maxY, Math.max(range.minY, offset.y))
    };
  }, [imageRef]);

  React.useLayoutEffect(() => {
    const wrap = wrapRef.current;
    if (!wrap) {
      return;
    }

    let frameId = 0;
    const syncFitSize = () => {
      const next = resolveStageFitSize(pageSize, resolveStageFitBounds(wrap), { viewScale });
      setPanOffset((current) => {
        const clamped = viewScale === null ? { x: 0, y: 0 } : clampPanOffset(current, next);
        panOffsetRef.current = clamped;
        return clamped;
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
  }, [applyPanOffset, page.id, viewResetKey]);

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

  return (
    <div ref={wrapRef} className="stage-wrap">
      <div
        ref={stageRef}
        className={`image-stage${panning || temporaryPanActive ? " panning" : ""}`}
        style={fitSize ? {
          width: `${fitSize.width}px`,
          height: `${fitSize.height}px`,
          transform: `translate(${panOffset.x}px, ${panOffset.y}px)`
        } : undefined}
        onPointerMove={(event) => {
          const pan = panRef.current;
          if (pan && pan.pointerId === event.pointerId) {
            event.preventDefault();
            applyPanOffset(clampPanOffset({
              x: pan.startPan.x + event.clientX - pan.startX,
              y: pan.startPan.y + event.clientY - pan.startY
            }));
            return;
          }
          onStagePointerMove(event);
        }}
        onPointerUp={(event) => {
          if (panRef.current?.pointerId === event.pointerId) {
            event.preventDefault();
            event.currentTarget.releasePointerCapture(event.pointerId);
            panRef.current = null;
            setPanning(false);
          }
          onStagePointerUp(event);
        }}
        onPointerCancel={(event) => {
          if (panRef.current?.pointerId === event.pointerId) {
            event.currentTarget.releasePointerCapture(event.pointerId);
            panRef.current = null;
            setPanning(false);
          }
          onStagePointerUp(event);
        }}
        onPointerDown={(event) => {
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
        }}
      >
        <img
          ref={imageRef}
          className="page-image"
          src={page.dataUrl}
          alt={page.name}
          draggable={false}
          style={{
            visibility: layerVisibility.image ? "visible" : "hidden",
            opacity: layerOpacity.image
          }}
        />
        {layerVisibility.inpaint ? (
          <div className="inpaint-layer-preview" style={{ opacity: layerOpacity.inpaint }}>
            {layerVisibility.inpaintResult && (page.inpaintResultDataUrl || (activeLayer === "inpaintResult" && !temporaryPanActive)) ? (
              <InpaintResultCanvas
                className="inpaint-result-canvas"
                dataUrl={page.inpaintResultDataUrl}
                pageSize={{ width: page.width, height: page.height }}
                tool={inpaintResultTool}
                brushSize={inpaintResultBrushSize}
                brushColor={inpaintResultBrushColor}
                brushHardness={inpaintResultBrushHardness}
                toolStrength={inpaintResultToolStrength}
                disabled={inpaintResultDisabled || temporaryPanActive}
                selectionRect={null}
                onChange={onInpaintResultLayerChange}
                onSelectionChange={onInpaintSelectionChange}
                style={{
                  zIndex: activeLayer === "inpaintResult" ? 3 : 1,
                  opacity: layerOpacity.inpaintResult,
                  maskImage: (activeLayer !== "inpaintResult" || temporaryPanActive) && inpaintMaskDataUrl ? `url(${inpaintMaskDataUrl})` : undefined,
                  WebkitMaskImage: (activeLayer !== "inpaintResult" || temporaryPanActive) && inpaintMaskDataUrl ? `url(${inpaintMaskDataUrl})` : undefined
                }}
              />
            ) : null}
            {layerVisibility.inpaintMask ? (
              <div
                className="inpaint-mask-layer-preview"
                style={{
                  zIndex: activeLayer === "inpaintMask" ? 3 : 2,
                  opacity: layerOpacity.inpaintMask,
                  pointerEvents: activeLayer === "inpaintMask" && !temporaryPanActive ? "auto" : "none"
                }}
              >
                <InpaintLayerCanvas
                  dataUrl={inpaintMaskDataUrl}
                  pageSize={{ width: page.width, height: page.height }}
                  tool={inpaintTool}
                  brushSize={inpaintBrushSize}
                  disabled={inpaintDisabled || temporaryPanActive}
                  selectionRect={null}
                  onChange={onInpaintLayerChange}
                  onSelectionChange={onInpaintSelectionChange}
                />
              </div>
            ) : null}
          </div>
        ) : null}
        {layerVisibility.overlay
          ? (
              <div
                className="overlay-layer-preview"
                style={{
                  opacity: layerOpacity.overlay,
                  pointerEvents: activeLayer === "overlay" && !temporaryPanActive ? "auto" : "none"
                }}
              >
                <OverlayRenderCanvas
                  page={page}
                  stageSize={stageSize ?? { width: page.width, height: page.height }}
                  editingEnabled={activeLayer === "overlay" && !temporaryPanActive}
                />
                {page.blocks.map((block) => (
                  <OverlayBlock
                    key={block.id}
                    block={block}
                    pageSize={{ width: page.width, height: page.height }}
                    stageSize={stageSize ?? { width: page.width, height: page.height }}
                    selected={block.id === selectedBlockId}
                    editingEnabled={activeLayer === "overlay" && !temporaryPanActive}
                    visualContentVisible={false}
                    onPointerDown={(event) => onBlockPointerDown(event, block, "move")}
                    onResizePointerDown={(event) => onBlockPointerDown(event, block, "resize")}
                    onRotatePointerDown={(event) => onBlockPointerDown(event, block, "rotate")}
                  />
                ))}
              </div>
            )
          : null}
        {rangeToolActive || inpaintSelectionRect ? (
          <div className={`stage-range-selection-layer ${rangeToolActive ? "active" : ""}`}>
            <InpaintLayerCanvas
              pageSize={{ width: page.width, height: page.height }}
              tool="select"
              brushSize={1}
              disabled={rangeSelectionDisabled || temporaryPanActive || !rangeToolActive}
              selectionRect={inpaintSelectionRect}
              onChange={() => undefined}
              onSelectionChange={onInpaintSelectionChange}
            />
          </div>
        ) : null}
        {zoomToolActive && !temporaryPanActive ? (
          <div
            className="stage-zoom-hit-area"
            aria-label="줌 도구"
            onPointerEnter={updateZoomCursor}
            onPointerMove={updateZoomCursor}
            onPointerLeave={() => setZoomCursor(null)}
            onPointerDown={(event) => {
              if (event.button !== 0) {
                return;
              }
              event.preventDefault();
              event.stopPropagation();
              updateZoomCursor(event);
              onZoomToolClick(event.altKey ? "out" : "in");
            }}
            onPointerUp={(event) => {
              event.preventDefault();
              event.stopPropagation();
            }}
            onPointerCancel={(event) => {
              event.preventDefault();
              event.stopPropagation();
            }}
          >
            {zoomCursor ? (
              <div
                className={`stage-zoom-cursor ${zoomCursor.altKey ? "zoom-out" : "zoom-in"}`}
                style={{
                  left: `${zoomCursor.x}px`,
                  top: `${zoomCursor.y}px`
                }}
                aria-hidden="true"
              >
                <span className="stage-zoom-cursor-mark" />
              </div>
            ) : null}
          </div>
        ) : null}
      </div>
    </div>
  );
}
