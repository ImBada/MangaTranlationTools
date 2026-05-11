import React from "react";
import type {
  ChapterSnapshot,
  FontPreset,
  FontPresetBackupSnapshot,
  FontPresetBackupSummary,
  FontSizePreset,
  ImageRect,
  MangaPage,
  TranslationBlock,
  TranslationBlockGroup,
  TranslationBlockGroupEffect
} from "../../../../shared/types";
import type { FontFamilyOption } from "../font/FontFamilyPicker";
import type { InpaintTool } from "../InpaintLayerCanvas";
import type { InpaintResultTool } from "../InpaintResultCanvas";
import type { BlockFontPatch, LinkableFontPresetKey } from "../../lib/fontPresets";
import type { ActiveLayer, LayerVisibility } from "../../lib/layerState";
import { DEFAULT_OVERLAY_FONT_FAMILY } from "../../lib/overlayLayout";
import { FontCharacterOverrideModal } from "./FontCharacterOverrideModal";
import { FontToolSection } from "./FontToolSection";
import { InpaintMaskToolSection } from "./InpaintMaskToolSection";
import { InpaintPsdToolSection } from "./InpaintPsdToolSection";
import { InpaintResultToolSection } from "./InpaintResultToolSection";
import { OutputToolSection } from "./OutputToolSection";
import { TranslationBlockGroupToolSection } from "./TranslationBlockGroupToolSection";
import type { ResultReportProgress } from "../../hooks/useResultReport";
import type { LayerToolFontControlValues } from "./LayerToolPanelTypes";

export type { LayerToolFontControlValues } from "./LayerToolPanelTypes";

type LayerToolPanelProps = {
  activeLayer: ActiveLayer;
  activeFontSizePresetId: string | null;
  canOpenLastResultReport: boolean;
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
  jobActive: boolean;
  lastImportedInpaintPsdAt: string | null;
  lastImportedInpaintPsdLabel: string | null;
  layerVisibility: LayerVisibility;
  rangeToolActive: boolean;
  reportBusy: boolean;
  reportProgress: ResultReportProgress | null;
  renderBusy: boolean;
  renderFontPresetLinkButton: (key: LinkableFontPresetKey, label: string) => React.ReactNode;
  renderFontPresetLinkGroupButton: (keys: LinkableFontPresetKey[], label: string) => React.ReactNode;
  selectedBlock: TranslationBlock | null;
  selectedBlockGroup: TranslationBlockGroup | null;
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
  onGenerateResultReport: () => void | Promise<void>;
  onOpenLastResultReport: () => void;
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
  onSelectedBlockGroupEffectsChange: (effects: TranslationBlockGroupEffect[]) => void;
};

export function LayerToolPanel({
  activeLayer,
  activeFontSizePresetId,
  canOpenLastResultReport,
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
  jobActive,
  lastImportedInpaintPsdAt,
  lastImportedInpaintPsdLabel,
  layerVisibility,
  rangeToolActive,
  reportBusy,
  reportProgress,
  renderBusy,
  renderFontPresetLinkButton,
  renderFontPresetLinkGroupButton,
  selectedBlock,
  selectedBlockGroup,
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
  onGenerateResultReport,
  onOpenLastResultReport,
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
  onSelectSharedInpaintTool,
  onSelectedBlockGroupEffectsChange
}: LayerToolPanelProps): React.JSX.Element {
  const [characterOverrideModalOpen, setCharacterOverrideModalOpen] = React.useState(false);
  const groupSelectionActive = activeLayer === "overlay" && selectedBlockGroup !== null;
  const layerToolTitle = resolveLayerToolTitle(activeLayer, groupSelectionActive);
  const activeFontPresetId = selectedBlock?.fontPresetId ?? (!selectedBlock ? editingFontPresetId : null);
  const activeFontPresetName = fontPresets.find((preset) => preset.id === activeFontPresetId)?.name ?? "선택한";
  const characterOverrideButtonEnabled = activeLayer === "overlay" && !groupSelectionActive && Boolean(activeFontPresetId && fontControlValues);

  React.useEffect(() => {
    if (!characterOverrideButtonEnabled) {
      setCharacterOverrideModalOpen(false);
    }
  }, [characterOverrideButtonEnabled]);

  return (
    <section className="layer-tool-panel layer-tools-panel">
      <h2>
        <span>{layerToolTitle}</span>
        {activeLayer === "overlay" && !groupSelectionActive ? (
          <button
            type="button"
            className="font-character-settings-button"
            disabled={!characterOverrideButtonEnabled}
            aria-label="커스텀 문자 설정"
            title={characterOverrideButtonEnabled ? "커스텀 문자 설정" : "폰트 프리셋 선택 후 설정 가능"}
            onClick={() => setCharacterOverrideModalOpen(true)}
          >
            <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
              <circle cx="12" cy="12" r="3.2" />
              <path d="M19.4 13.5a7.8 7.8 0 0 0 0-3l2-1.2-2-3.4-2.2.9a8 8 0 0 0-2.6-1.5L14.3 3h-4.6l-.3 2.3a8 8 0 0 0-2.6 1.5l-2.2-.9-2 3.4 2 1.2a7.8 7.8 0 0 0 0 3l-2 1.2 2 3.4 2.2-.9a8 8 0 0 0 2.6 1.5l.3 2.3h4.6l.3-2.3a8 8 0 0 0 2.6-1.5l2.2.9 2-3.4-2-1.2Z" />
            </svg>
          </button>
        ) : null}
      </h2>
      {activeLayer === "overlay" && selectedBlockGroup ? (
        <TranslationBlockGroupToolSection
          selectedBlockGroup={selectedBlockGroup}
          selectedPageEditLocked={selectedPageEditLocked}
          onEffectsChange={onSelectedBlockGroupEffectsChange}
        />
      ) : activeLayer === "overlay" ? (
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
        <OutputToolSection
          currentChapter={currentChapter}
          jobActive={jobActive}
          canOpenLastResultReport={canOpenLastResultReport}
          reportBusy={reportBusy}
          reportProgress={reportProgress}
          renderBusy={renderBusy}
          onGenerateResultReport={onGenerateResultReport}
          onOpenLastResultReport={onOpenLastResultReport}
        />
      )}
      {characterOverrideModalOpen && fontControlValues ? (
        <FontCharacterOverrideModal
          defaultFontFamily={fontControlValues.fontFamily ?? DEFAULT_OVERLAY_FONT_FAMILY}
          disabled={selectedPageEditLocked}
          fontFamilyOptions={fontFamilyOptions}
          overrides={fontControlValues.characterFontOverrides ?? []}
          presetName={activeFontPresetName}
          onCancel={() => setCharacterOverrideModalOpen(false)}
          onSubmit={(characterFontOverrides) => {
            onFontSettingChange({ characterFontOverrides });
            setCharacterOverrideModalOpen(false);
          }}
        />
      ) : null}
    </section>
  );
}

function resolveLayerToolTitle(activeLayer: ActiveLayer, groupSelectionActive = false): string {
  return activeLayer === "overlay"
    ? groupSelectionActive ? "그룹 설정" : "폰트 설정"
    : activeLayer === "inpaintMask"
      ? "마스크 도구"
      : activeLayer === "inpaintResult"
        ? "결과 레이어 도구"
        : activeLayer === "output"
          ? "최종 아웃풋 도구"
          : "도구";
}
