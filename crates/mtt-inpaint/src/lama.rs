//! LaMa: `<data>/models/lama-manga/lama-manga.safetensors` run in-process on LibTorch
//! (`lama_torch.rs`, `torch_runtime.rs`), or a command set with
//! `MANGA_TRANSLATOR_LAMA_COMMAND`.

use std::borrow::Cow;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use image::buffer::ConvertBuffer;
use image::{GrayImage, RgbaImage};
use mtt_core::paths::AppPaths;

use crate::lama_torch::TorchLama;
use crate::model_cache::{self, ModelCache};
use crate::torch_runtime::{self, RuntimeStatus};

/// `mayocream/lama-manga` (MIT), pinned to the revision that uploaded the model (later ones
/// only edit the model card) and checked by size and SHA-256 before it's used.
pub const MODEL_URL: &str = "https://huggingface.co/mayocream/lama-manga/resolve/bc1fd58e8d92133f437f62f4f18f7ee3aa7503f8/lama-manga.safetensors";
const MODEL_SIZE: u64 = 204_332_996;
const MODEL_SHA256: &str = "a790515e9da839b8d89af7d565ceb110d908b7d6fbdb991f2acb2ec7d9b08bdb";

#[derive(Debug, Clone)]
pub enum Lama {
    /// The model file on the LibTorch runtime in the folder `runtime`, window by window
    /// (`windows.rs`).
    Torch { model: PathBuf, runtime: PathBuf },
    /// A configured command, given the whole crop.
    Command(LamaCommand),
}

#[derive(Debug, Clone)]
pub struct LamaCommand {
    pub program: PathBuf,
    /// Arguments with `{source}`, `{mask}` and `{output}` placeholders.
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LamaStatus {
    pub model_exists: bool,
    pub model_path: PathBuf,
    /// The LibTorch runtime (shared with the text segmentation model).
    pub runtime: RuntimeStatus,
    /// The configured command, which is used instead of the model.
    pub command: Option<PathBuf>,
}

impl LamaStatus {
    pub fn ready(&self) -> bool {
        (self.model_exists && self.runtime.usable) || self.command.is_some()
    }
}

pub fn model_path(paths: &AppPaths) -> PathBuf {
    paths.models_dir.join("lama-manga").join("lama-manga.safetensors")
}

/// Asks the NVIDIA driver (`torch_runtime::status`), so not on the UI thread.
pub fn status(paths: &AppPaths) -> LamaStatus {
    let model_path = model_path(paths);
    LamaStatus {
        model_exists: model_path.is_file(),
        model_path,
        runtime: torch_runtime::status(paths),
        command: LamaCommand::from_env().map(|command| command.program),
    }
}

impl Lama {
    /// The configured command, otherwise the model when it and a runtime have been
    /// downloaded.
    pub fn detect(paths: &AppPaths) -> Option<Self> {
        if let Some(command) = LamaCommand::from_env() {
            return Some(Self::Command(command));
        }
        let model = model_path(paths);
        (model.is_file() && torch_runtime::available(paths))
            .then(|| Self::Torch { model, runtime: torch_runtime::libtorch_dir(paths) })
    }

    /// Inpaints a crop. `mask` is coverage (white = inpaint); the result matches `source`
    /// outside it. Transparent pixels go in over white, as the text segmentation model sees
    /// them, rather than as whatever color they hold (often black).
    pub fn inpaint(&self, source: &RgbaImage, mask: &GrayImage) -> Result<RgbaImage, String> {
        let source: Cow<'_, RgbaImage> = if source.pixels().all(|pixel| pixel[3] == 255) {
            Cow::Borrowed(source)
        } else {
            Cow::Owned(crate::text_segmentation::over_white(source).convert())
        };
        let source = source.as_ref();
        match self {
            Self::Torch { model, runtime } => {
                let model = MODEL.get(model, runtime, |device| TorchLama::load(model, device))?;
                let _running = model_cache::exclusive();
                crate::windows::inpaint_by_windows(source, mask, |image, mask| model.inpaint(image, mask))
            }
            Self::Command(command) => command.run(source, mask),
        }
    }
}

/// The model, loaded on first use and kept for later inpaints.
static MODEL: ModelCache<TorchLama> = ModelCache::new("LaMa", TorchLama::device);

impl LamaCommand {
    /// `MANGA_TRANSLATOR_LAMA_COMMAND` + `MANGA_TRANSLATOR_LAMA_ARGS` (JSON string array).
    pub fn from_env() -> Option<Self> {
        let program = std::env::var("MANGA_TRANSLATOR_LAMA_COMMAND").ok().filter(|v| !v.trim().is_empty())?;
        let args = std::env::var("MANGA_TRANSLATOR_LAMA_ARGS")
            .ok()
            .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
            .unwrap_or_else(|| {
                ["--input", "{source}", "--mask", "{mask}", "--output", "{output}"].map(String::from).to_vec()
            });
        Some(Self { program: PathBuf::from(program.trim()), args })
    }

    /// Runs the command on a crop through temporary PNGs. `mask` is coverage
    /// (white = inpaint).
    pub fn run(&self, source: &RgbaImage, mask: &GrayImage) -> Result<RgbaImage, String> {
        let dir = tempfile::Builder::new().prefix("manga-inpaint-").tempdir().map_err(|e| e.to_string())?;
        let source_path = dir.path().join("source.png");
        let mask_path = dir.path().join("mask.png");
        let output_path = dir.path().join("output.png");
        source.save(&source_path).map_err(|e| e.to_string())?;
        RgbaImage::from_fn(mask.width(), mask.height(), |x, y| {
            let v = mask.get_pixel(x, y)[0];
            image::Rgba([v, v, v, 255])
        })
        .save(&mask_path)
        .map_err(|e| e.to_string())?;
        let substitute = |arg: &str| {
            arg.replace("{source}", &path_text(&source_path))
                .replace("{mask}", &path_text(&mask_path))
                .replace("{output}", &path_text(&output_path))
        };
        let output = Command::new(&self.program)
            .args(self.args.iter().map(|arg| substitute(arg)))
            .output()
            .map_err(|error| format!("LaMa 실행 실패 ({}): {error}", self.program.display()))?;
        if !output.status.success() {
            let detail =
                String::from_utf8_lossy(if output.stderr.is_empty() { &output.stdout } else { &output.stderr })
                    .trim()
                    .to_owned();
            return Err(format!(
                "LaMa 인페인트 명령이 실패했습니다. code={:?}: {}",
                output.status.code(),
                tail(&detail, 600)
            ));
        }
        let result = image::open(&output_path).map_err(|e| format!("LaMa 결과를 읽지 못했습니다: {e}"))?.to_rgba8();
        Ok(if result.dimensions() == source.dimensions() {
            result
        } else {
            image::imageops::resize(&result, source.width(), source.height(), image::imageops::FilterType::Triangle)
        })
    }
}

/// What `download_lama` is fetching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Download {
    Model,
    Runtime,
}

/// The error of a download whose progress callback asked it to stop. What it got so far
/// stays for the next download of the file to resume.
pub const CANCELLED: &str = "다운로드를 취소했습니다.";

/// `startLamaModelDownload`: whatever is missing of the model (into
/// `models/lama-manga/`, checked against the pinned size and SHA-256) and the LibTorch
/// runtime (CUDA's when the GPU allows, `torch_runtime::download`). `progress` gets the
/// part, the bytes so far and the total, and returns false to stop ([`CANCELLED`]).
pub fn download_lama(
    paths: &AppPaths,
    mut progress: impl FnMut(Download, u64, Option<u64>) -> bool,
) -> Result<(), String> {
    let model = model_path(paths);
    if !model.is_file() {
        let dir = model.parent().expect("model path has a parent");
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        write_model_readme(dir, MODEL_URL, &model);
        let unverified = model.with_extension("safetensors.unverified");
        download(MODEL_URL, &unverified, |written, total| progress(Download::Model, written, total))?;
        install_verified(&unverified, &model, MODEL_SIZE, MODEL_SHA256, "LaMa 모델")?;
    }
    torch_runtime::download(paths, |written, total| progress(Download::Runtime, written, total))
}

/// Moves the downloaded `unverified` file to `path` when it has the pinned `size` and
/// `sha256`, so `path` only ever holds a checked file; otherwise deletes it.
fn install_verified(unverified: &Path, path: &Path, size: u64, sha256: &str, label: &str) -> Result<(), String> {
    let checked = torch_runtime::verify(unverified, size, sha256, label);
    if checked.is_err() {
        let _ = std::fs::remove_file(unverified);
    }
    checked?;
    std::fs::rename(unverified, path).map_err(|e| e.to_string())
}

/// A note in the model's folder on where the file comes from, for a manual download.
pub(crate) fn write_model_readme(model_dir: &Path, url: &str, model: &Path) {
    let eol = if cfg!(windows) { "\r\n" } else { "\n" };
    let name = model.file_name().unwrap_or_default().to_string_lossy();
    let text = [&format!("Download {name} from:"), url, "", "Save it as:", &model.to_string_lossy(), ""].join(eol);
    let _ = std::fs::write(model_dir.join("README.txt"), text);
}

/// How `download` waits and retries.
#[derive(Debug, Clone, Copy)]
struct Patience {
    /// Each of resolving, connecting (with TLS) and sending the request.
    connect: Duration,
    /// For the response headers.
    response: Duration,
    /// The longest wait for more of the body: a stalled connection fails after it.
    stall: Duration,
    /// Tries in all; each after the first resumes where the last one stopped.
    attempts: u32,
    retry_delay: Duration,
    https_only: bool,
}

const PATIENCE: Patience = Patience {
    connect: Duration::from_secs(30),
    response: Duration::from_secs(60),
    stall: Duration::from_secs(60),
    attempts: 4,
    retry_delay: Duration::from_secs(3),
    https_only: true,
};

/// Downloads `url` (https only, redirects too) to `output` via `<output>.download`, with
/// timeouts on connecting, on the response and on a body that stops coming. A connection
/// that drops or stalls resumes where it stopped (an HTTP range request) a few times; when
/// it still fails, or `progress` (bytes so far, total) returns false ([`CANCELLED`]), the
/// part stays for the next download to resume. A write error (a full disk) deletes it.
pub fn download(url: &str, output: &Path, progress: impl FnMut(u64, Option<u64>) -> bool) -> Result<(), String> {
    download_with(&PATIENCE, url, output, progress)
}

/// Why a try of `download_with` stopped.
enum Failure {
    /// The connection: worth another try.
    Network(String),
    /// Writing the part, which is deleted.
    Write(String),
    /// An HTTP error or a cancel: no retry.
    Final(String),
}

fn download_with(
    patience: &Patience,
    url: &str,
    output: &Path,
    mut progress: impl FnMut(u64, Option<u64>) -> bool,
) -> Result<(), String> {
    let mut temp = output.as_os_str().to_owned();
    temp.push(".download");
    let temp = PathBuf::from(temp);
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .http_status_as_error(false)
            .https_only(patience.https_only)
            .timeout_resolve(Some(patience.connect))
            .timeout_connect(Some(patience.connect))
            .timeout_send_request(Some(patience.connect))
            .timeout_recv_response(Some(patience.response))
            .timeout_per_read(Some(patience.stall))
            .build(),
    );
    let mut attempt = 1;
    loop {
        match fetch(&agent, url, &temp, &mut progress) {
            Ok(()) => break,
            Err(Failure::Network(error)) if attempt < patience.attempts => {
                log::warn!("download of {url} stopped ({error}), trying again from where it stopped");
                attempt += 1;
                std::thread::sleep(patience.retry_delay);
            }
            Err(Failure::Network(error) | Failure::Final(error)) => return Err(error),
            Err(Failure::Write(error)) => {
                let _ = std::fs::remove_file(&temp);
                return Err(format!("다운로드한 파일을 쓰지 못했습니다: {error}"));
            }
        }
    }
    std::fs::rename(&temp, output).map_err(|e| e.to_string())
}

/// One try: resumes the part in `temp` when there is one and the server sends the rest
/// (206 from where it ends), otherwise starts it over.
fn fetch(
    agent: &ureq::Agent,
    url: &str,
    temp: &Path,
    progress: &mut impl FnMut(u64, Option<u64>) -> bool,
) -> Result<(), Failure> {
    let have = std::fs::metadata(temp).map_or(0, |metadata| metadata.len());
    let mut request = agent.get(url).header("User-Agent", "MangaTranslationTools");
    if have > 0 {
        request = request.header("Range", format!("bytes={have}-"));
    }
    let mut response = request.call().map_err(|error| {
        let message = format!("다운로드 실패: {url}: {error}");
        match error {
            ureq::Error::Io(_)
            | ureq::Error::Timeout(_)
            | ureq::Error::HostNotFound
            | ureq::Error::ConnectionFailed
            | ureq::Error::Protocol(_) => Failure::Network(message),
            // Plain http, a bad address, TLS: trying again won't help.
            _ => Failure::Final(message),
        }
    })?;
    let header = |name: &str| response.headers().get(name).and_then(|value| value.to_str().ok()).map(str::to_owned);
    let length: Option<u64> = header("content-length").and_then(|value| value.parse().ok());
    let range = header("content-range").and_then(|value| content_range(&value));
    let (start, total) = match (response.status().as_u16(), range) {
        (206, Some((first, total))) if first == have => (have, total.or(length.map(|length| have + length))),
        // A range that doesn't continue the part, or none left past it: start over.
        (206, _) | (416, _) => {
            let _ = std::fs::remove_file(temp);
            return Err(Failure::Network("이어받을 수 없어 처음부터 다시 받습니다".into()));
        }
        (200..=299, _) => (0, length),
        (status, _) => return Err(Failure::Final(format!("다운로드 실패 ({status}): {url}"))),
    };
    let file =
        if start > 0 { std::fs::OpenOptions::new().append(true).open(temp) } else { std::fs::File::create(temp) };
    let mut file = file.map_err(|e| Failure::Write(e.to_string()))?;
    let mut reader = response.body_mut().with_config().limit(u64::MAX).reader();
    let mut buffer = vec![0u8; 1 << 16];
    let mut written = start;
    if !progress(written, total) {
        return Err(Failure::Final(CANCELLED.into()));
    }
    loop {
        let read = reader.read(&mut buffer).map_err(|e| Failure::Network(format!("다운로드 중 오류: {e}")))?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read]).map_err(|e| Failure::Write(e.to_string()))?;
        written += read as u64;
        if !progress(written, total) {
            return Err(Failure::Final(CANCELLED.into()));
        }
    }
    if let Some(total) = total.filter(|total| written < *total) {
        return Err(Failure::Network(format!("다운로드 중 연결이 끊겼습니다 ({written}/{total}바이트)")));
    }
    file.sync_all().map_err(|e| Failure::Write(e.to_string()))
}

/// The first byte and the total (unless `*`) of a `Content-Range: bytes <first>-<last>/<total>`.
fn content_range(value: &str) -> Option<(u64, Option<u64>)> {
    let (range, total) = value.trim().strip_prefix("bytes ")?.split_once('/')?;
    let first = range.split_once('-')?.0.trim().parse().ok()?;
    Some((first, total.trim().parse().ok()))
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn tail(text: &str, max: usize) -> &str {
    let start = text.char_indices().rev().nth(max).map(|(index, _)| index).unwrap_or(0);
    &text[start..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_of_empty_data_dir() {
        let dir = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_root(dir.path());
        let status = status(&paths);
        assert!(!status.model_exists);
        assert!(status.model_path.ends_with("models/lama-manga/lama-manga.safetensors"));
        assert!(!status.runtime.installed && !status.runtime.usable && !status.ready());
        assert!(Lama::detect(&paths).is_none() || status.command.is_some());
    }

    #[cfg(unix)]
    #[test]
    fn command_runs_and_substitutes_placeholders() {
        // A fake engine that copies the source to the output.
        let command =
            LamaCommand { program: PathBuf::from("/bin/cp"), args: vec!["{source}".into(), "{output}".into()] };
        let source = RgbaImage::from_pixel(4, 3, image::Rgba([1, 2, 3, 255]));
        let result = Lama::Command(command).inpaint(&source, &GrayImage::new(4, 3)).unwrap();
        assert_eq!(result, source);
    }

    #[cfg(unix)]
    #[test]
    fn transparent_pixels_go_in_over_white() {
        // The engine sees what `cp` hands back: a transparent black pixel as white, a half
        // transparent one half way to white.
        let command =
            LamaCommand { program: PathBuf::from("/bin/cp"), args: vec!["{source}".into(), "{output}".into()] };
        let mut source = RgbaImage::from_pixel(3, 1, image::Rgba([0, 0, 0, 0]));
        source.put_pixel(1, 0, image::Rgba([0, 0, 0, 128]));
        source.put_pixel(2, 0, image::Rgba([10, 20, 30, 255]));
        let result = Lama::Command(command).inpaint(&source, &GrayImage::new(3, 1)).unwrap();
        let pixels: Vec<[u8; 4]> = result.pixels().map(|pixel| pixel.0).collect();
        assert_eq!(pixels, [[255, 255, 255, 255], [127, 127, 127, 255], [10, 20, 30, 255]]);
    }

    #[test]
    fn download_reports_http_errors() {
        // Port 9 (discard) is closed on test machines; the error must be a message, not a panic.
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("x");
        assert!(download_with(&TEST_PATIENCE, "http://127.0.0.1:9/x", &output, |_, _| true).is_err());
        let started = std::time::Instant::now();
        let error = download("http://127.0.0.1:9/x", &output, |_, _| true).unwrap_err();
        assert!(error.to_lowercase().contains("https"), "plain http is refused: {error}");
        assert!(started.elapsed() < Duration::from_secs(1), "and not tried again");
    }

    #[test]
    fn the_model_is_pinned_and_checked_before_it_is_used() {
        let revision = MODEL_URL.split("/resolve/").nth(1).and_then(|rest| rest.split('/').next()).unwrap();
        assert!(MODEL_URL.starts_with("https://huggingface.co/mayocream/lama-manga/"));
        assert!(revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit()), "{revision}");
        assert!(MODEL_SHA256.len() == 64 && MODEL_SHA256.bytes().all(|byte| byte.is_ascii_hexdigit()));

        let dir = tempfile::tempdir().unwrap();
        let (unverified, model) = (dir.path().join("model.unverified"), dir.path().join("model"));
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        std::fs::write(&unverified, b"abd").unwrap();
        let error = install_verified(&unverified, &model, 3, abc, "LaMa 모델").unwrap_err();
        assert!(error.contains("LaMa 모델 파일이 손상되었습니다"), "{error}");
        assert!(!unverified.exists() && !model.exists(), "a bad download is dropped, not installed");
        std::fs::write(&unverified, b"abc").unwrap();
        install_verified(&unverified, &model, 3, abc, "LaMa 모델").unwrap();
        assert!(!unverified.exists() && std::fs::read(&model).unwrap() == b"abc");
    }

    /// Fast patience over plain http, for the local server.
    const TEST_PATIENCE: Patience = Patience {
        connect: Duration::from_secs(5),
        response: Duration::from_secs(5),
        stall: Duration::from_millis(300),
        attempts: 3,
        retry_delay: Duration::ZERO,
        https_only: false,
    };

    /// A local HTTP server for `connections` connections; `respond` gets each one's index
    /// and request and returns the raw response and how long to wait before closing. Joins
    /// to the requests it got.
    fn serve(
        connections: usize,
        mut respond: impl FnMut(usize, &str) -> (Vec<u8>, Duration) + Send + 'static,
    ) -> (String, std::thread::JoinHandle<Vec<String>>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/model", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let mut requests = Vec::new();
            for index in 0..connections {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = Vec::new();
                let mut byte = [0u8];
                while !request.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
                    request.push(byte[0]);
                }
                let request = String::from_utf8_lossy(&request).to_lowercase();
                let (response, pause) = respond(index, &request);
                let _ = stream.write_all(&response);
                std::thread::sleep(pause);
                requests.push(request);
            }
            requests
        });
        (url, server)
    }

    fn body() -> Vec<u8> {
        (0..200_000u32).map(|i| (i % 251) as u8).collect()
    }

    /// `body` from byte `from`: all of it as a 200, or the rest as a 206.
    fn response(body: &[u8], from: usize, partial: bool) -> Vec<u8> {
        let head = if partial {
            format!(
                "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {from}-{}/{}\r\nContent-Length: {}\r\n",
                body.len() - 1,
                body.len(),
                body.len() - from
            )
        } else {
            format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n", body.len())
        };
        let mut response = format!("{head}Connection: close\r\n\r\n").into_bytes();
        response.extend_from_slice(&body[if partial { from } else { 0 }..]);
        response
    }

    /// The `Range: bytes=<from>-` of a request.
    fn range_from(request: &str) -> Option<usize> {
        request.lines().find_map(|line| line.strip_prefix("range: bytes=")?.trim_end_matches('-').parse().ok())
    }

    #[test]
    fn a_dropped_download_resumes_where_it_stopped() {
        let body = body();
        let served = body.clone();
        let (url, server) = serve(2, move |index, request| match index {
            // Cut off after 70000 of its bytes.
            0 => (response(&served, 0, false)[..70_000 + 60].to_vec(), Duration::ZERO),
            _ => (response(&served, range_from(request).unwrap(), true), Duration::ZERO),
        });
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("model");
        let mut last = (0, None);
        download_with(&TEST_PATIENCE, &url, &output, |written, total| {
            last = (written, total);
            true
        })
        .unwrap();
        assert_eq!(std::fs::read(&output).unwrap(), body);
        assert_eq!(last, (200_000, Some(200_000)));
        let requests = server.join().unwrap();
        let resumed = range_from(&requests[1]).expect("the second request asks for the rest");
        assert!(resumed > 0 && resumed <= 70_000, "{resumed}");
        assert!(!dir.path().join("model.download").exists());
    }

    #[test]
    fn a_stalled_download_fails_and_keeps_its_part() {
        let body = body();
        let (url, server) =
            serve(1, move |_, _| (response(&body, 0, false)[..10_000].to_vec(), Duration::from_secs(2)));
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("model");
        let started = std::time::Instant::now();
        let patience = Patience { attempts: 1, ..TEST_PATIENCE };
        let error = download_with(&patience, &url, &output, |_, _| true).unwrap_err();
        assert!(started.elapsed() < Duration::from_millis(1500), "the stall watchdog: {:?}", started.elapsed());
        assert!(error.contains("다운로드 중 오류"), "{error}");
        let part = std::fs::metadata(dir.path().join("model.download")).unwrap().len();
        assert!(part > 0 && part < 10_000, "{part}");
        assert!(!output.exists());
        server.join().unwrap();
    }

    #[test]
    fn a_cancelled_download_resumes_next_time_or_starts_over() {
        let body = body();
        let served = body.clone();
        // The second time the server ignores the range and sends everything.
        let (url, server) = serve(2, move |_, _| (response(&served, 0, false), Duration::ZERO));
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("model");
        let error = download_with(&TEST_PATIENCE, &url, &output, |written, _| written == 0).unwrap_err();
        assert_eq!(error, CANCELLED);
        assert!(std::fs::metadata(dir.path().join("model.download")).unwrap().len() > 0);
        download_with(&TEST_PATIENCE, &url, &output, |_, _| true).unwrap();
        assert_eq!(std::fs::read(&output).unwrap(), body);
        let requests = server.join().unwrap();
        assert!(range_from(&requests[0]).is_none() && range_from(&requests[1]).is_some());
    }

    #[cfg(unix)]
    #[test]
    fn a_write_error_deletes_the_part() {
        use std::os::unix::fs::PermissionsExt;
        let body = body();
        let (url, server) =
            serve(1, move |_, request| (response(&body, range_from(request).unwrap_or(0), true), Duration::ZERO));
        let dir = tempfile::tempdir().unwrap();
        let part = dir.path().join("model.download");
        std::fs::write(&part, [0; 10]).unwrap();
        std::fs::set_permissions(&part, std::fs::Permissions::from_mode(0o444)).unwrap();
        let error = download_with(&TEST_PATIENCE, &url, &dir.path().join("model"), |_, _| true).unwrap_err();
        assert!(error.contains("쓰지 못했습니다"), "{error}");
        assert!(!part.exists());
        server.join().unwrap();
    }

    #[test]
    fn content_ranges_read_their_first_byte_and_total() {
        assert_eq!(content_range("bytes 100-199/200"), Some((100, Some(200))));
        assert_eq!(content_range("bytes 5-9/*"), Some((5, None)));
        assert_eq!(content_range("bytes */200"), None);
    }

    #[test]
    fn tail_keeps_end() {
        assert_eq!(tail("abcdef", 2), "def");
        assert_eq!(tail("ab", 10), "ab");
    }
}
