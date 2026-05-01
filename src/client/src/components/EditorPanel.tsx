import React from "react";
import { resolveBlockRotationDeg } from "../../../shared/geometry";
import type { RenderTextDirection, TranslationBlock } from "../../../shared/types";
import { rangeProgressStyle } from "../lib/rangeProgressStyle";

type EditorPanelProps = {
  block: TranslationBlock | null;
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
        <select value={block.type} disabled={disabled} onChange={(event) => onUpdate({ type: event.target.value as TranslationBlock["type"] })}>
          <option value="speech">speech</option>
          <option value="sfx">sfx</option>
          <option value="caption">caption</option>
          <option value="other">other</option>
        </select>
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
        <select
          value={block.renderDirection}
          disabled={disabled}
          onChange={(event) => onUpdate({ renderDirection: event.target.value as RenderTextDirection })}
        >
          <option value="horizontal">horizontal</option>
          <option value="vertical">vertical</option>
          <option value="hidden">hidden</option>
        </select>
      </label>
      <div className="rotation-control grid gap-1.5 text-xs font-semibold text-soft">
        <span>회전</span>
        <div className="rotation-row">
          <input
            type="range"
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
      <div className="padding-row">
        <label className="grid gap-1.5 text-xs font-semibold text-soft">
          패딩
          <input
            type="number"
            min="0"
            max="80"
            step="1"
            value={block.textPaddingPx ?? ""}
            placeholder="자동"
            disabled={disabled}
            onChange={(event) =>
              onUpdate({
                textPaddingPx: event.target.value === "" ? undefined : Number(event.target.value)
              })
            }
          />
        </label>
        <button type="button" disabled={disabled || block.textPaddingPx === undefined} onClick={() => onUpdate({ textPaddingPx: undefined })}>
          자동
        </button>
      </div>
      <div className="color-row grid grid-cols-2 gap-2.5">
        <label className="grid gap-1.5 text-xs font-semibold text-soft">
          배경색
          <input
            type="color"
            value={block.backgroundColor}
            disabled={disabled}
            onChange={(event) => onUpdate({ backgroundColor: event.target.value })}
          />
        </label>
      </div>
      <div className="block-actions">
        <button className="primary block-action" onClick={onApplyInpaint} disabled={disabled}>인페인트 실행</button>
        <button className="block-action" onClick={onDuplicate} disabled={disabled}>복제</button>
        <button className="danger block-action" onClick={onDelete} disabled={disabled}>삭제</button>
      </div>
    </section>
  );
}
