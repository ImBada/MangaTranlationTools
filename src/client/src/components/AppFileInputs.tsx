import React from "react";
import type { ImportSourceKind } from "../../../shared/types";

type AppFileInputsProps = {
  batchImportInputRef: React.RefObject<HTMLInputElement | null>;
  folderImportInputRef: React.RefObject<HTMLInputElement | null>;
  imageImportInputRef: React.RefObject<HTMLInputElement | null>;
  inpaintPsdInputRef: React.RefObject<HTMLInputElement | null>;
  zipImportInputRef: React.RefObject<HTMLInputElement | null>;
  onImportInputChange: (mode: ImportSourceKind, event: React.ChangeEvent<HTMLInputElement>) => void | Promise<unknown>;
  onInpaintPsdInputChange: (event: React.ChangeEvent<HTMLInputElement>) => void | Promise<unknown>;
};

export function AppFileInputs({
  batchImportInputRef,
  folderImportInputRef,
  imageImportInputRef,
  inpaintPsdInputRef,
  zipImportInputRef,
  onImportInputChange,
  onInpaintPsdInputChange
}: AppFileInputsProps): React.JSX.Element {
  return (
    <>
      <input
        ref={imageImportInputRef}
        type="file"
        accept="image/png,image/jpeg,image/webp"
        multiple
        hidden
        onChange={(event) => void onImportInputChange("images", event)}
      />
      <input
        ref={folderImportInputRef}
        type="file"
        accept="image/png,image/jpeg,image/webp"
        multiple
        hidden
        {...{ webkitdirectory: "" }}
        onChange={(event) => void onImportInputChange("folder", event)}
      />
      <input
        ref={zipImportInputRef}
        type="file"
        accept=".zip,application/zip"
        hidden
        onChange={(event) => void onImportInputChange("zip", event)}
      />
      <input
        ref={batchImportInputRef}
        type="file"
        accept="image/png,image/jpeg,image/webp,.zip,application/zip"
        multiple
        hidden
        {...{ webkitdirectory: "" }}
        onChange={(event) => void onImportInputChange("zip-folder", event)}
      />
      <input
        ref={inpaintPsdInputRef}
        type="file"
        accept=".psd,image/vnd.adobe.photoshop,application/octet-stream"
        hidden
        onChange={(event) => void onInpaintPsdInputChange(event)}
      />
    </>
  );
}
