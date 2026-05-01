import React from "react";
import type { MangaPage } from "../../../shared/types";

type PageListProps = {
  pages: MangaPage[];
  selectedPageId: string | null;
  jobActive: boolean;
  onSelect: (pageId: string) => void;
  onRetranslate: (pageId: string) => void;
  onRemove: (pageId: string) => void;
  onReorder: (sourcePageId: string, targetPageId: string) => void;
  onToggleProgress: (pageId: string) => void;
};

export function PageList({
  pages,
  selectedPageId,
  jobActive,
  onSelect,
  onRetranslate,
  onRemove,
  onReorder,
  onToggleProgress
}: PageListProps): React.JSX.Element {
  const [draggingPageId, setDraggingPageId] = React.useState<string | null>(null);
  const [dragOverPageId, setDragOverPageId] = React.useState<string | null>(null);
  const pageItemRefs = React.useRef<Record<string, HTMLDivElement | null>>({});

  React.useEffect(() => {
    if (!selectedPageId) {
      return;
    }
    pageItemRefs.current[selectedPageId]?.scrollIntoView({
      block: "nearest"
    });
  }, [selectedPageId]);

  const completedCount = pages.filter((p) => p.progressCompleted).length;
  const selectedPageIndex = selectedPageId ? pages.findIndex((page) => page.id === selectedPageId) : -1;
  const previousPage = selectedPageIndex > 0 ? pages[selectedPageIndex - 1] : null;
  const nextPage = selectedPageIndex >= 0 && selectedPageIndex < pages.length - 1 ? pages[selectedPageIndex + 1] : null;

  return (
    <section className="page-list grid min-h-0 grid-rows-[auto_minmax(0,1fr)] gap-2.5">
      <div className="panel-header flex items-center justify-between gap-2">
        <h2 className="inline-flex items-center gap-2">
          페이지
          <span className="panel-count">{completedCount}/{pages.length}</span>
        </h2>
        <div className="page-navigation-actions" aria-label="페이지 이동">
          <button
            type="button"
            className="page-nav-button"
            disabled={!previousPage}
            onClick={() => {
              if (previousPage) {
                onSelect(previousPage.id);
              }
            }}
            aria-label="이전 페이지"
            title={previousPage ? `이전 페이지: ${previousPage.name}` : "이전 페이지 없음"}
          >
            ←
          </button>
          <button
            type="button"
            className="page-nav-button"
            disabled={!nextPage}
            onClick={() => {
              if (nextPage) {
                onSelect(nextPage.id);
              }
            }}
            aria-label="다음 페이지"
            title={nextPage ? `다음 페이지: ${nextPage.name}` : "다음 페이지 없음"}
          >
            →
          </button>
        </div>
      </div>
      <div className="page-list-scroll grid min-h-0 content-start gap-2 overflow-auto pr-1">
        {pages.length ? (
          pages.map((page) => (
            <div
              key={page.id}
              ref={(element) => {
                pageItemRefs.current[page.id] = element;
              }}
              className={[
                "page-item grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-2 transition-opacity duration-150",
                page.id === selectedPageId ? "active" : "",
                page.id === draggingPageId ? "page-item-dragging" : "",
                page.id === dragOverPageId ? "page-item-drag-over" : ""
              ].filter(Boolean).join(" ")}
              draggable={!jobActive}
              onDragStart={(event) => {
                setDraggingPageId(page.id);
                event.dataTransfer.effectAllowed = "move";
              }}
              onDragEnd={() => {
                setDraggingPageId(null);
                setDragOverPageId(null);
              }}
              onDragOver={(event) => {
                if (!jobActive && draggingPageId && draggingPageId !== page.id) {
                  event.preventDefault();
                  event.dataTransfer.dropEffect = "move";
                  setDragOverPageId(page.id);
                }
              }}
              onDragLeave={() => {
                setDragOverPageId(null);
              }}
              onDrop={(event) => {
                event.preventDefault();
                setDragOverPageId(null);
                if (!draggingPageId || draggingPageId === page.id || jobActive) {
                  return;
                }
                onReorder(draggingPageId, page.id);
                setDraggingPageId(null);
              }}
            >
              <input
                type="checkbox"
                className="page-progress-checkbox"
                checked={page.progressCompleted ?? false}
                onChange={() => onToggleProgress(page.id)}
                aria-label={`${page.name} 작업 완료`}
                title="작업 완료"
              />
              <button className="page-select px-2.5 py-2" onClick={() => onSelect(page.id)}>
                <span className="min-w-0 truncate">{page.name}</span>
                {page.id !== selectedPageId ? (
                  <span
                    className={`page-status-icon ${resolveStatusTone(page)}`}
                    aria-label={`${page.name} AI 번역 상태: ${resolveStatusLabel(page)}`}
                    title={`AI 번역 상태: ${resolveStatusLabel(page)}`}
                  >
                    <span className="page-status-translation-mark" aria-hidden="true">
                      <span>A</span>
                      <span>가</span>
                    </span>
                  </span>
                ) : null}
              </button>
              <div className="page-side flex items-center justify-end">
                {page.id === selectedPageId ? (
                  <div className="page-actions flex gap-2">
                    <button
                      className="page-icon-button grid size-7 place-items-center p-0"
                      onClick={() => onRetranslate(page.id)}
                      disabled={jobActive}
                      aria-label={`${page.name} 재번역`}
                      title="재번역"
                    >
                      ↻
                    </button>
                    <button
                      className="page-remove page-icon-button grid size-7 place-items-center p-0"
                      onClick={() => onRemove(page.id)}
                      disabled={jobActive}
                      aria-label={`${page.name} 삭제`}
                      title="삭제"
                    >
                      ×
                    </button>
                  </div>
                ) : null}
              </div>
            </div>
          ))
        ) : (
          <p className="panel-empty">불러온 페이지가 없습니다.</p>
        )}
      </div>
    </section>
  );
}

function resolveStatusTone(page: MangaPage): string {
  switch (page.analysisStatus) {
    case "completed":
      return "completed";
    case "running":
      return "running";
    case "failed":
      return "failed";
    default:
      return "idle";
  }
}

function resolveStatusLabel(page: MangaPage): string {
  switch (page.analysisStatus) {
    case "completed":
      return "번역 완료";
    case "running":
      return "번역 진행";
    case "failed":
      return "번역 실패";
    default:
      return "번역 대기";
  }
}
