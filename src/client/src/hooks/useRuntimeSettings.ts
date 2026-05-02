import React from "react";
import type { AppSettings, LamaRuntimeStatus, SystemFont } from "../../../shared/types";
import {
  FORCE_INCOMPLETE_LAMA_NOTICE,
  LAMA_TEST_INSTALL_GUIDE,
  type LamaNoticePlatform
} from "../lib/lamaRuntimeNotice";

type RuntimeSettingsOptions = {
  pushStatus: (line: string) => void;
};

type RuntimeSettingsState = {
  displayedLamaStatus: LamaRuntimeStatus | null;
  downloadLamaModelFromEmptyState: () => Promise<void>;
  lamaActionBusy: boolean;
  lamaActionMessage: string | null;
  lamaNoticePlatform: LamaNoticePlatform;
  openSettings: () => Promise<void>;
  prepareLamaFromEmptyState: () => Promise<void>;
  refreshLamaStatus: () => Promise<LamaRuntimeStatus>;
  resetSettings: () => Promise<void>;
  settings: AppSettings | null;
  settingsBusy: boolean;
  settingsOpen: boolean;
  setLamaNoticePlatform: React.Dispatch<React.SetStateAction<LamaNoticePlatform>>;
  setSettingsOpen: React.Dispatch<React.SetStateAction<boolean>>;
  showLamaEmptyNotice: boolean;
  submitSettings: (nextSettings: AppSettings) => Promise<void>;
  systemFonts: SystemFont[];
};

export function useRuntimeSettings({ pushStatus }: RuntimeSettingsOptions): RuntimeSettingsState {
  const [settings, setSettings] = React.useState<AppSettings | null>(null);
  const [settingsOpen, setSettingsOpen] = React.useState(false);
  const [settingsBusy, setSettingsBusy] = React.useState(false);
  const [lamaStatus, setLamaStatus] = React.useState<LamaRuntimeStatus | null>(null);
  const [lamaActionBusy, setLamaActionBusy] = React.useState(false);
  const [lamaActionMessage, setLamaActionMessage] = React.useState<string | null>(null);
  const [lamaNoticePlatform, setLamaNoticePlatform] = React.useState<LamaNoticePlatform>("win32");
  const [systemFonts, setSystemFonts] = React.useState<SystemFont[]>([]);

  const refreshSettings = React.useCallback(async () => {
    const next = await window.mangaApi.getSettings();
    setSettings(next);
    return next;
  }, []);

  const refreshLamaStatus = React.useCallback(async () => {
    const next = await window.mangaApi.getLamaRuntimeStatus();
    setLamaStatus(next);
    return next;
  }, []);

  React.useEffect(() => {
    void refreshSettings().catch((error) => {
      console.error(error);
    });
  }, [refreshSettings]);

  React.useEffect(() => {
    void refreshLamaStatus().catch((error) => {
      console.error(error);
    });
  }, [refreshLamaStatus]);

  React.useEffect(() => {
    if (!lamaStatus?.runtimePreparing && !lamaStatus?.modelDownloading) {
      return;
    }
    const interval = window.setInterval(() => {
      void refreshLamaStatus().catch((error) => {
        console.error(error);
      });
    }, 2500);
    return () => window.clearInterval(interval);
  }, [lamaStatus?.modelDownloading, lamaStatus?.runtimePreparing, refreshLamaStatus]);

  React.useEffect(() => {
    void window.mangaApi
      .getSystemFonts()
      .then(setSystemFonts)
      .catch((error) => {
        console.error(error);
      });
  }, []);

  const openSettings = React.useCallback(async () => {
    if (settings) {
      setSettingsOpen(true);
      return;
    }

    setSettingsBusy(true);
    try {
      await refreshSettings();
      setSettingsOpen(true);
    } catch (error) {
      console.error(error);
      pushStatus("설정을 불러오지 못했습니다.");
    } finally {
      setSettingsBusy(false);
    }
  }, [pushStatus, refreshSettings, settings]);

  const submitSettings = React.useCallback(async (nextSettings: AppSettings) => {
    setSettingsBusy(true);
    try {
      const saved = await window.mangaApi.saveSettings(nextSettings);
      setSettings(saved);
      setSettingsOpen(false);
      pushStatus("설정을 저장했습니다. 다음 번 번역 실행부터 적용됩니다.");
    } catch (error) {
      console.error(error);
      pushStatus("설정을 저장하지 못했습니다.");
    } finally {
      setSettingsBusy(false);
    }
  }, [pushStatus]);

  const resetSettings = React.useCallback(async () => {
    setSettingsBusy(true);
    try {
      const reset = await window.mangaApi.resetSettings();
      setSettings(reset);
      pushStatus("설정을 기본값으로 복원했습니다. 다음 번 번역 실행부터 적용됩니다.");
    } catch (error) {
      console.error(error);
      pushStatus("기본 설정을 복원하지 못했습니다.");
    } finally {
      setSettingsBusy(false);
    }
  }, [pushStatus]);

  const prepareLamaFromEmptyState = React.useCallback(async () => {
    if (FORCE_INCOMPLETE_LAMA_NOTICE) {
      setLamaActionMessage("테스트 표시 모드입니다. 실제 환경 준비는 실행하지 않습니다.");
      return;
    }
    setLamaActionBusy(true);
    setLamaActionMessage("LaMa 환경 준비를 시작합니다.");
    try {
      const next = await window.mangaApi.prepareLamaRuntime();
      setLamaStatus(next);
      setLamaActionMessage(next.pythonAvailable ? "LaMa 환경 준비 중입니다." : `Python 설치가 필요합니다: ${next.pythonInstallCommand}`);
    } catch (error) {
      console.error(error);
      setLamaActionMessage(error instanceof Error ? error.message : String(error));
    } finally {
      setLamaActionBusy(false);
    }
  }, []);

  const downloadLamaModelFromEmptyState = React.useCallback(async () => {
    if (FORCE_INCOMPLETE_LAMA_NOTICE) {
      setLamaActionMessage("테스트 표시 모드입니다. 실제 모델 다운로드는 실행하지 않습니다.");
      return;
    }
    setLamaActionBusy(true);
    setLamaActionMessage("LaMa 모델 다운로드를 시작합니다.");
    try {
      const next = await window.mangaApi.downloadLamaModel();
      setLamaStatus(next);
      setLamaActionMessage(next.modelExists ? "LaMa 모델이 준비되어 있습니다." : "LaMa 모델 다운로드 중입니다.");
    } catch (error) {
      console.error(error);
      setLamaActionMessage(error instanceof Error ? error.message : String(error));
    } finally {
      setLamaActionBusy(false);
    }
  }, []);

  const displayedLamaStatus = React.useMemo(() => {
    if (!FORCE_INCOMPLETE_LAMA_NOTICE || !lamaStatus) {
      return lamaStatus;
    }
    return {
      ...lamaStatus,
      pythonAvailable: false,
      pythonInstallCommand: LAMA_TEST_INSTALL_GUIDE[lamaNoticePlatform].command,
      pythonInstallHelp: LAMA_TEST_INSTALL_GUIDE[lamaNoticePlatform].help,
      runtimeReady: false,
      runtimePreparing: false,
      modelExists: false,
      modelDownloading: false
    };
  }, [lamaNoticePlatform, lamaStatus]);

  const showLamaEmptyNotice = Boolean(
    displayedLamaStatus && !(displayedLamaStatus.pythonAvailable && displayedLamaStatus.runtimeReady && displayedLamaStatus.modelExists)
  );

  return {
    displayedLamaStatus,
    downloadLamaModelFromEmptyState,
    lamaActionBusy,
    lamaActionMessage,
    lamaNoticePlatform,
    openSettings,
    prepareLamaFromEmptyState,
    refreshLamaStatus,
    resetSettings,
    settings,
    settingsBusy,
    settingsOpen,
    setLamaNoticePlatform,
    setSettingsOpen,
    showLamaEmptyNotice,
    submitSettings,
    systemFonts
  };
}
