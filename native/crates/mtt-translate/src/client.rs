//! OpenAI-compatible `/chat/completions` and Responses API (`/responses`, SSE) requests
//! (`simple-page-translate.cjs`).

use std::time::Duration;

use serde_json::{Value, json};

use crate::images::{ImageVariant, variant_caption};

/// Everything needed to talk to one endpoint.
#[derive(Debug, Clone)]
pub struct Endpoint {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub api: Api,
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
        ureq::Agent::config_builder().http_status_as_error(false).timeout_global(Some(timeout)).build(),
    )
}

/// `isOllamaCompatibleEndpoint`: Ollama accepts extra flags to disable thinking.
pub fn is_ollama(base_url: &str) -> bool {
    let lower = base_url.to_ascii_lowercase();
    let host = lower.split("://").nth(1).unwrap_or(&lower).split('/').next().unwrap_or("");
    let (hostname, port) = host.rsplit_once(':').map(|(h, p)| (h, Some(p))).unwrap_or((host, None));
    hostname == "ollama.com" || hostname.ends_with(".ollama.com") || port == Some("11434")
}

pub fn chat_request_body(endpoint: &Endpoint, system: &str, prompt: &str, images: &[ImageVariant]) -> Value {
    let mut user: Vec<Value> = images
        .iter()
        .enumerate()
        .flat_map(|(index, image)| {
            [
                json!({ "type": "text", "text": variant_caption(image.role, index) }),
                json!({ "type": "image_url", "image_url": { "url": image.data_url } }),
            ]
        })
        .collect();
    user.push(json!({ "type": "text", "text": prompt }));
    let messages = json!([
        { "role": "system", "content": [{ "type": "text", "text": system }] },
        { "role": "user", "content": user }
    ]);
    match &endpoint.api {
        Api::ChatCompletions { temperature, top_p, max_tokens } => {
            let mut body = json!({
                "model": endpoint.model,
                "temperature": temperature,
                "top_p": top_p,
                "presence_penalty": 0,
                "frequency_penalty": 0,
                "max_tokens": max_tokens,
                "messages": messages,
            });
            if is_ollama(&endpoint.base_url) {
                body["reasoning_budget"] = json!(0);
                body["enable_thinking"] = json!(false);
                body["think"] = json!(false);
                body["reasoning_effort"] = json!("none");
            }
            body
        }
        Api::Responses { reasoning_effort } => {
            let mut content: Vec<Value> = images
                .iter()
                .enumerate()
                .flat_map(|(index, image)| {
                    [
                        json!({ "type": "input_text", "text": variant_caption(image.role, index) }),
                        json!({ "type": "input_image", "image_url": image.data_url }),
                    ]
                })
                .collect();
            content.push(json!({ "type": "input_text", "text": prompt }));
            json!({
                "model": endpoint.model,
                "instructions": system,
                "input": [{ "role": "user", "content": content }],
                "reasoning": { "effort": reasoning_effort },
                "stream": true,
                "store": false,
            })
        }
    }
}

/// Sends a request and returns the model's text output.
pub fn send(endpoint: &Endpoint, body: &Value, timeout: Duration) -> Result<String, String> {
    let path = match endpoint.api {
        Api::ChatCompletions { .. } => "chat/completions",
        Api::Responses { .. } => "responses",
    };
    let url = format!("{}/{path}", endpoint.base_url.trim_end_matches('/'));
    let mut request = agent(timeout).post(&url).header("Content-Type", "application/json");
    if let Some(key) = endpoint.api_key.as_deref().filter(|k| !k.trim().is_empty()) {
        request = request.header("Authorization", &format!("Bearer {}", key.trim()));
    }
    let mut response = request.send_json(body).map_err(|e| format!("요청 전송 실패 ({url}): {e}"))?;
    let status = response.status();
    let text = response
        .body_mut()
        .with_config()
        .limit(64 * 1024 * 1024)
        .read_to_string()
        .map_err(|e| format!("응답을 읽지 못했습니다: {e}"))?;
    if !status.is_success() {
        return Err(format!("요청 실패 ({}): {}", status.as_u16(), preview(&text, 600)));
    }
    let output = match endpoint.api {
        Api::Responses { .. } => parse_responses_sse(&text),
        Api::ChatCompletions { .. } => {
            let parsed: Value = serde_json::from_str(&text)
                .map_err(|e| format!("응답 JSON 파싱 실패: {e}: {}", preview(&text, 400)))?;
            extract_output_text(&parsed)
        }
    };
    if output.trim().is_empty() {
        return Err(format!("모델이 빈 응답을 반환했습니다: {}", preview(&text, 400)));
    }
    Ok(output.trim().to_owned())
}

/// `GET /models` to check the base URL and key before a batch (`verifyEndpoint`).
pub fn verify(endpoint: &Endpoint) -> Result<(), String> {
    let url = format!("{}/models", endpoint.base_url.trim_end_matches('/'));
    let mut request = agent(Duration::from_secs(30)).get(&url);
    if let Some(key) = endpoint.api_key.as_deref().filter(|k| !k.trim().is_empty()) {
        request = request.header("Authorization", &format!("Bearer {}", key.trim()));
    }
    let mut response =
        request.call().map_err(|e| format!("엔드포인트에 연결하지 못했습니다. Base URL을 확인하세요. ({url}: {e})"))?;
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let text = response.body_mut().read_to_string().unwrap_or_default();
    let hint = match status.as_u16() {
        401 | 403 => "API 키를 확인하세요.",
        404 => "Base URL이 /v1 까지 포함하는지 확인하세요.",
        429 => "요청 한도를 초과했습니다. 잠시 후 다시 시도하세요.",
        _ => "",
    };
    Err(format!("엔드포인트 확인 실패 ({}). {hint} {}", status.as_u16(), preview(&text, 300)))
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
        };
        let images =
            [ImageVariant { role: crate::images::VariantRole::Original, data_url: "data:image/png;base64,AA".into() }];
        let body = chat_request_body(&endpoint, "sys", "prompt", &images);
        assert_eq!(body["messages"][1]["content"][1]["image_url"]["url"], "data:image/png;base64,AA");
        assert_eq!(body["messages"][1]["content"][2]["text"], "prompt");
        assert_eq!(body["think"], false);
        assert_eq!(body["max_tokens"], 900);
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
