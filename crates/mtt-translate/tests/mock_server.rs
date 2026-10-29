//! End-to-end translation against a local mock OpenAI-compatible server.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mtt_translate::{Api, Endpoint, TranslationConfig, translate_page};

/// Serves `/models` and `/chat/completions`; the first completion is garbage so the retry
/// path is exercised. Returns the base URL and the captured request bodies.
fn spawn_server() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let bodies = Arc::new(Mutex::new(Vec::new()));
    let captured = Arc::clone(&bodies);
    std::thread::spawn(move || {
        let mut completions = 0;
        for stream in listener.incoming().take(4) {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            reader.read_line(&mut request_line).unwrap();
            let mut content_length = 0usize;
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                if header == "\r\n" || header.is_empty() {
                    break;
                }
                if let Some(value) = header.to_ascii_lowercase().strip_prefix("content-length:") {
                    content_length = value.trim().parse().unwrap();
                }
            }
            let mut body = vec![0u8; content_length];
            reader.read_exact(&mut body).unwrap();
            let response_body = if request_line.contains("/models") {
                r#"{"data":[{"id":"mock"}]}"#.to_owned()
            } else {
                captured.lock().unwrap().push(String::from_utf8_lossy(&body).into_owned());
                completions += 1;
                let content = if completions == 1 {
                    "죄송합니다, 형식을 모르겠습니다.".to_owned()
                } else {
                    "id: 1\\ntype: dialogue\\nx: 100\\ny: 100\\nw: 200\\nh: 150\\njp: こんにちは\\nko: 안녕하세요"
                        .to_owned()
                };
                format!(r#"{{"choices":[{{"message":{{"content":"{content}"}}}}]}}"#)
            };
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            )
            .unwrap();
        }
    });
    (format!("http://{address}/v1"), bodies)
}

#[test]
fn translates_with_retry_against_mock_server() {
    let (base_url, bodies) = spawn_server();
    let dir = std::env::temp_dir().join(format!("mtt-mock-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let page = dir.join("page.png");
    image::RgbaImage::from_pixel(40, 60, image::Rgba([255, 255, 255, 255])).save(&page).unwrap();

    let config = TranslationConfig {
        endpoint: Endpoint {
            base_url,
            api_key: Some("secret".into()),
            model: "mock".into(),
            api: Api::ChatCompletions { temperature: 0.0, top_p: 0.85, max_tokens: 900 },
            auth: None,
        },
        include_enhanced_variant: true,
        attempts: 3,
        concurrency: 1,
        agent: mtt_translate::client::job_agent(Duration::from_secs(30)),
        instructions: None,
    };
    mtt_translate::verify_endpoint(&config).unwrap();
    let mut failures = Vec::new();
    let items =
        translate_page(&config, &page, |attempt, error| failures.push((attempt, error.to_owned())), || false).unwrap();

    assert_eq!(items.len(), 1);
    assert_eq!(items[0].ko, "안녕하세요");
    assert_eq!(failures.len(), 1, "first garbage answer triggers one retry");
    let bodies = bodies.lock().unwrap();
    assert_eq!(bodies.len(), 2);
    assert!(bodies[0].contains("Image 2: the same full manga page"), "assist view included");
    assert!(bodies[1].contains("Return only plain text records for a downstream parser"), "retry prompt used");
    std::fs::remove_dir_all(dir).ok();
}

/// Reads one request's head and body from `stream`.
fn read_request(stream: &std::net::TcpStream) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut content_length = 0usize;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).unwrap();
        if header == "\r\n" || header.is_empty() {
            break;
        }
        if let Some(value) = header.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = value.trim().parse().unwrap();
        }
    }
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body).unwrap();
}

fn page_and_config(base_url: String, attempts: u32) -> (std::path::PathBuf, TranslationConfig) {
    let dir = std::env::temp_dir().join(format!("mtt-mock-{}-{attempts}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let page = dir.join("page.png");
    image::RgbaImage::from_pixel(20, 30, image::Rgba([255, 255, 255, 255])).save(&page).unwrap();
    let config = TranslationConfig {
        endpoint: Endpoint {
            base_url,
            api_key: None,
            model: "mock".into(),
            api: Api::ChatCompletions { temperature: 0.0, top_p: 0.85, max_tokens: 900 },
            auth: None,
        },
        include_enhanced_variant: false,
        attempts,
        concurrency: 1,
        agent: mtt_translate::client::job_agent(Duration::from_secs(30)),
        instructions: None,
    };
    (page, config)
}

#[test]
fn cancel_does_not_wait_for_a_slow_model() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    // Takes the request and never answers (a model still thinking).
    std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        read_request(&stream);
        std::thread::sleep(Duration::from_secs(20));
        drop(stream);
    });
    let (page, config) = page_and_config(base_url, 3);
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(300));
        flag.store(true, Ordering::SeqCst);
    });
    let started = Instant::now();
    let result = translate_page(&config, &page, |_, _| {}, || cancel.load(Ordering::SeqCst));
    assert_eq!(result.unwrap_err(), "취소됨");
    assert!(started.elapsed() < Duration::from_secs(2), "returned after {:?}", started.elapsed());
}

#[test]
fn rejected_requests_are_not_retried() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    let requests = Arc::new(Mutex::new(0));
    let counted = Arc::clone(&requests);
    std::thread::spawn(move || {
        for stream in listener.incoming().take(5) {
            let mut stream = stream.unwrap();
            read_request(&stream);
            *counted.lock().unwrap() += 1;
            let body = r#"{"error":"bad key"}"#;
            let _ = write!(
                stream,
                "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    let (page, config) = page_and_config(base_url, 5);
    let mut failures = 0;
    let error = translate_page(&config, &page, |_, _| failures += 1, || false).unwrap_err();
    assert!(error.contains("401"), "{error}");
    assert_eq!((failures, *requests.lock().unwrap()), (1, 1), "one attempt only");
}

#[test]
fn translates_regions_with_close_ups() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base_url = format!("http://{}/v1", listener.local_addr().unwrap());
    let body = Arc::new(Mutex::new(String::new()));
    let captured = Arc::clone(&body);
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut content_length = 0usize;
        loop {
            let mut header = String::new();
            reader.read_line(&mut header).unwrap();
            if header == "\r\n" || header.is_empty() {
                break;
            }
            if let Some(value) = header.to_ascii_lowercase().strip_prefix("content-length:") {
                content_length = value.trim().parse().unwrap();
            }
        }
        let mut request = vec![0u8; content_length];
        reader.read_exact(&mut request).unwrap();
        *captured.lock().unwrap() = String::from_utf8_lossy(&request).into_owned();
        let content = "id: 2\\njp: いや\\nko: 아니\\n\\nid: 1\\njp: えっ\\nko: 엥?";
        let response = format!(r#"{{"choices":[{{"message":{{"content":"{content}"}}}}]}}"#);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
            response.len()
        )
        .unwrap();
    });
    let (page, mut config) = page_and_config(base_url, 1);
    config = config.with_instructions("", "하루: 주인공");
    let regions = [
        mtt_translate::Region { id: 1, bbox: mtt_core::model::BBox::new(0.0, 0.0, 500.0, 500.0) },
        mtt_translate::Region { id: 2, bbox: mtt_core::model::BBox::new(500.0, 500.0, 500.0, 500.0) },
    ];
    let texts = mtt_translate::translate_regions(&config, &page, &regions, |_, _| {}, || false).unwrap();
    let ko: Vec<(u32, &str)> = texts.iter().map(|text| (text.id, text.ko.as_str())).collect();
    assert_eq!(ko, [(2, "아니"), (1, "엥?")]);
    let request = body.lock().unwrap();
    assert!(request.contains("Image 2: close-up of region 1."), "close-ups captioned");
    assert!(request.contains("Image 3: close-up of region 2."));
    assert!(request.contains("id: 2, x: 500, y: 500, w: 500, h: 500"), "regions listed");
    assert!(request.contains("하루: 주인공"), "work notes in the prompt");
}
