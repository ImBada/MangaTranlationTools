//! ChatGPT sign-in and a Codex request against a local mock of OpenAI's servers.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use mtt_translate::client::{Api, Endpoint, send_bytes};
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
/// and `/responses`, which rejects the first access token once. Returns the base URL
/// and the `/responses` requests' headers.
fn spawn_server() -> (String, SeenHeaders) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&seen);
    std::thread::spawn(move || {
        let (mut polls, mut rejected) = (0, false);
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
                        (200, "text/event-stream", format!("data: {event}\n\ndata: [DONE]\n\n"))
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
fn cancelling_stops_waiting_for_approval() {
    let (base, _) = spawn_server();
    let dir = std::env::temp_dir().join(format!("mtt-codex-cancel-{}", std::process::id()));
    let auth = CodexAuth::with_auth_base(dir.join("codex-auth.json"), &base);
    let device = auth.start_device_login().unwrap();
    let error = auth.finish_device_login(&device, &|| true).unwrap_err();
    assert!(error.contains("취소"), "{error}");
    assert!(auth.account().is_none());
}
