import React from "react";
import type { AppSettings, ImportPreviewResult, LibraryIndex } from "../../../shared/types";
import { ImportModal, type ImportModalSubmit } from "./ImportModal";
import { RenameModal } from "./RenameModal";
import { SettingsModal } from "./SettingsModal";

type RenameTarget =
  | {
      kind: "work";
      id: string;
      title: string;
    }
  | {
      kind: "chapter";
      id: string;
      title: string;
    };

type AppModalsProps = {
  importBusy: boolean;
  importPreview: ImportPreviewResult | null;
  jobActive: boolean;
  library: LibraryIndex;
  renameBusy: boolean;
  renameTarget: RenameTarget | null;
  settings: AppSettings | null;
  settingsBusy: boolean;
  settingsOpen: boolean;
  onCancelImport: () => void;
  onCancelRename: () => void;
  onCancelSettings: () => void;
  onDeleteRenameTarget: () => void | Promise<unknown>;
  onResetSettings: () => void | Promise<unknown>;
  onSubmitImport: (payload: ImportModalSubmit) => void | Promise<unknown>;
  onSubmitRename: (title: string) => void | Promise<unknown>;
  onSubmitSettings: (nextSettings: AppSettings) => void | Promise<unknown>;
};

export function AppModals({
  importBusy,
  importPreview,
  jobActive,
  library,
  renameBusy,
  renameTarget,
  settings,
  settingsBusy,
  settingsOpen,
  onCancelImport,
  onCancelRename,
  onCancelSettings,
  onDeleteRenameTarget,
  onResetSettings,
  onSubmitImport,
  onSubmitRename,
  onSubmitSettings
}: AppModalsProps): React.JSX.Element {
  return (
    <>
      {importPreview ? (
        <ImportModal
          library={library}
          preview={importPreview}
          busy={importBusy}
          onCancel={onCancelImport}
          onSubmit={(payload) => void onSubmitImport(payload)}
        />
      ) : null}

      {renameTarget ? (
        <RenameModal
          kind={renameTarget.kind}
          initialTitle={renameTarget.title}
          busy={renameBusy}
          onCancel={() => {
            if (!renameBusy) {
              onCancelRename();
            }
          }}
          onDelete={() => void onDeleteRenameTarget()}
          onSubmit={(title) => void onSubmitRename(title)}
        />
      ) : null}

      {settingsOpen && settings ? (
        <SettingsModal
          initialSettings={settings}
          busy={settingsBusy}
          jobActive={jobActive}
          onCancel={() => {
            if (!settingsBusy) {
              onCancelSettings();
            }
          }}
          onReset={() => void onResetSettings()}
          onSubmit={(nextSettings) => void onSubmitSettings(nextSettings)}
        />
      ) : null}
    </>
  );
}
