import type { BBox, FontPreset, MangaPage, TranslationBlock } from "../../../shared/types";
import { bboxToPixels, clamp } from "../../../shared/geometry";

export type PixelRect = {
  x: number;
  y: number;
  width: number;
  height: number;
};

export type ResultReportRow = {
  rowIndex: number;
  pageIndex: number;
  pageName: string;
  pageSize: string;
  pageId: string;
  blockIndex: number;
  blockId: string;
  type: string;
  sourceCropDataUrl: string;
  outputCropDataUrl: string;
  sourceRect: PixelRect;
  outputRect: PixelRect;
  sourceText: string;
  translatedText: string;
  fontPresetName: string;
  fontPresetLinks: string;
  fontFamily: string;
  characterFontOverrides: string;
  fontSizePx: number;
  fontWeight: number | null;
  fontStyle: string;
  textDecoration: string;
  lineHeight: number;
  letterSpacingPx: number;
  textAlign: string;
  textPosition: string;
  textPaddingPx: number;
  renderDirection: string;
  sourceDirection: string;
  rotationDeg: number;
  textColor: string;
  backgroundColor: string;
  opacity: number;
  outline: string;
  secondaryOutline: string;
  shadow: string;
  screentone: string;
  autoFitText: string;
  inpainted: string;
  sourceRectLabel: string;
  outputRectLabel: string;
  confidence: number;
  inpaintStatus: string;
  inpaintSettings: string;
  updatedAt: string;
};

export type BuildResultReportHtmlOptions = {
  chapterTitle: string;
  generatedAt: string;
  rows: readonly ResultReportRow[];
};

export const RESULT_REPORT_FOCUS_BLOCK_MESSAGE = "manga-result-report:focus-block";

type ReportColumn = {
  key: keyof ResultReportRow | "sourceImage" | "outputImage";
  label: string;
  type: "text" | "number" | "date";
  image?: "source" | "output";
  className?: string;
};

const LINK_FIELD_LABELS: readonly [keyof TranslationBlock, string][] = [
  ["fontSizeLinkedToPreset", "크기"],
  ["lineHeightLinkedToPreset", "줄높이"],
  ["letterSpacingLinkedToPreset", "자간"],
  ["outlineColorLinkedToPreset", "외곽선색"],
  ["outlineWidthLinkedToPreset", "외곽선두께"],
  ["secondaryOutlineColorLinkedToPreset", "2차색"],
  ["secondaryOutlineWidthLinkedToPreset", "2차두께"],
  ["shadowEnabledLinkedToPreset", "그림자"],
  ["shadowColorLinkedToPreset", "그림자색"],
  ["shadowAngleDegLinkedToPreset", "그림자각도"],
  ["shadowDistancePxLinkedToPreset", "그림자거리"],
  ["autoFitTextLinkedToPreset", "자동맞춤"],
  ["textColorLinkedToPreset", "글자색"],
  ["screentoneFillEnabledLinkedToPreset", "스크린톤"],
  ["screentoneFillIntensityLinkedToPreset", "스크린톤강도"],
  ["screentoneFillDensityLinkedToPreset", "스크린톤밀도"],
  ["screentoneFillAntialiasLinkedToPreset", "스크린톤AA"],
  ["fontWeightLinkedToPreset", "굵기"],
  ["fontStyleLinkedToPreset", "스타일"],
  ["textDecorationLinkedToPreset", "장식"]
];

const REPORT_COLUMNS: readonly ReportColumn[] = [
  { key: "rowIndex", label: "순번", type: "number" },
  { key: "pageIndex", label: "페이지", type: "number" },
  { key: "pageName", label: "페이지명", type: "text" },
  { key: "pageSize", label: "페이지 크기", type: "text" },
  { key: "blockIndex", label: "블록", type: "number" },
  { key: "type", label: "유형", type: "text" },
  { key: "sourceImage", label: "원본 이미지", type: "number", image: "source", className: "image-cell" },
  { key: "outputImage", label: "최종 아웃풋", type: "number", image: "output", className: "image-cell" },
  { key: "sourceText", label: "원문", type: "text", className: "text-cell" },
  { key: "translatedText", label: "번역문", type: "text", className: "text-cell" },
  { key: "fontPresetName", label: "폰트 프리셋", type: "text" },
  { key: "fontPresetLinks", label: "프리셋 링크", type: "text" },
  { key: "fontFamily", label: "폰트", type: "text" },
  { key: "characterFontOverrides", label: "문자별 폰트", type: "text" },
  { key: "fontSizePx", label: "크기", type: "number" },
  { key: "fontWeight", label: "굵기", type: "number" },
  { key: "fontStyle", label: "스타일", type: "text" },
  { key: "textDecoration", label: "장식", type: "text" },
  { key: "lineHeight", label: "줄높이", type: "number" },
  { key: "letterSpacingPx", label: "자간", type: "number" },
  { key: "textAlign", label: "정렬", type: "text" },
  { key: "textPosition", label: "위치", type: "text" },
  { key: "textPaddingPx", label: "패딩", type: "number" },
  { key: "renderDirection", label: "렌더 방향", type: "text" },
  { key: "sourceDirection", label: "원문 방향", type: "text" },
  { key: "rotationDeg", label: "회전", type: "number" },
  { key: "textColor", label: "글자색", type: "text" },
  { key: "backgroundColor", label: "배경/불투명도", type: "text" },
  { key: "outline", label: "외곽선", type: "text" },
  { key: "secondaryOutline", label: "2차 외곽선", type: "text" },
  { key: "shadow", label: "그림자", type: "text" },
  { key: "screentone", label: "스크린톤", type: "text" },
  { key: "autoFitText", label: "자동맞춤", type: "text" },
  { key: "inpainted", label: "블록 인페인트", type: "text" },
  { key: "sourceRectLabel", label: "원본 범위", type: "text" },
  { key: "outputRectLabel", label: "출력 범위", type: "text" },
  { key: "confidence", label: "신뢰도", type: "number" },
  { key: "inpaintStatus", label: "인페인트", type: "text" },
  { key: "inpaintSettings", label: "인페인트 설정", type: "text" },
  { key: "updatedAt", label: "페이지 수정일", type: "date" },
  { key: "pageId", label: "페이지 ID", type: "text" },
  { key: "blockId", label: "블록 ID", type: "text" }
];

export async function cropImageDataUrl(source: string, rect: PixelRect): Promise<string> {
  return cropLoadedImageDataUrl(await loadReportImage(source), rect);
}

export function loadReportImage(source: string): Promise<HTMLImageElement> {
  return loadImage(source);
}

export function cropLoadedImageDataUrl(image: CanvasImageSource, rect: PixelRect): string {
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(rect.width));
  canvas.height = Math.max(1, Math.round(rect.height));
  const context = canvas.getContext("2d");
  if (!context) {
    throw new Error("보고서 이미지 캔버스를 만들지 못했습니다.");
  }

  context.drawImage(
    image,
    rect.x,
    rect.y,
    rect.width,
    rect.height,
    0,
    0,
    canvas.width,
    canvas.height
  );
  return canvas.toDataURL("image/png");
}

export function resolveBlockReportRects(page: MangaPage, block: TranslationBlock): { sourceRect: PixelRect; outputRect: PixelRect } {
  return {
    sourceRect: resolvePixelRect(page, block.bbox, block.bboxSpace),
    outputRect: resolvePixelRect(page, block.renderBbox ?? block.bbox, block.renderBbox ? block.renderBboxSpace ?? block.bboxSpace : block.bboxSpace)
  };
}

export function createResultReportRow(options: {
  block: TranslationBlock;
  blockIndex: number;
  fontPreset: FontPreset | undefined;
  outputCropDataUrl: string;
  outputRect: PixelRect;
  page: MangaPage;
  pageIndex: number;
  rowIndex: number;
  sourceCropDataUrl: string;
  sourceRect: PixelRect;
}): ResultReportRow {
  const { block, blockIndex, fontPreset, outputCropDataUrl, outputRect, page, pageIndex, rowIndex, sourceCropDataUrl, sourceRect } = options;
  const fontPresetName = resolveFontPresetName(block, fontPreset);

  return {
    rowIndex,
    pageIndex,
    pageName: page.name,
    pageSize: `${page.width}×${page.height}`,
    pageId: page.id,
    blockIndex,
    blockId: block.id,
    type: block.type,
    sourceCropDataUrl,
    outputCropDataUrl,
    sourceRect,
    outputRect,
    sourceText: block.sourceText,
    translatedText: block.translatedText,
    fontPresetName,
    fontPresetLinks: formatLinkedPresetFields(block),
    fontFamily: block.fontFamily ?? "",
    characterFontOverrides: formatCharacterFontOverrides(block),
    fontSizePx: block.fontSizePx,
    fontWeight: block.fontWeight ?? null,
    fontStyle: block.fontStyle ?? "normal",
    textDecoration: block.textDecoration ?? "none",
    lineHeight: block.lineHeight,
    letterSpacingPx: block.letterSpacingPx ?? 0,
    textAlign: block.textAlign,
    textPosition: block.textPosition ?? "center",
    textPaddingPx: block.textPaddingPx ?? 0,
    renderDirection: block.renderDirection,
    sourceDirection: block.sourceDirection,
    rotationDeg: block.rotationDeg ?? 0,
    textColor: block.textColor,
    backgroundColor: `${block.backgroundColor} / ${formatNumber(block.opacity)}`,
    opacity: block.opacity,
    outline: formatOutline(block.outlineColor, block.outlineWidthPx),
    secondaryOutline: formatOutline(block.secondaryOutlineColor, block.secondaryOutlineWidthPx),
    shadow: formatShadow(block),
    screentone: formatScreentone(block),
    autoFitText: formatBoolean(block.autoFitText),
    inpainted: formatBoolean(block.inpainted),
    sourceRectLabel: formatRect(sourceRect),
    outputRectLabel: formatRect(outputRect),
    confidence: block.confidence,
    inpaintStatus: page.inpaintStatus ?? "idle",
    inpaintSettings: formatInpaintSettings(page),
    updatedAt: page.updatedAt
  };
}

export function buildResultReportHtml({ chapterTitle, generatedAt, rows }: BuildResultReportHtmlOptions): string {
  const safeTitle = escapeHtml(chapterTitle);
  const blockCount = rows.length;
  const pageCount = new Set(rows.map((row) => row.pageId)).size;

  return `<!doctype html>
<html lang="ko">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>${safeTitle} 결과 보고서</title>
  <style>
    :root {
      color-scheme: light;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      background: #f5f7fa;
      color: #17202a;
    }
    * { box-sizing: border-box; }
    body { margin: 0; background: #f5f7fa; }
    header {
      position: sticky;
      top: 0;
      z-index: 5;
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 16px;
      padding: 16px 20px;
      border-bottom: 1px solid #d6dde6;
      background: rgba(250, 252, 255, 0.96);
      backdrop-filter: blur(10px);
    }
    h1 { margin: 0; font-size: 20px; line-height: 1.25; }
    .meta { display: flex; flex-wrap: wrap; gap: 8px; color: #52616f; font-size: 12px; }
    .chip { padding: 4px 8px; border: 1px solid #d6dde6; border-radius: 999px; background: #ffffff; }
    main { padding: 16px 20px 28px; }
    .report-controls {
      display: flex;
      align-items: center;
      gap: 10px;
      flex-wrap: wrap;
      margin-bottom: 10px;
    }
    .global-filter {
      display: grid;
      grid-template-columns: auto minmax(220px, 360px);
      align-items: center;
      gap: 8px;
      color: #425160;
      font-size: 12px;
      font-weight: 700;
    }
    .global-filter input,
    .column-filter {
      min-width: 0;
      height: 30px;
      border: 1px solid #c8d1dc;
      border-radius: 6px;
      background: #ffffff;
      color: #17202a;
      font: inherit;
    }
    .global-filter input {
      width: 100%;
      padding: 5px 8px;
    }
    .column-filter {
      display: block;
      width: calc(100% - 12px);
      height: 26px;
      margin: 0 6px 7px;
      padding: 4px 6px;
      font-size: 11px;
    }
    .clear-filters {
      min-height: 30px;
      border: 1px solid #c8d1dc;
      border-radius: 6px;
      background: #ffffff;
      color: #25313d;
      font-size: 12px;
      font-weight: 700;
      cursor: pointer;
    }
    .filter-count { color: #52616f; font-size: 12px; }
    .jump-button {
      min-height: 30px;
      border: 1px solid #bfd0e3;
      border-radius: 6px;
      background: #eef5ff;
      color: #174a83;
      font-size: 12px;
      font-weight: 700;
      white-space: nowrap;
      cursor: pointer;
    }
    .action-header,
    .action-cell {
      min-width: 104px;
      text-align: center;
    }
    .table-wrap {
      overflow: auto;
      max-height: calc(100vh - 150px);
      border: 1px solid #d6dde6;
      background: #ffffff;
    }
    table { width: max-content; min-width: 100%; border-collapse: separate; border-spacing: 0; }
    th, td { border-right: 1px solid #dfe5ec; border-bottom: 1px solid #dfe5ec; vertical-align: top; }
    th {
      position: sticky;
      top: 0;
      z-index: 2;
      background: #eef3f8;
      color: #25313d;
      font-size: 12px;
      text-align: left;
      white-space: nowrap;
    }
    th button {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 8px;
      width: 100%;
      min-height: 34px;
      border: 0;
      padding: 8px 10px;
      background: transparent;
      color: inherit;
      font: inherit;
      cursor: pointer;
    }
    th button::after { content: "↕"; color: #7b8794; font-size: 11px; }
    th[aria-sort="ascending"] button::after { content: "↑"; color: #1f6feb; }
    th[aria-sort="descending"] button::after { content: "↓"; color: #1f6feb; }
    th:focus-within { background: #e5edf5; }
    td { max-width: 320px; padding: 8px 10px; font-size: 12px; line-height: 1.45; background: #ffffff; }
    tbody tr:nth-child(even) td { background: #fbfcfe; }
    .image-cell { min-width: 180px; max-width: 240px; }
    .crop {
      display: block;
      max-width: 210px;
      max-height: 210px;
      width: auto;
      height: auto;
      border: 1px solid #c8d1dc;
      background: #f1f4f8;
      object-fit: contain;
    }
    .crop-meta { display: block; margin-top: 6px; color: #617080; font-size: 11px; white-space: nowrap; }
    .text-cell { min-width: 220px; white-space: pre-wrap; }
    .empty {
      padding: 28px;
      border: 1px solid #d6dde6;
      background: #ffffff;
      color: #52616f;
    }
  </style>
</head>
<body>
  <header>
    <div>
      <h1>${safeTitle} 결과 보고서</h1>
      <div class="meta">
        <span class="chip">생성: ${escapeHtml(generatedAt)}</span>
        <span class="chip">페이지: ${pageCount}</span>
        <span class="chip">블록: ${blockCount}</span>
      </div>
    </div>
  </header>
  <main>
    ${rows.length > 0 ? buildReportTable(rows) : '<div class="empty">보고서에 포함할 텍스트 블록이 없습니다.</div>'}
  </main>
  <script>
    (() => {
      const table = document.querySelector("[data-report-table]");
      if (!table) return;
      const tbody = table.querySelector("tbody");
      const headers = [...table.querySelectorAll("th[data-sort-key]")];
      const rows = [...tbody.querySelectorAll("tr")];
      const globalFilter = document.querySelector("[data-global-filter]");
      const filterInputs = [...table.querySelectorAll("[data-filter-key]")];
      const clearFilters = document.querySelector("[data-clear-filters]");
      const filterCount = document.querySelector("[data-filter-count]");
      const focusButtons = [...table.querySelectorAll("[data-focus-block]")];
      const focusMessageType = ${JSON.stringify(RESULT_REPORT_FOCUS_BLOCK_MESSAGE)};
      let state = { key: "", direction: "asc" };

      const getSortValue = (row, key) => row.querySelector('[data-col="' + key + '"]')?.dataset.sort ?? "";
      const getFilterValue = (row, key) => row.querySelector('[data-col="' + key + '"]')?.textContent ?? "";
      const getRowFilterText = (row) => [...row.querySelectorAll("[data-col]")]
        .map((cell) => cell.textContent || "")
        .join(" ");
      const normalizeFilter = (value) => value.toLocaleLowerCase("ko-KR").trim();
      const compareText = (left, right) => left.localeCompare(right, "ko", { numeric: true, sensitivity: "base" });
      const compareRows = (key, type, direction) => (leftRow, rightRow) => {
        const left = getSortValue(leftRow, key);
        const right = getSortValue(rightRow, key);
        let result;
        if (type === "number") {
          result = (Number.parseFloat(left) || 0) - (Number.parseFloat(right) || 0);
        } else if (type === "date") {
          result = (Date.parse(left) || 0) - (Date.parse(right) || 0);
        } else {
          result = compareText(left, right);
        }
        return direction === "asc" ? result : -result;
      };
      const applyFilters = () => {
        const globalQuery = normalizeFilter(globalFilter?.value ?? "");
        const columnQueries = filterInputs
          .map((input) => ({ key: input.dataset.filterKey || "", query: normalizeFilter(input.value || "") }))
          .filter(({ key, query }) => key && query);
        let visible = 0;
        rows.forEach((row) => {
          const rowText = normalizeFilter(getRowFilterText(row));
          const globalMatch = !globalQuery || rowText.includes(globalQuery);
          const columnMatch = columnQueries.every(({ key, query }) => normalizeFilter(getFilterValue(row, key)).includes(query));
          const matched = globalMatch && columnMatch;
          row.hidden = !matched;
          if (matched) visible += 1;
        });
        if (filterCount) {
          filterCount.textContent = visible + " / " + rows.length + " 표시";
        }
      };

      headers.forEach((header) => {
        header.querySelector("button")?.addEventListener("click", () => {
          const key = header.dataset.sortKey || "";
          const type = header.dataset.sortType || "text";
          const direction = state.key === key && state.direction === "asc" ? "desc" : "asc";
          state = { key, direction };
          headers.forEach((candidate) => candidate.setAttribute("aria-sort", "none"));
          header.setAttribute("aria-sort", direction === "asc" ? "ascending" : "descending");
          [...tbody.querySelectorAll("tr")]
            .sort(compareRows(key, type, direction))
            .forEach((row) => tbody.appendChild(row));
        });
      });
      focusButtons.forEach((button) => {
        button.addEventListener("click", () => {
          const pageId = button.dataset.pageId || "";
          const blockId = button.dataset.blockId || "";
          if (!pageId || !blockId || !window.opener || window.opener.closed) {
            return;
          }
          window.opener.postMessage({ type: focusMessageType, pageId, blockId }, "*");
          window.opener.focus?.();
        });
      });
      globalFilter?.addEventListener("input", applyFilters);
      filterInputs.forEach((input) => input.addEventListener("input", applyFilters));
      clearFilters?.addEventListener("click", () => {
        if (globalFilter) globalFilter.value = "";
        filterInputs.forEach((input) => { input.value = ""; });
        applyFilters();
      });
      applyFilters();
    })();
  </script>
</body>
</html>`;
}

function buildReportTable(rows: readonly ResultReportRow[]): string {
  return `<div class="report-controls">
    <label class="global-filter">
      <span>전체 필터</span>
      <input type="search" data-global-filter placeholder="전체 컬럼 검색">
    </label>
    <button type="button" class="clear-filters" data-clear-filters>필터 초기화</button>
    <span class="filter-count" data-filter-count>${rows.length} / ${rows.length} 표시</span>
  </div>
  <div class="table-wrap">
    <table data-report-table>
      <thead>
        <tr>${REPORT_COLUMNS.map(buildHeaderCell).join("")}<th class="action-header" aria-sort="none">이동</th></tr>
      </thead>
      <tbody>${rows.map(buildBodyRow).join("")}</tbody>
    </table>
  </div>`;
}

function buildHeaderCell(column: ReportColumn): string {
  const key = String(column.key);
  return `<th data-sort-key="${escapeAttribute(key)}" data-sort-type="${column.type}" aria-sort="none"><button type="button">${escapeHtml(column.label)}</button><input class="column-filter" type="search" data-filter-key="${escapeAttribute(key)}" aria-label="${escapeAttribute(`${column.label} 필터`)}" placeholder="필터"></th>`;
}

function buildBodyRow(row: ResultReportRow): string {
  return `<tr>${REPORT_COLUMNS.map((column) => buildBodyCell(row, column)).join("")}${buildActionCell(row)}</tr>`;
}

function buildBodyCell(row: ResultReportRow, column: ReportColumn): string {
  if (column.image === "source") {
    const sortValue = row.sourceRect.width * row.sourceRect.height;
    return `<td class="${column.className ?? ""}" data-col="${escapeAttribute(String(column.key))}" data-sort="${sortValue}">
      <img class="crop" src="${escapeAttribute(row.sourceCropDataUrl)}" alt="${escapeAttribute(`${row.pageName} ${row.blockIndex} 원본`)}">
      <span class="crop-meta">${escapeHtml(row.sourceRectLabel)}</span>
    </td>`;
  }

  if (column.image === "output") {
    const sortValue = row.outputRect.width * row.outputRect.height;
    return `<td class="${column.className ?? ""}" data-col="${escapeAttribute(String(column.key))}" data-sort="${sortValue}">
      <img class="crop" src="${escapeAttribute(row.outputCropDataUrl)}" alt="${escapeAttribute(`${row.pageName} ${row.blockIndex} 최종 출력`)}">
      <span class="crop-meta">${escapeHtml(row.outputRectLabel)}</span>
    </td>`;
  }

  const value = row[column.key as keyof ResultReportRow];
  const sortValue = resolveSortValue(value);
  const filterValue = formatCellValue(value);
  return `<td class="${column.className ?? ""}" data-col="${escapeAttribute(String(column.key))}" data-sort="${escapeAttribute(sortValue)}">${escapeHtml(filterValue)}</td>`;
}

function buildActionCell(row: ResultReportRow): string {
  return `<td class="action-cell"><button type="button" class="jump-button" data-focus-block data-page-id="${escapeAttribute(row.pageId)}" data-block-id="${escapeAttribute(row.blockId)}">이동</button></td>`;
}

function resolvePixelRect(page: MangaPage, bbox: BBox, space: "normalized_1000" | "pixels" | undefined): PixelRect {
  const pixelBox = space === "pixels" ? bbox : bboxToPixels(bbox, page.width, page.height);
  const rawLeft = pixelBox.x;
  const rawTop = pixelBox.y;
  const rawRight = pixelBox.x + pixelBox.w;
  const rawBottom = pixelBox.y + pixelBox.h;
  const left = Math.floor(clamp(rawLeft, 0, Math.max(0, page.width - 1)));
  const top = Math.floor(clamp(rawTop, 0, Math.max(0, page.height - 1)));
  const right = Math.ceil(clamp(rawRight, left + 1, page.width));
  const bottom = Math.ceil(clamp(rawBottom, top + 1, page.height));
  return {
    x: left,
    y: top,
    width: Math.max(1, right - left),
    height: Math.max(1, bottom - top)
  };
}

function resolveFontPresetName(block: TranslationBlock, fontPreset: FontPreset | undefined): string {
  if (!block.fontPresetId) {
    return "개별값";
  }
  return fontPreset ? fontPreset.name : `삭제됨: ${block.fontPresetId}`;
}

function formatLinkedPresetFields(block: TranslationBlock): string {
  if (!block.fontPresetId) {
    return "";
  }
  const labels = LINK_FIELD_LABELS.filter(([key]) => block[key] !== false).map(([, label]) => label);
  return labels.length > 0 ? labels.join(", ") : "없음";
}

function formatCharacterFontOverrides(block: TranslationBlock): string {
  return block.characterFontOverrides?.map((override) => `${override.character}: ${override.fontFamily}`).join(", ") ?? "";
}

function formatOutline(color: string | undefined, widthPx: number | undefined): string {
  const width = widthPx ?? 0;
  return width > 0 ? `${formatNumber(width)}px ${color ?? "#000000"}` : "없음";
}

function formatShadow(block: TranslationBlock): string {
  if (!block.shadowEnabled) {
    return "꺼짐";
  }
  return `${block.shadowColor ?? "#000000"} / ${formatNumber(block.shadowAngleDeg ?? 0)}deg / ${formatNumber(block.shadowDistancePx ?? 0)}px`;
}

function formatScreentone(block: TranslationBlock): string {
  if (!block.screentoneFillEnabled) {
    return "꺼짐";
  }
  return `강도 ${formatNumber(block.screentoneFillIntensity ?? 0)}, 밀도 ${formatNumber(block.screentoneFillDensity ?? 0)}, AA ${formatBoolean(block.screentoneFillAntialias)}`;
}

function formatInpaintSettings(page: MangaPage): string {
  const settings = page.inpaintSettings;
  if (!settings) {
    return "";
  }
  const cleanup = settings.artifactCleanupPx === undefined ? "" : `, cleanup ${settings.artifactCleanupPx}`;
  return `${settings.engine}, padding ${settings.paddingPx}, feather ${settings.featherPx}, tile ${settings.tileSize}${cleanup}`;
}

function formatBoolean(value: boolean | undefined): string {
  return value ? "켜짐" : "꺼짐";
}

function formatRect(rect: PixelRect): string {
  return `${rect.x}, ${rect.y}, ${rect.width}×${rect.height}`;
}

function formatNumber(value: number): string {
  return Number.isInteger(value) ? String(value) : value.toFixed(3).replace(/0+$/u, "").replace(/\.$/u, "");
}

function resolveSortValue(value: ResultReportRow[keyof ResultReportRow]): string {
  if (typeof value === "number") {
    return String(value);
  }
  if (value === null || value === undefined) {
    return "";
  }
  if (typeof value === "object") {
    return "";
  }
  return String(value);
}

function formatCellValue(value: ResultReportRow[keyof ResultReportRow]): string {
  if (typeof value === "number") {
    return formatNumber(value);
  }
  if (value === null || value === undefined) {
    return "";
  }
  if (typeof value === "object") {
    return "";
  }
  return String(value);
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/gu, "&amp;")
    .replace(/</gu, "&lt;")
    .replace(/>/gu, "&gt;")
    .replace(/"/gu, "&quot;")
    .replace(/'/gu, "&#39;");
}

function escapeAttribute(value: string): string {
  return escapeHtml(value);
}

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const image = new Image();
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("보고서 이미지를 불러오지 못했습니다."));
    image.src = src;
  });
}
