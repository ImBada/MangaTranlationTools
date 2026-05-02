import React from "react";
import type { ChapterSnapshot, MangaPage } from "../../../../shared/types";

type InpaintPsdToolSectionProps = {
  currentChapter: ChapterSnapshot | null;
  inpaintPsdBusy: boolean;
  lastImportedInpaintPsdAt: string | null;
  lastImportedInpaintPsdLabel: string | null;
  selectedPage: MangaPage | null;
  selectedPageEditLocked: boolean;
  onDownloadLastImportedInpaintPsd: () => void | Promise<void>;
  onExportInpaintPsd: () => void | Promise<void>;
  onSelectInpaintPsdFile: () => void;
};

export function InpaintPsdToolSection({
  currentChapter,
  inpaintPsdBusy,
  lastImportedInpaintPsdAt,
  lastImportedInpaintPsdLabel,
  selectedPage,
  selectedPageEditLocked,
  onDownloadLastImportedInpaintPsd,
  onExportInpaintPsd,
  onSelectInpaintPsdFile
}: InpaintPsdToolSectionProps): React.JSX.Element {
  return (
    <>
      <div className="result-action-grid psd-action-grid">
        <button
          type="button"
          onClick={() => void onExportInpaintPsd()}
          disabled={selectedPageEditLocked || inpaintPsdBusy || !selectedPage}
        >
          PSD 내보내기
        </button>
        <button
          type="button"
          onClick={onSelectInpaintPsdFile}
          disabled={selectedPageEditLocked || inpaintPsdBusy || !selectedPage}
        >
          PSD 가져오기
        </button>
      </div>
      <button
        type="button"
        className="psd-last-import-button"
        onClick={() => void onDownloadLastImportedInpaintPsd()}
        disabled={inpaintPsdBusy || !currentChapter || !selectedPage || !lastImportedInpaintPsdAt}
      >
        마지막으로 사용한 PSD 내려받기
      </button>
      {lastImportedInpaintPsdLabel ? (
        <p className="psd-helper-line">마지막 사용: {lastImportedInpaintPsdLabel}</p>
      ) : null}
      <p className="psd-helper-line">
        글자에 레이어 효과가 들어갈 경우 임포트 전에 스마트 오브젝트로 변환한 후 임포트해야 보이는 그대로 불러올 수 있습니다.
      </p>
    </>
  );
}
