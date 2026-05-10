import React from "react";
import type { ChapterSnapshot, MangaPage } from "../../../shared/types";
import type { FontWeightAvailability } from "../lib/overlayLayout";
import { OUTPUT_RENDER_OPTIONS } from "../lib/outputRenderOptions";
import { renderPageToPngDataUrl } from "../lib/pageRender";
import {
  buildResultReportHtml,
  createResultReportRow,
  cropLoadedImageDataUrl,
  loadReportImage,
  resolveBlockReportRects,
  type ResultReportRow
} from "../lib/resultReport";

type UseResultReportOptions = {
  currentChapterRef: React.RefObject<ChapterSnapshot | null>;
  dirty: boolean;
  fontWeightAvailability: readonly FontWeightAvailability[];
  jobActive: boolean;
  pushStatus: (line: string) => void;
  renderBusy: boolean;
  saveNow: () => Promise<void>;
};

type UseResultReportState = {
  canOpenLastResultReport: boolean;
  generateResultReport: () => Promise<void>;
  openLastResultReport: () => void;
  reportBusy: boolean;
  reportProgress: ResultReportProgress | null;
};

export type ResultReportProgress = {
  current: number;
  total: number;
  label: string;
};

export function useResultReport({
  currentChapterRef,
  dirty,
  fontWeightAvailability,
  jobActive,
  pushStatus,
  renderBusy,
  saveNow
}: UseResultReportOptions): UseResultReportState {
  const [reportBusy, setReportBusy] = React.useState(false);
  const [reportProgress, setReportProgress] = React.useState<ResultReportProgress | null>(null);
  const [lastReport, setLastReport] = React.useState<{ html: string } | null>(null);

  const generateResultReport = React.useCallback(async () => {
    const initialChapter = currentChapterRef.current;
    if (!initialChapter || initialChapter.pages.length === 0 || reportBusy || renderBusy || jobActive) {
      return;
    }

    setReportBusy(true);
    setReportProgress({ current: 0, total: countReportPages(initialChapter), label: "준비 중" });
    try {
      if (dirty) {
        setReportProgress((current) => current ? { ...current, label: "변경사항 저장 중" } : current);
        await saveNow();
      }

      const chapter = currentChapterRef.current;
      if (!chapter || chapter.pages.length === 0) {
        return;
      }

      const reportPages = chapter.pages
        .map((page, pageIndex) => ({ page, pageIndex: pageIndex + 1 }))
        .filter(({ page }) => page.blocks.length > 0);
      setReportProgress({ current: 0, total: reportPages.length, label: "보고서 생성 시작" });
      pushStatus(`결과 보고서 생성 시작: ${reportPages.length}p`);
      const rows: ResultReportRow[] = [];
      let rowIndex = 1;
      for (const [reportPageIndex, { page, pageIndex }] of reportPages.entries()) {
        const current = reportPageIndex + 1;
        setReportProgress({
          current: reportPageIndex,
          total: reportPages.length,
          label: `${page.name} 원본 불러오는 중`
        });

        const sourceDataUrl = await window.mangaApi.resolveImageDataUrl(page.dataUrl);
        setReportProgress({
          current: reportPageIndex,
          total: reportPages.length,
          label: `${page.name} 최종 아웃풋 렌더 중`
        });
        const outputDataUrl = await renderPageToPngDataUrl(page, {
          ...OUTPUT_RENDER_OPTIONS,
          fontWeightAvailability
        });
        setReportProgress({
          current: reportPageIndex,
          total: reportPages.length,
          label: `${page.name} 블록 이미지 자르는 중`
        });
        const pageRows = await buildPageReportRows({
          page: {
            ...page,
            dataUrl: sourceDataUrl
          },
          sourceDataUrl,
          outputDataUrl,
          pageIndex,
          firstRowIndex: rowIndex,
          fontPresets: chapter.fontPresets ?? []
        });
        rowIndex += pageRows.length;
        rows.push(...pageRows);
        setReportProgress({
          current,
          total: reportPages.length,
          label: `${page.name} 완료`
        });
      }

      const generatedAt = new Date().toLocaleString("ko-KR");
      const html = buildResultReportHtml({
        chapterTitle: chapter.title,
        generatedAt,
        rows
      });
      setLastReport({ html });
      pushStatus(`결과 보고서 생성 완료: ${rows.length}개 블록. 마지막 보고서 확인 버튼으로 열 수 있습니다.`);
    } catch (error) {
      console.error(error);
      pushStatus(error instanceof Error ? error.message : "결과 보고서 생성에 실패했습니다.");
    } finally {
      setReportProgress(null);
      setReportBusy(false);
    }
  }, [currentChapterRef, dirty, fontWeightAvailability, jobActive, pushStatus, renderBusy, reportBusy, saveNow]);

  const openLastResultReport = React.useCallback(() => {
    if (!lastReport) {
      return;
    }
    openReportHtml(lastReport.html);
  }, [lastReport]);

  return {
    canOpenLastResultReport: Boolean(lastReport),
    generateResultReport,
    openLastResultReport,
    reportBusy,
    reportProgress
  };
}

function countReportPages(chapter: ChapterSnapshot): number {
  return chapter.pages.filter((page) => page.blocks.length > 0).length;
}

async function buildPageReportRows(options: {
  firstRowIndex: number;
  fontPresets: NonNullable<ChapterSnapshot["fontPresets"]>;
  outputDataUrl: string;
  page: MangaPage;
  pageIndex: number;
  sourceDataUrl: string;
}): Promise<ResultReportRow[]> {
  const { firstRowIndex, fontPresets, outputDataUrl, page, pageIndex, sourceDataUrl } = options;
  const fontPresetById = new Map(fontPresets.map((preset) => [preset.id, preset]));
  const [sourceImage, outputImage] = await Promise.all([
    loadReportImage(sourceDataUrl),
    loadReportImage(outputDataUrl)
  ]);
  const rows: ResultReportRow[] = [];

  for (const [blockIndex, block] of page.blocks.entries()) {
    const { sourceRect, outputRect } = resolveBlockReportRects(page, block);
    const sourceCropDataUrl = cropLoadedImageDataUrl(sourceImage, sourceRect);
    const outputCropDataUrl = cropLoadedImageDataUrl(outputImage, outputRect);
    rows.push(createResultReportRow({
      block,
      blockIndex: blockIndex + 1,
      fontPreset: block.fontPresetId ? fontPresetById.get(block.fontPresetId) : undefined,
      outputCropDataUrl,
      outputRect,
      page,
      pageIndex,
      rowIndex: firstRowIndex + rows.length,
      sourceCropDataUrl,
      sourceRect
    }));
  }

  return rows;
}

function openReportHtml(html: string): void {
  const blob = new Blob([html], { type: "text/html;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  window.open(url, "_blank");
  window.setTimeout(() => URL.revokeObjectURL(url), 60_000);
}
