import React from "react";
import { resolveBlockRotationDeg } from "../../../shared/geometry";
import type { RenderTextDirection, TranslationBlock } from "../../../shared/types";

type EditorPanelProps = {
  block: TranslationBlock | null;
  fontPresetName?: string;
  disabled: boolean;
  onUpdate: (patch: Partial<TranslationBlock>) => void;
  onDelete: () => void;
  onDuplicate: () => void;
  onApplyInpaint: () => void;
  onApplyBatchInpaint: () => void;
  onUndoInpaint: () => void;
  batchInpaintDisabled: boolean;
  undoInpaintDisabled: boolean;
};

export function EditorPanel({
  block,
  fontPresetName,
  disabled,
  onUpdate,
  onDelete,
  onDuplicate,
  onApplyInpaint,
  onApplyBatchInpaint,
  onUndoInpaint,
  batchInpaintDisabled,
  undoInpaintDisabled
}: EditorPanelProps): React.JSX.Element {
  if (!block) {
    return (
      <section className="editor-panel muted">
        <h2>블록</h2>
        <p>블록을 선택하면 문구와 배치 방향을 바로 조정할 수 있습니다.</p>
        <button className="primary" onClick={onApplyBatchInpaint} disabled={batchInpaintDisabled}>
          전체 블록 인페인트
        </button>
        <button onClick={onUndoInpaint} disabled={undoInpaintDisabled}>
          인페인트 되돌리기
        </button>
      </section>
    );
  }

  const rotationDeg = Math.round(resolveBlockRotationDeg(block) * 10) / 10;

  return (
    <section className="editor-panel">
      <div className="block-panel-heading">
        <h2>블록</h2>
        {fontPresetName ? <span className="font-preset-tag block-preset-tag">{fontPresetName}</span> : null}
      </div>
      <label>
        종류
        <select value={block.type} disabled={disabled} onChange={(event) => onUpdate({ type: event.target.value as TranslationBlock["type"] })}>
          <option value="speech">speech</option>
          <option value="sfx">sfx</option>
          <option value="caption">caption</option>
          <option value="other">other</option>
        </select>
      </label>
      <label>
        한국어
        <textarea value={block.translatedText} disabled={disabled} onChange={(event) => onUpdate({ translatedText: event.target.value })} />
      </label>
      <label>
        OCR
        <textarea value={block.sourceText} disabled={disabled} onChange={(event) => onUpdate({ sourceText: event.target.value })} />
      </label>
      <label>
        방향
        <select
          value={block.renderDirection}
          disabled={disabled}
          onChange={(event) => onUpdate({ renderDirection: event.target.value as RenderTextDirection })}
        >
          <option value="horizontal">horizontal</option>
          <option value="vertical">vertical</option>
          <option value="rotated">rotated</option>
          <option value="hidden">hidden</option>
        </select>
      </label>
      <label>
        회전
        <input
          type="range"
          min="-180"
          max="180"
          step="1"
          value={rotationDeg}
          disabled={disabled}
          onChange={(event) => onUpdate({ rotationDeg: Number(event.target.value) })}
        />
      </label>
      <div className="rotation-row">
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
      <div className="padding-row">
        <label>
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
      <div className="color-row">
        <label>
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
        <button onClick={onApplyInpaint} disabled={disabled}>블록 인페인트 실행</button>
        <button onClick={onDuplicate} disabled={disabled}>복제</button>
        <button className="danger" onClick={onDelete} disabled={disabled}>삭제</button>
      </div>
      <button onClick={onUndoInpaint} disabled={undoInpaintDisabled}>
        인페인트 되돌리기
      </button>
    </section>
  );
}
