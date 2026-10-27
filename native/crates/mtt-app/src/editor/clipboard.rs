//! Block and font-style copy/paste through the system clipboard in the TS app's JSON
//! format (`editorUtils.ts`: `serializeTranslationBlockForClipboard`,
//! `serializeTranslationBlockFontStyleForClipboard`), so both apps can paste each
//! other's copies. An in-app copy is kept as a fallback like the TS refs.

use mtt_core::model::TranslationBlock;
use serde_json::{Map, Value, json};

use super::{Editor, Layer};

const BLOCK_KIND: &str = "manga-translation-tools/translation-block";
const FONT_STYLE_KIND: &str = "manga-translation-tools/translation-block-font-style";

/// `TRANSLATION_BLOCK_FONT_STYLE_KEYS` (preset id, link flags and every style field).
const FONT_STYLE_KEYS: &[&str] = &[
    "fontPresetId",
    "fontSizeLinkedToPreset",
    "lineHeightLinkedToPreset",
    "letterSpacingLinkedToPreset",
    "outlineColorLinkedToPreset",
    "outlineWidthLinkedToPreset",
    "secondaryOutlineEnabledLinkedToPreset",
    "secondaryOutlineColorLinkedToPreset",
    "secondaryOutlineWidthLinkedToPreset",
    "shadowEnabledLinkedToPreset",
    "shadowColorLinkedToPreset",
    "shadowOpacityLinkedToPreset",
    "shadowBlurPxLinkedToPreset",
    "shadowAngleDegLinkedToPreset",
    "shadowDistancePxLinkedToPreset",
    "autoFitTextLinkedToPreset",
    "textColorLinkedToPreset",
    "screentoneFillEnabledLinkedToPreset",
    "screentoneFillIntensityLinkedToPreset",
    "screentoneFillDensityLinkedToPreset",
    "screentoneFillAntialiasLinkedToPreset",
    "filterEffectEnabledLinkedToPreset",
    "filterZigzagEnabledLinkedToPreset",
    "filterZigzagIntensityLinkedToPreset",
    "filterWaveEnabledLinkedToPreset",
    "filterWaveIntensityLinkedToPreset",
    "filterGlitchEnabledLinkedToPreset",
    "filterGlitchIntensityLinkedToPreset",
    "filterPixelateEnabledLinkedToPreset",
    "filterPixelateIntensityLinkedToPreset",
    "filterBlurEnabledLinkedToPreset",
    "filterBlurIntensityLinkedToPreset",
    "filterNoiseEnabledLinkedToPreset",
    "filterNoiseIntensityLinkedToPreset",
    "fontBackgroundEnabledLinkedToPreset",
    "fontBackgroundScaleLinkedToPreset",
    "fontBackgroundColorLinkedToPreset",
    "fontBackgroundRadiusPxLinkedToPreset",
    "fontBackgroundOpacityLinkedToPreset",
    "fontWeightLinkedToPreset",
    "fontStyleLinkedToPreset",
    "textDecorationLinkedToPreset",
    "fontRenderingModeLinkedToPreset",
    "fontFamily",
    "characterFontOverrides",
    "fontWeight",
    "fontStyle",
    "textDecoration",
    "fontRenderingMode",
    "fontSizePx",
    "lineHeight",
    "letterSpacingPx",
    "outlineColor",
    "outlineWidthPx",
    "secondaryOutlineEnabled",
    "secondaryOutlineColor",
    "secondaryOutlineWidthPx",
    "shadowEnabled",
    "shadowColor",
    "shadowOpacity",
    "shadowBlurPx",
    "shadowAngleDeg",
    "shadowDistancePx",
    "autoFitText",
    "textAlign",
    "textPosition",
    "textColor",
    "screentoneFillEnabled",
    "screentoneFillIntensity",
    "screentoneFillDensity",
    "screentoneFillAntialias",
    "filterEffectEnabled",
    "filterZigzagEnabled",
    "filterZigzagIntensity",
    "filterWaveEnabled",
    "filterWaveIntensity",
    "filterGlitchEnabled",
    "filterGlitchIntensity",
    "filterPixelateEnabled",
    "filterPixelateIntensity",
    "filterBlurEnabled",
    "filterBlurIntensity",
    "filterNoiseEnabled",
    "filterNoiseIntensity",
    "fontBackgroundEnabled",
    "fontBackgroundScale",
    "fontBackgroundColor",
    "fontBackgroundRadiusPx",
    "fontBackgroundOpacity",
];

/// Copied font style: values present on the source block and keys it left unset.
#[derive(Debug, Clone, PartialEq)]
pub struct FontStyleCopy {
    values: Map<String, Value>,
    unset: Vec<String>,
}

fn block_json(block: &TranslationBlock) -> Map<String, Value> {
    match serde_json::to_value(block) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    }
}

pub fn serialize_block(block: &TranslationBlock) -> String {
    json!({ "kind": BLOCK_KIND, "version": 1, "block": block }).to_string()
}

pub fn parse_block(text: &str) -> Option<TranslationBlock> {
    let value: Value = serde_json::from_str(text).ok()?;
    if value["kind"] != BLOCK_KIND || value["version"] != 1 {
        return None;
    }
    serde_json::from_value(value["block"].clone()).ok()
}

/// `extractTranslationBlockFontStyle`.
pub fn extract_font_style(block: &TranslationBlock) -> FontStyleCopy {
    let json = block_json(block);
    let mut values = Map::new();
    let mut unset = Vec::new();
    for key in FONT_STYLE_KEYS {
        match json.get(*key) {
            Some(value) if !value.is_null() => {
                values.insert((*key).to_owned(), value.clone());
            }
            _ => unset.push((*key).to_owned()),
        }
    }
    FontStyleCopy { values, unset }
}

pub fn serialize_font_style(style: &FontStyleCopy) -> String {
    json!({ "kind": FONT_STYLE_KIND, "version": 1, "values": style.values, "unset": style.unset }).to_string()
}

pub fn parse_font_style(text: &str) -> Option<FontStyleCopy> {
    let value: Value = serde_json::from_str(text).ok()?;
    if value["kind"] != FONT_STYLE_KIND || value["version"] != 1 {
        return None;
    }
    let values = value["values"].as_object()?;
    let unset_list = value["unset"].as_array()?;
    let mut style = FontStyleCopy { values: Map::new(), unset: Vec::new() };
    for key in FONT_STYLE_KEYS {
        if unset_list.iter().any(|item| item == key) {
            style.unset.push((*key).to_owned());
        } else if let Some(value) = values.get(*key) {
            style.values.insert((*key).to_owned(), value.clone());
        }
    }
    Some(style)
}

/// `applyTranslationBlockFontStyle`.
pub fn apply_font_style(block: &mut TranslationBlock, style: &FontStyleCopy) {
    let mut json = block_json(block);
    for key in &style.unset {
        json.remove(key);
    }
    for (key, value) in &style.values {
        json.insert(key.clone(), value.clone());
    }
    if let Ok(next) = serde_json::from_value(Value::Object(json)) {
        *block = next;
    }
}

fn millis() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or_default()
}

/// `offsetBlockBboxes(block, 16, 16)` with a TS-style id suffix.
fn offset_copy(block: &TranslationBlock, suffix: &str) -> TranslationBlock {
    let mut copy = block.clone();
    copy.id = format!("{}-{suffix}-{}", block.id, millis());
    mtt_core::geometry::offset_block_bboxes(&mut copy, 16.0, 16.0);
    copy
}

impl Editor {
    /// ⌘C (`copySelectedBlockToClipboard`): the primary selected block.
    pub fn copy_selected(&mut self, ctx: &eframe::egui::Context) {
        let Some(block) = self.selected_blocks().into_iter().next() else { return };
        self.font_clipboard = None;
        ctx.copy_text(serialize_block(&block));
        self.clipboard = vec![block];
        self.status = "선택한 텍스트 블록을 복사했습니다.".into();
    }

    /// "글꼴 설정 복사" / canvas COPY (`copySelectedBlockFontStyleToClipboard`).
    pub fn copy_font_settings(&mut self, ctx: &eframe::egui::Context) {
        let Some(block) = self.selected_blocks().into_iter().next() else { return };
        let style = extract_font_style(&block);
        ctx.copy_text(serialize_font_style(&style));
        self.clipboard.clear();
        self.font_clipboard = Some(style);
        self.status = "선택한 블록의 폰트 설정을 복사했습니다.".into();
    }

    /// ⌘V: with a selected block a copied font style is applied; otherwise (or when the
    /// clipboard holds no style) a copied block is pasted. `text` is the system
    /// clipboard content when the platform delivered it.
    pub fn paste(&mut self, text: Option<&str>) {
        if let Some(text) = text {
            if let Some(style) = parse_font_style(text) {
                self.font_clipboard = Some(style);
                self.clipboard.clear();
            } else if let Some(block) = parse_block(text) {
                self.clipboard = vec![block];
                self.font_clipboard = None;
            }
        }
        if let Some(style) = self.font_clipboard.clone()
            && let Some(id) = self.selection.first().cloned()
        {
            self.edit_page(|page| {
                if let Some(block) = page.blocks.iter_mut().find(|block| block.id == id) {
                    apply_font_style(block, &style);
                }
            });
            self.status = "복사한 폰트 설정을 선택한 블록에 적용했습니다.".into();
            return;
        }
        let Some(source) = self.clipboard.first().cloned() else {
            self.status = "붙여넣을 텍스트 블록이 없습니다.".into();
            return;
        };
        let pasted = offset_copy(&source, "paste");
        self.visibility.overlay = true;
        if self.layer != Layer::Overlay {
            self.set_layer(Layer::Overlay);
        }
        self.insert_blocks(vec![pasted]);
    }

    /// 복제 / ⌘D / ⌘+click (`duplicateBlock`).
    pub fn duplicate_selected(&mut self) {
        let copies: Vec<TranslationBlock> =
            self.selected_blocks().iter().map(|block| offset_copy(block, "copy")).collect();
        self.insert_blocks(copies);
    }

    /// ⌘+click while inline-editing with text selected (`duplicateBlockTextSelection`):
    /// the selected text moves into a new block right after the original.
    pub fn split_block_text(&mut self, block_id: &str, text: &str, start: usize, end: usize) -> bool {
        let chars: Vec<char> = text.chars().collect();
        let (start, end) = (start.min(end).min(chars.len()), start.max(end).min(chars.len()));
        if start == end {
            return false;
        }
        let selected: String = chars[start..end].iter().collect();
        let remaining: String = chars[..start].iter().chain(&chars[end..]).collect();
        let copy_id = format!("{block_id}-split-{}", millis());
        let mut done = false;
        self.edit_page(|page| {
            let Some(index) = page.blocks.iter().position(|block| block.id == block_id) else { return };
            let mut copy = page.blocks[index].clone();
            copy.id = copy_id.clone();
            copy.translated_text = selected.clone();
            mtt_core::geometry::offset_block_bboxes(&mut copy, 16.0, 16.0);
            page.blocks[index].translated_text = remaining.clone();
            page.blocks.insert(index + 1, copy);
            done = true;
        });
        if done {
            self.selection = vec![copy_id];
        }
        done
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtt_core::model::BBox;

    #[test]
    fn clipboard_payloads_round_trip_in_ts_format() {
        let mut block = TranslationBlock {
            id: "b".into(),
            bbox: BBox::new(1.0, 2.0, 3.0, 4.0),
            translated_text: "안녕".into(),
            ..TranslationBlock::default()
        };
        block.style.font_size_px = Some(30.0);
        block.style.text_color = Some("#ff0000".into());
        let text = serialize_block(&block);
        assert!(text.contains("\"kind\":\"manga-translation-tools/translation-block\""));
        assert_eq!(parse_block(&text).unwrap().translated_text, "안녕");
        assert!(parse_block("hello").is_none());

        let style = extract_font_style(&block);
        let parsed = parse_font_style(&serialize_font_style(&style)).unwrap();
        assert_eq!(parsed, style);
        let mut target = TranslationBlock { id: "t".into(), translated_text: "그대로".into(), ..Default::default() };
        target.style.outline_width_px = Some(5.0);
        apply_font_style(&mut target, &parsed);
        assert_eq!(target.style.font_size_px, Some(30.0));
        assert_eq!(target.style.outline_width_px, None, "unset keys are cleared like the TS app");
        assert_eq!(target.translated_text, "그대로");
        assert_eq!(target.id, "t");
    }
}
