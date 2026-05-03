import React from "react";
import type { BBox, ChapterSnapshot, MangaPage, TranslationBlock } from "../../../shared/types";
import {
  applyEditableBlockBbox,
  clampRotationDeg,
  resolveBlockRotationDeg,
  resolveEditableBlockBbox
} from "../../../shared/geometry";
import { angleBetweenPointsDeg, isEditableTarget } from "../lib/editorUtils";
import type { ActiveLayer } from "../lib/layerState";
import type { ViewportSize } from "../lib/overlayLayout";
import { clampStageViewScale } from "../lib/stageFit";
import { useStageSize } from "./useStageSize";

type DragMode = "move" | "resize" | "rotate";
type StageZoomDirection = "in" | "out";

type DragState = {
  mode: DragMode;
  blockId: string;
  captureElement: Element | null;
  startX: number;
  startY: number;
  startBbox: BBox;
  startRotationDeg: number;
  startAngleDeg: number;
  centerX: number;
  centerY: number;
  undoRecorded: boolean;
};

type UseStageInteractionOptions = {
  activeLayer: ActiveLayer;
  currentChapter: ChapterSnapshot | null;
  recordTranslationUndoSnapshot: (label: string) => boolean;
  selectedPage: MangaPage | null;
  selectedPageEditLocked: boolean;
  setSelectedBlockId: React.Dispatch<React.SetStateAction<string | null>>;
  updateCurrentChapter: (pageId: string | undefined, updater: (chapter: ChapterSnapshot) => ChapterSnapshot) => void;
};

type UseStageInteractionState = {
  fitStageToWorkspace: () => void;
  handleZoomToolClick: (direction: StageZoomDirection) => void;
  imageRef: React.RefObject<HTMLImageElement | null>;
  onBlockPointerDown: (event: React.PointerEvent, block: TranslationBlock, mode: DragMode) => void;
  onStagePointerMove: (event: React.PointerEvent) => void;
  onStagePointerUp: (event: React.PointerEvent) => void;
  stageRef: React.RefObject<HTMLDivElement | null>;
  stageSize: ViewportSize | null;
  stageViewResetKey: number;
  stageViewScale: number | null;
  stageZoomLabel: string;
  showOriginalStageSize: () => void;
  zoomInStage: () => void;
  zoomOutStage: () => void;
};

const STAGE_ZOOM_STEP = 1.2;

function blurActiveEditableElement(): void {
  if (typeof document === "undefined" || typeof HTMLElement === "undefined") {
    return;
  }

  const activeElement = document.activeElement;
  if (!(activeElement instanceof HTMLElement) || !isEditableTarget(activeElement)) {
    return;
  }

  activeElement.blur();
}

function trySetPointerCapture(element: Element, pointerId: number): void {
  try {
    element.setPointerCapture(pointerId);
  } catch {
    // Some automated or synthetic pointer events do not create an active pointer.
  }
}

function tryReleasePointerCapture(element: Element | null, pointerId: number): void {
  try {
    if (element?.hasPointerCapture(pointerId)) {
      element.releasePointerCapture(pointerId);
    }
  } catch {
    // Ignore stale pointer capture state after cancellation or synthetic events.
  }
}

export function useStageInteraction({
  activeLayer,
  currentChapter,
  recordTranslationUndoSnapshot,
  selectedPage,
  selectedPageEditLocked,
  setSelectedBlockId,
  updateCurrentChapter
}: UseStageInteractionOptions): UseStageInteractionState {
  const [stageViewScale, setStageViewScale] = React.useState<number | null>(null);
  const [stageViewResetKey, setStageViewResetKey] = React.useState(0);
  const stageRef = React.useRef<HTMLDivElement | null>(null);
  const imageRef = React.useRef<HTMLImageElement | null>(null);
  const dragRef = React.useRef<DragState | null>(null);
  const selectedPageSize = React.useMemo(
    () => (selectedPage ? { width: selectedPage.width, height: selectedPage.height } : null),
    [selectedPage?.height, selectedPage?.width]
  );
  const stageSize = useStageSize(imageRef, selectedPageSize);
  const currentStageScale = selectedPage && stageSize
    ? stageSize.width / Math.max(1, selectedPage.width)
    : (stageViewScale ?? 1);
  const stageZoomLabel = `${Math.round(currentStageScale * 100)}%`;

  const zoomStage = React.useCallback((factor: number) => {
    setStageViewScale((current) => clampStageViewScale((current ?? currentStageScale) * factor));
  }, [currentStageScale]);

  const handleZoomToolClick = React.useCallback((direction: StageZoomDirection) => {
    zoomStage(direction === "out" ? 1 / STAGE_ZOOM_STEP : STAGE_ZOOM_STEP);
  }, [zoomStage]);

  const zoomInStage = React.useCallback(() => {
    zoomStage(STAGE_ZOOM_STEP);
  }, [zoomStage]);

  const zoomOutStage = React.useCallback(() => {
    zoomStage(1 / STAGE_ZOOM_STEP);
  }, [zoomStage]);

  const fitStageToWorkspace = React.useCallback(() => {
    setStageViewScale(null);
    setStageViewResetKey((current) => current + 1);
  }, []);

  const showOriginalStageSize = React.useCallback(() => {
    setStageViewScale(1);
  }, []);

  const onBlockPointerDown = React.useCallback((event: React.PointerEvent, block: TranslationBlock, mode: DragMode) => {
    if (!stageRef.current || selectedPageEditLocked || activeLayer !== "overlay") {
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    blurActiveEditableElement();
    setSelectedBlockId(block.id);
    const target = resolveEditableBlockBbox(block);
    const stageRect = stageRef.current.getBoundingClientRect();
    const centerX = stageRect.left + ((target.bbox.x + target.bbox.w / 2) / 1000) * stageRect.width;
    const centerY = stageRect.top + ((target.bbox.y + target.bbox.h / 2) / 1000) * stageRect.height;
    dragRef.current = {
      mode,
      blockId: block.id,
      captureElement: event.currentTarget,
      startX: event.clientX,
      startY: event.clientY,
      startBbox: target.bbox,
      startRotationDeg: resolveBlockRotationDeg(block),
      startAngleDeg: angleBetweenPointsDeg(centerX, centerY, event.clientX, event.clientY),
      centerX,
      centerY,
      undoRecorded: false
    };
    trySetPointerCapture(event.currentTarget, event.pointerId);
  }, [activeLayer, selectedPageEditLocked, setSelectedBlockId]);

  const onStagePointerMove = React.useCallback((event: React.PointerEvent) => {
    const drag = dragRef.current;
    const page = selectedPage;
    const stage = stageRef.current;
    if (!drag || !page || !stage || !currentChapter || selectedPageEditLocked) {
      return;
    }
    const rect = stage.getBoundingClientRect();
    const dx = ((event.clientX - drag.startX) / Math.max(1, rect.width)) * 1000;
    const dy = ((event.clientY - drag.startY) / Math.max(1, rect.height)) * 1000;
    const next =
      drag.mode === "move"
        ? {
            ...drag.startBbox,
            x: drag.startBbox.x + dx,
            y: drag.startBbox.y + dy
          }
        : drag.mode === "resize"
          ? {
              ...drag.startBbox,
              w: drag.startBbox.w + dx,
              h: drag.startBbox.h + dy
            }
          : drag.startBbox;
    const nextRotationDeg =
      drag.mode === "rotate"
        ? clampRotationDeg(drag.startRotationDeg + angleBetweenPointsDeg(drag.centerX, drag.centerY, event.clientX, event.clientY) - drag.startAngleDeg)
        : null;

    if (!drag.undoRecorded) {
      recordTranslationUndoSnapshot("번역 블록 위치 변경");
      drag.undoRecorded = true;
    }

    updateCurrentChapter(page.id, (chapter) => ({
      ...chapter,
      pages: chapter.pages.map((candidate) =>
        candidate.id !== page.id
          ? candidate
          : {
              ...candidate,
              updatedAt: new Date().toISOString(),
              blocks: candidate.blocks.map((block) =>
                block.id === drag.blockId
                  ? nextRotationDeg === null
                    ? applyEditableBlockBbox(block, next)
                    : { ...block, rotationDeg: nextRotationDeg }
                  : block
              )
            }
      )
    }));
  }, [currentChapter, recordTranslationUndoSnapshot, selectedPage, selectedPageEditLocked, updateCurrentChapter]);

  const onStagePointerUp = React.useCallback((event: React.PointerEvent) => {
    const drag = dragRef.current;
    tryReleasePointerCapture(drag?.captureElement ?? null, event.pointerId);
    dragRef.current = null;
  }, []);

  return {
    fitStageToWorkspace,
    handleZoomToolClick,
    imageRef,
    onBlockPointerDown,
    onStagePointerMove,
    onStagePointerUp,
    stageRef,
    stageSize,
    stageViewResetKey,
    stageViewScale,
    stageZoomLabel,
    showOriginalStageSize,
    zoomInStage,
    zoomOutStage
  };
}
