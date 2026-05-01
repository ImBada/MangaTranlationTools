import React from "react";
import type { SystemFont } from "../../../../shared/types";
import { DEFAULT_OVERLAY_FONT_FAMILY } from "../../lib/overlayLayout";

export type FontFamilyOption = {
  label: string;
  value: string;
};

const FONT_FAMILY_OPTIONS: FontFamilyOption[] = [
  { label: "맑은 고딕", value: DEFAULT_OVERLAY_FONT_FAMILY },
  { label: "Apple SD Gothic Neo", value: "\"Apple SD Gothic Neo\", \"Malgun Gothic\", sans-serif" },
  { label: "본고딕", value: "\"Noto Sans CJK KR\", \"Noto Sans KR\", \"Malgun Gothic\", sans-serif" },
  { label: "바탕", value: "Batang, \"AppleMyungjo\", serif" },
  { label: "돋움", value: "Dotum, \"Apple SD Gothic Neo\", sans-serif" }
];

type FontFamilyPickerProps = {
  options: FontFamilyOption[];
  value: string;
  disabled: boolean;
  onChange: (value: string) => void;
};

export function buildFontFamilyOptions(systemFonts: SystemFont[], selectedFontFamily?: string): FontFamilyOption[] {
  const options = new Map<string, FontFamilyOption>();
  for (const option of FONT_FAMILY_OPTIONS) {
    options.set(option.value, option);
  }
  for (const font of systemFonts) {
    if (!options.has(font.cssFamily)) {
      options.set(font.cssFamily, { label: font.family, value: font.cssFamily });
    }
  }
  if (selectedFontFamily && !options.has(selectedFontFamily)) {
    options.set(selectedFontFamily, { label: selectedFontFamily, value: selectedFontFamily });
  }
  return [...options.values()];
}

export function FontFamilyPicker({ options, value, disabled, onChange }: FontFamilyPickerProps): React.JSX.Element {
  const [open, setOpen] = React.useState(false);
  const [query, setQuery] = React.useState("");
  const rootRef = React.useRef<HTMLDivElement | null>(null);
  const inputRef = React.useRef<HTMLInputElement | null>(null);
  const selectedOption = options.find((option) => option.value === value) ?? { label: value, value };
  const normalizedQuery = normalizeFontSearchText(query);
  const filteredOptions = normalizedQuery
    ? options.filter((option) => normalizeFontSearchText(option.label).includes(normalizedQuery))
    : options;

  React.useEffect(() => {
    if (!open) {
      return;
    }

    const handlePointerDown = (event: PointerEvent) => {
      if (!rootRef.current?.contains(event.target as Node)) {
        setOpen(false);
      }
    };
    window.addEventListener("pointerdown", handlePointerDown);
    window.setTimeout(() => inputRef.current?.focus(), 0);
    return () => window.removeEventListener("pointerdown", handlePointerDown);
  }, [open]);

  return (
    <div className="font-picker" ref={rootRef}>
      <button
        type="button"
        className="font-picker-trigger"
        disabled={disabled}
        aria-haspopup="listbox"
        aria-expanded={open}
        onClick={() => setOpen((current) => !current)}
      >
        <span className="font-picker-current-name">{selectedOption.label}</span>
        <span className="font-picker-current-preview" style={{ fontFamily: selectedOption.value }}>
          번역 미리보기 Aa
        </span>
      </button>
      {open ? (
        <div className="font-picker-popover">
          <input
            ref={inputRef}
            className="font-picker-search"
            type="search"
            value={query}
            placeholder="폰트 검색"
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                setOpen(false);
              }
            }}
          />
          <div className="font-picker-count">{filteredOptions.length.toLocaleString()}개</div>
          <div className="font-picker-list" role="listbox">
            {filteredOptions.length > 0 ? (
              filteredOptions.map((option) => (
                <button
                  key={option.value}
                  type="button"
                  className={option.value === value ? "font-picker-option active" : "font-picker-option"}
                  role="option"
                  aria-selected={option.value === value}
                  onClick={() => {
                    onChange(option.value);
                    setOpen(false);
                    setQuery("");
                  }}
                >
                  <span className="font-picker-option-name">{option.label}</span>
                  <span className="font-picker-option-preview" style={{ fontFamily: option.value }}>
                    오늘의 번역 Aa 123
                  </span>
                </button>
              ))
            ) : (
              <div className="font-picker-empty">검색 결과 없음</div>
            )}
          </div>
        </div>
      ) : null}
    </div>
  );
}

function normalizeFontSearchText(value: string): string {
  return value.normalize("NFKC").toLocaleLowerCase().replace(/\s+/g, "");
}
