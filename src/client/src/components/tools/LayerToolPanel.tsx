import React from "react";
import type {
  ChapterSnapshot,
  FontPreset,
  FontPresetBackupSnapshot,
  FontPresetBackupSummary,
  FontSizePreset,
  ImageRect,
  MangaPage,
  TranslationBlock
} from "../../../../shared/types";
import type { FontFamilyOption } from "../font/FontFamilyPicker";
import type { InpaintTool } from "../InpaintLayerCanvas";
import type { InpaintResultTool } from "../InpaintResultCanvas";
import type { BlockFontPatch, LinkableFontPresetKey } from "../../lib/fontPresets";
import type { ActiveLayer, LayerVisibility } from "../../lib/layerState";
import { FontToolSection } from "./FontToolSection";
import { InpaintMaskToolSection } from "./InpaintMaskToolSection";
import { InpaintPsdToolSection } from "./InpaintPsdToolSection";
import { InpaintResultToolSection } from "./InpaintResultToolSection";
import type { LayerToolFontControlValues } from "./LayerToolPanelTypes";

export type { LayerToolFontControlValues } from "./LayerToolPanelTypes";

type LayerToolPanelProps = {
  activeLayer: ActiveLayer;
  activeFontSizePresetId: string | null;
  currentChapter: ChapterSnapshot | null;
  editingFontPresetId: string | null;
  favoriteFontPresetIds: string[];
  fontControlValues: LayerToolFontControlValues | null;
  fontFamilyOptions: FontFamilyOption[];
  fontPresetName: string;
  fontPresets: FontPreset[];
  fontSizePresets: FontSizePreset[];
  inpaintBrushSize: number;
  inpaintBusy: boolean;
  inpaintPsdBusy: boolean;
  inpaintResultBrushColor: string;
  inpaintResultBrushHardness: number;
  inpaintResultBrushSize: number;
  inpaintResultTool: InpaintResultTool;
  inpaintResultToolStrength: number;
  inpaintSelectionRect: ImageRect | null;
  inpaintTool: InpaintTool;
  lastImportedInpaintPsdAt: string | null;
  lastImportedInpaintPsdLabel: string | null;
  layerVisibility: LayerVisibility;
  rangeToolActive: boolean;
  renderFontPresetLinkButton: (key: LinkableFontPresetKey, label: string) => React.ReactNode;
  renderFontPresetLinkGroupButton: (keys: LinkableFontPresetKey[], label: string) => React.ReactNode;
  selectedBlock: TranslationBlock | null;
  selectedPage: MangaPage | null;
  selectedPageEditLocked: boolean;
  onClearEditingFontPreset: () => void;
  onClearInpaintMask: () => void;
  onClearInpaintResult: () => void;
  onClearSelectedBlockFontPreset: () => void;
  onCreateFontPreset: () => void;
  onCreateFontPresetListBackup: (name: string) => Promise<FontPresetBackupSnapshot | null>;
  onCreateFontSizePreset: () => void;
  onDeleteFontPresetBackup: (backupId: string) => Promise<FontPresetBackupSummary[]>;
  onDeleteFontPreset: (presetId: string) => void;
  onDeleteFontSizePreset: (presetId: string) => void;
  onDownloadLastImportedInpaintPsd: () => void | Promise<void>;
  onExportInpaintPsd: () => void | Promise<void>;
  onFillSelectedInpaintSelection: () => void | Promise<void>;
  onFavoriteFontPresetToggle: (presetId: string) => void;
  onFontPresetNameChange: (value: string) => void;
  onFontPresetRename: (presetId: string, name: string) => void;
  onFontSettingChange: (patch: BlockFontPatch) => void;
  onInpaintBrushSizeChange: (value: number) => void;
  onInpaintResultBrushColorChange: (value: string) => void;
  onInpaintResultBrushHardnessChange: (value: number) => void;
  onInpaintResultBrushSizeChange: (value: number) => void;
  onInpaintResultToolStrengthChange: (value: number) => void;
  onInpaintSelectionClear: () => void;
  onRerunInpaintForSelection: () => void | Promise<void>;
  onRerunInpaintWithCurrentMask: () => void | Promise<void>;
  onListFontPresetBackups: () => Promise<FontPresetBackupSummary[]>;
  onRestoreFontPresetListBackup: (backupId: string) => Promise<void>;
  onSelectFontPreset: (presetId: string) => void;
  onSelectFontSizePreset: (presetId: string | null) => void;
  onSelectInpaintPsdFile: () => void;
  onSelectInpaintResultEditTool: (tool: Exclude<InpaintResultTool, "select">) => void;
  onSelectSharedInpaintTool: (tool: InpaintTool) => void;
};

export function LayerToolPanel({
  activeLayer,
  activeFontSizePresetId,
  currentChapter,
  editingFontPresetId,
  favoriteFontPresetIds,
  fontControlValues,
  fontFamilyOptions,
  fontPresetName,
  fontPresets,
  fontSizePresets,
  inpaintBrushSize,
  inpaintBusy,
  inpaintPsdBusy,
  inpaintResultBrushColor,
  inpaintResultBrushHardness,
  inpaintResultBrushSize,
  inpaintResultTool,
  inpaintResultToolStrength,
  inpaintSelectionRect,
  inpaintTool,
  lastImportedInpaintPsdAt,
  lastImportedInpaintPsdLabel,
  layerVisibility,
  rangeToolActive,
  renderFontPresetLinkButton,
  renderFontPresetLinkGroupButton,
  selectedBlock,
  selectedPage,
  selectedPageEditLocked,
  onClearEditingFontPreset,
  onClearInpaintMask,
  onClearInpaintResult,
  onClearSelectedBlockFontPreset,
  onCreateFontPreset,
  onCreateFontPresetListBackup,
  onCreateFontSizePreset,
  onDeleteFontPresetBackup,
  onDeleteFontPreset,
  onDeleteFontSizePreset,
  onDownloadLastImportedInpaintPsd,
  onExportInpaintPsd,
  onFillSelectedInpaintSelection,
  onFavoriteFontPresetToggle,
  onFontPresetNameChange,
  onFontPresetRename,
  onFontSettingChange,
  onInpaintBrushSizeChange,
  onInpaintResultBrushColorChange,
  onInpaintResultBrushHardnessChange,
  onInpaintResultBrushSizeChange,
  onInpaintResultToolStrengthChange,
  onInpaintSelectionClear,
  onRerunInpaintForSelection,
  onRerunInpaintWithCurrentMask,
  onListFontPresetBackups,
  onRestoreFontPresetListBackup,
  onSelectFontPreset,
  onSelectFontSizePreset,
  onSelectInpaintPsdFile,
  onSelectInpaintResultEditTool,
  onSelectSharedInpaintTool
}: LayerToolPanelProps): React.JSX.Element {
  return (
    <section className="layer-tool-panel layer-tools-panel">
      <h2>{resolveLayerToolTitle(activeLayer)}</h2>
      {activeLayer === "overlay" ? (
        <FontToolSection
          currentChapter={currentChapter}
          editingFontPresetId={editingFontPresetId}
          activeFontSizePresetId={activeFontSizePresetId}
          favoriteFontPresetIds={favoriteFontPresetIds}
          fontControlValues={fontControlValues}
          fontFamilyOptions={fontFamilyOptions}
          fontPresetName={fontPresetName}
          fontPresets={fontPresets}
          fontSizePresets={fontSizePresets}
          renderFontPresetLinkButton={renderFontPresetLinkButton}
          renderFontPresetLinkGroupButton={renderFontPresetLinkGroupButton}
          selectedBlock={selectedBlock}
          selectedPageEditLocked={selectedPageEditLocked}
          onClearEditingFontPreset={onClearEditingFontPreset}
          onClearSelectedBlockFontPreset={onClearSelectedBlockFontPreset}
          onCreateFontPreset={onCreateFontPreset}
          onCreateFontPresetListBackup={onCreateFontPresetListBackup}
          onCreateFontSizePreset={onCreateFontSizePreset}
          onDeleteFontPresetBackup={onDeleteFontPresetBackup}
          onDeleteFontPreset={onDeleteFontPreset}
          onDeleteFontSizePreset={onDeleteFontSizePreset}
          onFavoriteFontPresetToggle={onFavoriteFontPresetToggle}
          onFontPresetNameChange={onFontPresetNameChange}
          onFontPresetRename={onFontPresetRename}
          onFontSettingChange={onFontSettingChange}
          onListFontPresetBackups={onListFontPresetBackups}
          onRestoreFontPresetListBackup={onRestoreFontPresetListBackup}
          onSelectFontPreset={onSelectFontPreset}
          onSelectFontSizePreset={onSelectFontSizePreset}
        />
      ) : activeLayer === "inpaintMask" ? (
        <InpaintMaskToolSection
          inpaintBrushSize={inpaintBrushSize}
          inpaintBusy={inpaintBusy}
          inpaintSelectionRect={inpaintSelectionRect}
          inpaintTool={inpaintTool}
          layerVisibility={layerVisibility}
          rangeToolActive={rangeToolActive}
          selectedPage={selectedPage}
          selectedPageEditLocked={selectedPageEditLocked}
          onClearInpaintMask={onClearInpaintMask}
          onFillSelectedInpaintSelection={onFillSelectedInpaintSelection}
          onInpaintBrushSizeChange={onInpaintBrushSizeChange}
          onInpaintSelectionClear={onInpaintSelectionClear}
          onRerunInpaintForSelection={onRerunInpaintForSelection}
          onSelectSharedInpaintTool={onSelectSharedInpaintTool}
        />
      ) : activeLayer === "image" ? (
        <p className="muted-line">원본 이미지 레이어에는 사용할 도구가 없습니다.</p>
      ) : activeLayer === "inpaint" ? (
        <InpaintPsdToolSection
          currentChapter={currentChapter}
          inpaintPsdBusy={inpaintPsdBusy}
          lastImportedInpaintPsdAt={lastImportedInpaintPsdAt}
          lastImportedInpaintPsdLabel={lastImportedInpaintPsdLabel}
          selectedPage={selectedPage}
          selectedPageEditLocked={selectedPageEditLocked}
          onDownloadLastImportedInpaintPsd={onDownloadLastImportedInpaintPsd}
          onExportInpaintPsd={onExportInpaintPsd}
          onSelectInpaintPsdFile={onSelectInpaintPsdFile}
        />
      ) : activeLayer === "inpaintResult" ? (
        <InpaintResultToolSection
          inpaintBusy={inpaintBusy}
          inpaintResultBrushColor={inpaintResultBrushColor}
          inpaintResultBrushHardness={inpaintResultBrushHardness}
          inpaintResultBrushSize={inpaintResultBrushSize}
          inpaintResultTool={inpaintResultTool}
          inpaintResultToolStrength={inpaintResultToolStrength}
          inpaintSelectionRect={inpaintSelectionRect}
          layerVisibility={layerVisibility}
          rangeToolActive={rangeToolActive}
          selectedPage={selectedPage}
          selectedPageEditLocked={selectedPageEditLocked}
          onClearInpaintResult={onClearInpaintResult}
          onFillSelectedInpaintSelection={onFillSelectedInpaintSelection}
          onInpaintResultBrushColorChange={onInpaintResultBrushColorChange}
          onInpaintResultBrushHardnessChange={onInpaintResultBrushHardnessChange}
          onInpaintResultBrushSizeChange={onInpaintResultBrushSizeChange}
          onInpaintResultToolStrengthChange={onInpaintResultToolStrengthChange}
          onInpaintSelectionClear={onInpaintSelectionClear}
          onRerunInpaintForSelection={onRerunInpaintForSelection}
          onRerunInpaintWithCurrentMask={onRerunInpaintWithCurrentMask}
          onSelectInpaintResultEditTool={onSelectInpaintResultEditTool}
          onSelectSharedInpaintTool={onSelectSharedInpaintTool}
        />
      ) : (
        <p className="muted-line">최종 아웃풋 레이어에는 사용할 도구가 없습니다.</p>
      )}
    </section>
  );
}

function resolveLayerToolTitle(activeLayer: ActiveLayer): string {
  return activeLayer === "overlay"
    ? "폰트 설정"
    : activeLayer === "inpaintMask"
      ? "마스크 도구"
      : activeLayer === "inpaintResult"
        ? "결과 레이어 도구"
        : "도구";
}
