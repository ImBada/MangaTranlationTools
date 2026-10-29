//! Result report (`resultReport.ts`, `buildPageReportRows` from `useResultReport.ts`).
//!
//! Builds one row per translation block (source/output crops, text, resolved style values)
//! and renders them into a self-contained, sortable/filterable HTML document. The only
//! intended difference from the TS document is the "이동" (focus block) button: besides the
//! `window.opener.postMessage` path, the native app passes a local `focus_url` that the
//! report pings with `?page=<id>&block=<id>`.

use std::collections::HashSet;
use std::fmt::Write as _;
use std::io::Cursor;

use base64::Engine as _;
use image::{RgbaImage, imageops};
use mtt_core::geometry::{bbox_to_pixels, clamp};
use mtt_core::model::{BBox, FontPreset, PageRecord, StyleKey, TranslationBlock};
use mtt_render::effects::FilterKind;
use mtt_render::layout::secondary_outline_enabled;

/// `RESULT_REPORT_FOCUS_BLOCK_MESSAGE`: `type` of the message posted to `window.opener`.
pub const FOCUS_BLOCK_MESSAGE: &str = "manga-result-report:focus-block";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// `ResultReportRow`. Crops are PNG data URLs.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ReportRow {
    pub row_index: usize,
    pub page_index: usize,
    pub page_name: String,
    pub page_size: String,
    pub page_id: String,
    pub block_index: usize,
    pub block_id: String,
    /// TS `type`.
    pub block_type: String,
    pub source_crop_data_url: String,
    pub output_crop_data_url: String,
    pub source_rect: PixelRect,
    pub output_rect: PixelRect,
    pub source_text: String,
    pub translated_text: String,
    pub font_preset_name: String,
    pub font_preset_links: String,
    pub font_family: String,
    pub character_font_overrides: String,
    pub font_size_px: f64,
    pub font_weight: Option<f64>,
    pub font_style: String,
    pub text_decoration: String,
    pub line_height: f64,
    pub letter_spacing_px: f64,
    pub text_align: String,
    pub text_position: String,
    pub text_padding_px: f64,
    pub render_direction: String,
    pub source_direction: String,
    pub rotation_deg: f64,
    pub text_color: String,
    pub background_color: String,
    pub opacity: f64,
    pub outline: String,
    pub secondary_outline: String,
    pub shadow: String,
    pub screentone: String,
    pub filter_effect: String,
    pub auto_fit_text: String,
    pub inpainted: String,
    pub source_rect_label: String,
    pub output_rect_label: String,
    pub confidence: f64,
    pub inpaint_status: String,
    pub inpaint_settings: String,
    pub updated_at: String,
}

/// Options of `createResultReportRow`.
#[derive(Debug, Clone)]
pub struct RowInput<'a> {
    pub block: &'a TranslationBlock,
    /// 1-based.
    pub block_index: usize,
    pub font_preset: Option<&'a FontPreset>,
    pub output_crop_data_url: String,
    pub output_rect: PixelRect,
    pub page: &'a PageRecord,
    /// 1-based chapter page number.
    pub page_index: usize,
    pub row_index: usize,
    pub source_crop_data_url: String,
    pub source_rect: PixelRect,
}

/// `LINK_FIELD_LABELS`: the preset-linked flags listed in the "프리셋 링크" column, in order.
const LINK_FIELD_LABELS: &[(StyleKey, &str)] = &[
    (StyleKey::FontSizePx, "크기"),
    (StyleKey::LineHeight, "줄높이"),
    (StyleKey::LetterSpacingPx, "자간"),
    (StyleKey::OutlineColor, "외곽선색"),
    (StyleKey::OutlineWidthPx, "외곽선두께"),
    (StyleKey::SecondaryOutlineEnabled, "2차개수"),
    (StyleKey::SecondaryOutlineColor, "2차색"),
    (StyleKey::SecondaryOutlineWidthPx, "2차두께"),
    (StyleKey::ShadowEnabled, "그림자"),
    (StyleKey::ShadowColor, "그림자색"),
    (StyleKey::ShadowOpacity, "그림자불투명도"),
    (StyleKey::ShadowBlurPx, "그림자흐림"),
    (StyleKey::ShadowAngleDeg, "그림자각도"),
    (StyleKey::ShadowDistancePx, "그림자거리"),
    (StyleKey::AutoFitText, "자동맞춤"),
    (StyleKey::TextColor, "글자색"),
    (StyleKey::ScreentoneFillEnabled, "스크린톤"),
    (StyleKey::ScreentoneFillIntensity, "스크린톤강도"),
    (StyleKey::ScreentoneFillDensity, "스크린톤밀도"),
    (StyleKey::ScreentoneFillAntialias, "스크린톤AA"),
    (StyleKey::FilterEffectEnabled, "필터효과"),
    // `FONT_FILTER_EFFECT_DEFINITIONS`: `[enabled, label]`, `[intensity, label + "강도"]`.
    (StyleKey::FilterZigzagEnabled, "지그재그"),
    (StyleKey::FilterZigzagIntensity, "지그재그강도"),
    (StyleKey::FilterWaveEnabled, "물결"),
    (StyleKey::FilterWaveIntensity, "물결강도"),
    (StyleKey::FilterGlitchEnabled, "글리치"),
    (StyleKey::FilterGlitchIntensity, "글리치강도"),
    (StyleKey::FilterPixelateEnabled, "픽셀화"),
    (StyleKey::FilterPixelateIntensity, "픽셀화강도"),
    (StyleKey::FilterBlurEnabled, "번짐"),
    (StyleKey::FilterBlurIntensity, "번짐강도"),
    (StyleKey::FilterNoiseEnabled, "노이즈"),
    (StyleKey::FilterNoiseIntensity, "노이즈강도"),
    (StyleKey::FontWeight, "굵기"),
    (StyleKey::FontStyle, "스타일"),
    (StyleKey::TextDecoration, "장식"),
];

enum CellValue<'a> {
    Number(f64),
    Null,
    Text(&'a str),
}

type CellAccessor = for<'a> fn(&'a ReportRow) -> CellValue<'a>;

enum ColumnSource {
    Value(CellAccessor),
    SourceImage,
    OutputImage,
}

struct ReportColumn {
    key: &'static str,
    label: &'static str,
    sort_type: &'static str,
    source: ColumnSource,
    class_name: &'static str,
}

const fn text(key: &'static str, label: &'static str, accessor: CellAccessor) -> ReportColumn {
    ReportColumn { key, label, sort_type: "text", source: ColumnSource::Value(accessor), class_name: "" }
}

const fn number(key: &'static str, label: &'static str, accessor: CellAccessor) -> ReportColumn {
    ReportColumn { key, label, sort_type: "number", source: ColumnSource::Value(accessor), class_name: "" }
}

const fn text_cell(key: &'static str, label: &'static str, accessor: CellAccessor) -> ReportColumn {
    ReportColumn { key, label, sort_type: "text", source: ColumnSource::Value(accessor), class_name: "text-cell" }
}

const fn image(key: &'static str, label: &'static str, source: ColumnSource) -> ReportColumn {
    ReportColumn { key, label, sort_type: "number", source, class_name: "image-cell" }
}

/// `REPORT_COLUMNS`. Keys stay camelCase: the report's script uses them as `data-col` ids.
const REPORT_COLUMNS: &[ReportColumn] = &[
    number("rowIndex", "순번", |row| CellValue::Number(row.row_index as f64)),
    number("pageIndex", "페이지", |row| CellValue::Number(row.page_index as f64)),
    text("pageName", "페이지명", |row| CellValue::Text(&row.page_name)),
    text("pageSize", "페이지 크기", |row| CellValue::Text(&row.page_size)),
    number("blockIndex", "블록", |row| CellValue::Number(row.block_index as f64)),
    text("type", "유형", |row| CellValue::Text(&row.block_type)),
    image("sourceImage", "원본 이미지", ColumnSource::SourceImage),
    image("outputImage", "최종 아웃풋", ColumnSource::OutputImage),
    text_cell("sourceText", "원문", |row| CellValue::Text(&row.source_text)),
    text_cell("translatedText", "번역문", |row| CellValue::Text(&row.translated_text)),
    text("fontPresetName", "폰트 프리셋", |row| CellValue::Text(&row.font_preset_name)),
    text("fontPresetLinks", "프리셋 링크", |row| CellValue::Text(&row.font_preset_links)),
    text("fontFamily", "폰트", |row| CellValue::Text(&row.font_family)),
    text("characterFontOverrides", "문자별 폰트", |row| CellValue::Text(&row.character_font_overrides)),
    number("fontSizePx", "크기", |row| CellValue::Number(row.font_size_px)),
    number("fontWeight", "굵기", |row| row.font_weight.map_or(CellValue::Null, CellValue::Number)),
    text("fontStyle", "스타일", |row| CellValue::Text(&row.font_style)),
    text("textDecoration", "장식", |row| CellValue::Text(&row.text_decoration)),
    number("lineHeight", "줄높이", |row| CellValue::Number(row.line_height)),
    number("letterSpacingPx", "자간", |row| CellValue::Number(row.letter_spacing_px)),
    text("textAlign", "정렬", |row| CellValue::Text(&row.text_align)),
    text("textPosition", "위치", |row| CellValue::Text(&row.text_position)),
    number("textPaddingPx", "패딩", |row| CellValue::Number(row.text_padding_px)),
    text("renderDirection", "렌더 방향", |row| CellValue::Text(&row.render_direction)),
    text("sourceDirection", "원문 방향", |row| CellValue::Text(&row.source_direction)),
    number("rotationDeg", "회전", |row| CellValue::Number(row.rotation_deg)),
    text("textColor", "글자색", |row| CellValue::Text(&row.text_color)),
    text("backgroundColor", "배경/불투명도", |row| CellValue::Text(&row.background_color)),
    text("outline", "외곽선", |row| CellValue::Text(&row.outline)),
    text("secondaryOutline", "2차 외곽선", |row| CellValue::Text(&row.secondary_outline)),
    text("shadow", "그림자", |row| CellValue::Text(&row.shadow)),
    text("screentone", "스크린톤", |row| CellValue::Text(&row.screentone)),
    text("filterEffect", "필터 효과", |row| CellValue::Text(&row.filter_effect)),
    text("autoFitText", "자동맞춤", |row| CellValue::Text(&row.auto_fit_text)),
    text("inpainted", "블록 인페인트", |row| CellValue::Text(&row.inpainted)),
    text("sourceRectLabel", "원본 범위", |row| CellValue::Text(&row.source_rect_label)),
    text("outputRectLabel", "출력 범위", |row| CellValue::Text(&row.output_rect_label)),
    number("confidence", "신뢰도", |row| CellValue::Number(row.confidence)),
    text("inpaintStatus", "인페인트", |row| CellValue::Text(&row.inpaint_status)),
    text("inpaintSettings", "인페인트 설정", |row| CellValue::Text(&row.inpaint_settings)),
    ReportColumn {
        key: "updatedAt",
        label: "페이지 수정일",
        sort_type: "date",
        source: ColumnSource::Value(|row| CellValue::Text(&row.updated_at)),
        class_name: "",
    },
    text("pageId", "페이지 ID", |row| CellValue::Text(&row.page_id)),
    text("blockId", "블록 ID", |row| CellValue::Text(&row.block_id)),
];

/// `cropLoadedImageDataUrl`: copies `rect` of `image` into a `max(1, w)×max(1, h)` canvas
/// (the part outside the image stays transparent) and encodes it as a PNG data URL.
pub fn crop_data_url(image: &RgbaImage, rect: PixelRect) -> String {
    let mut canvas = RgbaImage::new(rect.width.max(1), rect.height.max(1));
    if rect.width > 0 && rect.height > 0 {
        let visible = imageops::crop_imm(image, rect.x, rect.y, rect.width, rect.height).to_image();
        imageops::replace(&mut canvas, &visible, 0, 0);
    }
    let mut bytes = Vec::new();
    if canvas.write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png).is_err() {
        // `canvas.toDataURL` falls back to an empty data URL when it cannot encode.
        return "data:,".to_owned();
    }
    format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(bytes))
}

/// `resolveBlockReportRects`: `(source_rect, output_rect)` in page pixels.
pub fn block_report_rects(page: &PageRecord, block: &TranslationBlock) -> (PixelRect, PixelRect) {
    let source_rect = resolve_pixel_rect(page, block.bbox, block.bbox_space.as_deref());
    let output_rect = match block.render_bbox {
        Some(render_bbox) => {
            resolve_pixel_rect(page, render_bbox, block.render_bbox_space.as_deref().or(block.bbox_space.as_deref()))
        }
        None => resolve_pixel_rect(page, block.bbox, block.bbox_space.as_deref()),
    };
    (source_rect, output_rect)
}

/// `createResultReportRow`.
pub fn create_row(input: RowInput<'_>) -> ReportRow {
    let RowInput {
        block,
        block_index,
        font_preset,
        output_crop_data_url,
        output_rect,
        page,
        page_index,
        row_index,
        source_crop_data_url,
        source_rect,
    } = input;
    let style = &block.style;
    ReportRow {
        row_index,
        page_index,
        page_name: page.name.clone(),
        page_size: format!("{}×{}", page.width, page.height),
        page_id: page.id.clone(),
        block_index,
        block_id: block.id.clone(),
        block_type: block.block_type.as_str().to_owned(),
        source_crop_data_url,
        output_crop_data_url,
        source_rect,
        output_rect,
        source_text: block.source_text.clone(),
        translated_text: block.translated_text.clone(),
        font_preset_name: resolve_font_preset_name(block, font_preset),
        font_preset_links: format_linked_preset_fields(block),
        font_family: style.font_family.clone().unwrap_or_default(),
        character_font_overrides: format_character_font_overrides(block),
        font_size_px: block.font_size_px(),
        font_weight: style.font_weight,
        font_style: style.font_style.map_or("normal", |value| value.as_str()).to_owned(),
        text_decoration: style.text_decoration.map_or("none", |value| value.as_str()).to_owned(),
        line_height: block.line_height(),
        letter_spacing_px: style.letter_spacing_px.unwrap_or(0.0),
        text_align: block.text_align.as_str().to_owned(),
        text_position: block.text_position.map_or("center", |value| value.as_str()).to_owned(),
        text_padding_px: block.text_padding_px.unwrap_or(0.0),
        render_direction: block.render_direction.as_str().to_owned(),
        source_direction: block.source_direction.as_str().to_owned(),
        rotation_deg: block.rotation_deg.unwrap_or(0.0),
        text_color: block.text_color().to_owned(),
        background_color: format!("{} / {}", block.background_color, format_number(block.opacity)),
        opacity: block.opacity,
        outline: format_outline(style.outline_color.as_deref(), style.outline_width_px),
        secondary_outline: if secondary_outline_enabled(block) {
            format_outline(style.secondary_outline_color.as_deref(), style.secondary_outline_width_px)
        } else {
            "꺼짐".to_owned()
        },
        shadow: format_shadow(block),
        screentone: format_screentone(block),
        filter_effect: format_filter_effect(block),
        auto_fit_text: format_boolean(style.auto_fit_text).to_owned(),
        inpainted: format_boolean(block.inpainted).to_owned(),
        source_rect_label: format_rect(source_rect),
        output_rect_label: format_rect(output_rect),
        confidence: block.confidence,
        inpaint_status: page.inpaint_status.map_or("idle", |status| status.as_str()).to_owned(),
        inpaint_settings: format_inpaint_settings(page),
        updated_at: page.updated_at.clone(),
    }
}

/// `buildPageReportRows`: one row per block of `page`, cropping `source` (page image) and
/// `output` (rendered final page). `page_index` is the 1-based chapter page number and
/// `first_row_index` the running 1-based row counter.
pub fn build_page_rows(
    page: &PageRecord,
    source: &RgbaImage,
    output: &RgbaImage,
    page_index: usize,
    first_row_index: usize,
    presets: &[FontPreset],
) -> Vec<ReportRow> {
    let mut rows = Vec::with_capacity(page.blocks.len());
    for (block_index, block) in page.blocks.iter().enumerate() {
        let (source_rect, output_rect) = block_report_rects(page, block);
        let source_crop_data_url = crop_data_url(source, source_rect);
        let output_crop_data_url = crop_data_url(output, output_rect);
        // `new Map(presets.map(...))`: a later preset with the same id wins.
        let font_preset = font_preset_id(block).and_then(|id| presets.iter().rev().find(|preset| preset.id == id));
        let row_index = first_row_index + rows.len();
        rows.push(create_row(RowInput {
            block,
            block_index: block_index + 1,
            font_preset,
            output_crop_data_url,
            output_rect,
            page,
            page_index,
            row_index,
            source_crop_data_url,
            source_rect,
        }));
    }
    rows
}

/// `buildResultReportHtml`. When `focus_url` is set, the "이동" buttons request
/// `focus_url?page=<pageId>&block=<blockId>` (the native app has no `window.opener`).
pub fn build_html(chapter_title: &str, generated_at: &str, rows: &[ReportRow], focus_url: Option<&str>) -> String {
    let safe_title = escape_html(chapter_title);
    let block_count = rows.len();
    let page_count = rows.iter().map(|row| row.page_id.as_str()).collect::<HashSet<_>>().len();
    let main = if rows.is_empty() {
        r#"<div class="empty">보고서에 포함할 텍스트 블록이 없습니다.</div>"#.to_owned()
    } else {
        build_report_table(rows)
    };

    let mut html = String::with_capacity(
        HTML_HEAD.len() + HTML_STYLE.len() + HTML_SCRIPT_START.len() + HTML_SCRIPT_END.len() + main.len() + 1024,
    );
    html.push_str(HTML_HEAD);
    html.push_str(&safe_title);
    html.push_str(HTML_STYLE);
    html.push_str(&safe_title);
    let _ = write!(
        html,
        r#" 결과 보고서</h1>
      <div class="meta">
        <span class="chip">생성: {generated_at}</span>
        <span class="chip">페이지: {page_count}</span>
        <span class="chip">블록: {block_count}</span>
      </div>
    </div>
  </header>
  <main>
    {main}
  </main>
  <script>
    (() => {{"#,
        generated_at = escape_html(generated_at),
    );
    html.push_str(HTML_SCRIPT_START);
    let _ = write!(
        html,
        "\n      const focusMessageType = {};\n      const focusUrl = {};",
        script_json(FOCUS_BLOCK_MESSAGE),
        focus_url.map_or_else(|| "null".to_owned(), script_json),
    );
    html.push_str(HTML_SCRIPT_END);
    html
}

fn build_report_table(rows: &[ReportRow]) -> String {
    let headers: String = REPORT_COLUMNS.iter().map(build_header_cell).collect();
    let body: String = rows.iter().map(build_body_row).collect();
    let count = rows.len();
    format!(
        r#"<div class="report-controls">
    <label class="global-filter">
      <span>전체 필터</span>
      <input type="search" data-global-filter placeholder="전체 컬럼 검색">
    </label>
    <button type="button" class="clear-filters" data-clear-filters>필터 초기화</button>
    <span class="filter-count" data-filter-count>{count} / {count} 표시</span>
  </div>
  <div class="table-wrap">
    <table data-report-table>
      <thead>
        <tr>{headers}<th class="action-header" aria-sort="none">이동</th></tr>
      </thead>
      <tbody>{body}</tbody>
    </table>
  </div>"#
    )
}

fn build_header_cell(column: &ReportColumn) -> String {
    let key = escape_html(column.key);
    format!(
        r#"<th data-sort-key="{key}" data-sort-type="{sort_type}" aria-sort="none"><button type="button">{label}</button><input class="column-filter" type="search" data-filter-key="{key}" aria-label="{aria_label}" placeholder="필터"></th>"#,
        sort_type = column.sort_type,
        label = escape_html(column.label),
        aria_label = escape_html(&format!("{} 필터", column.label)),
    )
}

fn build_body_row(row: &ReportRow) -> String {
    let mut html = String::from("<tr>");
    for column in REPORT_COLUMNS {
        html.push_str(&build_body_cell(row, column));
    }
    html.push_str(&build_action_cell(row));
    html.push_str("</tr>");
    html
}

fn build_body_cell(row: &ReportRow, column: &ReportColumn) -> String {
    let (rect, data_url, rect_label, alt_suffix) = match column.source {
        ColumnSource::Value(accessor) => {
            let value = accessor(row);
            return format!(
                r#"<td class="{class}" data-col="{key}" data-sort="{sort}">{content}</td>"#,
                class = column.class_name,
                key = escape_html(column.key),
                sort = escape_html(&resolve_sort_value(&value)),
                content = escape_html(&format_cell_value(&value)),
            );
        }
        ColumnSource::SourceImage => (row.source_rect, &row.source_crop_data_url, &row.source_rect_label, "원본"),
        ColumnSource::OutputImage => (row.output_rect, &row.output_crop_data_url, &row.output_rect_label, "최종 출력"),
    };
    let sort_value = u64::from(rect.width) * u64::from(rect.height);
    format!(
        r#"<td class="{class}" data-col="{key}" data-sort="{sort_value}">
      <img class="crop" src="{src}" alt="{alt}">
      <span class="crop-meta">{label}</span>
    </td>"#,
        class = column.class_name,
        key = escape_html(column.key),
        src = escape_html(data_url),
        alt = escape_html(&format!("{} {} {alt_suffix}", row.page_name, row.block_index)),
        label = escape_html(rect_label),
    )
}

fn build_action_cell(row: &ReportRow) -> String {
    format!(
        r#"<td class="action-cell"><button type="button" class="jump-button" data-focus-block data-page-id="{}" data-block-id="{}">이동</button></td>"#,
        escape_html(&row.page_id),
        escape_html(&row.block_id),
    )
}

fn resolve_pixel_rect(page: &PageRecord, bbox: BBox, space: Option<&str>) -> PixelRect {
    let page_width = f64::from(page.width);
    let page_height = f64::from(page.height);
    let pixel_box = if space == Some("pixels") { bbox } else { bbox_to_pixels(bbox, page_width, page_height) };
    let left = clamp(pixel_box.x, 0.0, (page_width - 1.0).max(0.0)).floor();
    let top = clamp(pixel_box.y, 0.0, (page_height - 1.0).max(0.0)).floor();
    let right = clamp(pixel_box.x + pixel_box.w, left + 1.0, page_width).ceil();
    let bottom = clamp(pixel_box.y + pixel_box.h, top + 1.0, page_height).ceil();
    PixelRect {
        x: left as u32,
        y: top as u32,
        width: (right - left).max(1.0) as u32,
        height: (bottom - top).max(1.0) as u32,
    }
}

/// `block.fontPresetId` when truthy (an empty id counts as "no preset" like in JS).
fn font_preset_id(block: &TranslationBlock) -> Option<&str> {
    block.font_preset_id.as_deref().filter(|id| !id.is_empty())
}

fn resolve_font_preset_name(block: &TranslationBlock, font_preset: Option<&FontPreset>) -> String {
    let Some(preset_id) = font_preset_id(block) else {
        return "개별값".to_owned();
    };
    match font_preset {
        Some(preset) => preset.name.clone(),
        None => format!("삭제됨: {preset_id}"),
    }
}

fn format_linked_preset_fields(block: &TranslationBlock) -> String {
    if font_preset_id(block).is_none() {
        return String::new();
    }
    let labels: Vec<&str> = LINK_FIELD_LABELS
        .iter()
        .filter(|(key, _)| block.links.get(*key) != Some(false))
        .map(|(_, label)| *label)
        .collect();
    if labels.is_empty() { "없음".to_owned() } else { labels.join(", ") }
}

fn format_character_font_overrides(block: &TranslationBlock) -> String {
    block
        .style
        .character_font_overrides
        .as_ref()
        .map(|overrides| {
            overrides
                .iter()
                .map(|item| format!("{}: {}", item.character, item.font_family))
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

fn format_outline(color: Option<&str>, width_px: Option<f64>) -> String {
    let width = width_px.unwrap_or(0.0);
    if width > 0.0 { format!("{}px {}", format_number(width), color.unwrap_or("#000000")) } else { "없음".to_owned() }
}

fn format_shadow(block: &TranslationBlock) -> String {
    let style = &block.style;
    let enabled = style
        .shadow_enabled
        .unwrap_or(style.shadow_distance_px.unwrap_or(0.0) > 0.0 || style.shadow_blur_px.unwrap_or(0.0) > 0.0);
    if !enabled {
        return "꺼짐".to_owned();
    }
    format!(
        "{} / {}% / 흐림 {}px / {}deg / {}px",
        style.shadow_color.as_deref().unwrap_or("#000000"),
        format_number(style.shadow_opacity.unwrap_or(1.0) * 100.0),
        format_number(style.shadow_blur_px.unwrap_or(0.0)),
        format_number(style.shadow_angle_deg.unwrap_or(0.0)),
        format_number(style.shadow_distance_px.unwrap_or(0.0)),
    )
}

fn format_screentone(block: &TranslationBlock) -> String {
    let style = &block.style;
    if style.screentone_fill_enabled != Some(true) {
        return "꺼짐".to_owned();
    }
    format!(
        "강도 {}, 밀도 {}, AA {}",
        format_number(style.screentone_fill_intensity.unwrap_or(0.0)),
        format_number(style.screentone_fill_density.unwrap_or(0.0)),
        format_boolean(style.screentone_fill_antialias),
    )
}

fn format_filter_effect(block: &TranslationBlock) -> String {
    if block.style.filter_effect_enabled != Some(true) {
        return "꺼짐".to_owned();
    }
    let active: Vec<String> = FilterKind::ALL
        .iter()
        .filter(|kind| kind.is_active(block))
        .map(|kind| format!("{} 강도 {}%", kind.label(), format_number(kind.intensity(block) * 100.0)))
        .collect();
    if active.is_empty() { "켜짐 / 활성 필터 없음".to_owned() } else { active.join(", ") }
}

fn format_inpaint_settings(page: &PageRecord) -> String {
    let Some(settings) = &page.inpaint_settings else {
        return String::new();
    };
    let cleanup =
        settings.artifact_cleanup_px.map(|value| format!(", cleanup {}", js_number_string(value))).unwrap_or_default();
    format!(
        "{}, padding {}, feather {}, tile {}{cleanup}{}",
        settings.engine.as_str(),
        js_number_string(settings.padding_px),
        js_number_string(settings.feather_px),
        js_number_string(settings.tile_size),
        if settings.aliased { ", aliased" } else { "" },
    )
}

fn format_boolean(value: Option<bool>) -> &'static str {
    if value == Some(true) { "켜짐" } else { "꺼짐" }
}

fn format_rect(rect: PixelRect) -> String {
    format!("{}, {}, {}×{}", rect.x, rect.y, rect.width, rect.height)
}

/// `formatNumber`: integers as `String(value)`, otherwise `toFixed(3)` without trailing zeros.
fn format_number(value: f64) -> String {
    if value.is_finite() && value.fract() == 0.0 {
        return js_number_string(value);
    }
    let fixed = js_to_fixed_3(value);
    let trimmed = fixed.trim_end_matches('0');
    trimmed.strip_suffix('.').unwrap_or(trimmed).to_owned()
}

/// `Number.prototype.toFixed(3)`: exact rounding with ties going to the larger magnitude.
fn js_to_fixed_3(value: f64) -> String {
    if !value.is_finite() || value.abs() >= 1e21 {
        return js_number_string(value);
    }
    let sign = if value < 0.0 { "-" } else { "" };
    let magnitude = value.abs();
    // A binary double sits exactly halfway between two 3-decimal values only when it is an
    // odd multiple of 1/16; Rust rounds those ties to even, JS rounds them up.
    let sixteenths = magnitude * 16.0;
    if sixteenths.fract() == 0.0 && sixteenths < 9_007_199_254_740_992.0 && sixteenths % 2.0 == 1.0 {
        let thousandths = (sixteenths as u64 * 125).div_ceil(2);
        return format!("{sign}{}.{:03}", thousandths / 1000, thousandths % 1000);
    }
    format!("{sign}{magnitude:.3}")
}

/// `String(value)` for a JS number.
fn js_number_string(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_owned();
    }
    if value.is_infinite() {
        return if value > 0.0 { "Infinity" } else { "-Infinity" }.to_owned();
    }
    if value == 0.0 {
        return "0".to_owned();
    }
    let magnitude = value.abs();
    if !(1e-6..1e21).contains(&magnitude) {
        let formatted = format!("{value:e}");
        return match formatted.split_once('e') {
            Some((mantissa, exponent)) if !exponent.starts_with('-') => format!("{mantissa}e+{exponent}"),
            _ => formatted,
        };
    }
    format!("{value}")
}

fn resolve_sort_value(value: &CellValue<'_>) -> String {
    match value {
        CellValue::Number(number) => js_number_string(*number),
        CellValue::Null => String::new(),
        CellValue::Text(text) => (*text).to_owned(),
    }
}

fn format_cell_value(value: &CellValue<'_>) -> String {
    match value {
        CellValue::Number(number) => format_number(*number),
        CellValue::Null => String::new(),
        CellValue::Text(text) => (*text).to_owned(),
    }
}

/// `escapeHtml` / `escapeAttribute`.
fn escape_html(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

/// `JSON.stringify` for embedding a string in the inline script (`<` escaped so a value
/// can never close the `<script>` element).
fn script_json(value: &str) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "\"\"".to_owned()).replace('<', "\\u003c")
}

const HTML_HEAD: &str = r#"<!doctype html>
<html lang="ko">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>"#;

const HTML_STYLE: &str = r##" 결과 보고서</title>
  <style>
    :root {
      color-scheme: light;
      font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
      background: #f5f7fa;
      color: #17202a;
    }
    * { box-sizing: border-box; }
    body { margin: 0; background: #f5f7fa; }
    header {
      position: sticky;
      top: 0;
      z-index: 5;
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 16px;
      padding: 16px 20px;
      border-bottom: 1px solid #d6dde6;
      background: rgba(250, 252, 255, 0.96);
      backdrop-filter: blur(10px);
    }
    h1 { margin: 0; font-size: 20px; line-height: 1.25; }
    .meta { display: flex; flex-wrap: wrap; gap: 8px; color: #52616f; font-size: 12px; }
    .chip { padding: 4px 8px; border: 1px solid #d6dde6; border-radius: 999px; background: #ffffff; }
    main { padding: 16px 20px 28px; }
    .report-controls {
      display: flex;
      align-items: center;
      gap: 10px;
      flex-wrap: wrap;
      margin-bottom: 10px;
    }
    .global-filter {
      display: grid;
      grid-template-columns: auto minmax(220px, 360px);
      align-items: center;
      gap: 8px;
      color: #425160;
      font-size: 12px;
      font-weight: 700;
    }
    .global-filter input,
    .column-filter {
      min-width: 0;
      height: 30px;
      border: 1px solid #c8d1dc;
      border-radius: 6px;
      background: #ffffff;
      color: #17202a;
      font: inherit;
    }
    .global-filter input {
      width: 100%;
      padding: 5px 8px;
    }
    .column-filter {
      display: block;
      width: calc(100% - 12px);
      height: 26px;
      margin: 0 6px 7px;
      padding: 4px 6px;
      font-size: 11px;
    }
    .clear-filters {
      min-height: 30px;
      border: 1px solid #c8d1dc;
      border-radius: 6px;
      background: #ffffff;
      color: #25313d;
      font-size: 12px;
      font-weight: 700;
      cursor: pointer;
    }
    .filter-count { color: #52616f; font-size: 12px; }
    .jump-button {
      min-height: 30px;
      border: 1px solid #bfd0e3;
      border-radius: 6px;
      background: #eef5ff;
      color: #174a83;
      font-size: 12px;
      font-weight: 700;
      white-space: nowrap;
      cursor: pointer;
    }
    .action-header,
    .action-cell {
      min-width: 104px;
      text-align: center;
    }
    .table-wrap {
      overflow: auto;
      max-height: calc(100vh - 150px);
      border: 1px solid #d6dde6;
      background: #ffffff;
    }
    table { width: max-content; min-width: 100%; border-collapse: separate; border-spacing: 0; }
    th, td { border-right: 1px solid #dfe5ec; border-bottom: 1px solid #dfe5ec; vertical-align: top; }
    th {
      position: sticky;
      top: 0;
      z-index: 2;
      background: #eef3f8;
      color: #25313d;
      font-size: 12px;
      text-align: left;
      white-space: nowrap;
    }
    th button {
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 8px;
      width: 100%;
      min-height: 34px;
      border: 0;
      padding: 8px 10px;
      background: transparent;
      color: inherit;
      font: inherit;
      cursor: pointer;
    }
    th button::after { content: "↕"; color: #7b8794; font-size: 11px; }
    th[aria-sort="ascending"] button::after { content: "↑"; color: #1f6feb; }
    th[aria-sort="descending"] button::after { content: "↓"; color: #1f6feb; }
    th:focus-within { background: #e5edf5; }
    td { max-width: 320px; padding: 8px 10px; font-size: 12px; line-height: 1.45; background: #ffffff; }
    tbody tr:nth-child(even) td { background: #fbfcfe; }
    .image-cell { min-width: 180px; max-width: 240px; }
    .crop {
      display: block;
      max-width: 210px;
      max-height: 210px;
      width: auto;
      height: auto;
      border: 1px solid #c8d1dc;
      background: #f1f4f8;
      object-fit: contain;
    }
    .crop-meta { display: block; margin-top: 6px; color: #617080; font-size: 11px; white-space: nowrap; }
    .text-cell { min-width: 220px; white-space: pre-wrap; }
    .empty {
      padding: 28px;
      border: 1px solid #d6dde6;
      background: #ffffff;
      color: #52616f;
    }
  </style>
</head>
<body>
  <header>
    <div>
      <h1>"##;

/// Inline script up to the `focusMessageType` / `focusUrl` constants.
const HTML_SCRIPT_START: &str = r##"
      const table = document.querySelector("[data-report-table]");
      if (!table) return;
      const tbody = table.querySelector("tbody");
      const headers = [...table.querySelectorAll("th[data-sort-key]")];
      const rows = [...tbody.querySelectorAll("tr")];
      const globalFilter = document.querySelector("[data-global-filter]");
      const filterInputs = [...table.querySelectorAll("[data-filter-key]")];
      const clearFilters = document.querySelector("[data-clear-filters]");
      const filterCount = document.querySelector("[data-filter-count]");
      const focusButtons = [...table.querySelectorAll("[data-focus-block]")];"##;

/// Rest of the inline script after the `focusMessageType` / `focusUrl` constants.
const HTML_SCRIPT_END: &str = r##"
      let state = { key: "", direction: "asc" };

      const getSortValue = (row, key) => row.querySelector('[data-col="' + key + '"]')?.dataset.sort ?? "";
      const getFilterValue = (row, key) => row.querySelector('[data-col="' + key + '"]')?.textContent ?? "";
      const getRowFilterText = (row) => [...row.querySelectorAll("[data-col]")]
        .map((cell) => cell.textContent || "")
        .join(" ");
      const normalizeFilter = (value) => value.toLocaleLowerCase("ko-KR").trim();
      const compareText = (left, right) => left.localeCompare(right, "ko", { numeric: true, sensitivity: "base" });
      const compareRows = (key, type, direction) => (leftRow, rightRow) => {
        const left = getSortValue(leftRow, key);
        const right = getSortValue(rightRow, key);
        let result;
        if (type === "number") {
          result = (Number.parseFloat(left) || 0) - (Number.parseFloat(right) || 0);
        } else if (type === "date") {
          result = (Date.parse(left) || 0) - (Date.parse(right) || 0);
        } else {
          result = compareText(left, right);
        }
        return direction === "asc" ? result : -result;
      };
      const applyFilters = () => {
        const globalQuery = normalizeFilter(globalFilter?.value ?? "");
        const columnQueries = filterInputs
          .map((input) => ({ key: input.dataset.filterKey || "", query: normalizeFilter(input.value || "") }))
          .filter(({ key, query }) => key && query);
        let visible = 0;
        rows.forEach((row) => {
          const rowText = normalizeFilter(getRowFilterText(row));
          const globalMatch = !globalQuery || rowText.includes(globalQuery);
          const columnMatch = columnQueries.every(({ key, query }) => normalizeFilter(getFilterValue(row, key)).includes(query));
          const matched = globalMatch && columnMatch;
          row.hidden = !matched;
          if (matched) visible += 1;
        });
        if (filterCount) {
          filterCount.textContent = visible + " / " + rows.length + " 표시";
        }
      };

      headers.forEach((header) => {
        header.querySelector("button")?.addEventListener("click", () => {
          const key = header.dataset.sortKey || "";
          const type = header.dataset.sortType || "text";
          const direction = state.key === key && state.direction === "asc" ? "desc" : "asc";
          state = { key, direction };
          headers.forEach((candidate) => candidate.setAttribute("aria-sort", "none"));
          header.setAttribute("aria-sort", direction === "asc" ? "ascending" : "descending");
          [...tbody.querySelectorAll("tr")]
            .sort(compareRows(key, type, direction))
            .forEach((row) => tbody.appendChild(row));
        });
      });
      focusButtons.forEach((button) => {
        button.addEventListener("click", () => {
          const pageId = button.dataset.pageId || "";
          const blockId = button.dataset.blockId || "";
          if (!pageId || !blockId) {
            return;
          }
          if (window.opener && !window.opener.closed) {
            window.opener.postMessage({ type: focusMessageType, pageId, blockId }, "*");
            window.opener.focus?.();
          }
          if (focusUrl) {
            fetch(focusUrl + "?page=" + encodeURIComponent(pageId) + "&block=" + encodeURIComponent(blockId), { mode: "no-cors" })
              .catch(() => {});
          }
        });
      });
      globalFilter?.addEventListener("input", applyFilters);
      filterInputs.forEach((input) => input.addEventListener("input", applyFilters));
      clearFilters?.addEventListener("click", () => {
        if (globalFilter) globalFilter.value = "";
        filterInputs.forEach((input) => { input.value = ""; });
        applyFilters();
      });
      applyFilters();
    })();
  </script>
</body>
</html>"##;

// ---- editor integration (`useResultReport.ts`) ------------------------------------------

/// `ResultReportProgress`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportProgress {
    pub current: usize,
    pub total: usize,
    pub label: String,
}

pub enum ReportEvent {
    Progress(ReportProgress),
    /// HTML file path and block count.
    Finished(Result<(std::path::PathBuf, usize), String>),
}

/// `new Date().toLocaleString("ko-KR")`, e.g. `2026. 10. 27. 오후 3:04:05`.
pub fn ko_kr_timestamp(time: &chrono::DateTime<chrono::Local>) -> String {
    use chrono::{Datelike, Timelike};
    let (pm, hour) = time.hour12();
    format!(
        "{}. {}. {}. {} {}:{:02}:{:02}",
        time.year(),
        time.month(),
        time.day(),
        if pm { "오후" } else { "오전" },
        hour,
        time.minute(),
        time.second()
    )
}

impl super::Editor {
    pub fn report_running(&self) -> bool {
        self.report.is_some()
    }

    /// `generateResultReport`: saves pending edits, then builds the report off the UI thread.
    pub fn start_report(
        &mut self,
        services: &mut crate::services::Services,
        worker: &crate::worker::Worker<crate::app::AppEvent>,
    ) {
        if self.report.is_some() || self.translation_running() || self.chapter.pages.is_empty() {
            return;
        }
        let Some(fonts) = services.fonts() else {
            self.status = "폰트를 불러오는 중입니다.".into();
            return;
        };
        // Its own font context, so rendering every page doesn't hold up the editor's.
        let Ok(mut fonts) = fonts.lock().map(|fonts| fonts.fork()) else {
            self.status = "폰트 잠금 실패".into();
            return;
        };
        if let Err(error) = self.flush(services) {
            self.status = format!("저장 실패: {error}");
            return;
        }
        let total = self.chapter.pages.iter().filter(|page| !page.blocks.is_empty()).count();
        self.report = Some(ReportProgress { current: 0, total, label: "보고서 생성 시작".into() });
        self.status = format!("결과 보고서 생성 시작: {total}p");
        let library = services.library.clone();
        let chapter = self.chapter.clone();
        let focus_url = services.report_focus_url.clone();
        let emitter = worker.emitter();
        std::thread::spawn(move || {
            let chapter_id = chapter.id.clone();
            let emit = |event| emitter.emit(crate::app::AppEvent::Report { chapter_id: chapter_id.clone(), event });
            let result = generate(&library, &chapter, &mut fonts, focus_url.as_deref(), |progress| {
                emit(ReportEvent::Progress(progress))
            });
            emit(ReportEvent::Finished(result));
        });
    }

    pub fn on_report_event(&mut self, chapter_id: &str, event: ReportEvent) {
        if chapter_id != self.chapter.id {
            return;
        }
        match event {
            ReportEvent::Progress(progress) => {
                if self.report.is_some() {
                    self.report = Some(progress);
                }
            }
            ReportEvent::Finished(result) => {
                self.report = None;
                self.status = match result {
                    Ok((path, rows)) => {
                        self.last_report = Some(path);
                        format!("결과 보고서 생성 완료: {rows}개 블록. 마지막 보고서 확인 버튼으로 열 수 있습니다.")
                    }
                    Err(error) => {
                        log::error!("result report failed: {error}");
                        error
                    }
                };
            }
        }
    }

    /// `openLastResultReport`.
    pub fn open_last_report(&mut self) {
        let Some(path) = &self.last_report else { return };
        if let Err(error) = open_with_system(path) {
            self.status = format!("보고서를 열 수 없습니다: {error}");
        }
    }

    /// `focusResultReportBlock`: shows the report row's block on the overlay layer.
    pub fn focus_report_block(&mut self, page_id: &str, block_id: &str) {
        let found = self.chapter.pages.iter().enumerate().find_map(|(index, page)| {
            (page.id == page_id)
                .then(|| {
                    page.blocks.iter().position(|block| block.id == block_id).map(|b| (index, b, page.name.clone()))
                })
                .flatten()
        });
        let Some((page_index, block_index, page_name)) = found else {
            self.status = "보고서의 블록을 현재 챕터에서 찾을 수 없습니다.".into();
            return;
        };
        self.select_layer_shortcut(2);
        self.go_to_page(page_index);
        self.selection = vec![block_id.to_owned()];
        self.status = format!("{page_name} 블록 {}로 이동했습니다.", block_index + 1);
    }
}

/// Opens a file with the system's default handler (the browser for `.html`).
pub fn open_with_system(path: &std::path::Path) -> std::io::Result<()> {
    let mut command = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
    } else if cfg!(windows) {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", ""]);
        command
    } else {
        std::process::Command::new("xdg-open")
    };
    command.arg(path).spawn().map(|_| ())
}

/// Builds the report for every page with blocks, reporting progress, and writes it to a
/// temporary HTML file.
pub fn generate(
    library: &mtt_core::library::Library,
    chapter: &mtt_core::model::Chapter,
    fonts: &mut mtt_render::FontContext,
    focus_url: Option<&str>,
    mut progress: impl FnMut(ReportProgress),
) -> Result<(std::path::PathBuf, usize), String> {
    let pages: Vec<(usize, &PageRecord)> =
        chapter.pages.iter().enumerate().filter(|(_, page)| !page.blocks.is_empty()).map(|(i, p)| (i + 1, p)).collect();
    let total = pages.len();
    let presets = chapter.font_presets.as_deref().unwrap_or(&[]);
    let mut rows = Vec::new();
    for (current, (page_index, page)) in pages.into_iter().enumerate() {
        let step = |label: String| ReportProgress { current, total, label };
        progress(step(format!("{} 원본 불러오는 중", page.name)));
        let load = super::assets::decode_page(library, chapter, page);
        let source = load.source?;
        progress(step(format!("{} 최종 아웃풋 렌더 중", page.name)));
        let images = mtt_render::page::PageImages {
            source: Some(&source),
            inpaint_mask: load.mask.as_ref(),
            inpaint_result: load.result.as_ref(),
        };
        let output = mtt_render::page::render_page(fonts, page, &images, &mtt_render::page::PageRenderOptions::OUTPUT)
            .ok_or_else(|| format!("{}: 렌더링 실패", page.name))?;
        progress(step(format!("{} 블록 이미지 자르는 중", page.name)));
        let first_row_index = rows.len() + 1;
        rows.extend(build_page_rows(page, &source, &output, page_index, first_row_index, presets));
        progress(ReportProgress { current: current + 1, total, label: format!("{} 완료", page.name) });
    }
    let html = build_html(&chapter.title, &ko_kr_timestamp(&chrono::Local::now()), &rows, focus_url);
    let safe_id: String =
        chapter.id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    let dir = std::env::temp_dir().join("MangaTranslationTools-reports");
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join(format!("result-report-{safe_id}-{}.html", chrono::Local::now().format("%Y%m%d-%H%M%S")));
    std::fs::write(&path, html).map_err(|error| format!("보고서 저장 실패: {error}"))?;
    Ok((path, rows.len()))
}

/// Local endpoint the report's "이동" buttons ping (`focus_url?page=…&block=…`); stands in
/// for the TS app's `window.opener.postMessage`. Returns the focus URL.
pub fn start_focus_server(emitter: crate::worker::Emitter<crate::app::AppEvent>) -> Option<String> {
    use std::io::{BufRead, BufReader, Write};
    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).ok()?;
    let port = listener.local_addr().ok()?.port();
    // A random path segment so other local pages cannot drive the editor.
    let token = uuid::Uuid::new_v4().simple().to_string();
    let prefix = format!("/focus/{token}");
    std::thread::Builder::new()
        .name("mtt-report-focus".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut line = String::new();
                if BufReader::new(&stream).read_line(&mut line).is_err() {
                    continue;
                }
                let target = line.split_whitespace().nth(1).unwrap_or("");
                let found = target.strip_prefix(prefix.as_str()).and_then(|rest| rest.strip_prefix('?')).and_then(|query| {
                    let param = |name: &str| {
                        query.split('&').find_map(|pair| {
                            let (key, value) = pair.split_once('=')?;
                            (key == name).then(|| percent_decode(value))
                        })
                    };
                    Some((param("page")?, param("block")?))
                });
                let status = if found.is_some() { "204 No Content" } else { "404 Not Found" };
                let mut stream = stream;
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nAccess-Control-Allow-Origin: *\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
                if let Some((page_id, block_id)) = found {
                    emitter.emit(crate::app::AppEvent::ReportFocus { page_id, block_id });
                }
            }
        })
        .ok()?;
    Some(format!("http://127.0.0.1:{port}/focus/{token}"))
}

/// Decodes `%XX` escapes and `+` from a query value.
fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let escaped = (bytes[index] == b'%')
            .then(|| bytes.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok());
        match (escaped, bytes[index]) {
            (Some(byte), _) => {
                out.push(byte);
                index += 3;
                continue;
            }
            (None, b'+') => out.push(b' '),
            (None, byte) => out.push(byte),
        }
        index += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mtt_core::model::{
        FontCharacterOverride, InpaintEngine, InpaintSettings, InpaintStatus, TextFontStyle, TextPosition, TextStyle,
    };

    fn page(width: u32, height: u32) -> PageRecord {
        PageRecord {
            id: "page-1".into(),
            name: "001.png".into(),
            width,
            height,
            updated_at: "2026-10-27T01:02:03.000Z".into(),
            ..PageRecord::default()
        }
    }

    #[test]
    fn rects_convert_normalized_and_pixel_boxes() {
        let page = page(200, 100);
        let block = TranslationBlock { bbox: BBox::new(100.0, 250.0, 500.0, 500.0), ..TranslationBlock::default() };
        let (source, output) = block_report_rects(&page, &block);
        assert_eq!(source, PixelRect { x: 20, y: 25, width: 100, height: 50 });
        assert_eq!(output, source);

        // Fractional pixel boxes floor the origin and ceil the far edge.
        let block = TranslationBlock {
            bbox: BBox::new(10.4, 5.6, 20.2, 10.1),
            bbox_space: Some("pixels".into()),
            render_bbox: Some(BBox::new(0.0, 0.0, 100.0, 100.0)),
            ..TranslationBlock::default()
        };
        let (source, output) = block_report_rects(&page, &block);
        assert_eq!(source, PixelRect { x: 10, y: 5, width: 21, height: 11 });
        // The render box inherits `bboxSpace` when it has no space of its own.
        assert_eq!(output, PixelRect { x: 0, y: 0, width: 100, height: 100 });

        let block = TranslationBlock {
            bbox: BBox::new(0.0, 0.0, 10.0, 10.0),
            bbox_space: Some("pixels".into()),
            render_bbox: Some(BBox::new(900.0, -50.0, 300.0, 100.0)),
            render_bbox_space: Some("normalized_1000".into()),
            ..TranslationBlock::default()
        };
        let (_, output) = block_report_rects(&page, &block);
        assert_eq!(output, PixelRect { x: 180, y: 0, width: 20, height: 5 });

        // Boxes fully outside the page still yield a 1px rect on the edge.
        let block = TranslationBlock { bbox: BBox::new(1200.0, 1200.0, 50.0, 50.0), ..TranslationBlock::default() };
        let (source, _) = block_report_rects(&page, &block);
        assert_eq!(source, PixelRect { x: 199, y: 99, width: 1, height: 1 });
    }

    #[test]
    fn crop_keeps_rect_size_and_leaves_outside_transparent() {
        let image = RgbaImage::from_pixel(4, 4, image::Rgba([10, 20, 30, 255]));
        let url = crop_data_url(&image, PixelRect { x: 2, y: 3, width: 4, height: 2 });
        let encoded = url.strip_prefix("data:image/png;base64,").expect("png data url");
        let bytes = base64::engine::general_purpose::STANDARD.decode(encoded).unwrap();
        let decoded = image::load_from_memory(&bytes).unwrap().to_rgba8();
        assert_eq!(decoded.dimensions(), (4, 2));
        assert_eq!(decoded.get_pixel(0, 0).0, [10, 20, 30, 255]);
        assert_eq!(decoded.get_pixel(1, 0).0, [10, 20, 30, 255]);
        assert_eq!(decoded.get_pixel(2, 0).0[3], 0);
        assert_eq!(decoded.get_pixel(0, 1).0[3], 0);

        let url = crop_data_url(&image, PixelRect { x: 0, y: 0, width: 0, height: 0 });
        let bytes =
            base64::engine::general_purpose::STANDARD.decode(url.trim_start_matches("data:image/png;base64,")).unwrap();
        assert_eq!(image::load_from_memory(&bytes).unwrap().to_rgba8().dimensions(), (1, 1));
    }

    fn styled_block() -> TranslationBlock {
        let mut block = TranslationBlock {
            id: "block-1".into(),
            block_type: mtt_core::model::BlockType::Speech,
            bbox: BBox::new(100.0, 100.0, 200.0, 100.0),
            source_text: "こんにちは".into(),
            translated_text: "안녕 <하세요>".into(),
            confidence: 0.875,
            rotation_deg: Some(-12.5),
            font_preset_id: Some("preset-a".into()),
            text_position: Some(TextPosition::TopLeft),
            background_color: "#ffffff".into(),
            opacity: 0.0625,
            inpainted: Some(true),
            style: TextStyle {
                font_family: Some("Noto Sans KR".into()),
                character_font_overrides: Some(vec![
                    FontCharacterOverride { character: "!".into(), font_family: "Bangers".into() },
                    FontCharacterOverride { character: "?".into(), font_family: "Arial".into() },
                ]),
                font_weight: Some(700.0),
                font_style: Some(TextFontStyle::Italic),
                font_size_px: Some(31.25),
                line_height: Some(1.18),
                outline_color: Some("#112233".into()),
                outline_width_px: Some(2.5),
                secondary_outline_width_px: Some(4.0),
                shadow_blur_px: Some(3.0),
                shadow_opacity: Some(0.35),
                shadow_angle_deg: Some(45.0),
                screentone_fill_enabled: Some(true),
                screentone_fill_intensity: Some(0.55),
                screentone_fill_antialias: Some(true),
                filter_effect_enabled: Some(true),
                filter_wave_enabled: Some(true),
                filter_noise_enabled: Some(true),
                filter_noise_intensity: Some(0.12345),
                filter_blur_enabled: Some(true),
                filter_blur_intensity: Some(0.0),
                ..TextStyle::default()
            },
            ..TranslationBlock::default()
        };
        for (key, _) in LINK_FIELD_LABELS {
            block.links.set(*key, Some(false));
        }
        block.links.font_size_linked_to_preset = Some(true);
        block.links.shadow_color_linked_to_preset = None;
        block.links.filter_wave_intensity_linked_to_preset = Some(true);
        block
    }

    #[test]
    fn row_formats_style_fields_like_the_ts_report() {
        let mut page = page(1000, 1500);
        page.inpaint_status = Some(InpaintStatus::Completed);
        page.inpaint_settings = Some(InpaintSettings {
            engine: InpaintEngine::Lama,
            padding_px: 6.0,
            feather_px: 2.5,
            tile_size: 1024.0,
            artifact_cleanup_px: Some(8.0),
            ..InpaintSettings::default()
        });
        let block = styled_block();
        let preset = FontPreset { id: "preset-a".into(), name: "대사".into(), ..FontPreset::default() };
        let (source_rect, output_rect) = block_report_rects(&page, &block);
        let row = create_row(RowInput {
            block: &block,
            block_index: 3,
            font_preset: Some(&preset),
            output_crop_data_url: "data:out".into(),
            output_rect,
            page: &page,
            page_index: 2,
            row_index: 7,
            source_crop_data_url: "data:src".into(),
            source_rect,
        });

        assert_eq!(row.page_size, "1000×1500");
        assert_eq!(row.block_type, "speech");
        assert_eq!(row.font_preset_name, "대사");
        assert_eq!(row.font_preset_links, "크기, 그림자색, 물결강도");
        assert_eq!(row.font_family, "Noto Sans KR");
        assert_eq!(row.character_font_overrides, "!: Bangers, ?: Arial");
        assert_eq!(row.font_size_px, 31.25);
        assert_eq!(row.font_weight, Some(700.0));
        assert_eq!(row.font_style, "italic");
        assert_eq!(row.text_decoration, "none");
        assert_eq!(row.letter_spacing_px, 0.0);
        assert_eq!(row.text_align, "center");
        assert_eq!(row.text_position, "top-left");
        assert_eq!(row.text_padding_px, 0.0);
        assert_eq!(row.render_direction, "horizontal");
        assert_eq!(row.rotation_deg, -12.5);
        assert_eq!(row.text_color, "#111111");
        assert_eq!(row.background_color, "#ffffff / 0.063");
        assert_eq!(row.outline, "2.5px #112233");
        assert_eq!(row.secondary_outline, "4px #000000");
        assert_eq!(row.shadow, "#000000 / 35% / 흐림 3px / 45deg / 0px");
        assert_eq!(row.screentone, "강도 0.55, 밀도 0, AA 켜짐");
        assert_eq!(row.filter_effect, "물결 강도 35%, 노이즈 강도 12.345%");
        assert_eq!(row.auto_fit_text, "꺼짐");
        assert_eq!(row.inpainted, "켜짐");
        assert_eq!(row.source_rect_label, "100, 150, 200×150");
        assert_eq!(row.inpaint_status, "completed");
        assert_eq!(row.inpaint_settings, "lama, padding 6, feather 2.5, tile 1024, cleanup 8");
        assert_eq!(row.updated_at, "2026-10-27T01:02:03.000Z");

        let plain = TranslationBlock { font_preset_id: Some(String::new()), ..TranslationBlock::default() };
        let row = create_row(RowInput {
            block: &plain,
            block_index: 1,
            font_preset: None,
            output_crop_data_url: String::new(),
            output_rect: PixelRect::default(),
            page: &PageRecord::default(),
            page_index: 1,
            row_index: 1,
            source_crop_data_url: String::new(),
            source_rect: PixelRect::default(),
        });
        assert_eq!(row.font_preset_name, "개별값");
        assert_eq!(row.font_preset_links, "");
        assert_eq!(row.outline, "없음");
        assert_eq!(row.secondary_outline, "꺼짐");
        assert_eq!(row.shadow, "꺼짐");
        assert_eq!(row.screentone, "꺼짐");
        assert_eq!(row.filter_effect, "꺼짐");
        assert_eq!(row.inpaint_status, "idle");
        assert_eq!(row.inpaint_settings, "");
        assert_eq!(row.font_size_px, 24.0);
        assert_eq!(row.line_height, 1.18);
    }

    #[test]
    fn page_rows_number_blocks_and_resolve_presets() {
        let mut page = page(10, 10);
        let mut linked = TranslationBlock { id: "a".into(), font_preset_id: Some("p".into()), ..Default::default() };
        linked.links.set_all(Some(false));
        let missing = TranslationBlock { id: "b".into(), font_preset_id: Some("gone".into()), ..Default::default() };
        let mut filtered = TranslationBlock { id: "c".into(), ..Default::default() };
        filtered.style.filter_effect_enabled = Some(true);
        page.blocks = vec![linked, missing, filtered];
        let presets = [
            FontPreset { id: "p".into(), name: "old".into(), ..FontPreset::default() },
            FontPreset { id: "p".into(), name: "new".into(), ..FontPreset::default() },
        ];
        let image = RgbaImage::new(10, 10);
        let rows = build_page_rows(&page, &image, &image, 4, 11, &presets);
        let summary: Vec<(usize, usize, usize, &str, &str)> = rows
            .iter()
            .map(|row| {
                (
                    row.row_index,
                    row.page_index,
                    row.block_index,
                    row.font_preset_name.as_str(),
                    row.filter_effect.as_str(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                (11, 4, 1, "new", "꺼짐"),
                (12, 4, 2, "삭제됨: gone", "꺼짐"),
                (13, 4, 3, "개별값", "켜짐 / 활성 필터 없음")
            ]
        );
        assert_eq!(rows[0].font_preset_links, "없음");
        assert!(rows.iter().all(|row| row.source_crop_data_url.starts_with("data:image/png;base64,")));
    }

    #[test]
    fn link_labels_follow_filter_metadata() {
        let filter_labels: Vec<&str> = LINK_FIELD_LABELS.iter().skip(21).take(12).map(|(_, label)| *label).collect();
        let expected: Vec<String> = FilterKind::ALL
            .iter()
            .flat_map(|kind| [kind.label().to_owned(), format!("{}강도", kind.label())])
            .collect();
        assert_eq!(filter_labels, expected);
    }

    #[test]
    fn numbers_format_like_javascript() {
        assert_eq!(format_number(24.0), "24");
        assert_eq!(format_number(-0.0), "0");
        assert_eq!(format_number(1.18), "1.18");
        assert_eq!(format_number(0.1 + 0.2), "0.3");
        assert_eq!(format_number(0.0625), "0.063");
        assert_eq!(format_number(-0.0625), "-0.063");
        assert_eq!(format_number(0.1875), "0.188");
        assert_eq!(format_number(12.3456), "12.346");
        assert_eq!(format_number(1e-10), "0");
        assert_eq!(format_number(-1e-10), "-0");
        assert_eq!(format_number(f64::NAN), "NaN");
        assert_eq!(js_number_string(0.875), "0.875");
        assert_eq!(js_number_string(1e-7), "1e-7");
        assert_eq!(js_number_string(1.5e21), "1.5e+21");
        assert_eq!(js_number_string(1e20), "100000000000000000000");
    }

    #[test]
    fn html_lists_every_column_and_escapes_content() {
        let mut page = page(100, 100);
        page.name = "p<1>".into();
        page.blocks = vec![styled_block()];
        let image = RgbaImage::new(100, 100);
        let rows = build_page_rows(&page, &image, &image, 1, 1, &[]);
        let html = build_html("Ch & <1>", "2026. 10. 27. 오후 3:04:05", &rows, Some("http://127.0.0.1:9/focus"));

        assert!(html.starts_with("<!doctype html>\n<html lang=\"ko\">"));
        assert!(html.ends_with("</body>\n</html>"));
        assert!(html.contains("<title>Ch &amp; &lt;1&gt; 결과 보고서</title>"));
        assert!(html.contains("<h1>Ch &amp; &lt;1&gt; 결과 보고서</h1>"));
        assert!(html.contains("<span class=\"chip\">페이지: 1</span>"));
        assert!(html.contains("<span class=\"chip\">블록: 1</span>"));
        for column in REPORT_COLUMNS {
            assert!(html.contains(&format!("<button type=\"button\">{}</button>", column.label)), "{}", column.label);
            assert!(html.contains(&format!("data-col=\"{}\"", column.key)), "{}", column.key);
        }
        assert!(html.contains("<th class=\"action-header\" aria-sort=\"none\">이동</th>"));
        assert!(html.contains(">안녕 &lt;하세요&gt;</td>"));
        assert!(html.contains("alt=\"p&lt;1&gt; 1 원본\""));
        assert!(html.contains("<td class=\"text-cell\" data-col=\"sourceText\" data-sort=\"こんにちは\">"));
        assert!(html.contains("data-col=\"fontWeight\" data-sort=\"700\">700</td>"));
        assert!(html.contains("data-col=\"confidence\" data-sort=\"0.875\">0.875</td>"));
        assert!(html.contains("data-page-id=\"page-1\" data-block-id=\"block-1\">이동</button>"));
        assert!(html.contains("1 / 1 표시"));
        assert!(html.contains("const focusMessageType = \"manga-result-report:focus-block\";"));
        assert!(html.contains("const focusUrl = \"http://127.0.0.1:9/focus\";"));
        assert!(!html.contains("<1>"));

        let empty = build_html("t", "now", &[], None);
        assert!(empty.contains("<div class=\"empty\">보고서에 포함할 텍스트 블록이 없습니다.</div>"));
        assert!(empty.contains("<span class=\"chip\">페이지: 0</span>"));
        assert!(empty.contains("const focusUrl = null;"));
        assert!(script_json("</script>").starts_with("\"\\u003c/script>"));
    }

    #[test]
    fn ko_kr_timestamp_and_query_decoding() {
        use chrono::TimeZone;
        let time = chrono::Local.with_ymd_and_hms(2026, 10, 27, 15, 4, 5).unwrap();
        assert_eq!(ko_kr_timestamp(&time), "2026. 10. 27. 오후 3:04:05");
        let midnight = chrono::Local.with_ymd_and_hms(2026, 1, 2, 0, 0, 9).unwrap();
        assert_eq!(ko_kr_timestamp(&midnight), "2026. 1. 2. 오전 12:00:09");
        assert_eq!(percent_decode("a%20b+c%ED%95%9C%zz%"), "a b c한%zz%");
    }

    #[test]
    fn focus_server_forwards_page_and_block() {
        let ctx = eframe::egui::Context::default();
        let worker = crate::worker::Worker::<crate::app::AppEvent>::new(&ctx, 1);
        let url = start_focus_server(worker.emitter()).expect("bind");
        let agent = ureq::Agent::new_with_config(ureq::Agent::config_builder().http_status_as_error(false).build());
        let wrong = agent.get(&format!("{}x?page=a&block=b", url)).call().unwrap();
        assert_eq!(wrong.status().as_u16(), 404);
        let ok = agent.get(&format!("{url}?page=p%201&block=b-2")).call().unwrap();
        assert_eq!(ok.status().as_u16(), 204);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let event = loop {
            if let Some(event) = worker.drain().into_iter().next() {
                break event;
            }
            assert!(std::time::Instant::now() < deadline, "no focus event");
            std::thread::sleep(std::time::Duration::from_millis(10));
        };
        match event {
            crate::app::AppEvent::ReportFocus { page_id, block_id } => {
                assert_eq!((page_id.as_str(), block_id.as_str()), ("p 1", "b-2"));
            }
            _ => panic!("unexpected event"),
        }
    }
}
