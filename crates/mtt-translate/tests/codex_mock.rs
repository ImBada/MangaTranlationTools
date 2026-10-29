//! ChatGPT sign-in and a Codex request against a local mock of OpenAI's servers.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use mtt_translate::client::{Api, Endpoint, list_models, send_bytes};
use mtt_translate::codex::CodexAuth;

fn jwt(account: &str, marker: &str) -> String {
    let encode = |value: serde_json::Value| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value.to_string());
    let claims = serde_json::json!({
        "https://api.openai.com/auth": { "chatgpt_account_id": account, "chatgpt_plan_type": "plus" },
        "https://api.openai.com/profile": { "email": "reader@example.com" },
        "marker": marker,
    });
    format!("{}.{}.sig", encode(serde_json::json!({ "alg": "none" })), encode(claims))
}

struct Request {
    line: String,
    headers: Vec<(String, String)>,
    body: String,
}

fn read_request(stream: &std::net::TcpStream) -> Request {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    let mut headers = Vec::new();
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).unwrap();
        if header == "\r\n" || header.is_empty() {
            break;
        }
        if let Some((name, value)) = header.split_once(':') {
            headers.push((name.trim().to_ascii_lowercase(), value.trim().to_owned()));
        }
    }
    let length = headers.iter().find(|(name, _)| name == "content-length").map_or(0, |(_, v)| v.parse().unwrap());
    let mut body = vec![0u8; length];
    reader.read_exact(&mut body).unwrap();
    Request { line, headers, body: String::from_utf8_lossy(&body).into_owned() }
}

/// Headers of each `/responses` request the mock got.
type SeenHeaders = Arc<Mutex<Vec<Vec<(String, String)>>>>;

/// Serves the device flow (approved on the second poll), token exchange and refresh,
/// `/responses` and `/models`, which each reject the first access token once. Returns the
/// base URL and the `/responses` and `/models` requests' headers.
fn spawn_server() -> (String, SeenHeaders) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&seen);
    std::thread::spawn(move || {
        let (mut polls, mut rejected, mut models_rejected) = (0, false, false);
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let request = read_request(&stream);
            let path = request.line.split_whitespace().nth(1).unwrap_or("").to_owned();
            let (status, content_type, body) = match path.as_str() {
                "/api/accounts/deviceauth/usercode" => (
                    200,
                    "application/json",
                    r#"{"device_auth_id":"dev-1","user_code":"ABCD-1234","interval":"1"}"#.to_owned(),
                ),
                "/api/accounts/deviceauth/token" => {
                    polls += 1;
                    if polls == 1 {
                        (403, "application/json", r#"{"error":"pending"}"#.to_owned())
                    } else {
                        (
                            200,
                            "application/json",
                            r#"{"authorization_code":"code-1","code_verifier":"verifier-1"}"#.to_owned(),
                        )
                    }
                }
                "/oauth/token" => {
                    let marker = if request.body.contains("grant_type=authorization_code")
                        && request.body.contains("code_verifier=verifier-1")
                    {
                        "first"
                    } else if request.body.contains("grant_type=refresh_token")
                        && request.body.contains("refresh_token=r-1")
                    {
                        "second"
                    } else {
                        "bad"
                    };
                    let body = serde_json::json!({
                        "access_token": jwt("acct-1", marker),
                        "refresh_token": if marker == "first" { "r-1" } else { "r-2" },
                        "expires_in": 3600,
                    });
                    (200, "application/json", body.to_string())
                }
                "/responses" => {
                    captured.lock().unwrap().push(request.headers.clone());
                    let bearer =
                        request.headers.iter().find(|(name, _)| name == "authorization").map(|(_, v)| v.clone());
                    if !rejected && bearer == Some(format!("Bearer {}", jwt("acct-1", "first"))) {
                        rejected = true;
                        (401, "application/json", r#"{"error":"expired"}"#.to_owned())
                    } else {
                        let event = serde_json::json!({ "type": "response.output_text.delta", "delta": "안녕" });
                        let done = serde_json::json!({ "type": "response.completed", "response": {} });
                        (200, "text/event-stream", format!("data: {event}\n\ndata: {done}\n\n"))
                    }
                }
                models if models.starts_with("/models?client_version=0.") => {
                    captured.lock().unwrap().push(request.headers.clone());
                    let bearer =
                        request.headers.iter().find(|(name, _)| name == "authorization").map(|(_, v)| v.clone());
                    if !models_rejected && bearer == Some(format!("Bearer {}", jwt("acct-1", "first"))) {
                        models_rejected = true;
                        (401, "application/json", r#"{"error":"expired"}"#.to_owned())
                    } else {
                        let catalog = serde_json::json!({ "models": [
                            { "slug": "gpt-later", "display_name": "GPT Later", "visibility": "list", "priority": 5 },
                            { "slug": "gpt-hidden", "visibility": "hide", "priority": 0 },
                            { "slug": "gpt-first", "display_name": "GPT First", "visibility": "list", "priority": 1,
                              "supported_reasoning_levels": [{ "effort": "low" }, { "effort": "medium" }] },
                        ]});
                        (200, "application/json", catalog.to_string())
                    }
                }
                _ => (404, "text/plain", "missing".to_owned()),
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (base, seen)
}

#[test]
fn device_sign_in_then_a_request_that_refreshes_once() {
    let (base, seen) = spawn_server();
    let dir = std::env::temp_dir().join(format!("mtt-codex-mock-{}", std::process::id()));
    let auth = Arc::new(CodexAuth::with_auth_base(dir.join("codex-auth.json"), &base));

    let device = auth.start_device_login().unwrap();
    assert_eq!(device.user_code, "ABCD-1234");
    assert_eq!(device.verification_url, format!("{base}/codex/device"));
    let account = auth.finish_device_login(&device, &|| false).unwrap();
    assert_eq!(account.id, "acct-1");
    assert_eq!(account.email.as_deref(), Some("reader@example.com"));
    assert_eq!(auth.account(), Some(account));

    let endpoint = Endpoint {
        base_url: base.clone(),
        api_key: None,
        model: "gpt-5.5".into(),
        api: Api::Responses { reasoning_effort: "low".into() },
        auth: Some(Arc::clone(&auth)),
    };
    let agent = mtt_translate::client::job_agent(Duration::from_secs(30));
    let output = send_bytes(&agent, &endpoint, br#"{"model":"gpt-5.5"}"#).unwrap();
    assert_eq!(output, "안녕");

    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2, "rejected once, then sent again after refreshing");
    let header = |index: usize, name: &str| {
        seen[index].iter().find(|(key, _)| key == name).map(|(_, value)| value.clone()).unwrap_or_default()
    };
    assert_eq!(header(1, "authorization"), format!("Bearer {}", jwt("acct-1", "second")));
    assert_eq!(header(1, "chatgpt-account-id"), "acct-1");
    assert_eq!(header(1, "openai-beta"), "responses=experimental");
    assert!(!header(1, "originator").is_empty());
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn lists_the_account_models_refreshing_once() {
    let (base, seen) = spawn_server();
    let dir = std::env::temp_dir().join(format!("mtt-codex-models-{}", std::process::id()));
    let auth = Arc::new(CodexAuth::with_auth_base(dir.join("codex-auth.json"), &base));
    let device = auth.start_device_login().unwrap();
    auth.finish_device_login(&device, &|| false).unwrap();
    let endpoint = Endpoint {
        base_url: base.clone(),
        api_key: None,
        model: String::new(),
        api: Api::Responses { reasoning_effort: "low".into() },
        auth: Some(auth),
    };
    let models = list_models(&endpoint).unwrap();
    let ids: Vec<&str> = models.iter().map(|model| model.id.as_str()).collect();
    assert_eq!(ids, ["gpt-first", "gpt-later"], "picker models, best first");
    assert_eq!(
        (models[0].name.as_str(), models[0].efforts.clone()),
        ("GPT First", vec!["low".into(), "medium".into()])
    );
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2, "rejected once, then asked again after refreshing");
    let header = |name: &str| seen[1].iter().find(|(key, _)| key == name).map(|(_, value)| value.clone());
    assert_eq!(header("authorization"), Some(format!("Bearer {}", jwt("acct-1", "second"))));
    assert_eq!(header("chatgpt-account-id").as_deref(), Some("acct-1"));
    assert_eq!(header("accept").as_deref(), Some("application/json"));
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn cancelling_stops_waiting_for_approval() {
    let (base, _) = spawn_server();
    let dir = std::env::temp_dir().join(format!("mtt-codex-cancel-{}", std::process::id()));
    let auth = CodexAuth::with_auth_base(dir.join("codex-auth.json"), &base);
    let device = auth.start_device_login().unwrap();
    let error = auth.finish_device_login(&device, &|| true).unwrap_err();
    assert!(error.contains("취소"), "{error}");
    assert!(auth.account().is_none());
}

/// Answers each `/oauth/token` refresh with `reply(refresh_token)` after a short pause (so
/// refreshes that aren't serialized would overlap). Returns the base URL and the refresh
/// tokens it was sent.
fn spawn_token_server(
    reply: impl Fn(&str) -> (u16, serde_json::Value) + Send + 'static,
) -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let sent = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&sent);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let request = read_request(&stream);
            let token = request
                .body
                .split('&')
                .find_map(|pair| pair.strip_prefix("refresh_token="))
                .unwrap_or_default()
                .to_owned();
            seen.lock().unwrap().push(token.clone());
            std::thread::sleep(Duration::from_millis(200));
            let (status, body) = reply(&token);
            let body = body.to_string();
            let _ = write!(
                stream,
                "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (base, sent)
}

/// A sign-in file in a fresh folder holding `refresh` and an access token marked `marker`.
fn sign_in_file(name: &str, marker: &str, refresh: &str, expires_at_ms: u64) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("mtt-codex-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("codex-auth.json");
    write_tokens(&path, marker, refresh, expires_at_ms);
    path
}

fn write_tokens(path: &std::path::Path, marker: &str, refresh: &str, expires_at_ms: u64) {
    let tokens = serde_json::json!({
        "access_token": jwt("acct-1", marker), "refresh_token": refresh, "id_token": "id-1",
        "expires_at_ms": expires_at_ms,
    });
    std::fs::write(path, tokens.to_string()).unwrap();
}

fn stored(path: &std::path::Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn later_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64 + 3_600_000
}

#[test]
fn two_sign_ins_for_one_file_spend_the_refresh_token_once() {
    let (base, sent) = spawn_token_server(|token| match token {
        "r-1" => (
            200,
            serde_json::json!({ "access_token": jwt("acct-1", "new"), "refresh_token": "r-2", "expires_in": 3600 }),
        ),
        _ => (401, serde_json::json!({ "error": { "code": "refresh_token_reused" } })),
    });
    let path = sign_in_file("shared", "old", "r-1", 0);
    // The translation job and the settings' model list each make their own.
    let sessions: Vec<_> = (0..2)
        .map(|_| {
            let auth = CodexAuth::with_auth_base(&path, &base);
            std::thread::spawn(move || auth.session())
        })
        .collect();
    for session in sessions {
        assert_eq!(session.join().unwrap().unwrap().access_token, jwt("acct-1", "new"));
    }
    assert_eq!(*sent.lock().unwrap(), ["r-1"], "refreshed once");
    assert_eq!(stored(&path)["refresh_token"], "r-2", "still signed in");
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn a_rejected_refresh_signs_out_only_the_tokens_it_rejected() {
    // Someone else refreshed while this request was out: their tokens stay.
    let path = sign_in_file("rejected-other", "old", "r-1", 0);
    let file = path.clone();
    let (base, _) = spawn_token_server(move |_| {
        write_tokens(&file, "theirs", "r-9", later_ms());
        (401, serde_json::json!({ "error": { "code": "refresh_token_reused" } }))
    });
    let session = CodexAuth::with_auth_base(&path, &base).session().unwrap();
    assert_eq!(session.access_token, jwt("acct-1", "theirs"));
    assert_eq!(stored(&path)["refresh_token"], "r-9");
    std::fs::remove_dir_all(path.parent().unwrap()).ok();

    let path = sign_in_file("rejected-same", "old", "r-1", 0);
    let (base, _) = spawn_token_server(|_| (401, serde_json::json!({ "error": { "code": "refresh_token_expired" } })));
    let error = CodexAuth::with_auth_base(&path, &base).session().unwrap_err();
    assert_eq!(error.retry, mtt_translate::Retry::Stop, "{}", error.message);
    assert!(!path.exists(), "signed out");
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}

#[test]
fn a_refresh_server_error_is_retried_and_omitted_tokens_are_kept() {
    let calls = Arc::new(Mutex::new(0));
    let counted = Arc::clone(&calls);
    let (base, _) = spawn_token_server(move |_| {
        *counted.lock().unwrap() += 1;
        match *counted.lock().unwrap() {
            1 => (503, serde_json::json!({ "error": "unavailable" })),
            _ => (200, serde_json::json!({ "access_token": jwt("acct-1", "new"), "expires_in": 3600 })),
        }
    });
    let path = sign_in_file("server-error", "old", "r-1", 0);
    let auth = CodexAuth::with_auth_base(&path, &base);
    let error = auth.session().unwrap_err();
    assert_eq!(error.retry, mtt_translate::Retry::Later(None), "{}", error.message);
    assert!(path.exists(), "still signed in");
    assert_eq!(auth.session().unwrap().access_token, jwt("acct-1", "new"));
    let tokens = stored(&path);
    assert_eq!((tokens["refresh_token"].as_str(), tokens["id_token"].as_str()), (Some("r-1"), Some("id-1")));
    std::fs::remove_dir_all(path.parent().unwrap()).ok();
}
