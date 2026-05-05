import React from "react";
import type { MangaPage } from "../../../shared/types";
import { collectFindReplaceMatches, type FindReplaceMatch } from "../lib/findReplace";

type FindReplaceModalProps = {
  pages: MangaPage[];
  replaceDisabled: boolean;
  onCancel: () => void;
  onFocusMatch: (pageId: string, blockId: string) => void;
  onReplaceAll: (keyword: string, replacement: string) => void;
  onReplaceOne: (pageId: string, blockId: string, keyword: string, replacement: string) => void;
};

export function FindReplaceModal({
  pages,
  replaceDisabled,
  onCancel,
  onFocusMatch,
  onReplaceAll,
  onReplaceOne
}: FindReplaceModalProps): React.JSX.Element {
  const [keyword, setKeyword] = React.useState("");
  const [replacement, setReplacement] = React.useState("");
  const [searchedKeyword, setSearchedKeyword] = React.useState<string | null>(null);
  const keywordInputRef = React.useRef<HTMLInputElement | null>(null);

  React.useEffect(() => {
    keywordInputRef.current?.focus();
  }, []);

  React.useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") {
        return;
      }
      event.preventDefault();
      onCancel();
    };

    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [onCancel]);

  const matches = React.useMemo(
    () => (searchedKeyword === null ? [] : collectFindReplaceMatches(pages, searchedKeyword, replacement)),
    [pages, replacement, searchedKeyword]
  );
  const changeableMatches = React.useMemo(
    () => matches.filter((match) => match.before !== match.after),
    [matches]
  );
  const totalOccurrences = React.useMemo(
    () => matches.reduce((total, match) => total + match.occurrenceCount, 0),
    [matches]
  );
  const canFind = keyword.length > 0;
  const canReplace = searchedKeyword !== null && changeableMatches.length > 0 && !replaceDisabled;

  const findMatches = React.useCallback(() => {
    if (!canFind) {
      return;
    }
    setSearchedKeyword(keyword);
  }, [canFind, keyword]);

  return (
    <div className="modal-backdrop">
      <div className="modal-card find-replace-modal">
        <div className="modal-header">
          <h2>찾아바꾸기</h2>
          <button className="ghost-button" type="button" onClick={onCancel}>
            닫기
          </button>
        </div>

        <form
          className="find-replace-form"
          onSubmit={(event) => {
            event.preventDefault();
            findMatches();
          }}
        >
          <label>
            찾을 키워드
            <input
              ref={keywordInputRef}
              value={keyword}
              onChange={(event) => setKeyword(event.target.value)}
              placeholder="검색어"
            />
          </label>
          <label>
            바꿀 내용
            <input
              value={replacement}
              onChange={(event) => setReplacement(event.target.value)}
              placeholder="새 텍스트"
            />
          </label>
          <div className="find-replace-actions">
            <button type="submit" className="primary" disabled={!canFind}>
              찾기
            </button>
            <button
              type="button"
              disabled={!canReplace}
              onClick={() => {
                if (searchedKeyword !== null) {
                  onReplaceAll(searchedKeyword, replacement);
                }
              }}
            >
              모두 바꾸기
            </button>
          </div>
        </form>

        {replaceDisabled ? (
          <p className="muted-line modal-note">번역 작업이 진행 중이라 바꾸기를 사용할 수 없습니다.</p>
        ) : null}

        {searchedKeyword !== null ? (
          <section className="find-replace-section" aria-label="찾아바꾸기 결과">
            <div className="find-replace-summary">
              <strong>{matches.length}개 문장</strong>
              <span>{totalOccurrences}개 일치</span>
            </div>
            {matches.length ? (
              <div className="find-replace-results">
                {matches.map((match) => (
                  <FindReplaceResult
                    key={match.id}
                    match={match}
                    keyword={searchedKeyword}
                    replacement={replacement}
                    replaceDisabled={replaceDisabled || match.before === match.after}
                    onFocusMatch={onFocusMatch}
                    onReplaceOne={onReplaceOne}
                  />
                ))}
              </div>
            ) : (
              <p className="panel-empty">일치하는 문장이 없습니다.</p>
            )}
          </section>
        ) : null}
      </div>
    </div>
  );
}

function FindReplaceResult({
  match,
  keyword,
  replacement,
  replaceDisabled,
  onFocusMatch,
  onReplaceOne
}: {
  match: FindReplaceMatch;
  keyword: string;
  replacement: string;
  replaceDisabled: boolean;
  onFocusMatch: (pageId: string, blockId: string) => void;
  onReplaceOne: (pageId: string, blockId: string, keyword: string, replacement: string) => void;
}): React.JSX.Element {
  return (
    <article className="find-replace-result">
      <div className="find-replace-result-header">
        <strong>{match.pageName}</strong>
        <span>블록 {match.blockIndex + 1} · {match.occurrenceCount}회</span>
      </div>
      <div className="find-replace-compare">
        <TextPreview label="수정 전" text={match.before} highlight={keyword} />
        <TextPreview label="수정 후" text={match.after} highlight={replacement} />
      </div>
      <div className="find-replace-row-actions">
        <button type="button" className="ghost-button" onClick={() => onFocusMatch(match.pageId, match.blockId)}>
          보기
        </button>
        <button
          type="button"
          className="primary"
          disabled={replaceDisabled}
          onClick={() => onReplaceOne(match.pageId, match.blockId, keyword, replacement)}
        >
          바꾸기
        </button>
      </div>
    </article>
  );
}

function TextPreview({
  label,
  text,
  highlight
}: {
  label: string;
  text: string;
  highlight: string;
}): React.JSX.Element {
  return (
    <div className="find-replace-preview">
      <span>{label}</span>
      <p>{highlight ? <HighlightedText text={text} highlight={highlight} /> : text}</p>
    </div>
  );
}

function HighlightedText({ text, highlight }: { text: string; highlight: string }): React.JSX.Element {
  const parts = text.split(highlight);
  return (
    <>
      {parts.map((part, index) => (
        <React.Fragment key={`${part}-${index}`}>
          {index > 0 ? <mark>{highlight}</mark> : null}
          {part}
        </React.Fragment>
      ))}
    </>
  );
}
