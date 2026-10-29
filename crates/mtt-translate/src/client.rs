//! OpenAI-compatible `/chat/completions` and Responses API (`/responses`, SSE) requests
//! (`simple-page-translate.cjs`).

use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;

use serde::Serialize;
use serde_json::Value;

use crate::images::{ImageVariant, variant_caption};

/// Everything needed to talk to one endpoint.
#[derive(Debug, Clone)]
pub struct Endpoint {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub api: Api,
    /// ChatGPT sign-in for the Codex endpoint (instead of an API key).
    pub auth: Option<std::sync::Arc<crate::codex::CodexAuth>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Api {
    /// `/chat/completions` with sampling parameters.
    ChatCompletions { temperature: f64, top_p: f64, max_tokens: u32 },
    /// `/responses` streaming with a reasoning effort (Codex/ChatGPT accounts).
    Responses { reasoning_effort: String },
}

fn agent(timeout: Duration) -> ureq::Agent {
    ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_global(Some(timeout))
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .build(),
    )
}

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// One agent for a whole job: its requests share connections (and TLS sessions).
pub fn job_agent(timeout: Duration) -> ureq::Agent {
    agent(timeout)
}

/// Whether a failed request is worth repeating, and when.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retry {
    /// It will fail the same way (bad key, wrong URL, rejected request).
    No,
    /// Right away (the model's answer could not be used; ask again).
    Now,
    /// After a pause; the server asked for one with `Retry-After`.
    Later(Option<Duration>),
    /// The job was cancelled.
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SendError {
    pub message: String,
    pub retry: Retry,
}

impl SendError {
    fn new(message: impl Into<String>, retry: Retry) -> Self {
        Self { message: message.into(), retry }
    }
}

/// How an HTTP error status should be retried.
pub fn retry_for_status(status: u16, retry_after: Option<Duration>) -> Retry {
    match status {
        408 | 409 | 425 | 429 | 500..=599 => Retry::Later(retry_after),
        _ => Retry::No,
    }
}

/// `Retry-After` in seconds (the HTTP-date form is rare for APIs and ignored).
fn retry_after(response: &ureq::http::Response<ureq::Body>) -> Option<Duration> {
    let value = response.headers().get("retry-after")?.to_str().ok()?;
    value.trim().parse::<u64>().ok().map(Duration::from_secs)
}

/// `isOllamaCompatibleEndpoint`: Ollama accepts extra flags to disable thinking.
pub fn is_ollama(base_url: &str) -> bool {
    let lower = base_url.to_ascii_lowercase();
    let host = lower.split("://").nth(1).unwrap_or(&lower).split('/').next().unwrap_or("");
    let (hostname, port) = host.rsplit_once(':').map(|(h, p)| (h, Some(p))).unwrap_or((host, None));
    hostname == "ollama.com" || hostname.ends_with(".ollama.com") || port == Some("11434")
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ChatPart<'a> {
    Text { text: &'a str },
    ImageUrl { image_url: ImageUrl<'a> },
}

#[derive(Serialize)]
struct ImageUrl<'a> {
    url: &'a str,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'static str,
    content: Vec<ChatPart<'a>>,
}

#[derive(Serialize)]
struct OllamaFlags {
    reasoning_budget: u8,
    enable_thinking: bool,
    think: bool,
    reasoning_effort: &'static str,
}

#[derive(Serialize)]
struct ChatBody<'a> {
    model: &'a str,
    temperature: f64,
    top_p: f64,
    presence_penalty: u8,
    frequency_penalty: u8,
    max_tokens: u32,
    messages: [ChatMessage<'a>; 2],
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    ollama: Option<OllamaFlags>,
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum InputPart<'a> {
    InputText { text: &'a str },
    InputImage { image_url: &'a str },
}

#[derive(Serialize)]
struct InputMessage<'a> {
    role: &'static str,
    content: Vec<InputPart<'a>>,
}

#[derive(Serialize)]
struct Reasoning<'a> {
    effort: &'a str,
}

#[derive(Serialize)]
struct ResponsesBody<'a> {
    model: &'a str,
    instructions: &'a str,
    input: [InputMessage<'a>; 1],
    reasoning: Reasoning<'a>,
    stream: bool,
    store: bool,
}

/// The request body, borrowing the (multi-megabyte) image data instead of copying it.
pub fn chat_request_body(endpoint: &Endpoint, system: &str, prompt: &str, images: &[ImageVariant]) -> Vec<u8> {
    let captions: Vec<String> =
        images.iter().enumerate().map(|(index, image)| variant_caption(image.role, index)).collect();
    let body = match &endpoint.api {
        Api::ChatCompletions { temperature, top_p, max_tokens } => {
            let mut user: Vec<ChatPart<'_>> = images
                .iter()
                .zip(&captions)
                .flat_map(|(image, caption)| {
                    [
                        ChatPart::Text { text: caption },
                        ChatPart::ImageUrl { image_url: ImageUrl { url: &image.data_url } },
                    ]
                })
                .collect();
            user.push(ChatPart::Text { text: prompt });
            serde_json::to_vec(&ChatBody {
                model: &endpoint.model,
                temperature: *temperature,
                top_p: *top_p,
                presence_penalty: 0,
                frequency_penalty: 0,
                max_tokens: *max_tokens,
                messages: [
                    ChatMessage { role: "system", content: vec![ChatPart::Text { text: system }] },
                    ChatMessage { role: "user", content: user },
                ],
                ollama: is_ollama(&endpoint.base_url).then_some(OllamaFlags {
                    reasoning_budget: 0,
                    enable_thinking: false,
                    think: false,
                    reasoning_effort: "none",
                }),
            })
        }
        Api::Responses { reasoning_effort } => {
            let mut content: Vec<InputPart<'_>> = images
                .iter()
                .zip(&captions)
                .flat_map(|(image, caption)| {
                    [InputPart::InputText { text: caption }, InputPart::InputImage { image_url: &image.data_url }]
                })
                .collect();
            content.push(InputPart::InputText { text: prompt });
            serde_json::to_vec(&ResponsesBody {
                model: &endpoint.model,
                instructions: system,
                input: [InputMessage { role: "user", content }],
                reasoning: Reasoning { effort: reasoning_effort },
                stream: true,
                store: false,
            })
        }
    };
    body.expect("request bodies serialize")
}

/// Sends a request and returns the model's text output (settings' model test).
pub fn send(endpoint: &Endpoint, body: &Value, timeout: Duration) -> Result<String, String> {
    let bytes = serde_json::to_vec(body).map_err(|error| error.to_string())?;
    send_bytes(&agent(timeout), endpoint, &bytes).map_err(|error| error.message)
}

/// `send_bytes` on a helper thread, returning as soon as `cancelled` says so (checked every
/// 100 ms). A cancelled request finishes on its own thread and its answer is dropped, so
/// cancelling never waits for the model.
pub fn send_cancellable(
    agent: &ureq::Agent,
    endpoint: &Endpoint,
    body: Vec<u8>,
    cancelled: &dyn Fn() -> bool,
) -> Result<String, SendError> {
    let (sender, receiver) = channel();
    let (agent, endpoint) = (agent.clone(), endpoint.clone());
    std::thread::Builder::new()
        .name("mtt-request".into())
        .spawn(move || {
            let _ = sender.send(send_bytes(&agent, &endpoint, &body));
        })
        .map_err(|error| SendError::new(format!("요청 스레드를 만들지 못했습니다: {error}"), Retry::Later(None)))?;
    loop {
        match receiver.recv_timeout(Duration::from_millis(100)) {
            Ok(result) => return result,
            Err(RecvTimeoutError::Timeout) if cancelled() => return Err(SendError::new("취소됨", Retry::Cancelled)),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                return Err(SendError::new("요청이 중간에 끊겼습니다.", Retry::Later(None)));
            }
        }
    }
}

/// Posts a JSON body and returns the model's text output. With a ChatGPT sign-in the
/// request carries its session, refreshed once if the server says it expired.
pub fn send_bytes(agent: &ureq::Agent, endpoint: &Endpoint, body: &[u8]) -> Result<String, SendError> {
    let path = match endpoint.api {
        Api::ChatCompletions { .. } => "chat/completions",
        Api::Responses { .. } => "responses",
    };
    let url = format!("{}/{path}", endpoint.base_url.trim_end_matches('/'));
    let session = match &endpoint.auth {
        Some(auth) => Some(auth.session().map_err(|error| SendError::new(error, Retry::No))?),
        None => None,
    };
    let post = |session: Option<&crate::codex::Session>| {
        let mut request = agent.post(&url).header("Content-Type", "application/json");
        if let Some(session) = session {
            for (name, value) in crate::codex::request_headers(session) {
                request = request.header(name, &value);
            }
        } else if let Some(key) = endpoint.api_key.as_deref().filter(|k| !k.trim().is_empty()) {
            request = request.header("Authorization", &format!("Bearer {}", key.trim()));
        }
        request.send(body).map_err(|e| SendError::new(format!("요청 전송 실패 ({url}): {e}"), Retry::Later(None)))
    };
    let mut response = post(session.as_ref())?;
    if let (Some(auth), Some(stale)) = (&endpoint.auth, &session)
        && response.status().as_u16() == 401
    {
        let fresh = auth.force_refresh(stale).map_err(|error| SendError::new(error, Retry::No))?;
        response = post(Some(&fresh))?;
    }
    let status = response.status();
    let wait = retry_after(&response);
    let text = response
        .body_mut()
        .with_config()
        .limit(64 * 1024 * 1024)
        .read_to_string()
        .map_err(|e| SendError::new(format!("응답을 읽지 못했습니다: {e}"), Retry::Later(None)))?;
    if !status.is_success() {
        return Err(SendError::new(
            format!("요청 실패 ({}): {}", status.as_u16(), preview(&text, 600)),
            retry_for_status(status.as_u16(), wait),
        ));
    }
    let output = match endpoint.api {
        Api::Responses { .. } => parse_responses_sse(&text),
        Api::ChatCompletions { .. } => {
            let parsed: Value = serde_json::from_str(&text).map_err(|e| {
                SendError::new(format!("응답 JSON 파싱 실패: {e}: {}", preview(&text, 400)), Retry::Now)
            })?;
            extract_output_text(&parsed)
        }
    };
    if output.trim().is_empty() {
        return Err(SendError::new(format!("모델이 빈 응답을 반환했습니다: {}", preview(&text, 400)), Retry::Now));
    }
    Ok(output.trim().to_owned())
}

/// `GET /models` to check the base URL and key before a batch (`verifyEndpoint`); with a
/// ChatGPT sign-in, that there is a usable session.
pub fn verify(endpoint: &Endpoint) -> Result<(), String> {
    if let Some(auth) = &endpoint.auth {
        return auth.session().map(|_| ());
    }
    get_models(endpoint).map(|_| ())
}

/// The Codex CLI release to ask the model catalog as when GitHub can't say which is the
/// newest. The catalog leaves out models newer than the asking client (0.146.0 got
/// GPT-5.6 but no GPT-6).
const CODEX_CLIENT_VERSION: &str = "0.165.0";
const CODEX_LATEST_RELEASE_URL: &str = "https://api.github.com/repos/openai/codex/releases/latest";
/// Model ids of an OpenAI-compatible server that can't translate a page.
const NON_CHAT_MODELS: [&str; 6] = ["embed", "whisper", "tts", "dall-e", "moderation", "transcribe"];

/// A model an endpoint offers.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelOption {
    pub id: String,
    /// The display name, or the id.
    pub name: String,
    pub description: String,
    /// Reasoning efforts it takes (`low`, `medium`…); empty when the endpoint doesn't say.
    pub efforts: Vec<String>,
}

/// The models `endpoint` offers to translate with: the ChatGPT account's Codex catalog
/// (models shown in pickers that read images, best first), or an OpenAI-compatible
/// server's `GET /models` (minus embedding, speech and image models, by id).
pub fn list_models(endpoint: &Endpoint) -> Result<Vec<ModelOption>, String> {
    let text = get_models(endpoint)?;
    let catalog: Value = serde_json::from_str(&text)
        .map_err(|e| format!("모델 목록을 읽지 못했습니다: {e}: {}", preview(&text, 200)))?;
    let models = if endpoint.auth.is_some() { codex_models(&catalog) } else { compatible_models(&catalog) };
    if models.is_empty() {
        return Err("쓸 수 있는 모델이 없습니다.".into());
    }
    Ok(models)
}

/// The body of `GET {base_url}/models`, as the signed-in account when there is one.
fn get_models(endpoint: &Endpoint) -> Result<String, String> {
    let base = endpoint.base_url.trim_end_matches('/');
    let url = match endpoint.auth {
        Some(_) => format!("{base}/models?client_version={}", codex_client_version(base)),
        None => format!("{base}/models"),
    };
    let session = endpoint.auth.as_ref().map(|auth| auth.session()).transpose()?;
    let get = |session: Option<&crate::codex::Session>| {
        let mut request = agent(Duration::from_secs(30)).get(&url).header("Accept", "application/json");
        if let Some(session) = session {
            for (name, value) in crate::codex::account_headers(session) {
                request = request.header(name, &value);
            }
        } else if let Some(key) = endpoint.api_key.as_deref().filter(|k| !k.trim().is_empty()) {
            request = request.header("Authorization", &format!("Bearer {}", key.trim()));
        }
        request.call().map_err(|e| format!("엔드포인트에 연결하지 못했습니다. Base URL을 확인하세요. ({url}: {e})"))
    };
    let mut response = get(session.as_ref())?;
    if let (Some(auth), Some(stale)) = (&endpoint.auth, &session)
        && response.status().as_u16() == 401
    {
        response = get(Some(&auth.force_refresh(stale)?))?;
    }
    let status = response.status();
    let text = response.body_mut().read_to_string().unwrap_or_default();
    if !status.is_success() {
        let hint = match status.as_u16() {
            401 | 403 if endpoint.auth.is_some() => "ChatGPT에 다시 로그인하세요.",
            401 | 403 => "API 키를 확인하세요.",
            404 => "Base URL이 /v1 까지 포함하는지 확인하세요.",
            429 => "요청 한도를 초과했습니다. 잠시 후 다시 시도하세요.",
            _ => "",
        };
        return Err(format!("엔드포인트 확인 실패 ({}). {hint} {}", status.as_u16(), preview(&text, 300)));
    }
    Ok(text)
}

/// The Codex CLI release to ask `base_url`'s catalog as: for ChatGPT, the newest stable
/// release on GitHub (looked up once per run, again after a failure), never older than
/// [`CODEX_CLIENT_VERSION`]; anything else (a mock, a proxy) gets that.
fn codex_client_version(base_url: &str) -> String {
    static LATEST: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);
    if base_url != crate::codex::CODEX_BASE_URL {
        return CODEX_CLIENT_VERSION.into();
    }
    let mut latest = LATEST.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if latest.is_none() {
        *latest = latest_codex_release();
    }
    match latest.as_deref() {
        Some(version) if version_parts(version) > version_parts(CODEX_CLIENT_VERSION) => version.to_owned(),
        _ => CODEX_CLIENT_VERSION.into(),
    }
}

/// The version of the newest stable Codex CLI release on GitHub.
fn latest_codex_release() -> Option<String> {
    let mut response = agent(Duration::from_secs(5))
        .get(CODEX_LATEST_RELEASE_URL)
        .header("Accept", "application/vnd.github+json")
        .header("User-Agent", "MangaTranslationTools")
        .call()
        .ok()
        .filter(|response| response.status().is_success())?;
    let release: Value = response.body_mut().read_json().ok()?;
    release_version(release.get("tag_name")?.as_str()?)
}

/// `rust-v0.165.0` → `0.165.0`; `None` for anything but a plain release number.
fn release_version(tag: &str) -> Option<String> {
    let version = tag.trim().trim_start_matches("rust-").trim_start_matches('v');
    (version.split('.').count() == 3 && version.split('.').all(|part| part.parse::<u64>().is_ok()))
        .then(|| version.to_owned())
}

fn version_parts(version: &str) -> Vec<u64> {
    version.split('.').map(|part| part.parse().unwrap_or(0)).collect()
}

/// Codex catalog rows (`{"models": [{"slug", "display_name", "visibility", "priority",
/// "supported_reasoning_levels", "input_modalities"…}]}`), like the Codex model picker.
fn codex_models(catalog: &Value) -> Vec<ModelOption> {
    let text = |row: &Value, key: &str| row.get(key).and_then(Value::as_str).map(str::trim).unwrap_or("").to_owned();
    let mut rows: Vec<(i64, ModelOption)> = catalog
        .get("models")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|row| row.get("visibility").and_then(Value::as_str).unwrap_or("list").eq_ignore_ascii_case("list"))
        .filter(|row| {
            row.get("input_modalities")
                .and_then(Value::as_array)
                .is_none_or(|inputs| inputs.iter().any(|input| input.as_str() == Some("image")))
        })
        .filter_map(|row| {
            let id = text(row, "slug");
            let id = if id.is_empty() { text(row, "id") } else { id };
            if id.is_empty() {
                return None;
            }
            let name = Some(text(row, "display_name")).filter(|name| !name.is_empty()).unwrap_or_else(|| id.clone());
            let mut efforts: Vec<String> = Vec::new();
            for level in row.get("supported_reasoning_levels").and_then(Value::as_array).into_iter().flatten() {
                if let Some(effort) = level.as_str().or_else(|| level.get("effort")?.as_str())
                    && !efforts.iter().any(|known| known == effort)
                {
                    efforts.push(effort.to_owned());
                }
            }
            let priority = row.get("priority").and_then(Value::as_i64).unwrap_or(i64::MAX);
            Some((priority, ModelOption { id, name, description: text(row, "description"), efforts }))
        })
        .collect();
    rows.sort_by_key(|(priority, _)| *priority);
    rows.into_iter().map(|(_, model)| model).collect()
}

/// OpenAI `GET /models` rows (`{"data": [{"id"}]}`), sorted by id.
fn compatible_models(catalog: &Value) -> Vec<ModelOption> {
    let mut ids: Vec<String> = catalog
        .get("data")
        .or_else(|| catalog.get("models"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| row.get("id").or_else(|| row.get("name")).and_then(Value::as_str))
        .map(|id| id.trim().to_owned())
        .filter(|id| !id.is_empty() && !NON_CHAT_MODELS.iter().any(|kind| id.to_ascii_lowercase().contains(kind)))
        .collect();
    ids.sort();
    ids.dedup();
    ids.into_iter()
        .map(|id| ModelOption { name: id.clone(), id, description: String::new(), efforts: Vec::new() })
        .collect()
}

/// `extractModelOutputText`.
pub fn extract_output_text(parsed: &Value) -> String {
    if let Some(text) = parsed.get("output_text").and_then(Value::as_str) {
        return text.trim().to_owned();
    }
    if let Some(content) = parsed.pointer("/choices/0/message/content") {
        if let Some(text) = content.as_str() {
            return text.trim().to_owned();
        }
        if let Some(parts) = content.as_array() {
            return parts
                .iter()
                .map(|p| p.get("text").and_then(Value::as_str).unwrap_or(""))
                .collect::<Vec<_>>()
                .join("\n")
                .trim()
                .to_owned();
        }
    }
    let Some(output) = parsed.get("output").and_then(Value::as_array) else { return String::new() };
    let mut parts = Vec::new();
    for item in output {
        match item.get("content") {
            Some(Value::String(text)) => parts.push(text.clone()),
            Some(Value::Array(contents)) => {
                parts.extend(contents.iter().filter_map(|c| c.get("text").and_then(Value::as_str).map(str::to_owned)));
            }
            _ => {}
        }
    }
    parts.join("\n").trim().to_owned()
}

/// `parseResponsesSseText`: concatenates `response.output_text.delta` events.
pub fn parse_responses_sse(raw: &str) -> String {
    let normalized = raw.replace("\r\n", "\n");
    let mut deltas = Vec::new();
    for block in normalized.split("\n\n") {
        let data: Vec<&str> =
            block.lines().filter_map(|line| line.strip_prefix("data:")).map(str::trim_start).collect();
        if data.is_empty() {
            continue;
        }
        let data = data.join("\n");
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let Ok(event) = serde_json::from_str::<Value>(&data) else { continue };
        let kind = event.get("type").and_then(Value::as_str).unwrap_or("");
        if kind == "response.output_text.delta" {
            if let Some(delta) = event.get("delta").and_then(Value::as_str) {
                deltas.push(delta.to_owned());
            }
            continue;
        }
        if kind == "response.completed" || kind == "response.incomplete" {
            continue;
        }
        let nested = extract_output_text(&event);
        if !nested.is_empty() {
            deltas.push(nested);
        }
    }
    deltas.concat()
}

pub fn preview(text: &str, max_chars: usize) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() <= max_chars {
        compact
    } else {
        format!("{}…", compact.chars().take(max_chars.saturating_sub(1)).collect::<String>())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn codex_release_tags_give_plain_versions() {
        assert_eq!(release_version("rust-v0.165.0").as_deref(), Some("0.165.0"));
        assert_eq!(release_version("v1.2.3").as_deref(), Some("1.2.3"));
        assert_eq!(release_version("rust-v0.166.0-alpha.6"), None, "prerelease");
        assert_eq!(release_version("latest"), None);
        assert!(version_parts("0.165.0") > version_parts("0.99.9"), "numeric, not text, order");
        // Only the real ChatGPT backend asks GitHub.
        assert_eq!(codex_client_version("http://127.0.0.1:9/codex"), CODEX_CLIENT_VERSION);
    }

    #[test]
    #[ignore = "asks GitHub"]
    fn finds_the_latest_codex_release() {
        let version = latest_codex_release().expect("a release");
        assert!(version_parts(&version) >= version_parts(CODEX_CLIENT_VERSION), "{version}");
    }

    #[test]
    fn codex_catalog_keeps_picker_models_that_read_images_best_first() {
        let catalog = json!({ "models": [
            { "slug": "gpt-b", "display_name": "GPT B", "visibility": "list", "priority": 2,
              "supported_reasoning_levels": [{ "effort": "low" }, { "effort": "high" }, "low"],
              "input_modalities": ["text", "image"] },
            { "slug": "hidden", "visibility": "hide", "priority": 0 },
            { "slug": "text-only", "visibility": "list", "priority": 0, "input_modalities": ["text"] },
            { "slug": "gpt-a", "description": "Best", "priority": 1 },
            { "display_name": "no id" },
        ]});
        let models = codex_models(&catalog);
        let ids: Vec<&str> = models.iter().map(|model| model.id.as_str()).collect();
        assert_eq!(ids, ["gpt-a", "gpt-b"]);
        assert_eq!((models[0].name.as_str(), models[0].description.as_str()), ("gpt-a", "Best"));
        assert_eq!(models[1].name, "GPT B");
        assert_eq!(models[1].efforts, ["low", "high"]);
    }

    #[test]
    fn compatible_models_skip_embedding_and_speech_models() {
        let catalog = json!({ "object": "list", "data": [
            { "id": "qwen3-vl:8b" }, { "id": "text-embedding-3-small" }, { "id": "whisper-1" },
            { "id": "gemma4:31b" }, { "id": "gemma4:31b" }, { "id": " " },
        ]});
        let ids: Vec<String> = compatible_models(&catalog).into_iter().map(|model| model.id).collect();
        assert_eq!(ids, ["gemma4:31b", "qwen3-vl:8b"]);
        // Some servers list `models` with names instead.
        let ids: Vec<String> =
            compatible_models(&json!({ "models": [{ "name": "llava" }] })).into_iter().map(|model| model.id).collect();
        assert_eq!(ids, ["llava"]);
    }

    #[test]
    fn ollama_detection() {
        assert!(is_ollama("https://ollama.com/v1"));
        assert!(is_ollama("http://127.0.0.1:11434/v1"));
        assert!(is_ollama("https://api.ollama.com/v1"));
        assert!(!is_ollama("https://api.openai.com/v1"));
    }

    #[test]
    fn chat_body_has_images_prompt_and_ollama_flags() {
        let endpoint = Endpoint {
            base_url: "http://127.0.0.1:11434/v1".into(),
            api_key: None,
            model: "gemma".into(),
            api: Api::ChatCompletions { temperature: 0.0, top_p: 0.85, max_tokens: 900 },
            auth: None,
        };
        let images =
            [ImageVariant { role: crate::images::VariantRole::Original, data_url: "data:image/png;base64,AA".into() }];
        let body: Value = serde_json::from_slice(&chat_request_body(&endpoint, "sys", "prompt", &images)).unwrap();
        assert_eq!(body["messages"][0]["content"][0]["type"], "text");
        assert_eq!(body["messages"][1]["content"][0]["text"], variant_caption(crate::images::VariantRole::Original, 0));
        assert_eq!(body["messages"][1]["content"][1]["type"], "image_url");
        assert_eq!(body["messages"][1]["content"][1]["image_url"]["url"], "data:image/png;base64,AA");
        assert_eq!(body["messages"][1]["content"][2]["text"], "prompt");
        assert_eq!(body["think"], false);
        assert_eq!(body["max_tokens"], 900);
    }

    #[test]
    fn responses_body_matches_the_api_shape() {
        let endpoint = Endpoint {
            base_url: "http://127.0.0.1:10531/v1".into(),
            api_key: None,
            model: "gpt".into(),
            api: Api::Responses { reasoning_effort: "low".into() },
            auth: None,
        };
        let images =
            [ImageVariant { role: crate::images::VariantRole::Original, data_url: "data:image/png;base64,AA".into() }];
        let body: Value = serde_json::from_slice(&chat_request_body(&endpoint, "sys", "p", &images)).unwrap();
        assert_eq!(body["instructions"], "sys");
        assert_eq!(
            body["input"][0]["content"][1],
            json!({ "type": "input_image", "image_url": "data:image/png;base64,AA" })
        );
        assert_eq!(body["input"][0]["content"][2], json!({ "type": "input_text", "text": "p" }));
        assert_eq!(body["reasoning"]["effort"], "low");
        assert_eq!((body["stream"].clone(), body["store"].clone()), (json!(true), json!(false)));
        assert!(body.get("think").is_none());
    }

    #[test]
    fn statuses_that_cannot_succeed_are_not_retried() {
        assert_eq!(retry_for_status(401, None), Retry::No);
        assert_eq!(retry_for_status(400, None), Retry::No);
        assert_eq!(retry_for_status(429, Some(Duration::from_secs(3))), Retry::Later(Some(Duration::from_secs(3))));
        assert_eq!(retry_for_status(503, None), Retry::Later(None));
    }

    #[test]
    fn output_extraction_variants() {
        assert_eq!(extract_output_text(&json!({ "choices": [{ "message": { "content": " hi " } }] })), "hi");
        assert_eq!(
            extract_output_text(
                &json!({ "choices": [{ "message": { "content": [{ "text": "a" }, { "text": "b" }] } }] })
            ),
            "a\nb"
        );
        assert_eq!(extract_output_text(&json!({ "output": [{ "content": [{ "text": "x" }] }] })), "x");
    }

    #[test]
    fn sse_deltas_are_joined() {
        let raw = "event: x\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"id: \"}\n\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"1\"}\r\n\r\ndata: {\"type\":\"response.completed\",\"response\":{}}\n\ndata: [DONE]\n\n";
        assert_eq!(parse_responses_sse(raw), "id: 1");
    }
}
