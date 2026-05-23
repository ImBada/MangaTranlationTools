import React from "react";
import { resolveBlockRotationDeg } from "../../../shared/geometry";
import type { RenderTextDirection, TranslationBlock } from "../../../shared/types";
import { mouseOnlyRangeInputProps } from "../lib/mouseOnlyCheckbox";
import { rangeProgressStyle } from "../lib/rangeProgressStyle";
import { CompactNumberControl } from "./controls/CompactNumberControl";
import { DropdownSelect } from "./controls/DropdownSelect";
import { FastColorPicker } from "./controls/FastColorPicker";

type EditorPanelProps = {
  block: TranslationBlock | null;
  selectedBlockCount?: number;
  fontPresetName?: string;
  disabled: boolean;
  onUpdate: (patch: Partial<TranslationBlock>) => void;
  onCreate: () => void;
  onDelete: () => void;
  onDuplicate: () => void;
  onApplyInpaint: () => void;
  onApplyBatchInpaint: () => void;
  batchInpaintDisabled: boolean;
};

export function EditorPanel({
  block,
  selectedBlockCount = 0,
  fontPresetName,
  disabled,
  onUpdate,
  onCreate,
  onDelete,
  onDuplicate,
  onApplyInpaint,
  onApplyBatchInpaint,
  batchInpaintDisabled
}: EditorPanelProps): React.JSX.Element {
  if (selectedBlockCount > 1) {
    return (
      <section className="editor-panel grid content-start gap-2.5">
        <h2>블록</h2>
        <p className="multi-block-selection-summary">{selectedBlockCount}개 블록 선택됨</p>
        <div className="block-actions multi-block-actions">
          <button className="primary block-action" onClick={onApplyInpaint} disabled={disabled}>선택된 블록 인페인트 실행</button>
          <button className="danger block-action" onClick={onDelete} disabled={disabled}>삭제</button>
        </div>
      </section>
    );
  }

  if (!block) {
    return (
      <section className="editor-panel muted grid content-start gap-2.5">
        <h2>블록</h2>
        <p>블록을 선택하면 문구와 배치 방향을 바로 조정할 수 있습니다.</p>
        <button className="primary" onClick={onCreate} disabled={disabled}>
          빈 블록 생성
        </button>
        <button className="primary" onClick={onApplyBatchInpaint} disabled={batchInpaintDisabled}>
          전체 블록 인페인트
        </button>
      </section>
    );
  }

  const rotationDeg = Math.round(resolveBlockRotationDeg(block) * 10) / 10;

  return (
    <section className="editor-panel grid content-start gap-2.5">
      <div className="block-panel-heading flex items-center justify-between gap-2">
        <h2>블록</h2>
        {fontPresetName ? <span className="font-preset-tag block-preset-tag">{fontPresetName}</span> : null}
      </div>
      <label className="grid gap-1.5 text-xs font-semibold text-soft">
        종류
        <DropdownSelect
          value={block.type}
          disabled={disabled}
          ariaLabel="블록 종류"
          options={[
            { value: "speech", label: "speech" },
            { value: "sfx", label: "sfx" },
            { value: "caption", label: "caption" },
            { value: "other", label: "other" }
          ]}
          onChange={(type) => onUpdate({ type: type as TranslationBlock["type"] })}
        />
      </label>
      <label className="grid gap-1.5 text-xs font-semibold text-soft">
        한국어
        <textarea
          data-block-text-field="translated"
          value={block.translatedText}
          disabled={disabled}
          onChange={(event) => onUpdate({ translatedText: event.target.value })}
        />
      </label>
      <label className="grid gap-1.5 text-xs font-semibold text-soft">
        OCR
        <textarea
          data-block-text-field="source"
          value={block.sourceText}
          disabled={disabled}
          onChange={(event) => onUpdate({ sourceText: event.target.value })}
        />
      </label>
      <label className="grid gap-1.5 text-xs font-semibold text-soft">
        방향
        <DropdownSelect
          value={block.renderDirection}
          disabled={disabled}
          ariaLabel="블록 방향"
          options={[
            { value: "horizontal", label: "horizontal" },
            { value: "vertical", label: "vertical" },
            { value: "hidden", label: "hidden" }
          ]}
          onChange={(renderDirection) => onUpdate({ renderDirection: renderDirection as RenderTextDirection })}
        />
      </label>
      <div className="rotation-control grid gap-1.5 text-xs font-semibold text-soft">
        <span>회전</span>
        <div className="rotation-row">
          <input
            type="range"
            {...mouseOnlyRangeInputProps}
            min="-180"
            max="180"
            step="1"
            value={rotationDeg}
            style={rangeProgressStyle(rotationDeg, -180, 180)}
            disabled={disabled}
            onChange={(event) => onUpdate({ rotationDeg: Number(event.target.value) })}
            aria-label="회전"
          />
          <input
            type="number"
            min="-180"
            max="180"
            step="1"
            value={rotationDeg}
            disabled={disabled}
            onChange={(event) => onUpdate({ rotationDeg: Number(event.target.value) })}
            aria-label="회전 각도"
          />
          <span>deg</span>
          <button type="button" disabled={disabled || rotationDeg === 0} onClick={() => onUpdate({ rotationDeg: 0 })}>
            초기화
          </button>
        </div>
      </div>
      <div className="block-appearance-row font-tool-grid">
        <div className="compact-tool-field font-number-field block-padding-field">
          <span>
            <span>패딩</span>
            <button
              type="button"
              className={`block-auto-chip ${block.textPaddingPx === undefined ? "active" : ""}`}
              disabled={disabled}
              onClick={(event) => {
                event.preventDefault();
                onUpdate({ textPaddingPx: undefined });
              }}
            >
              자동
            </button>
          </span>
          <CompactNumberControl
            ariaLabel="패딩"
            min={0}
            max={80}
            step={1}
            value={block.textPaddingPx ?? 0}
            suffix="px"
            disabled={disabled}
            onChange={(textPaddingPx) => onUpdate({ textPaddingPx })}
          />
        </div>
        <div className="compact-tool-field font-color-field">
          <span>배경색</span>
          <FastColorPicker
            ariaLabel="블록 배경색"
            value={block.backgroundColor}
            disabled={disabled}
            onChange={(backgroundColor) => onUpdate({ backgroundColor })}
          />
        </div>
      </div>
      <div className="block-actions">
        <button className="primary block-action" onClick={onApplyInpaint} disabled={disabled}>인페인트 실행</button>
        <button className="block-action" onClick={onDuplicate} disabled={disabled}>복제</button>
        <button className="danger block-action" onClick={onDelete} disabled={disabled}>삭제</button>
      </div>
    </section>
  );
}
