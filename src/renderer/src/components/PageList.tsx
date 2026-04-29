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
};

export function PageList({
  pages,
  selectedPageId,
  jobActive,
  onSelect,
  onRetranslate,
  onRemove,
  onReorder
}: PageListProps): React.JSX.Element {
  const [draggingPageId, setDraggingPageId] = React.useState<string | null>(null);
  const pageItemRefs = React.useRef<Record<string, HTMLDivElement | null>>({});

  React.useEffect(() => {
    if (!selectedPageId) {
      return;
    }
    pageItemRefs.current[selectedPageId]?.scrollIntoView({
      block: "nearest"
    });
  }, [selectedPageId]);

  return (
    <section className="page-list grid min-h-0 grid-rows-[auto_minmax(0,1fr)] gap-2.5">
      <div className="panel-header flex items-center justify-between gap-2">
        <h2 className="inline-flex items-center gap-2">
          페이지
          <span className="panel-count">{pages.length}</span>
        </h2>
      </div>
      <div className="page-list-scroll grid min-h-0 content-start gap-2 overflow-auto pr-1">
        {pages.length ? (
          pages.map((page) => (
            <div
              key={page.id}
              ref={(element) => {
                pageItemRefs.current[page.id] = element;
              }}
              className={page.id === selectedPageId ? "page-item active grid grid-cols-[minmax(0,1fr)_auto] items-center gap-2" : "page-item grid grid-cols-[minmax(0,1fr)_auto] items-center gap-2"}
              draggable={!jobActive}
              onDragStart={() => setDraggingPageId(page.id)}
              onDragEnd={() => setDraggingPageId(null)}
              onDragOver={(event) => {
                if (!jobActive) {
                  event.preventDefault();
                }
              }}
              onDrop={(event) => {
                event.preventDefault();
                if (!draggingPageId || draggingPageId === page.id || jobActive) {
                  return;
                }
                onReorder(draggingPageId, page.id);
                setDraggingPageId(null);
              }}
            >
              <button className="page-select flex min-w-0 items-center justify-start gap-3 px-2.5 py-2" onClick={() => onSelect(page.id)}>
                <span className="min-w-0 truncate">{page.name}</span>
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
                ) : (
                  <span className="page-status-badge">{resolveStatusLabel(page)}</span>
                )}
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

function resolveStatusLabel(page: MangaPage): string {
  switch (page.analysisStatus) {
    case "completed":
      return "완료";
    case "running":
      return "진행";
    case "failed":
      return "실패";
    default:
      return "대기";
  }
}
