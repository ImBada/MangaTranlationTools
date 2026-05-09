import React from "react";
import type { FontCharacterOverride } from "../../../../shared/types";
import { FontFamilyPicker, type FontFamilyOption } from "../font/FontFamilyPicker";
import { normalizeCharacterFontOverrides, normalizeCharacterOverrideCharacter } from "../../lib/fontPresets";

type FontCharacterOverrideModalProps = {
  defaultFontFamily: string;
  disabled: boolean;
  fontFamilyOptions: FontFamilyOption[];
  overrides: FontCharacterOverride[];
  presetName: string;
  onCancel: () => void;
  onSubmit: (overrides: FontCharacterOverride[]) => void;
};

export function FontCharacterOverrideModal({
  defaultFontFamily,
  disabled,
  fontFamilyOptions,
  overrides,
  presetName,
  onCancel,
  onSubmit
}: FontCharacterOverrideModalProps): React.JSX.Element {
  const [draftOverrides, setDraftOverrides] = React.useState<FontCharacterOverride[]>(() => normalizeCharacterFontOverrides(overrides));
  const [newCharacter, setNewCharacter] = React.useState("");
  const [newFontFamily, setNewFontFamily] = React.useState(defaultFontFamily);
  const [error, setError] = React.useState("");

  const pickerOptions = React.useMemo(() => {
    const optionByValue = new Map(fontFamilyOptions.map((option) => [option.value, option]));
    for (const override of draftOverrides) {
      if (!optionByValue.has(override.fontFamily)) {
        optionByValue.set(override.fontFamily, { label: override.fontFamily, value: override.fontFamily });
      }
    }
    if (newFontFamily && !optionByValue.has(newFontFamily)) {
      optionByValue.set(newFontFamily, { label: newFontFamily, value: newFontFamily });
    }
    return [...optionByValue.values()];
  }, [draftOverrides, fontFamilyOptions, newFontFamily]);

  const addOverride = React.useCallback(() => {
    const character = normalizeCharacterOverrideCharacter(newCharacter);
    if (!character) {
      setError("문자를 한 글자 입력하세요.");
      return;
    }

    setDraftOverrides((current) => {
      const next = current.filter((override) => override.character !== character);
      return [...next, { character, fontFamily: newFontFamily }];
    });
    setNewCharacter("");
    setError("");
  }, [newCharacter, newFontFamily]);

  const updateOverride = React.useCallback((index: number, patch: Partial<FontCharacterOverride>) => {
    setDraftOverrides((current) =>
      current.map((override, candidateIndex) =>
        candidateIndex === index
          ? {
              ...override,
              ...patch,
              character: patch.character !== undefined ? normalizeCharacterOverrideCharacter(patch.character) : override.character
            }
          : override
      )
    );
    setError("");
  }, []);

  const removeOverride = React.useCallback((index: number) => {
    setDraftOverrides((current) => current.filter((_, candidateIndex) => candidateIndex !== index));
    setError("");
  }, []);

  const submit = React.useCallback(() => {
    const normalized = normalizeCharacterFontOverrides(draftOverrides);
    onSubmit(normalized);
  }, [draftOverrides, onSubmit]);

  return (
    <div className="modal-backdrop">
      <div className="modal-card font-character-modal">
        <div className="modal-header">
          <h2>커스텀 문자</h2>
          <button type="button" className="ghost-button font-character-close" onClick={onCancel} aria-label="닫기">
            ×
          </button>
        </div>
        <section className="modal-section">
          <div className="font-character-preset-name">
            <span>프리셋</span>
            <strong>{presetName}</strong>
          </div>
          <div className="font-character-add-row">
            <label className="font-character-field">
              <span>문자</span>
              <input
                value={newCharacter}
                disabled={disabled}
                aria-label="추가할 커스텀 문자"
                onChange={(event) => setNewCharacter(normalizeCharacterOverrideCharacter(event.target.value))}
                onKeyDown={(event) => {
                  if (event.key === "Enter") {
                    event.preventDefault();
                    addOverride();
                  }
                }}
              />
            </label>
            <label className="font-character-font-field">
              <span>폰트</span>
              <FontFamilyPicker
                options={pickerOptions}
                value={newFontFamily}
                disabled={disabled}
                onChange={setNewFontFamily}
              />
            </label>
            <button type="button" disabled={disabled} onClick={addOverride}>
              추가
            </button>
          </div>
          <div className="font-character-list" aria-label="커스텀 문자 목록">
            {draftOverrides.length > 0 ? (
              draftOverrides.map((override, index) => (
                <div key={`${override.character}-${index}`} className="font-character-row">
                  <label className="font-character-field">
                    <span>문자</span>
                    <input
                      value={override.character}
                      disabled={disabled}
                      aria-label={`${override.character} 문자`}
                      onChange={(event) => updateOverride(index, { character: event.target.value })}
                    />
                  </label>
                  <div className="font-character-preview" style={{ fontFamily: override.fontFamily }} aria-hidden>
                    {override.character}
                  </div>
                  <label className="font-character-font-field">
                    <span>폰트</span>
                    <FontFamilyPicker
                      options={pickerOptions}
                      value={override.fontFamily}
                      disabled={disabled}
                      onChange={(fontFamily) => updateOverride(index, { fontFamily })}
                    />
                  </label>
                  <button type="button" className="font-character-remove" disabled={disabled} onClick={() => removeOverride(index)}>
                    삭제
                  </button>
                </div>
              ))
            ) : (
              <p className="muted-line modal-note">등록된 커스텀 문자가 없습니다.</p>
            )}
          </div>
          {error ? <p className="muted-line modal-note font-character-error">{error}</p> : null}
        </section>
        <div className="modal-actions">
          <button type="button" onClick={onCancel}>
            취소
          </button>
          <button type="button" disabled={disabled} onClick={submit}>
            저장
          </button>
        </div>
      </div>
    </div>
  );
}
