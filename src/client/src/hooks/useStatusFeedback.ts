import React from "react";

export type StatusToastTone = "default" | "failed";

export function useStatusFeedback(): {
  appendStatusLine: (line: string) => void;
  pushStatus: (line: string, tone?: StatusToastTone) => void;
  resetStatusLog: () => void;
  saveFlash: boolean;
  signalSaveComplete: () => void;
  statusLines: string[];
  statusToastLine: string | null;
  statusToastTone: StatusToastTone;
  statusWidgetOpen: boolean;
  setStatusWidgetOpen: React.Dispatch<React.SetStateAction<boolean>>;
} {
  const [statusLines, setStatusLines] = React.useState<string[]>([]);
  const [statusToastLine, setStatusToastLine] = React.useState<string | null>(null);
  const [statusToastTone, setStatusToastTone] = React.useState<StatusToastTone>("default");
  const [saveFlash, setSaveFlash] = React.useState(false);
  const [statusWidgetOpen, setStatusWidgetOpen] = React.useState(false);
  const saveFlashTimerRef = React.useRef<number | null>(null);
  const statusToastTimerRef = React.useRef<number | null>(null);

  const clearStatusToastTimer = React.useCallback(() => {
    if (statusToastTimerRef.current) {
      window.clearTimeout(statusToastTimerRef.current);
      statusToastTimerRef.current = null;
    }
  }, []);

  const appendStatusLine = React.useCallback((line: string, tone: StatusToastTone = "default") => {
    const next = line.trim();
    if (!next) {
      return;
    }
    setStatusToastLine(next);
    setStatusToastTone(tone);
    clearStatusToastTimer();
    statusToastTimerRef.current = window.setTimeout(() => {
      setStatusToastLine(null);
      statusToastTimerRef.current = null;
    }, 4000);
    setStatusLines((lines) => {
      if (lines[0] === next) {
        return lines;
      }
      return [next, ...lines].slice(0, 16);
    });
  }, [clearStatusToastTimer]);

  const signalSaveComplete = React.useCallback(() => {
    if (saveFlashTimerRef.current) {
      window.clearTimeout(saveFlashTimerRef.current);
    }
    setSaveFlash(true);
    saveFlashTimerRef.current = window.setTimeout(() => {
      saveFlashTimerRef.current = null;
      setSaveFlash(false);
    }, 1200);
  }, []);

  const resetStatusLog = React.useCallback(() => {
    clearStatusToastTimer();
    setStatusLines([]);
    setStatusToastLine(null);
    setStatusToastTone("default");
  }, [clearStatusToastTimer]);

  const pushStatus = React.useCallback(
    (line: string, tone: StatusToastTone = "default") => {
      void window.mangaApi.writeLog("info", "UI status", { line });
      appendStatusLine(line, tone);
    },
    [appendStatusLine]
  );

  React.useEffect(() => {
    return () => {
      if (saveFlashTimerRef.current) {
        window.clearTimeout(saveFlashTimerRef.current);
      }
      clearStatusToastTimer();
    };
  }, [clearStatusToastTimer]);

  return {
    appendStatusLine,
    pushStatus,
    resetStatusLog,
    saveFlash,
    signalSaveComplete,
    statusLines,
    statusToastLine,
    statusToastTone,
    statusWidgetOpen,
    setStatusWidgetOpen
  };
}
