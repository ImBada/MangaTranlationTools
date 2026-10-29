//! Editable Photoshop text layers: the `TySh` (type tool object setting) layer info, with
//! the descriptors and the text engine data (`EngineData`) Photoshop and GIMP read the
//! text, font, size, color and paragraph box from.
//!
//! Ported from koharu-psd (`engine_data.rs`, `descriptor.rs`, `export.rs`;
//! https://github.com/koharu-rs/koharu), Copyright 2025-2026 Mayo Takanashi and Koharu
//! contributors, licensed under MIT OR Apache-2.0. Its EngineData is shaped for GIMP's
//! parser (`psd-util.c`), which is stricter about number tokens than Photoshop.

use crate::psd::{push_i32, push_u16, push_u32};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Justification {
    Left,
    Center,
    Right,
}

/// One text layer's type, in page pixels (the PSD is 72 ppi, so a point is a pixel).
#[derive(Debug, Clone, PartialEq)]
pub struct TextLayer {
    /// Paragraphs separated by `\n`.
    pub text: String,
    /// PostScript name of the font (what Photoshop and GIMP look fonts up by).
    pub font: String,
    pub font_size: f64,
    pub faux_bold: bool,
    pub faux_italic: bool,
    /// Baseline to baseline (column to column when vertical).
    pub leading: f64,
    /// Extra space between characters in 1/1000 em.
    pub tracking: i32,
    /// Straight RGBA.
    pub color: [u8; 4],
    pub justification: Justification,
    pub vertical: bool,
    /// Size of the paragraph box, whose top-left corner is the transform's origin.
    pub box_width: f64,
    pub box_height: f64,
    /// Text space to page pixels: `[xx, xy, yx, yy, tx, ty]`.
    pub transform: [f64; 6],
}

impl TextLayer {
    /// Page bounds of the (transformed) paragraph box: left, top, right, bottom.
    fn page_bounds(&self) -> [f64; 4] {
        let [xx, xy, yx, yy, tx, ty] = self.transform;
        let corners = [(0.0, 0.0), (self.box_width, 0.0), (0.0, self.box_height), (self.box_width, self.box_height)]
            .map(|(u, v)| (xx * u + yx * v + tx, xy * u + yy * v + ty));
        let xs = corners.map(|corner| corner.0);
        let ys = corners.map(|corner| corner.1);
        let min = |values: [f64; 4]| values.into_iter().fold(f64::INFINITY, f64::min);
        let max = |values: [f64; 4]| values.into_iter().fold(f64::NEG_INFINITY, f64::max);
        [min(xs), min(ys), max(xs), max(ys)]
    }
}

/// The `TySh` additional layer info. `text_index` counts the document's text layers.
pub fn type_tool_info(layer: &TextLayer, text_index: i32) -> Vec<u8> {
    let orientation = if layer.vertical { "Vrtc" } else { "Hrzn" };
    // Photoshop keeps both boxes relative to the transform's origin.
    let bounds = |class| {
        Item::Object(
            class,
            vec![
                ("Left", Item::Points(0.0)),
                ("Top ", Item::Points(0.0)),
                ("Rght", Item::Points(layer.box_width)),
                ("Btom", Item::Points(layer.box_height)),
            ],
        )
    };
    let text = Item::Object(
        "TxLr",
        vec![
            ("Txt ", Item::Text(layer.text.replace("\r\n", "\n").replace('\n', "\r"))),
            ("textGridding", Item::Enum("textGridding", "None")),
            ("Ornt", Item::Enum("Ornt", orientation)),
            ("AntA", Item::Enum("Annt", "antiAliasSharp")),
            ("bounds", bounds("bounds")),
            ("boundingBox", bounds("boundingBox")),
            ("TextIndex", Item::Long(text_index)),
            ("EngineData", Item::Raw(engine_data(layer))),
        ],
    );
    let warp = Item::Object(
        "warp",
        vec![
            ("warpStyle", Item::Enum("warpStyle", "warpNone")),
            ("warpValue", Item::Double(0.0)),
            ("warpPerspective", Item::Double(0.0)),
            ("warpPerspectiveOther", Item::Double(0.0)),
            ("warpRotate", Item::Enum("Ornt", orientation)),
        ],
    );
    let mut out = Vec::new();
    push_u16(&mut out, 1);
    for value in layer.transform {
        out.extend_from_slice(&value.to_be_bytes());
    }
    push_u16(&mut out, 50);
    push_u32(&mut out, 16);
    write_descriptor(&mut out, &text);
    push_u16(&mut out, 1);
    push_u32(&mut out, 16);
    write_descriptor(&mut out, &warp);
    // Left, top, right, bottom as int32 (GIMP and psd-tools; both only report them).
    for value in layer.page_bounds() {
        push_i32(&mut out, value.round().clamp(f64::from(i32::MIN), f64::from(i32::MAX)) as i32);
    }
    out
}

// ----------------------------------------------------------------------------- descriptors

/// A descriptor value (`Objc` is a nested descriptor of a class with its items).
enum Item {
    Text(String),
    Enum(&'static str, &'static str),
    Long(i32),
    Double(f64),
    /// `UntF` in points.
    Points(f64),
    Raw(Vec<u8>),
    Object(&'static str, Vec<(&'static str, Item)>),
}

/// Unicode string: UTF-16 length including the terminator, the units, a NUL unit.
fn push_unicode(out: &mut Vec<u8>, text: &str) {
    let units: Vec<u16> = text.encode_utf16().collect();
    push_u32(out, units.len() as u32 + 1);
    units.into_iter().chain([0]).for_each(|unit| push_u16(out, unit));
}

/// A key or class ID: four characters as a bare ID (length 0), anything else length-prefixed.
fn push_id(out: &mut Vec<u8>, id: &str) {
    push_u32(out, if id.len() == 4 { 0 } else { id.len() as u32 });
    out.extend_from_slice(id.as_bytes());
}

/// An `Objc` item's descriptor (empty name, class, items); other items are not descriptors.
fn write_descriptor(out: &mut Vec<u8>, object: &Item) {
    let Item::Object(class, items) = object else { return };
    push_unicode(out, "");
    push_id(out, class);
    push_u32(out, items.len() as u32);
    for (key, item) in items {
        push_id(out, key);
        match item {
            Item::Text(text) => {
                out.extend_from_slice(b"TEXT");
                push_unicode(out, text);
            }
            Item::Enum(kind, value) => {
                out.extend_from_slice(b"enum");
                push_id(out, kind);
                push_id(out, value);
            }
            Item::Long(value) => {
                out.extend_from_slice(b"long");
                push_i32(out, *value);
            }
            Item::Double(value) => {
                out.extend_from_slice(b"doub");
                out.extend_from_slice(&value.to_be_bytes());
            }
            Item::Points(value) => {
                out.extend_from_slice(b"UntF#Pnt");
                out.extend_from_slice(&value.to_be_bytes());
            }
            Item::Raw(bytes) => {
                out.extend_from_slice(b"tdta");
                push_u32(out, bytes.len() as u32);
                out.extend_from_slice(bytes);
            }
            Item::Object(..) => {
                out.extend_from_slice(b"Objc");
                write_descriptor(out, item);
            }
        }
    }
}

// ----------------------------------------------------------------------------- EngineData

/// A value of the PostScript-like EngineData syntax.
enum Value {
    Int(i32),
    Float(f64),
    Bool(bool),
    Text(String),
    Array(Vec<Value>),
    Dict(Vec<(&'static str, Value)>),
}

use Value::{Array, Bool, Dict, Float, Int};

fn text_value(text: &str) -> Value {
    Value::Text(text.to_owned())
}

fn floats(values: &[f64]) -> Value {
    Array(values.iter().copied().map(Float).collect())
}

/// `Type 1` (ARGB) color with components in 0..=1.
fn color(rgba: [u8; 4]) -> Value {
    let [r, g, b, a] = rgba.map(|channel| f64::from(channel) / 255.0);
    Dict(vec![("Type", Int(1)), ("Values", floats(&[a, r, g, b]))])
}

/// The EngineData of one text layer: the text, one paragraph run per line, one style run.
fn engine_data(layer: &TextLayer) -> Vec<u8> {
    // Paragraphs end in `\r`, the last one included.
    let text = format!("{}\r", layer.text.replace("\r\n", "\n").replace('\n', "\r"));
    let paragraph_lengths: Vec<i32> =
        text.split_inclusive('\r').map(|paragraph| paragraph.encode_utf16().count() as i32).collect();
    let length = text.encode_utf16().count() as i32;
    let (writing_direction, procession) = if layer.vertical { (2, 1) } else { (0, 0) };
    let adjustments = || Dict(vec![("Axis", floats(&[1.0, 0.0, 1.0])), ("XY", floats(&[0.0, 0.0]))]);
    let paragraph_sheet = |properties| Dict(vec![("DefaultStyleSheet", Int(0)), ("Properties", Dict(properties))]);
    let style = Dict(vec![
        ("Font", Int(0)),
        ("FontSize", Float(layer.font_size)),
        ("FauxBold", Bool(layer.faux_bold)),
        ("FauxItalic", Bool(layer.faux_italic)),
        ("AutoLeading", Bool(false)),
        ("Leading", Float(layer.leading)),
        ("Tracking", Int(layer.tracking)),
        ("AutoKerning", Bool(true)),
        ("Kerning", Int(0)),
        ("FillColor", color(layer.color)),
    ]);
    let engine = Dict(vec![
        ("Editor", Dict(vec![("Text", Value::Text(text))])),
        (
            "ParagraphRun",
            Dict(vec![
                (
                    "DefaultRunData",
                    Dict(vec![("ParagraphSheet", paragraph_sheet(Vec::new())), ("Adjustments", adjustments())]),
                ),
                (
                    "RunArray",
                    Array(
                        paragraph_lengths
                            .iter()
                            .map(|_| {
                                Dict(vec![
                                    ("ParagraphSheet", paragraph_sheet(paragraph_properties(layer.justification))),
                                    ("Adjustments", adjustments()),
                                ])
                            })
                            .collect(),
                    ),
                ),
                ("RunLengthArray", Array(paragraph_lengths.iter().copied().map(Int).collect())),
                ("IsJoinable", Int(1)),
            ]),
        ),
        (
            "StyleRun",
            Dict(vec![
                ("DefaultRunData", Dict(vec![("StyleSheet", Dict(vec![("StyleSheetData", Dict(Vec::new()))]))])),
                ("RunArray", Array(vec![Dict(vec![("StyleSheet", Dict(vec![("StyleSheetData", style)]))])])),
                ("RunLengthArray", Array(vec![Int(length)])),
                ("IsJoinable", Int(2)),
            ]),
        ),
        (
            "GridInfo",
            Dict(vec![
                ("GridIsOn", Bool(false)),
                ("ShowGrid", Bool(false)),
                ("GridSize", Float(18.0)),
                ("GridLeading", Float(22.0)),
                ("GridColor", color([0, 0, 255, 255])),
                ("GridLeadingFillColor", color([0, 0, 255, 255])),
                ("AlignLineHeightToGridFlags", Bool(false)),
            ]),
        ),
        ("AntiAlias", Int(4)),
        ("UseFractionalGlyphWidths", Bool(true)),
        (
            "Rendered",
            Dict(vec![
                ("Version", Int(1)),
                (
                    "Shapes",
                    Dict(vec![
                        ("WritingDirection", Int(writing_direction)),
                        (
                            "Children",
                            Array(vec![Dict(vec![
                                // A paragraph (box) text shape.
                                ("ShapeType", Int(1)),
                                ("Procession", Int(procession)),
                                (
                                    "Lines",
                                    Dict(vec![
                                        ("WritingDirection", Int(writing_direction)),
                                        ("Children", Array(vec![])),
                                    ]),
                                ),
                                (
                                    "Cookie",
                                    Dict(vec![(
                                        "Photoshop",
                                        Dict(vec![
                                            ("ShapeType", Int(1)),
                                            ("BoxBounds", floats(&[0.0, 0.0, layer.box_width, layer.box_height])),
                                            (
                                                "Base",
                                                Dict(vec![
                                                    ("ShapeType", Int(1)),
                                                    ("TransformPoint0", floats(&[1.0, 0.0])),
                                                    ("TransformPoint1", floats(&[0.0, 1.0])),
                                                    ("TransformPoint2", floats(&[0.0, 0.0])),
                                                ]),
                                            ),
                                        ]),
                                    )]),
                                ),
                            ])]),
                        ),
                    ]),
                ),
            ]),
        ),
    ]);
    let root = Dict(vec![
        ("EngineDict", engine),
        ("ResourceDict", resource_dict(layer)),
        ("DocumentResources", resource_dict(layer)),
    ]);
    let mut out = Vec::new();
    write_value(&mut out, &root, 0, "");
    out.push(b'\n');
    out
}

fn paragraph_properties(justification: Justification) -> Vec<(&'static str, Value)> {
    let justification = match justification {
        Justification::Left => 0,
        Justification::Right => 1,
        Justification::Center => 2,
    };
    vec![
        ("Justification", Int(justification)),
        ("FirstLineIndent", Float(0.0)),
        ("StartIndent", Float(0.0)),
        ("EndIndent", Float(0.0)),
        ("SpaceBefore", Float(0.0)),
        ("SpaceAfter", Float(0.0)),
        ("AutoHyphenate", Bool(true)),
        ("HyphenatedWordSize", Int(6)),
        ("PreHyphen", Int(2)),
        ("PostHyphen", Int(2)),
        ("ConsecutiveHyphens", Int(8)),
        ("Zone", Float(36.0)),
        ("WordSpacing", floats(&[0.8, 1.0, 1.33])),
        ("LetterSpacing", floats(&[0.0, 0.0, 0.0])),
        ("GlyphSpacing", floats(&[1.0, 1.0, 1.0])),
        ("AutoLeading", Float(1.2)),
        ("LeadingType", Int(0)),
        ("Hanging", Bool(false)),
        ("Burasagari", Bool(false)),
        ("KinsokuOrder", Int(0)),
        ("EveryLineComposer", Bool(false)),
    ]
}

/// The document's "Normal RGB" character style.
fn normal_style_sheet() -> Value {
    Dict(vec![
        ("Font", Int(0)),
        ("FontSize", Float(12.0)),
        ("FauxBold", Bool(false)),
        ("FauxItalic", Bool(false)),
        ("AutoLeading", Bool(true)),
        ("Leading", Float(0.0)),
        ("HorizontalScale", Float(1.0)),
        ("VerticalScale", Float(1.0)),
        ("Tracking", Int(0)),
        ("AutoKerning", Bool(true)),
        ("Kerning", Int(0)),
        ("BaselineShift", Float(0.0)),
        ("FontCaps", Int(0)),
        ("FontBaseline", Int(0)),
        ("Underline", Bool(false)),
        ("Strikethrough", Bool(false)),
        ("Ligatures", Bool(true)),
        ("DLigatures", Bool(false)),
        ("BaselineDirection", Int(2)),
        ("Tsume", Float(0.0)),
        ("StyleRunAlignment", Int(2)),
        ("Language", Int(0)),
        ("NoBreak", Bool(false)),
        ("FillColor", color([0, 0, 0, 255])),
        ("StrokeColor", color([0, 0, 0, 255])),
        ("FillFlag", Bool(true)),
        ("StrokeFlag", Bool(false)),
        ("FillFirst", Bool(true)),
        ("YUnderline", Int(1)),
        ("OutlineWidth", Float(1.0)),
        ("CharacterDirection", Int(0)),
        ("HindiNumbers", Bool(false)),
        ("Kashida", Int(1)),
        ("DiacriticPos", Int(2)),
    ])
}

/// Fonts (the layer's at index 0, then Photoshop's invisible-characters font), sheets and
/// Photoshop's default Japanese line-breaking sets.
fn resource_dict(layer: &TextLayer) -> Value {
    let font = |name: &str| {
        Dict(vec![("Name", text_value(name)), ("Script", Int(0)), ("FontType", Int(0)), ("Synthetic", Int(0))])
    };
    let kinsoku = |name: &str, no_start: &str, no_end: &str| {
        Dict(vec![
            ("Name", text_value(name)),
            ("NoStart", text_value(no_start)),
            ("NoEnd", text_value(no_end)),
            ("Keep", text_value("\u{2015}\u{2025}")),
            ("Hanging", text_value("\u{3001}\u{3002}.,")),
        ])
    };
    let moji_kumi = |name: &str| Dict(vec![("InternalName", text_value(name))]);
    Dict(vec![
        (
            "KinsokuSet",
            Array(vec![
                kinsoku(
                    "PhotoshopKinsokuHard",
                    "\u{3001}\u{3002}\u{ff0c}\u{ff0e}\u{30fb}\u{ff1a}\u{ff1b}\u{ff1f}\u{ff01}\u{30fc}\u{2015}\u{2019}\u{201d}\u{ff09}\u{3015}\u{ff3d}\u{ff5d}\u{3009}\u{300b}\u{300d}\u{300f}\u{3011}\u{30fd}\u{30fe}\u{309d}\u{309e}\u{3005}\u{3041}\u{3043}\u{3045}\u{3047}\u{3049}\u{3063}\u{3083}\u{3085}\u{3087}\u{308e}\u{30a1}\u{30a3}\u{30a5}\u{30a7}\u{30a9}\u{30c3}\u{30e3}\u{30e5}\u{30e7}\u{30ee}\u{30f5}\u{30f6}\u{309b}\u{309c}?!)]},.:;\u{2103}\u{2109}\u{00a2}\u{ff05}\u{2030}",
                    "\u{2018}\u{201c}\u{ff08}\u{3014}\u{ff3b}\u{ff5b}\u{3008}\u{300a}\u{300c}\u{300e}\u{3010}([{\u{ffe5}\u{ff04}\u{00a3}\u{ff20}\u{00a7}\u{3012}\u{ff03}",
                ),
                kinsoku(
                    "PhotoshopKinsokuSoft",
                    "\u{3001}\u{3002}\u{ff0c}\u{ff0e}\u{30fb}\u{ff1a}\u{ff1b}\u{ff1f}\u{ff01}\u{2019}\u{201d}\u{ff09}\u{3015}\u{ff3d}\u{ff5d}\u{3009}\u{300b}\u{300d}\u{300f}\u{3011}\u{30fd}\u{30fe}\u{309d}\u{309e}\u{3005}",
                    "\u{2018}\u{201c}\u{ff08}\u{3014}\u{ff3b}\u{ff5b}\u{3008}\u{300a}\u{300c}\u{300e}\u{3010}",
                ),
            ]),
        ),
        (
            "MojiKumiSet",
            Array(
                [
                    "Photoshop6MojiKumiSet1",
                    "Photoshop6MojiKumiSet2",
                    "Photoshop6MojiKumiSet3",
                    "Photoshop6MojiKumiSet4",
                ]
                .map(moji_kumi)
                .into(),
            ),
        ),
        ("TheNormalStyleSheet", Int(0)),
        ("TheNormalParagraphSheet", Int(0)),
        (
            "ParagraphSheetSet",
            Array(vec![Dict(vec![
                ("Name", text_value("Normal RGB")),
                ("DefaultStyleSheet", Int(0)),
                ("Properties", Dict(paragraph_properties(Justification::Left))),
            ])]),
        ),
        (
            "StyleSheetSet",
            Array(vec![Dict(vec![("Name", text_value("Normal RGB")), ("StyleSheetData", normal_style_sheet())])]),
        ),
        ("FontSet", Array(vec![font(&layer.font), font("AdobeInvisFont")])),
        ("SuperscriptSize", Float(0.583)),
        ("SuperscriptPosition", Float(0.333)),
        ("SubscriptSize", Float(0.583)),
        ("SubscriptPosition", Float(0.333)),
        ("SmallCapSize", Float(0.7)),
    ])
}

fn is_scalar(value: &Value) -> bool {
    !matches!(value, Array(_) | Dict(_))
}

fn push_tabs(out: &mut Vec<u8>, count: usize) {
    out.extend(std::iter::repeat_n(b'\t', count));
}

/// Writes `value` (the value of `key`) at the current position; nested lines get `indent`.
fn write_value(out: &mut Vec<u8>, value: &Value, indent: usize, key: &str) {
    match value {
        Int(number) => out.extend_from_slice(number.to_string().as_bytes()),
        Float(number) => out.extend_from_slice(float_token(*number, key).as_bytes()),
        Bool(flag) => out.extend_from_slice(if *flag { b"true" } else { b"false" }),
        // UTF-16BE with a byte order mark; parentheses and backslashes escaped byte-wise.
        Value::Text(text) => {
            out.extend_from_slice(b"(\xFE\xFF");
            for byte in text.encode_utf16().flat_map(u16::to_be_bytes) {
                if matches!(byte, b'(' | b')' | b'\\') {
                    out.push(b'\\');
                }
                out.push(byte);
            }
            out.push(b')');
        }
        Array(items) if items.iter().all(is_scalar) => {
            out.push(b'[');
            for item in items {
                out.push(b' ');
                write_value(out, item, indent, key);
            }
            out.extend_from_slice(b" ]");
        }
        Array(items) => {
            out.extend_from_slice(b"[\n");
            for item in items {
                push_tabs(out, indent + 1);
                write_value(out, item, indent + 1, key);
                out.push(b'\n');
            }
            push_tabs(out, indent);
            out.push(b']');
        }
        Dict(entries) => {
            out.extend_from_slice(b"<<\n");
            for (entry_key, entry) in entries {
                push_tabs(out, indent + 1);
                out.push(b'/');
                out.extend_from_slice(entry_key.as_bytes());
                if let Dict(_) = entry {
                    out.push(b'\n');
                    push_tabs(out, indent + 1);
                } else {
                    out.push(b' ');
                }
                write_value(out, entry, indent + 1, entry_key);
                out.push(b'\n');
            }
            push_tabs(out, indent);
            out.extend_from_slice(b">>");
        }
    }
}

/// Numbers of keys Photoshop always writes as reals keep a decimal point (GIMP's parser
/// goes by the token); whole numbers elsewhere are written as integers.
fn float_token(value: f64, key: &str) -> String {
    let real_key = matches!(
        key,
        "Axis"
            | "XY"
            | "Zone"
            | "WordSpacing"
            | "FirstLineIndent"
            | "GlyphSpacing"
            | "StartIndent"
            | "EndIndent"
            | "SpaceBefore"
            | "SpaceAfter"
            | "LetterSpacing"
            | "Values"
            | "GridSize"
            | "GridLeading"
            | "PointBase"
            | "BoxBounds"
            | "TransformPoint0"
            | "TransformPoint1"
            | "TransformPoint2"
            | "FontSize"
            | "Leading"
            | "HorizontalScale"
            | "VerticalScale"
            | "BaselineShift"
            | "Tsume"
            | "OutlineWidth"
            | "AutoLeading"
    );
    if !real_key && value.fract() == 0.0 {
        return (value as i32).to_string();
    }
    let mut token = format!("{value:.5}");
    while token.ends_with('0') && !token.ends_with(".0") {
        token.pop();
    }
    token
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(text: &str) -> TextLayer {
        TextLayer {
            text: text.into(),
            font: "AppleSDGothicNeo-Bold".into(),
            font_size: 31.52,
            faux_bold: false,
            faux_italic: false,
            leading: 37.824,
            tracking: 0,
            color: [17, 34, 51, 255],
            justification: Justification::Center,
            vertical: false,
            box_width: 180.0,
            box_height: 90.0,
            transform: [1.0, 0.0, 0.0, 1.0, 110.0, 120.0],
        }
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|window| window == needle)
    }

    fn utf16(text: &str) -> Vec<u8> {
        text.encode_utf16().flat_map(u16::to_be_bytes).collect()
    }

    #[test]
    fn engine_data_has_the_text_runs_font_and_style() {
        let data = engine_data(&sample("안녕(하세요)\n두 번째 줄"));
        // `(` is escaped inside EngineData strings.
        let mut text = b"/Text (\xFE\xFF".to_vec();
        text.extend(utf16("안녕").iter().chain(&[0, b'\\', b'(']).chain(&utf16("하세요")));
        assert!(contains(&data, &text));
        assert!(contains(&data, &utf16("두 번째 줄\r")));
        // "안녕(하세요)\r" is 8 units, "두 번째 줄\r" 7; one style run over all 15.
        assert!(contains(&data, b"/RunLengthArray [ 8 7 ]"));
        assert!(contains(&data, b"/RunLengthArray [ 15 ]"));
        assert!(contains(&data, b"/FontSize 31.52"));
        assert!(contains(&data, b"/Leading 37.824"));
        assert!(contains(&data, b"/Justification 2"));
        assert!(contains(&data, b"/BoxBounds [ 0.0 0.0 180.0 90.0 ]"));
        assert!(contains(&data, b"/Values [ 1.0 0.06667 0.13333 0.2 ]"));
        assert!(contains(&data, &utf16("AppleSDGothicNeo-Bold")));
        assert!(contains(&data, &utf16("AdobeInvisFont")));
    }

    #[test]
    fn type_tool_info_layout() {
        let layer = TextLayer { vertical: true, ..sample("세로") };
        let info = type_tool_info(&layer, 3);
        assert_eq!(&info[..2], &[0, 1]);
        assert_eq!(f64::from_be_bytes(info[34..42].try_into().unwrap()), 110.0, "tx");
        assert_eq!(&info[50..56], &[0, 50, 0, 0, 0, 16]);
        assert!(contains(&info, b"\0\0\0\0TxLr"));
        assert!(contains(&info, &[b"TEXT".as_slice(), &[0, 0, 0, 3], &utf16("세로"), &[0, 0]].concat()));
        assert!(contains(&info, b"Orntenum\0\0\0\0Ornt\0\0\0\0Vrtc"));
        assert!(contains(&info, b"\0\0\0\x09TextIndexlong\0\0\0\x03"));
        assert!(contains(&info, b"\0\0\0\x0aEngineDatatdta"));
        assert!(contains(&info, b"/WritingDirection 2"));
        // Box bounds in page pixels close the block.
        let tail: Vec<i32> =
            info[info.len() - 16..].chunks(4).map(|c| i32::from_be_bytes(c.try_into().unwrap())).collect();
        assert_eq!(tail, [110, 120, 290, 210]);
    }

    #[test]
    fn rotated_box_bounds() {
        let (sin, cos) = 90f64.to_radians().sin_cos();
        let layer = TextLayer { transform: [cos, sin, -sin, cos, 100.0, 0.0], ..sample("x") };
        let [left, top, right, bottom] = layer.page_bounds().map(f64::round);
        assert_eq!([left, top, right, bottom], [10.0, 0.0, 100.0, 180.0]);
    }

    #[test]
    fn float_tokens() {
        assert_eq!(float_token(14.0, "FontSize"), "14.0");
        assert_eq!(float_token(14.0, "Tracking"), "14");
        assert_eq!(float_token(0.5, "Kerning"), "0.5");
        assert_eq!(float_token(1.0 / 3.0, "Values"), "0.33333");
        assert_eq!(float_token(-2.25, "XY"), "-2.25");
    }
}
