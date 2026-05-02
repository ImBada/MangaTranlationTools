import React from "react";

export function useStatusFeedback(): {
  appendStatusLine: (line: string) => void;
  pushStatus: (line: string) => void;
  resetStatusLog: () => void;
  saveFlash: boolean;
  signalSaveComplete: () => void;
  statusLines: string[];
  statusToastLine: string | null;
  statusWidgetOpen: boolean;
  setStatusWidgetOpen: React.Dispatch<React.SetStateAction<boolean>>;
} {
  const [statusLines, setStatusLines] = React.useState<string[]>([]);
  const [statusToastLine, setStatusToastLine] = React.useState<string | null>(null);
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

  const appendStatusLine = React.useCallback((line: string) => {
    const next = line.trim();
    if (!next) {
      return;
    }
    setStatusToastLine(next);
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
  }, [clearStatusToastTimer]);

  const pushStatus = React.useCallback(
    (line: string) => {
      void window.mangaApi.writeLog("info", "UI status", { line });
      appendStatusLine(line);
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
    statusWidgetOpen,
    setStatusWidgetOpen
  };
}
