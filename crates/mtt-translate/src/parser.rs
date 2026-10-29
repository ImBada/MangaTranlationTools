//! Lenient parsing of model output into overlay items (`overlay-parser.cjs`).
//!
//! The prompt asks for plain `key: value` records; JSON (optionally fenced or with trailing
//! commas) is accepted too because some models ignore the instruction.

use regex::Regex;
use serde_json::Value;
use std::sync::LazyLock;

#[derive(Debug, Clone, PartialEq)]
pub struct OverlayItem {
    pub id: u32,
    pub item_type: String,
    /// Box in 0..1000 page space.
    pub bbox: (f64, f64, f64, f64),
    pub jp: String,
    pub ko: String,
}

/// Parses model output into items. Errors when nothing usable is found.
pub fn parse_items(raw: &str) -> Result<Vec<OverlayItem>, String> {
    // A JSON-looking part is only trusted when it parses: `[?]` or `{웃음}` inside a record
    // would otherwise cut the records down to the text between them.
    let items = match parse_json_candidate(raw).map(|value| normalize_items(&value)) {
        Some(items) if !items.is_empty() => items,
        _ => parse_loose_items(raw),
    };
    if items.is_empty() {
        return Err("모델 응답에서 번역 블록을 찾지 못했습니다.".into());
    }
    Ok(items.into_iter().enumerate().map(|(index, item)| OverlayItem { id: index as u32 + 1, ..item }).collect())
}

/// Source and translation the model gave for one region of [`crate::translate_regions`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionText {
    pub id: u32,
    pub jp: String,
    pub ko: String,
}

/// Parses `id` / `jp` / `ko` records (or JSON with those keys) for the region `ids`;
/// unknown ids, and regions marked as having no text (`ko: -`), are dropped. Errors when
/// no region is answered.
pub fn parse_region_texts(raw: &str, ids: &[u32]) -> Result<Vec<RegionText>, String> {
    let mut texts: Vec<RegionText> = Vec::new();
    let push = |texts: &mut Vec<RegionText>, id: Option<u32>, jp: Option<String>, ko: Option<String>| {
        let (Some(id), Some(ko)) = (id, ko.map(|ko| ko.trim().to_owned())) else { return };
        if ids.contains(&id) && !matches!(ko.as_str(), "" | "-") && !texts.iter().any(|text| text.id == id) {
            texts.push(RegionText { id, jp: jp.unwrap_or_default().trim().to_owned(), ko });
        }
    };
    if let Some(value) = parse_json_candidate(raw) {
        let list = value.as_array().or_else(|| value.get("items").and_then(Value::as_array));
        for item in list.into_iter().flatten() {
            push(
                &mut texts,
                number(item.get("id")).map(|id| id as u32),
                first_text(item, &["jp", "japanese", "source"]),
                first_text(item, &["ko", "korean", "translation"]),
            );
        }
    }
    if texts.is_empty() {
        let (mut id, mut jp, mut ko) = (None, None, None);
        let cleaned = raw.replace("```", "");
        for line in cleaned.lines().map(str::trim).chain([""]) {
            let Some((key, value)) = line.split_once(':') else {
                if line.is_empty() {
                    push(&mut texts, id.take(), jp.take(), ko.take());
                }
                continue;
            };
            let value = value.trim().trim_matches(|c| c == '"' || c == '\'' || c == ',').trim().to_owned();
            match key.trim().trim_matches('"').to_ascii_lowercase().as_str() {
                "id" => {
                    push(&mut texts, id.take(), jp.take(), ko.take());
                    id = value.parse().ok();
                }
                "jp" => jp = Some(value),
                "ko" => ko = Some(value),
                _ => {}
            }
        }
    }
    if texts.is_empty() {
        return Err("모델 응답에서 블록 번역을 찾지 못했습니다.".into());
    }
    Ok(texts)
}

/// The JSON in `raw`, when there is some that parses: the whole text, a fenced block, or
/// the span between the outermost brackets (the one that opens first is tried first).
fn parse_json_candidate(raw: &str) -> Option<Value> {
    static FENCE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?is)```(?:json)?\s*(.*?)```").unwrap());
    let trimmed = raw.trim();
    let mut candidates = vec![trimmed];
    if let Some(inner) = FENCE.captures(trimmed).and_then(|captures| captures.get(1)) {
        candidates.push(inner.as_str().trim());
    }
    let mut spans: Vec<(usize, &str)> = [('{', '}'), ('[', ']')]
        .into_iter()
        .filter_map(|(open, close)| {
            let (first, last) = (trimmed.find(open)?, trimmed.rfind(close)?);
            (last > first).then(|| (first, &trimmed[first..=last]))
        })
        .collect();
    spans.sort_by_key(|(first, _)| *first);
    candidates.extend(spans.into_iter().map(|(_, span)| span));
    candidates
        .into_iter()
        .filter(|candidate| candidate.starts_with('{') || candidate.starts_with('['))
        .find_map(parse_json_relaxed)
}

fn parse_json_relaxed(candidate: &str) -> Option<Value> {
    static TRAILING_COMMA: LazyLock<Regex> = LazyLock::new(|| Regex::new(r",\s*([}\]])").unwrap());
    serde_json::from_str(candidate)
        .ok()
        .or_else(|| serde_json::from_str(&TRAILING_COMMA.replace_all(candidate, "$1")).ok())
}

fn number(value: Option<&Value>) -> Option<f64> {
    match value? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
    .filter(|n: &f64| n.is_finite())
}

fn normalize_bbox(item: &Value) -> Option<(f64, f64, f64, f64)> {
    let boxed = ["bbox", "box", "rect", "region"]
        .iter()
        .find_map(|key| item.get(*key))
        .filter(|v| v.is_object())
        .unwrap_or(item);
    if ["x1", "y1", "x2", "y2"].iter().all(|key| boxed.get(*key).is_some()) {
        let (x1, y1, x2, y2) =
            (number(boxed.get("x1"))?, number(boxed.get("y1"))?, number(boxed.get("x2"))?, number(boxed.get("y2"))?);
        return Some((x1, y1, x2 - x1, y2 - y1));
    }
    let pick = |a: &str, b: &str| number(boxed.get(a)).or_else(|| number(boxed.get(b)));
    Some((pick("x", "left")?, pick("y", "top")?, pick("w", "width")?, pick("h", "height")?))
}

fn first_text(item: &Value, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| item.get(*key)?.as_str().map(str::trim).filter(|s| !s.is_empty()).map(str::to_owned))
}

fn normalize_items(value: &Value) -> Vec<OverlayItem> {
    let list = value
        .as_array()
        .or_else(|| value.get("items").and_then(Value::as_array))
        .or_else(|| value.get("blocks").and_then(Value::as_array));
    let Some(list) = list else { return Vec::new() };
    list.iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let ko = first_text(item, &["ko", "korean", "translation", "translated", "text_ko"])?;
            let bbox = normalize_bbox(item)?;
            Some(OverlayItem {
                id: number(item.get("id")).map(|n| n as u32).unwrap_or(index as u32 + 1),
                item_type: item
                    .get("type")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .unwrap_or("dialogue")
                    .to_owned(),
                bbox,
                jp: first_text(item, &["jp", "japanese", "source", "ocr", "text_jp"]).unwrap_or_default(),
                ko,
            })
        })
        .collect()
}

#[derive(Default)]
struct Draft {
    id: Option<u32>,
    item_type: Option<String>,
    x: Option<f64>,
    y: Option<f64>,
    w: Option<f64>,
    h: Option<f64>,
    jp: Option<String>,
    ko: Option<String>,
}

impl Draft {
    fn finish(self, items: &mut Vec<OverlayItem>) {
        let (Some(x), Some(y), Some(w), Some(h)) = (self.x, self.y, self.w, self.h) else { return };
        let Some(ko) = self.ko.map(|ko| ko.trim().to_owned()).filter(|ko| !ko.is_empty()) else { return };
        items.push(OverlayItem {
            id: self.id.unwrap_or(items.len() as u32 + 1),
            item_type: self.item_type.unwrap_or_else(|| "dialogue".into()),
            bbox: (x, y, w, h),
            jp: self.jp.unwrap_or_default(),
            ko,
        });
    }
}

/// `parseLooseItemList`: `id:` starts a record; `x/y/w/h`, `type`, `jp`, `ko` fill it.
fn parse_loose_items(raw: &str) -> Vec<OverlayItem> {
    static ID: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"^(?i)(?:\{?\s*)?(?:"?id[^:"]*"?|a)\s*:\s*["']?([A-Za-z0-9_-]+)["']?"#).unwrap());
    static TYPE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"^(?i)"?type[^:"]*"?\s*:\s*["']?([^"',}]+)["']?"#).unwrap());
    // A box line starts with a coordinate key and may hold several (`"x": 1, "y": 2,`); a key
    // inside other text (`ko: Crash: 2`) is not one.
    static COORD_LINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"^\{?\s*["']?[xywh]["']?\s*:"#).unwrap());
    static COORD: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"(?:^|[{,\s])["']?([xywh])["']?\s*:\s*([0-9.]+)"#).unwrap());
    static JP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"^(?i)"?jp[^:"]*"?\s*:\s*["']?(.+?)["']?,?$"#).unwrap());
    static KO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"^(?i)"?ko[^:"]*"?\s*:\s*["']?(.+?)["']?,?$"#).unwrap());
    static A_KEY: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\ba\s*:").unwrap());
    let cleaned = raw.replace("```json", "").replace("```JSON", "").replace("```", "");
    let mut items = Vec::new();
    let mut current: Option<Draft> = None;
    for raw_line in cleaned.lines() {
        let line = A_KEY.replace_all(raw_line.trim(), "x:").into_owned();
        if line.is_empty() {
            if let Some(draft) = current.take() {
                draft.finish(&mut items);
            }
            continue;
        }
        if let Some(captures) = ID.captures(&line) {
            if let Some(draft) = current.take() {
                draft.finish(&mut items);
            }
            current = Some(Draft { id: captures[1].parse().ok(), ..Draft::default() });
            continue;
        }
        let draft = current.get_or_insert_with(Draft::default);
        if let Some(captures) = TYPE.captures(&line) {
            draft.item_type = Some(captures[1].trim().to_owned());
            continue;
        }
        if COORD_LINE.is_match(&line) {
            for captures in COORD.captures_iter(&line) {
                let value = captures[2].parse().ok();
                match &captures[1] {
                    "x" => draft.x = value,
                    "y" => draft.y = value,
                    "w" => draft.w = value,
                    _ => draft.h = value,
                }
            }
            continue;
        }
        if let Some(captures) = JP.captures(&line) {
            draft.jp = Some(captures[1].to_owned());
            continue;
        }
        if let Some(captures) = KO.captures(&line) {
            draft.ko = Some(captures[1].to_owned());
        }
    }
    if let Some(draft) = current {
        draft.finish(&mut items);
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_region_records() {
        let raw = "id: 2\njp: なに\nko: 뭐?\n\nid: 1\njp:\nko: -\n\nid: 9\nko: 모름\nid: 3\nko: 셋";
        let texts = parse_region_texts(raw, &[1, 2, 3]).unwrap();
        assert_eq!(
            texts,
            [
                RegionText { id: 2, jp: "なに".into(), ko: "뭐?".into() },
                RegionText { id: 3, jp: String::new(), ko: "셋".into() },
            ]
        );
        let json = r#"```json
[{"id": 1, "jp": "あ", "ko": "아"},]
```"#;
        assert_eq!(parse_region_texts(json, &[1]).unwrap()[0].ko, "아");
        assert!(parse_region_texts("죄송합니다", &[1]).is_err());
    }

    #[test]
    fn parses_plain_records() {
        let raw = "id: 1\ntype: dialogue\nx: 120\ny: 80\nw: 160\nh: 240\njp: 馬鹿者\nko: 바보 같은 녀석.\n\nid: 2\ntype: sfx\nx: 300\ny: 120\nw: 140\nh: 220\njp: ドン\nko: 쾅";
        let items = parse_items(raw).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].bbox, (120.0, 80.0, 160.0, 240.0));
        assert_eq!(items[0].ko, "바보 같은 녀석.");
        assert_eq!(items[1].item_type, "sfx");
    }

    #[test]
    fn parses_fenced_json_with_trailing_commas_and_corner_boxes() {
        let raw = "Here you go:\n```json\n{\"items\": [{\"type\": \"narration\", \"box\": {\"x1\": 10, \"y1\": 20, \"x2\": 110, \"y2\": 70}, \"japanese\": \"昔\", \"translation\": \"옛날\"},]}\n```";
        let items = parse_items(raw).unwrap();
        assert_eq!(items[0].bbox, (10.0, 20.0, 100.0, 50.0));
        assert_eq!(items[0].item_type, "narration");
        assert_eq!(items[0].jp, "昔");
    }

    #[test]
    fn records_without_translation_or_box_are_dropped() {
        assert!(parse_items("id: 1\nx: 1\ny: 1\nw: 1\nh: 1\njp: あ").is_err());
        let items = parse_items("[{\"x\":1,\"y\":2,\"w\":3,\"h\":4,\"ko\":\"가\"},{\"ko\":\"나\"}]").unwrap();
        assert_eq!(items.len(), 1);
    }

    /// Records in the prompt's format, one per `(jp, ko)`.
    fn records(texts: &[(&str, &str)]) -> String {
        texts
            .iter()
            .enumerate()
            .map(|(i, (jp, ko))| {
                format!("id: {}\ntype: dialogue\nx: {}\ny: 10\nw: 50\nh: 60\njp: {jp}\nko: {ko}", i + 1, i * 100)
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    #[test]
    fn brackets_and_braces_inside_records_are_text() {
        let raw = records(&[("あ", "아"), ("[?]だ", "[?]다"), ("う", "우")]);
        let items = parse_items(&raw).unwrap();
        let ko: Vec<&str> = items.iter().map(|item| item.ko.as_str()).collect();
        assert_eq!(ko, ["아", "[?]다", "우"]);

        let raw = records(&[("[?]", "[?]"), ("い", "이"), ("う", "우"), ("え[?]", "에[?]"), ("お", "오")]);
        let items = parse_items(&raw).unwrap();
        assert_eq!(items.len(), 5, "no record lost between the brackets");
        assert_eq!((items[0].ko.as_str(), items[4].ko.as_str()), ("[?]", "오"));

        let raw = records(&[("ふふ", "{웃음} 후후"), ("い", "이 {웃음}")]);
        let items = parse_items(&raw).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[1].ko, "이 {웃음}");

        let regions = "id: 1\njp: [?]\nko: [?]\n\nid: 2\njp: え\nko: {웃음}";
        let texts = parse_region_texts(regions, &[1, 2]).unwrap();
        let ko: Vec<&str> = texts.iter().map(|text| text.ko.as_str()).collect();
        assert_eq!(ko, ["[?]", "{웃음}"]);
    }

    #[test]
    fn coordinates_are_only_read_from_box_lines() {
        let raw = "id: 1\nx: 10\ny: 20\nw: 30\nh: 40\njp: クラッシュ\nko: Crash: 2";
        let items = parse_items(raw).unwrap();
        assert_eq!(items[0].bbox, (10.0, 20.0, 30.0, 40.0));
        assert_eq!(items[0].ko, "Crash: 2");
        let raw = "id: 1\n{x: 1, y: 2, w: 3, h: 4}\nko: 가";
        assert_eq!(parse_items(raw).unwrap()[0].bbox, (1.0, 2.0, 3.0, 4.0));
    }

    #[test]
    fn json_after_prose_is_found() {
        let raw =
            "Here: [{\"x\":1,\"y\":2,\"w\":3,\"h\":4,\"ko\":\"가\"},{\"x\":5,\"y\":6,\"w\":7,\"h\":8,\"ko\":\"나\"}]";
        assert_eq!(parse_items(raw).unwrap().len(), 2);
    }

    #[test]
    fn quoted_loose_records() {
        let raw = "\"id\": 3,\n\"x\": 1, \"y\": 2,\n\"w\": 3, \"h\": 4,\n\"ko\": \"안녕\",";
        let items = parse_items(raw).unwrap();
        assert_eq!(items[0].ko, "안녕");
        assert_eq!(items[0].id, 1, "ids are renumbered");
    }
}
