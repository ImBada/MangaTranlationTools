//! LaMa through the Python sidecar prepared by the TypeScript app (`lamaRuntime.ts`):
//! `<data>/tools/lama-manga-venv` + `<data>/tools/Er0mangaInpaint` +
//! `<data>/models/lama-manga/lama-manga.safetensors`. The sidecar script is embedded.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use image::{GrayImage, RgbaImage};
use mtt_core::paths::AppPaths;

const SCRIPT: &str = include_str!("../assets/lama-inpaint.py");
pub const MODEL_URL: &str = "https://huggingface.co/mayocream/lama-manga/resolve/main/lama-manga.safetensors";

#[derive(Debug, Clone)]
pub struct LamaCommand {
    pub program: PathBuf,
    /// Arguments with `{source}`, `{mask}` and `{output}` placeholders.
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    /// Arguments that start the embedded script as a server (`--serve`), which keeps the
    /// model loaded between inpaints. `None` for a user-configured command.
    pub serve_args: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LamaStatus {
    pub python_ready: bool,
    pub code_ready: bool,
    pub model_exists: bool,
    pub model_path: PathBuf,
}

impl LamaStatus {
    pub fn ready(&self) -> bool {
        self.python_ready && self.code_ready && self.model_exists
    }
}

struct LamaPaths {
    python: PathBuf,
    code_dir: PathBuf,
    model: PathBuf,
    script: PathBuf,
}

fn lama_paths(paths: &AppPaths) -> LamaPaths {
    let venv = paths.tools_dir.join("lama-manga-venv");
    LamaPaths {
        python: if cfg!(windows) { venv.join("Scripts").join("python.exe") } else { venv.join("bin").join("python") },
        code_dir: paths.tools_dir.join("Er0mangaInpaint"),
        model: paths.models_dir.join("lama-manga").join("lama-manga.safetensors"),
        script: paths.tools_dir.join("native").join("lama-inpaint.py"),
    }
}

pub fn status(paths: &AppPaths) -> LamaStatus {
    let lama = lama_paths(paths);
    LamaStatus {
        python_ready: lama.python.is_file(),
        code_ready: lama.code_dir.join("saicinpainting").join("training").join("modules").join("ffc.py").is_file(),
        model_exists: lama.model.is_file(),
        model_path: lama.model,
    }
}

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
        Some(Self { program: PathBuf::from(program.trim()), args, env: Vec::new(), serve_args: None })
    }

    /// The prepared runtime under the data directory, writing the embedded script if needed.
    pub fn detect(paths: &AppPaths) -> Option<Self> {
        if let Some(command) = Self::from_env() {
            return Some(command);
        }
        if !status(paths).ready() {
            return None;
        }
        let lama = lama_paths(paths);
        let current = std::fs::read_to_string(&lama.script).ok();
        if current.as_deref() != Some(SCRIPT) {
            if let Some(parent) = lama.script.parent() {
                std::fs::create_dir_all(parent).ok()?;
            }
            std::fs::write(&lama.script, SCRIPT).ok()?;
        }
        Some(Self {
            program: lama.python,
            args: [
                lama.script.to_string_lossy().as_ref(),
                "--input",
                "{source}",
                "--mask",
                "{mask}",
                "--output",
                "{output}",
                "--weights",
                lama.model.to_string_lossy().as_ref(),
            ]
            .map(String::from)
            .to_vec(),
            env: vec![
                ("MANGA_TRANSLATOR_LAMA_CODE_DIR".into(), lama.code_dir.to_string_lossy().into_owned()),
                ("MANGA_TRANSLATOR_LAMA_WEIGHTS".into(), lama.model.to_string_lossy().into_owned()),
            ],
            serve_args: Some(
                [lama.script.to_string_lossy().as_ref(), "--serve", "--weights", lama.model.to_string_lossy().as_ref()]
                    .map(String::from)
                    .to_vec(),
            ),
        })
    }

    /// Inpaints a crop. `mask` is coverage (white = inpaint).
    pub fn run(&self, source: &RgbaImage, mask: &GrayImage) -> Result<RgbaImage, String> {
        let dir = tempfile::Builder::new().prefix("manga-inpaint-").tempdir().map_err(|e| e.to_string())?;
        let source_path = dir.path().join("source.png");
        let mask_path = dir.path().join("mask.png");
        let output_path = dir.path().join("output.png");
        source.save(&source_path).map_err(|e| e.to_string())?;
        if let Some(serve_args) = &self.serve_args {
            // The embedded script reads the mask as grayscale.
            mask.save(&mask_path).map_err(|e| e.to_string())?;
            match serve_request(self, serve_args, &source_path, &mask_path, &output_path) {
                Ok(()) => return read_output(&output_path, source),
                // The script failed on this image; running it again the one-off way would too.
                Err(ServeError::Request(error)) => return Err(error),
                // Started fresh next time; this one runs the one-off way.
                Err(ServeError::Server(error)) => log::warn!("LaMa server unavailable, running once instead: {error}"),
            }
        } else {
            RgbaImage::from_fn(mask.width(), mask.height(), |x, y| {
                let v = mask.get_pixel(x, y)[0];
                image::Rgba([v, v, v, 255])
            })
            .save(&mask_path)
            .map_err(|e| e.to_string())?;
        }
        let substitute = |arg: &str| {
            arg.replace("{source}", &path_text(&source_path))
                .replace("{mask}", &path_text(&mask_path))
                .replace("{output}", &path_text(&output_path))
        };
        let output = Command::new(&self.program)
            .args(self.args.iter().map(|arg| substitute(arg)))
            .envs(self.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
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
        read_output(&output_path, source)
    }
}

fn read_output(path: &Path, source: &RgbaImage) -> Result<RgbaImage, String> {
    let result = image::open(path).map_err(|e| format!("LaMa 결과를 읽지 못했습니다: {e}"))?.to_rgba8();
    Ok(if result.dimensions() == source.dimensions() {
        result
    } else {
        image::imageops::resize(&result, source.width(), source.height(), image::imageops::FilterType::Triangle)
    })
}

/// The script's reply lines start with this; anything else on stdout is ignored.
const PROTOCOL_PREFIX: &str = "MTT-LAMA ";
/// A server that has not been used for this long is stopped (it holds the model in memory).
const SERVER_IDLE_LIMIT: Duration = Duration::from_secs(10 * 60);

/// The running `--serve` process, started on first use and kept for later inpaints so the
/// model loads once instead of once per page. Inpaints run one at a time, so one is enough.
struct LamaServer {
    /// Program and arguments it was started with.
    key: Vec<String>,
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    stderr_tail: Arc<Mutex<String>>,
    last_used: Instant,
}

impl Drop for LamaServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

static SERVER: LazyLock<Mutex<Option<LamaServer>>> = LazyLock::new(Mutex::default);

impl LamaServer {
    fn start(command: &LamaCommand, args: &[String], key: Vec<String>) -> Result<Self, String> {
        let mut child = Command::new(&command.program)
            .args(args)
            .envs(command.env.iter().map(|(k, v)| (k.as_str(), v.as_str())))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("LaMa 서버 실행 실패 ({}): {error}", command.program.display()))?;
        let stdin = child.stdin.take().ok_or("LaMa 서버 입력을 열지 못했습니다.")?;
        let stdout = BufReader::new(child.stdout.take().ok_or("LaMa 서버 출력을 열지 못했습니다.")?);
        // Drained so the process never blocks on a full pipe; the end is kept for errors.
        let stderr_tail = Arc::new(Mutex::new(String::new()));
        if let Some(stderr) = child.stderr.take() {
            let tail_buffer = Arc::clone(&stderr_tail);
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    let mut buffer = tail_buffer.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                    buffer.push_str(&line);
                    buffer.push('\n');
                    if buffer.len() > 8000 {
                        let cut = buffer.len() - 4000;
                        let cut = (cut..buffer.len()).find(|index| buffer.is_char_boundary(*index)).unwrap_or(cut);
                        buffer.drain(..cut);
                    }
                }
            });
        }
        let mut server = Self { key, child, stdin, stdout, stderr_tail, last_used: Instant::now() };
        let ready = server.read_reply()?;
        if ready.get("ready").and_then(serde_json::Value::as_bool) != Some(true) {
            return Err(server.describe(&ready));
        }
        log::info!("LaMa server ready ({})", command.program.display());
        Ok(server)
    }

    /// The next reply line, skipping anything else the process printed.
    fn read_reply(&mut self) -> Result<serde_json::Value, String> {
        let mut line = String::new();
        loop {
            line.clear();
            let read = self.stdout.read_line(&mut line).map_err(|error| error.to_string())?;
            if read == 0 {
                let stderr = self.stderr_tail.lock().map(|tail| tail.clone()).unwrap_or_default();
                return Err(format!("LaMa 서버가 종료되었습니다: {}", tail(stderr.trim(), 600)));
            }
            if let Some(json) = line.trim_end().strip_prefix(PROTOCOL_PREFIX) {
                return serde_json::from_str(json)
                    .map_err(|error| format!("LaMa 서버 응답을 읽지 못했습니다: {error}"));
            }
        }
    }

    fn describe(&self, reply: &serde_json::Value) -> String {
        let error = reply.get("error").and_then(serde_json::Value::as_str).unwrap_or("알 수 없는 오류");
        format!("LaMa 인페인트 실패: {}", tail(error, 600))
    }

    fn request(&mut self, source: &Path, mask: &Path, output: &Path) -> Result<(), ServeError> {
        let request = serde_json::json!({ "input": source, "mask": mask, "output": output });
        writeln!(self.stdin, "{request}")
            .and_then(|()| self.stdin.flush())
            .map_err(|error| ServeError::Server(format!("LaMa 서버에 요청을 보내지 못했습니다: {error}")))?;
        let reply = self.read_reply().map_err(ServeError::Server)?;
        self.last_used = Instant::now();
        match reply.get("ok").and_then(serde_json::Value::as_bool) {
            Some(true) => Ok(()),
            _ => Err(ServeError::Request(self.describe(&reply))),
        }
    }
}

enum ServeError {
    /// The server could not start or stopped answering.
    Server(String),
    /// The script reported an error for this request.
    Request(String),
}

/// Runs one inpaint on the server, starting it if needed. A failed server is dropped.
fn serve_request(
    command: &LamaCommand,
    args: &[String],
    source: &Path,
    mask: &Path,
    output: &Path,
) -> Result<(), ServeError> {
    let key: Vec<String> =
        std::iter::once(command.program.to_string_lossy().into_owned()).chain(args.iter().cloned()).collect();
    let mut guard = SERVER.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if guard.as_ref().is_none_or(|server| server.key != key) {
        *guard = None;
        *guard = Some(LamaServer::start(command, args, key).map_err(ServeError::Server)?);
        start_idle_watch();
    }
    let server = guard.as_mut().expect("started above");
    let result = server.request(source, mask, output);
    if let Err(ServeError::Server(_)) = &result {
        // The process is gone or out of step; the next request starts a new one.
        *guard = None;
    }
    result
}

/// Stops the server once it has been idle for `SERVER_IDLE_LIMIT`.
fn start_idle_watch() {
    std::thread::spawn(|| {
        loop {
            std::thread::sleep(Duration::from_secs(30));
            let mut guard = SERVER.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            match guard.as_ref() {
                Some(server) if server.last_used.elapsed() >= SERVER_IDLE_LIMIT => {
                    log::info!("LaMa server idle, stopping it");
                    *guard = None;
                    return;
                }
                Some(_) => {}
                None => return,
            }
        }
    });
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
        let status = status(&AppPaths::from_data_root(dir.path()));
        assert!(!status.ready());
        assert!(status.model_path.ends_with("models/lama-manga/lama-manga.safetensors"));
    }

    #[cfg(unix)]
    #[test]
    fn command_runs_and_substitutes_placeholders() {
        // A fake engine that copies the source to the output.
        let dir = tempfile::tempdir().unwrap();
        let command = LamaCommand {
            program: PathBuf::from("/bin/cp"),
            args: vec!["{source}".into(), "{output}".into()],
            env: Vec::new(),
            serve_args: None,
        };
        let source = RgbaImage::from_pixel(4, 3, image::Rgba([1, 2, 3, 255]));
        let result = command.run(&source, &GrayImage::new(4, 3)).unwrap();
        assert_eq!(result, source);
        drop(dir);
    }

    /// Speaks the `--serve` protocol: copies the source to the output.
    const STUB_SERVER: &str = r#"
import json, shutil, sys
print("library chatter before the model is ready")
print("MTT-LAMA " + json.dumps({"ready": True}), flush=True)
for line in sys.stdin:
    request = json.loads(line)
    shutil.copy(request["input"], request["output"])
    print("MTT-LAMA " + json.dumps({"ok": True}), flush=True)
"#;

    #[cfg(unix)]
    #[test]
    fn server_is_reused_and_a_broken_one_falls_back_to_one_runs() {
        if Command::new("python3").arg("--version").output().is_err() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let script = dir.path().join("stub.py");
        std::fs::write(&script, STUB_SERVER).unwrap();
        let command = LamaCommand {
            program: PathBuf::from("python3"),
            args: Vec::new(),
            env: Vec::new(),
            serve_args: Some(vec![script.to_string_lossy().into_owned()]),
        };
        let source = RgbaImage::from_pixel(4, 3, image::Rgba([9, 8, 7, 255]));
        assert_eq!(command.run(&source, &GrayImage::new(4, 3)).unwrap(), source);
        let first = SERVER.lock().unwrap().as_ref().map(|server| server.child.id());
        assert_eq!(command.run(&source, &GrayImage::new(4, 3)).unwrap(), source);
        let second = SERVER.lock().unwrap().as_ref().map(|server| server.child.id());
        assert!(first.is_some() && first == second, "one process for both requests");

        // `cp --serve` can't serve, so the request runs the one-off command instead.
        let broken = LamaCommand {
            program: PathBuf::from("/bin/cp"),
            args: vec!["{source}".into(), "{output}".into()],
            env: Vec::new(),
            serve_args: Some(vec!["--serve".into()]),
        };
        assert_eq!(broken.run(&source, &GrayImage::new(4, 3)).unwrap(), source);
    }

    #[test]
    fn tail_keeps_end() {
        assert_eq!(tail("abcdef", 2), "def");
        assert_eq!(tail("ab", 10), "ab");
    }
}
