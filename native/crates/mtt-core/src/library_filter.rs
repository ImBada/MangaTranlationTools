//! Library search (`libraryFilter.ts`).

use unicode_normalization::UnicodeNormalization;

use crate::library::{ChapterSummary, WorkSummary};

/// `filterLibraryIndex`: a work whose title matches keeps all its chapters; any other work keeps
/// only its matching chapters and is dropped when none match. A blank query returns everything.
pub fn filter_library(works: &[WorkSummary], query: &str) -> Vec<WorkSummary> {
    let normalized_query = normalize_search_text(query);
    if normalized_query.is_empty() {
        return works.to_vec();
    }
    works
        .iter()
        .filter_map(|summary| {
            if matches_search(&summary.work.title, &normalized_query) {
                return Some(summary.clone());
            }
            let chapters: Vec<ChapterSummary> = summary
                .chapters
                .iter()
                .filter(|chapter| matches_search(&chapter.title, &normalized_query))
                .cloned()
                .collect();
            (!chapters.is_empty()).then(|| WorkSummary { work: summary.work.clone(), chapters })
        })
        .collect()
}

/// `matchesSearch`: whether `value` contains a query already passed through [`normalize_search_text`].
pub fn matches_search(value: &str, normalized_query: &str) -> bool {
    normalize_search_text(value).contains(normalized_query)
}

/// `normalizeSearchText`: `value.normalize("NFKC").trim().toLocaleLowerCase()`.
pub fn normalize_search_text(value: &str) -> String {
    let composed: String = value.nfkc().collect();
    composed.trim_matches(is_js_trim_whitespace).to_lowercase()
}

/// Characters removed by JS `String.prototype.trim` (WhiteSpace + LineTerminator). Unlike
/// `str::trim` this includes U+FEFF and excludes U+0085.
fn is_js_trim_whitespace(ch: char) -> bool {
    matches!(
        ch,
        '\u{9}'..='\u{D}'
            | ' '
            | '\u{A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ChapterStatus, Work};

    #[test]
    fn search_text_is_nfkc_trimmed_lowercase() {
        assert_eq!(normalize_search_text("  ＯｎｅＰｉｅｃｅ　"), "onepiece");
        assert_eq!(normalize_search_text("\u{FEFF}Ch ①\n"), "ch 1");
        assert_eq!(normalize_search_text("ﾊﾟ"), "パ");
        assert_eq!(normalize_search_text("\u{85}A"), "\u{85}a", "JS trim keeps U+0085");
        assert_eq!(normalize_search_text("ΌΣΟΣ"), "όσος", "final sigma like JS toLowerCase");
        assert_eq!(normalize_search_text(" \u{3000} "), "");
    }

    fn summary(title: &str, chapters: &[&str]) -> WorkSummary {
        let work = Work { id: title.into(), title: title.into(), ..Work::default() };
        let chapters = chapters
            .iter()
            .map(|chapter| ChapterSummary {
                id: (*chapter).into(),
                work_id: work.id.clone(),
                title: (*chapter).into(),
                status: ChapterStatus::Idle,
                page_count: 0,
                completed_page_count: 0,
                created_at: String::new(),
                updated_at: String::new(),
            })
            .collect();
        WorkSummary { work, chapters }
    }

    fn titles(works: &[WorkSummary]) -> Vec<(String, Vec<String>)> {
        works
            .iter()
            .map(|summary| {
                (summary.work.title.clone(), summary.chapters.iter().map(|chapter| chapter.title.clone()).collect())
            })
            .collect()
    }

    #[test]
    fn works_match_by_title_or_matching_chapters() {
        let works = [summary("원피스", &["1화", "2화"]), summary("Naruto", &["Ch 1", "원피스 콜라보 특별편"])];
        let pair = |title: &str, chapters: &[&str]| {
            (title.to_owned(), chapters.iter().map(|chapter| (*chapter).to_owned()).collect::<Vec<_>>())
        };

        assert_eq!(
            titles(&filter_library(&works, " 원피스 ")),
            [pair("원피스", &["1화", "2화"]), pair("Naruto", &["원피스 콜라보 특별편"])]
        );
        assert_eq!(
            titles(&filter_library(&works, "ＮＡＲＵＴＯ")),
            [pair("Naruto", &["Ch 1", "원피스 콜라보 특별편"])]
        );
        assert_eq!(titles(&filter_library(&works, "ch")), [pair("Naruto", &["Ch 1"])]);
        assert!(filter_library(&works, "zzz").is_empty());
        assert_eq!(filter_library(&works, " \t"), works);
    }
}
