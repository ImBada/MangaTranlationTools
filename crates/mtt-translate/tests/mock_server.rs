//! End-to-end translation against a local mock OpenAI-compatible server.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

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
        },
        include_enhanced_variant: true,
        attempts: 3,
        concurrency: 1,
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
