import React from "react";
import type { SystemFont } from "../../../../shared/types";
import { DEFAULT_OVERLAY_FONT_FAMILY } from "../../lib/overlayLayout";

export type FontFamilyOption = {
  label: string;
  value: string;
  weights?: number[];
};

type FontLanguageGroup = "ko" | "en" | "ja" | "other";

type FontFamilyOptionGroup = {
  group: FontLanguageGroup;
  options: FontFamilyOption[];
};

const FONT_FAMILY_OPTIONS: FontFamilyOption[] = [
  { label: "맑은 고딕", value: DEFAULT_OVERLAY_FONT_FAMILY },
  { label: "Apple SD Gothic Neo", value: "\"Apple SD Gothic Neo\", \"Malgun Gothic\", sans-serif" },
  { label: "본고딕", value: "\"Noto Sans CJK KR\", \"Noto Sans KR\", \"Malgun Gothic\", sans-serif" },
  { label: "바탕", value: "Batang, \"AppleMyungjo\", serif" },
  { label: "돋움", value: "Dotum, \"Apple SD Gothic Neo\", sans-serif" }
];

const RECENT_FONT_FAMILY_STORAGE_KEY = "manga-translation-tools.recent-font-families";
const RECENT_FONT_FAMILY_LIMIT = 5;
const FONT_LANGUAGE_GROUP_LABELS: Record<FontLanguageGroup, string> = {
  ko: "한국어",
  en: "영어",
  ja: "일본어",
  other: "기타"
};
const FONT_LANGUAGE_GROUP_ORDER: FontLanguageGroup[] = ["ko", "en", "ja", "other"];
const FONT_LANGUAGE_GROUP_RANK: Record<FontLanguageGroup, number> = {
  ko: 0,
  en: 1,
  ja: 2,
  other: 3
};

type FontFamilyPickerProps = {
  options: FontFamilyOption[];
  value: string;
  disabled: boolean;
  onChange: (value: string) => void;
};

export function buildFontFamilyOptions(systemFonts: SystemFont[], selectedFontFamily?: string): FontFamilyOption[] {
  const options = new Map<string, FontFamilyOption>();
  for (const option of FONT_FAMILY_OPTIONS) {
    options.set(option.value, { ...option });
  }
  for (const font of systemFonts) {
    const existing = options.get(font.cssFamily);
    if (existing) {
      existing.weights ??= font.weights;
    } else {
      options.set(font.cssFamily, { label: font.family, value: font.cssFamily, weights: font.weights });
    }
  }
  if (selectedFontFamily && !options.has(selectedFontFamily)) {
    options.set(selectedFontFamily, { label: selectedFontFamily, value: selectedFontFamily });
  }
  return [...options.values()].sort(compareFontFamilyOptions);
}

export function FontFamilyPicker({ options, value, disabled, onChange }: FontFamilyPickerProps): React.JSX.Element {
  const [open, setOpen] = React.useState(false);
  const [query, setQuery] = React.useState("");
  const [recentFontFamilies, setRecentFontFamilies] = React.useState<FontFamilyOption[]>(readRecentFontFamilies);
  const rootRef = React.useRef<HTMLDivElement | null>(null);
  const inputRef = React.useRef<HTMLInputElement | null>(null);
  const selectedOption = options.find((option) => option.value === value) ?? { label: value, value };
  const normalizedQuery = normalizeFontSearchText(query);
  const recentOptions = React.useMemo(() => {
    const optionByValue = new Map(options.map((option) => [option.value, option]));
    return recentFontFamilies.map((recent) => optionByValue.get(recent.value) ?? recent);
  }, [options, recentFontFamilies]);
  const filteredRecentOptions = normalizedQuery
    ? recentOptions.filter((option) => getFontOptionSearchText(option).includes(normalizedQuery))
    : recentOptions;
  const filteredOptions = normalizedQuery
    ? options.filter((option) => getFontOptionSearchText(option).includes(normalizedQuery))
    : options;
  const listedOptions = [...filteredRecentOptions, ...filteredOptions];
  const filteredOptionGroups = React.useMemo(() => groupFontFamilyOptions(filteredOptions), [filteredOptions]);

  const selectFontFamily = React.useCallback((option: FontFamilyOption) => {
    const nextRecentFontFamilies = [
      option,
      ...recentFontFamilies.filter((recent) => recent.value !== option.value)
    ].slice(0, RECENT_FONT_FAMILY_LIMIT);

    setRecentFontFamilies(nextRecentFontFamilies);
    writeRecentFontFamilies(nextRecentFontFamilies);
    onChange(option.value);
    setOpen(false);
    setQuery("");
  }, [onChange, recentFontFamilies]);

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

  const renderFontOption = React.useCallback((option: FontFamilyOption, keyPrefix: string) => (
    <button
      key={`${keyPrefix}-${option.value}`}
      type="button"
      className={option.value === value ? "font-picker-option active" : "font-picker-option"}
      role="option"
      aria-selected={option.value === value}
      onClick={() => selectFontFamily(option)}
    >
      <span className="font-picker-option-name">{option.label}</span>
      <span className="font-picker-option-preview" style={{ fontFamily: option.value }}>
        오늘의 번역 Aa 123
      </span>
    </button>
  ), [selectFontFamily, value]);

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
          <div className="font-picker-count">{listedOptions.length.toLocaleString()}개</div>
          <div className="font-picker-list" role="listbox">
            {listedOptions.length > 0 ? (
              <>
                {filteredRecentOptions.map((option) => renderFontOption(option, "recent"))}
                {filteredRecentOptions.length > 0 && filteredOptions.length > 0 ? <div className="font-picker-divider" role="separator" /> : null}
                {filteredOptionGroups.map((group) => (
                  <React.Fragment key={group.group}>
                    <div className="font-picker-group-divider" role="separator">
                      <span>{FONT_LANGUAGE_GROUP_LABELS[group.group]}</span>
                    </div>
                    {group.options.map((option) => renderFontOption(option, "all"))}
                  </React.Fragment>
                ))}
              </>
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

function getFontOptionSearchText(option: FontFamilyOption): string {
  return normalizeFontSearchText(`${option.label} ${option.value}`);
}

function groupFontFamilyOptions(options: FontFamilyOption[]): FontFamilyOptionGroup[] {
  return FONT_LANGUAGE_GROUP_ORDER.map((group) => ({
    group,
    options: options.filter((option) => detectFontLanguageGroup(option) === group)
  })).filter((group) => group.options.length > 0);
}

function compareFontFamilyOptions(a: FontFamilyOption, b: FontFamilyOption): number {
  const languageRank = getFontLanguageRank(a) - getFontLanguageRank(b);
  if (languageRank !== 0) {
    return languageRank;
  }
  return a.label.localeCompare(b.label, undefined, { sensitivity: "base", numeric: true });
}

function getFontLanguageRank(option: FontFamilyOption): number {
  return FONT_LANGUAGE_GROUP_RANK[detectFontLanguageGroup(option)];
}

function detectFontLanguageGroup(option: FontFamilyOption): FontLanguageGroup {
  const searchableName = `${option.label} ${option.value}`.normalize("NFKC").toLocaleLowerCase();

  if (
    /[\u1100-\u11ff\u3130-\u318f\uac00-\ud7af]/.test(searchableName) ||
    /\b(?:kr|korean|korea|hangul|hangeul|malgun|batang|dotum|gulim|gungsuh|nanum|kopub|pretendard|spoqa|seoul|jeju|hamchorom)\b/.test(searchableName) ||
    searchableName.includes("apple sd gothic") ||
    searchableName.includes("noto sans cjk kr") ||
    searchableName.includes("noto serif cjk kr") ||
    searchableName.includes("source han sans k") ||
    searchableName.includes("source han serif k")
  ) {
    return "ko";
  }

  if (
    /[\u3040-\u30ff]/.test(searchableName) ||
    /\b(?:jp|japanese|japan|jis|hiragino|meiryo|osaka|kozuka)\b/.test(searchableName) ||
    searchableName.includes("yu gothic") ||
    searchableName.includes("yu mincho") ||
    searchableName.includes("noto sans cjk jp") ||
    searchableName.includes("noto serif cjk jp") ||
    searchableName.includes("source han sans j") ||
    searchableName.includes("source han serif j")
  ) {
    return "ja";
  }

  if (/^[\u0000-\u007f]+$/.test(searchableName)) {
    return "en";
  }

  return "other";
}

function readRecentFontFamilies(): FontFamilyOption[] {
  if (typeof window === "undefined") {
    return [];
  }

  try {
    const rawValue = window.localStorage.getItem(RECENT_FONT_FAMILY_STORAGE_KEY);
    if (!rawValue) {
      return [];
    }
    const parsedValue = JSON.parse(rawValue);
    if (!Array.isArray(parsedValue)) {
      return [];
    }
    return parsedValue
      .filter((option): option is FontFamilyOption => (
        option &&
        typeof option.label === "string" &&
        typeof option.value === "string" &&
        option.value.trim().length > 0
      ))
      .slice(0, RECENT_FONT_FAMILY_LIMIT);
  } catch {
    return [];
  }
}

function writeRecentFontFamilies(options: FontFamilyOption[]): void {
  if (typeof window === "undefined") {
    return;
  }

  try {
    window.localStorage.setItem(RECENT_FONT_FAMILY_STORAGE_KEY, JSON.stringify(options.slice(0, RECENT_FONT_FAMILY_LIMIT)));
  } catch {
    // Storage failure should not block font selection.
  }
}
