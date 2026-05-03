import React from "react";
import { AppModals } from "./AppModals";

type AppModalsProps = React.ComponentProps<typeof AppModals>;

type AppModalsSlotProps = Omit<AppModalsProps, "onCancelImport" | "onCancelRename" | "onCancelSettings"> & {
  onCloseImport: () => void;
  onCloseRename: () => void;
  onCloseSettings: () => void;
};

export function AppModalsSlot({
  onCloseImport,
  onCloseRename,
  onCloseSettings,
  ...props
}: AppModalsSlotProps): React.JSX.Element {
  return (
    <AppModals
      {...props}
      onCancelImport={onCloseImport}
      onCancelRename={onCloseRename}
      onCancelSettings={onCloseSettings}
    />
  );
}
