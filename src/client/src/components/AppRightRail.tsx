import React from "react";
import type { MangaPage, TranslationBlock } from "../../../shared/types";
import { EditorPanel } from "./EditorPanel";
import { LayerPanel } from "./layers/LayerPanel";

type LayerPanelProps = React.ComponentProps<typeof LayerPanel>;
type EditorPanelProps = React.ComponentProps<typeof EditorPanel>;

type AppRightRailProps = LayerPanelProps & {
  block: TranslationBlock | null;
  fontPresetName?: string;
  inpaintBusy: boolean;
  onApplyBatchInpaint: () => void | Promise<void>;
  onApplyInpaint: () => void | Promise<void>;
  onCreate: EditorPanelProps["onCreate"];
  onDelete: EditorPanelProps["onDelete"];
  onDuplicate: EditorPanelProps["onDuplicate"];
  onUpdate: EditorPanelProps["onUpdate"];
  selectedPage: MangaPage | null;
  selectedPageEditLocked: boolean;
};

export function AppRightRail({
  block,
  fontPresetName,
  inpaintBusy,
  onApplyBatchInpaint,
  onApplyInpaint,
  onCreate,
  onDelete,
  onDuplicate,
  onUpdate,
  selectedPage,
  selectedPageEditLocked,
  ...layerPanelProps
}: AppRightRailProps): React.JSX.Element {
  const editorDisabled = selectedPageEditLocked || inpaintBusy || !selectedPage;
  const batchInpaintDisabled = selectedPageEditLocked || inpaintBusy || !selectedPage || selectedPage.blocks.length === 0;

  return (
    <>
      <LayerPanel {...layerPanelProps} />
      <EditorPanel
        block={block}
        fontPresetName={fontPresetName}
        disabled={editorDisabled}
        onUpdate={onUpdate}
        onCreate={onCreate}
        onDelete={onDelete}
        onDuplicate={onDuplicate}
        onApplyInpaint={() => void onApplyInpaint()}
        onApplyBatchInpaint={() => void onApplyBatchInpaint()}
        batchInpaintDisabled={batchInpaintDisabled}
      />
    </>
  );
}
