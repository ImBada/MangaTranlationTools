//! Page translation through a vision model (`wholePagePipeline.ts`,
//! `runtime/simple-page-translate.cjs`, `runtime/overlay-parser.cjs`).
//!
//! The model sees the page (plus an assist view in accuracy mode) and returns one record
//! per text region with a 0..1000 box, the Japanese source and the Korean translation.

pub mod client;
pub mod codex;
pub mod images;
pub mod parser;

use std::path::Path;
use std::time::Duration;

use mtt_core::font_presets::DEFAULT_LINE_HEIGHT;
use mtt_core::geometry::{clamp_bbox, estimate_font_size_px};
use mtt_core::model::{
    BBox, BlockType, LAYOUT_VERSION, RenderTextDirection, SourceTextDirection, TextAlign, TextStyle, TranslationBlock,
};
use mtt_core::settings::{AppSettings, ModelProvider, ReasoningEffort};

pub use client::{Api, Endpoint, ModelOption, Retry, SendError};
pub use parser::OverlayItem;

pub const PROMPT: &str = "You are given the same Japanese manga page in multiple full-page renderings.
Image 1 is the original full page. Another image is a grayscale/high-contrast assist view of the exact same page.
Task: detect each speech bubble, narration box, name call, or sound-effect block that contains visible Japanese text and return a Korean replacement text with one bounding box per item.
Use coordinates for the ORIGINAL page only.
Return only plain text records in this exact field format:
id: 1
type: dialogue
x: 120
y: 80
w: 160
h: 240
jp: 馬鹿者… 無理をするな
ko: 바보 같은 녀석… 무리하지 마라.

id: 2
type: dialogue
x: 300
y: 120
w: 140
h: 220
jp: ...
ko: ...
Rules:
- Use only these keys: id, type, x, y, w, h, jp, ko.
- Put exactly one field on each line.
- Put one blank line between items.
- Do not output JSON, braces, bullets, markdown fences, or commentary.
- x, y, w, h must be integers in a 0..1000 coordinate space relative to the original page size.
- Make each box large enough to cover the original Japanese text region and to fit the Korean replacement.
- Merge multiple vertical lines that belong to the same speech bubble into one item.
- Keep Korean concise, natural, and short enough to fit as an on-image overlay.
- Include short interjections, names, and visible sound effects when meaningful.
- Use type values such as dialogue, narration, name, or sfx.
- Prefer 4 to 12 items for one manga page unless the page clearly has more.
- Keep jp and ko on a single line each. Replace internal newlines with spaces.
- If OCR is uncertain, keep only the uncertain fragment as [?] and still give the best short Korean translation.";

/// Shorter prompt used for retries (`buildRetryPrompt`).
pub const RETRY_PROMPT: &str = "You are given the same Japanese manga page in multiple full-page renderings.
Return only plain text records for a downstream parser.
Use this exact field format and nothing else:
id: 1
type: dialogue
x: 120
y: 80
w: 160
h: 240
jp: 馬鹿者… 無理をするな
ko: 바보 같은 녀석… 무리하지 마라.

Rules:
- One field per line.
- One blank line between items.
- No JSON, no braces, no bullets, no markdown fences, no commentary.
- Use only keys: id, type, x, y, w, h, jp, ko.
- x, y, w, h must be integers in 0..1000 coordinates.
- Keep jp and ko on one line each.
- If uncertain, still output the best approximate item instead of skipping it.";

pub const SYSTEM_PROMPT: &str = "You generate machine-readable overlay blocks for a downstream parser.

Follow the requested field names and output format exactly.

Never add prose, notes, markdown fences, or explanations.

If some text is uncertain, still emit the best approximate block instead of skipping the item.";

pub const DEFAULT_PAGE_ATTEMPTS: u32 = 5;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(600);

/// Request settings derived from [`AppSettings`] (`buildBaseTranslationOptions`).
#[derive(Debug, Clone)]
pub struct TranslationConfig {
    pub endpoint: Endpoint,
    pub include_enhanced_variant: bool,
    pub attempts: u32,
    pub concurrency: usize,
    /// Shared by the job's requests (connection reuse).
    pub agent: ureq::Agent,
    /// The user's instructions and the work's notes, added to every prompt.
    pub instructions: Option<String>,
}

/// Pause before retrying after a server or network error: 1 s doubling to 20 s, or what
/// the server asked for (up to 2 minutes).
fn retry_delay(attempt: u32, asked: Option<Duration>) -> Duration {
    asked.map_or_else(
        || Duration::from_secs(1 << (attempt - 1).min(5)).min(Duration::from_secs(20)),
        |asked| asked.min(Duration::from_secs(120)),
    )
}

/// Sleeps in short steps; `false` when cancelled meanwhile.
fn wait_unless_cancelled(duration: Duration, cancelled: &dyn Fn() -> bool) -> bool {
    let until = std::time::Instant::now() + duration;
    while std::time::Instant::now() < until {
        if cancelled() {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100).min(until - std::time::Instant::now()));
    }
    !cancelled()
}

/// `readNumberEnv`: a finite number from the environment.
fn env_number(name: &str) -> Option<f64> {
    std::env::var(name).ok()?.trim().parse::<f64>().ok().filter(|value| value.is_finite())
}

/// The Responses API's name for `effort`.
pub fn effort_name(effort: ReasoningEffort) -> &'static str {
    match effort {
        ReasoningEffort::None => "none",
        ReasoningEffort::Low => "low",
        ReasoningEffort::Medium => "medium",
        ReasoningEffort::High => "high",
        ReasoningEffort::Xhigh => "xhigh",
    }
}

impl TranslationConfig {
    pub fn from_settings(settings: &AppSettings) -> Self {
        let mode = settings.mode_defaults();
        let endpoint = match settings.model_provider {
            // The ChatGPT account signed in from the settings.
            ModelProvider::OpenAiCodex => Endpoint {
                base_url: codex::CODEX_BASE_URL.into(),
                api_key: None,
                model: settings.codex.model.clone(),
                api: Api::Responses { reasoning_effort: effort_name(settings.codex.reasoning_effort).into() },
                auth: Some(std::sync::Arc::new(codex::CodexAuth::new(
                    mtt_core::paths::AppPaths::resolve().codex_auth_path,
                ))),
            },
            ModelProvider::OpenAiCompatible => Endpoint {
                base_url: settings.openai_compatible.base_url.clone(),
                api_key: Some(settings.openai_compatible.api_key.clone()).filter(|key| !key.is_empty()),
                model: settings.openai_compatible.model.clone(),
                api: Api::ChatCompletions {
                    temperature: env_number("MANGA_TRANSLATOR_TEMPERATURE").unwrap_or(0.0),
                    top_p: env_number("MANGA_TRANSLATOR_TOP_P").unwrap_or(0.85),
                    max_tokens: env_number("MANGA_TRANSLATOR_MAX_TOKENS").map_or(mode.max_tokens, |v| v as u32),
                },
                auth: None,
            },
        };
        let concurrency = if settings.translation_parallel.enabled {
            settings.translation_parallel.max_concurrency.max(1) as usize
        } else {
            1
        };
        Self {
            endpoint,
            include_enhanced_variant: mode.include_enhanced_variant,
            // `readNumberEnv("MANGA_TRANSLATOR_PAGE_RETRIES", 5)`, at least one attempt.
            attempts: env_number("MANGA_TRANSLATOR_PAGE_RETRIES").map_or(DEFAULT_PAGE_ATTEMPTS, |v| v.max(1.0) as u32),
            concurrency,
            agent: client::job_agent(REQUEST_TIMEOUT),
            instructions: None,
        }
        .with_instructions(&settings.translation_instructions, "")
    }

    /// Sets the prompt additions: general `instructions` and `notes` for the work.
    pub fn with_instructions(mut self, instructions: &str, notes: &str) -> Self {
        let mut parts = Vec::new();
        if !instructions.trim().is_empty() {
            parts.push(format!("Additional instructions from the user:\n{}", instructions.trim()));
        }
        if !notes.trim().is_empty() {
            parts
                .push(format!("Notes for this work (use these names, terms and tone consistently):\n{}", notes.trim()));
        }
        self.instructions = (!parts.is_empty()).then(|| parts.join("\n\n"));
        self
    }

    pub fn describe(&self) -> String {
        format!("{} @ {}", self.endpoint.model, self.endpoint.base_url)
    }
}

/// Checks the endpoint is reachable (`/models`), or that the ChatGPT sign-in is usable.
pub fn verify_endpoint(config: &TranslationConfig) -> Result<(), String> {
    client::verify(&config.endpoint)
}

/// The models the configured engine offers (see [`client::list_models`]).
pub fn list_models(config: &TranslationConfig) -> Result<Vec<ModelOption>, String> {
    client::list_models(&config.endpoint)
}

/// One request/parse attempt: `prompt` with `variants`, the output read by `parse`.
fn request_attempt<T>(
    config: &TranslationConfig,
    variants: &[images::ImageVariant],
    prompt: &str,
    parse: impl Fn(&str) -> Result<T, String>,
    cancelled: &dyn Fn() -> bool,
) -> Result<T, SendError> {
    let prompt = match &config.instructions {
        Some(instructions) => format!("{prompt}\n\n{instructions}"),
        None => prompt.to_owned(),
    };
    let body = client::chat_request_body(&config.endpoint, SYSTEM_PROMPT, &prompt, variants);
    let output = client::send_cancellable(&config.agent, &config.endpoint, body, cancelled)?;
    parse(&output).map_err(|error| SendError {
        message: format!("{error} 응답 미리보기: {}", client::preview(&output, 240)),
        retry: Retry::Now,
    })
}

/// Translates a page with retries. `on_attempt(attempt, error)` reports failed attempts.
/// `cancelled` stops waiting for a request in flight, a pause, or the next attempt.
/// Errors that would repeat (a bad key, a rejected request) are not retried.
pub fn translate_page(
    config: &TranslationConfig,
    image_path: &Path,
    on_attempt: impl FnMut(u32, &str),
    cancelled: impl Fn() -> bool,
) -> Result<Vec<OverlayItem>, String> {
    // The images (and their base64) are the same for every attempt.
    let variants = images::prepare_variants(image_path, config.include_enhanced_variant)?;
    with_retries(
        config,
        |attempt, cancelled| {
            let prompt = if attempt > 1 { RETRY_PROMPT } else { PROMPT };
            request_attempt(config, &variants, prompt, parser::parse_items, cancelled)
        },
        on_attempt,
        &cancelled,
    )
}

/// A region of the page to read and translate on its own (a block's box, 0..1000 space).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Region {
    pub id: u32,
    pub bbox: BBox,
}

/// Prompt for [`translate_regions`]: the regions listed with their boxes.
pub fn region_prompt(regions: &[Region]) -> String {
    let list: String = regions
        .iter()
        .map(|region| {
            let b = region.bbox;
            format!(
                "id: {}, x: {}, y: {}, w: {}, h: {}\n",
                region.id,
                b.x.round(),
                b.y.round(),
                b.w.round(),
                b.h.round()
            )
        })
        .collect();
    format!(
        "You are given a Japanese manga page and close-ups of some regions of it.
Image 1 is the original full page; each following image is the close-up of one region, in the order listed below.
Regions, as boxes in a 0..1000 coordinate space relative to the original page size:
{list}
For every region, read the Japanese text inside it and translate it into Korean.
Return only plain text records in this exact field format, one per region, using the same ids:
id: 1
jp: 馬鹿者… 無理をするな
ko: 바보 같은 녀석… 무리하지 마라.

Rules:
- Use only these keys: id, jp, ko. Put exactly one field on each line and one blank line between records.
- Do not output JSON, braces, bullets, markdown fences, or commentary.
- Keep jp and ko on a single line each. Replace internal newlines with spaces.
- Translate only the text inside each region; use the rest of the page for context.
- Keep Korean concise, natural, and short enough to fit the region as an on-image overlay.
- If a region has no readable text, write ko: -
- If OCR is uncertain, keep only the uncertain fragment as [?] and still give the best short Korean translation."
    )
}

/// Reads and translates `regions` of a page with retries (like [`translate_page`]); the
/// model gets the page and a close-up of each region. Regions it leaves out, or marks as
/// having no text, are missing from the result.
pub fn translate_regions(
    config: &TranslationConfig,
    image_path: &Path,
    regions: &[Region],
    on_attempt: impl FnMut(u32, &str),
    cancelled: impl Fn() -> bool,
) -> Result<Vec<parser::RegionText>, String> {
    let mut variants = vec![images::original_variant(image_path)?];
    let page =
        mtt_core::image_io::open_image(image_path).map_err(|e| format!("{}: {e}", image_path.display()))?.to_rgba8();
    for region in regions {
        variants.push(images::region_variant(&page, region.id, region.bbox)?);
    }
    let prompt = region_prompt(regions);
    let ids: Vec<u32> = regions.iter().map(|region| region.id).collect();
    with_retries(
        config,
        |_, cancelled| {
            request_attempt(config, &variants, &prompt, |output| parser::parse_region_texts(output, &ids), cancelled)
        },
        on_attempt,
        &cancelled,
    )
}

/// Runs `attempt(number, cancelled)` up to `config.attempts` times, pausing after server
/// errors and stopping early on errors that would repeat or on cancel.
fn with_retries<T>(
    config: &TranslationConfig,
    mut attempt: impl FnMut(u32, &dyn Fn() -> bool) -> Result<T, SendError>,
    mut on_attempt: impl FnMut(u32, &str),
    cancelled: &dyn Fn() -> bool,
) -> Result<T, String> {
    let attempts = config.attempts.max(1);
    let mut last_error = String::new();
    for number in 1..=attempts {
        if cancelled() {
            return Err("취소됨".into());
        }
        let error = match attempt(number, cancelled) {
            Ok(value) => return Ok(value),
            Err(error) => error,
        };
        if error.retry == Retry::Cancelled {
            return Err("취소됨".into());
        }
        log::warn!("translation attempt {number} failed: {}", error.message);
        on_attempt(number, &error.message);
        last_error = error.message;
        match error.retry {
            Retry::No => return Err(format!("다시 시도해도 같은 오류라 멈췄습니다: {last_error}")),
            Retry::Later(asked)
                if number < attempts && !wait_unless_cancelled(retry_delay(number, asked), cancelled) =>
            {
                return Err("취소됨".into());
            }
            _ => {}
        }
    }
    Err(format!("{attempts}회 시도 후 실패: {last_error}"))
}

/// `mapOverlayType`.
pub fn map_overlay_type(value: &str) -> BlockType {
    match value.trim().to_ascii_lowercase().as_str() {
        "speech" | "dialogue" | "dialog" | "balloon" | "bubble" => BlockType::Speech,
        "sfx" | "sound" | "effect" | "onomatopoeia" => BlockType::Sfx,
        "caption" | "narration" | "name" => BlockType::Caption,
        _ => BlockType::Other,
    }
}

/// `overlayItemToBlock`: default styling before the block-type preset is applied.
pub fn overlay_item_to_block(
    item: &OverlayItem,
    page_id: &str,
    page_width: u32,
    page_height: u32,
    index: usize,
) -> TranslationBlock {
    let block_type = map_overlay_type(&item.item_type);
    let bbox = clamp_bbox(BBox::new(item.bbox.0, item.bbox.1, item.bbox.2, item.bbox.3));
    let translated = item.ko.trim().to_owned();
    let source = item.jp.trim().to_owned();
    let estimate_text = if !translated.is_empty() {
        translated.as_str()
    } else if !source.is_empty() {
        source.as_str()
    } else {
        "..."
    };
    TranslationBlock {
        id: format!("{page_id}-block-{}", index + 1),
        block_type,
        bbox,
        bbox_space: Some("normalized_1000".into()),
        confidence: if source.is_empty() { 0.75 } else { 0.92 },
        source_direction: SourceTextDirection::Vertical,
        render_direction: RenderTextDirection::Horizontal,
        text_align: TextAlign::Center,
        background_color: if block_type == BlockType::Sfx { "#fff4ea".into() } else { "#fffdf5".into() },
        opacity: if block_type == BlockType::Sfx { 0.7 } else { 0.88 },
        style: TextStyle {
            font_size_px: Some(estimate_font_size_px(
                estimate_text,
                bbox,
                f64::from(page_width),
                f64::from(page_height),
            )),
            line_height: Some(DEFAULT_LINE_HEIGHT),
            letter_spacing_px: Some(0.0),
            outline_color: Some("#000000".into()),
            outline_width_px: Some(0.0),
            secondary_outline_enabled: Some(false),
            secondary_outline_color: Some("#ffffff".into()),
            secondary_outline_width_px: Some(0.0),
            shadow_enabled: Some(false),
            shadow_color: Some("#000000".into()),
            shadow_angle_deg: Some(45.0),
            shadow_distance_px: Some(0.0),
            text_color: Some("#111111".into()),
            auto_fit_text: Some(true),
            ..TextStyle::default()
        },
        source_text: source,
        translated_text: translated,
        layout_version: Some(LAYOUT_VERSION),
        ..TranslationBlock::default()
    }
}

/// Short connectivity test like the settings screen's "모델 테스트".
pub fn test_model(config: &TranslationConfig) -> Result<String, String> {
    verify_endpoint(config)?;
    let body = match &config.endpoint.api {
        Api::ChatCompletions { temperature, top_p, .. } => serde_json::json!({
            "model": config.endpoint.model,
            "temperature": temperature,
            "top_p": top_p,
            "max_tokens": 32,
            "messages": [
                { "role": "system", "content": [{ "type": "text", "text": "Reply in one short sentence." }] },
                { "role": "user", "content": [{ "type": "text", "text": "Say 'model test ok'." }] }
            ]
        }),
        Api::Responses { reasoning_effort } => serde_json::json!({
            "model": config.endpoint.model,
            "instructions": "Reply in one short sentence.",
            "input": [{ "role": "user", "content": [{ "type": "input_text", "text": "Say 'model test ok'." }] }],
            "reasoning": { "effort": reasoning_effort },
            "stream": true,
            "store": false
        }),
    };
    client::send(&config.endpoint, &body, Duration::from_secs(120)).map(|reply| client::preview(&reply, 120))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_from_settings_per_provider() {
        let mut settings = AppSettings::default();
        let codex = TranslationConfig::from_settings(&settings);
        assert_eq!(codex.endpoint.base_url, codex::CODEX_BASE_URL);
        assert!(matches!(codex.endpoint.api, Api::Responses { .. }));
        assert!(codex.endpoint.auth.is_some(), "signed-in session instead of a key");
        assert_eq!(codex.concurrency, 1);

        settings.model_provider = ModelProvider::OpenAiCompatible;
        settings.openai_compatible.api_key = "k".into();
        settings.translation_mode = mtt_core::settings::TranslationMode::Accuracy;
        settings.translation_parallel.enabled = true;
        settings.translation_parallel.max_concurrency = 4;
        let compatible = TranslationConfig::from_settings(&settings);
        assert_eq!(compatible.endpoint.api_key.as_deref(), Some("k"));
        assert!(compatible.include_enhanced_variant);
        assert_eq!(compatible.concurrency, 4);
        assert!(matches!(compatible.endpoint.api, Api::ChatCompletions { max_tokens: 1400, .. }));
        assert_eq!(compatible.instructions, None);

        settings.translation_instructions = "  반말로  ".into();
        let config = TranslationConfig::from_settings(&settings).with_instructions("반말로", "주인공: 하루");
        let text = config.instructions.unwrap();
        assert!(text.starts_with("Additional instructions from the user:\n반말로\n\n"), "{text}");
        assert!(text.ends_with("consistently):\n주인공: 하루"), "{text}");
        assert_eq!(TranslationConfig::from_settings(&settings).with_instructions("", " ").instructions, None);
    }

    #[test]
    fn retry_delays_back_off_and_follow_the_server() {
        assert_eq!(retry_delay(1, None), Duration::from_secs(1));
        assert_eq!(retry_delay(3, None), Duration::from_secs(4));
        assert_eq!(retry_delay(9, None), Duration::from_secs(20));
        assert_eq!(retry_delay(1, Some(Duration::from_secs(7))), Duration::from_secs(7));
        assert_eq!(retry_delay(1, Some(Duration::from_secs(900))), Duration::from_secs(120));
    }

    #[test]
    fn overlay_item_becomes_styled_block() {
        let item = OverlayItem {
            id: 1,
            item_type: "sfx".into(),
            bbox: (990.0, 10.0, 50.0, 40.0),
            jp: "ドン".into(),
            ko: "쾅".into(),
        };
        let block = overlay_item_to_block(&item, "p1", 1000, 1500, 0);
        assert_eq!(block.id, "p1-block-1");
        assert_eq!(block.block_type, BlockType::Sfx);
        assert_eq!(block.bbox.w, 10.0, "clamped to page");
        assert_eq!(block.opacity, 0.7);
        assert!(block.style.font_size_px.unwrap() >= 12.0);
        assert_eq!(map_overlay_type("narration"), BlockType::Caption);
        assert_eq!(map_overlay_type("thought"), BlockType::Other);
    }
}
