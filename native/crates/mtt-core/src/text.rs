//! Translated-text normalization applied before a chapter is saved (`textNormalization.ts`,
//! `normalizeChapterTranslatedText` in `editorUtils.ts`).

use crate::model::Chapter;
use crate::time::now_iso;

/// `normalizeKoreanText`: each run of three or more `.` becomes `…` repeated `len / 3` times
/// (leftover dots are dropped, so `....` is a single `…`), then every non-overlapping `--`
/// becomes `—`.
pub fn normalize_korean_text(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());
    let mut dots = 0;
    for ch in value.chars() {
        if ch == '.' {
            dots += 1;
            continue;
        }
        push_dot_run(&mut normalized, dots);
        dots = 0;
        normalized.push(ch);
    }
    push_dot_run(&mut normalized, dots);
    normalized.replace("--", "—")
}

fn push_dot_run(out: &mut String, count: usize) {
    if count >= 3 {
        out.extend(std::iter::repeat_n('…', count / 3));
    } else {
        out.extend(std::iter::repeat_n('.', count));
    }
}

/// `normalizeChapterTranslatedText`: applies [`normalize_korean_text`] to `translated_text` of
/// every block (no other field). Pages with a changed block, and the chapter when any page
/// changed, get one fresh `updated_at`. Returns the changed page ids in `pages` order; the save
/// call site (`useChapterSession.ts`) unions them with the editor's own dirty page ids.
pub fn normalize_chapter_translated_text(chapter: &mut Chapter) -> Vec<String> {
    let updated_at = now_iso();
    let mut dirty_page_ids = Vec::new();
    for page in &mut chapter.pages {
        let mut page_changed = false;
        for block in &mut page.blocks {
            let translated_text = normalize_korean_text(&block.translated_text);
            if translated_text != block.translated_text {
                block.translated_text = translated_text;
                page_changed = true;
            }
        }
        if page_changed {
            page.updated_at = updated_at.clone();
            dirty_page_ids.push(page.id.clone());
        }
    }
    if !dirty_page_ids.is_empty() {
        chapter.updated_at = updated_at;
    }
    dirty_page_ids
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{PageRecord, TranslationBlock};

    #[test]
    fn korean_text_punctuation() {
        let cases = [
            ("", ""),
            ("a.b", "a.b"),
            ("..", ".."),
            ("...", "…"),
            ("....", "…"),
            (".....", "…"),
            ("......", "……"),
            ("안녕...뭐?..", "안녕…뭐?.."),
            ("--", "—"),
            ("---", "—-"),
            ("----", "——"),
            ("a - b", "a - b"),
            ("기다려.......--!", "기다려……—!"),
            ("…", "…"),
        ];
        for (input, expected) in cases {
            assert_eq!(normalize_korean_text(input), expected, "{input:?}");
        }
    }

    #[test]
    fn chapter_normalization_touches_only_translated_text_of_changed_pages() {
        const OLD: &str = "2026-01-01T00:00:00.000Z";
        let block = |source: &str, translated: &str| TranslationBlock {
            source_text: source.into(),
            translated_text: translated.into(),
            ..TranslationBlock::default()
        };
        let page =
            |id: &str, blocks| PageRecord { id: id.into(), blocks, updated_at: OLD.into(), ..PageRecord::default() };
        let mut chapter = Chapter {
            pages: vec![
                page("p1", vec![block("...", "음..."), block("", "그래")]),
                page("p2", vec![block("--", "OK")]),
                page("p3", vec![block("", "아니--")]),
            ],
            updated_at: OLD.into(),
            ..Chapter::default()
        };

        assert_eq!(normalize_chapter_translated_text(&mut chapter), ["p1", "p3"]);
        assert_eq!(chapter.pages[0].blocks[0].translated_text, "음…");
        assert_eq!(chapter.pages[0].blocks[0].source_text, "...", "source text is not normalized");
        assert_eq!(chapter.pages[2].blocks[0].translated_text, "아니—");
        assert_eq!(chapter.pages[1].blocks[0].source_text, "--");
        assert_ne!(chapter.updated_at, OLD);
        assert_eq!(chapter.pages[0].updated_at, chapter.updated_at);
        assert_eq!(chapter.pages[2].updated_at, chapter.updated_at);
        assert_eq!(chapter.pages[1].updated_at, OLD);

        let before = chapter.clone();
        assert!(normalize_chapter_translated_text(&mut chapter).is_empty());
        assert_eq!(chapter, before, "already-normalized chapters keep their timestamps");
    }
}
