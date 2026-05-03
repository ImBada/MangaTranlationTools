import type { RecoverableFailure } from "./useRecoverableFailures";
import type { JobState, MangaPage } from "../../../shared/types";

export function useAppStatusState({
  dirty,
  jobActive,
  jobState,
  recoverableFailures,
  rerunInpaintWithCurrentMask,
  saveFlash,
  selectedPage
}: {
  dirty: boolean;
  jobActive: boolean;
  jobState: JobState;
  recoverableFailures: RecoverableFailure[];
  rerunInpaintWithCurrentMask: () => Promise<void>;
  saveFlash: boolean;
  selectedPage: MangaPage | null;
}) {
  const saveStatusTone = saveFlash ? "saved" : dirty ? "unsaved" : "synced";
  const saveStatusLabel = saveFlash ? "저장 완료" : dirty ? "저장되지 않은 변경 있음" : "최신 상태";
  const statusWidgetTone = `${jobState.status} ${recoverableFailures.length ? "failed" : saveStatusTone}`;
  const statusIndicatorLabel = jobActive ? jobState.progressText : saveStatusLabel;
  const selectedPageInpaintNotice =
    selectedPage?.inpaintStatus === "running"
      ? { tone: "running" as const, title: "인페인트 중", message: selectedPage.name }
      : selectedPage?.inpaintStatus === "failed"
        ? {
            tone: "failed" as const,
            title: "인페인트 실패",
            message: `${selectedPage.name} - 마스크와 레이어 상태는 유지됨`,
            actionLabel: "다시 실행",
            onAction: rerunInpaintWithCurrentMask
          }
        : null;

  return {
    selectedPageInpaintNotice,
    statusIndicatorLabel,
    statusWidgetTone
  };
}
