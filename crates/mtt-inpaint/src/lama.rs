//! LaMa: `<data>/models/lama-manga/lama-manga.safetensors` run in-process on LibTorch
//! (`lama_torch.rs`, `torch_runtime.rs`), or a command set with
//! `MANGA_TRANSLATOR_LAMA_COMMAND`.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use image::{GrayImage, RgbaImage};
use mtt_core::paths::AppPaths;

use crate::lama_torch::TorchLama;
use crate::torch_runtime;

pub const MODEL_URL: &str = "https://huggingface.co/mayocream/lama-manga/resolve/main/lama-manga.safetensors";

#[derive(Debug, Clone)]
pub enum Lama {
    /// The model file on the LibTorch runtime in `runtime`, window by window
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
    /// The LibTorch runtime's folder; `None` where none is published for this platform.
    pub runtime_path: Option<PathBuf>,
    pub runtime_exists: bool,
    /// The configured command, which is used instead of the model.
    pub command: Option<PathBuf>,
}

impl LamaStatus {
    pub fn ready(&self) -> bool {
        (self.model_exists && self.runtime_exists) || self.command.is_some()
    }
}

pub fn model_path(paths: &AppPaths) -> PathBuf {
    paths.models_dir.join("lama-manga").join("lama-manga.safetensors")
}

pub fn status(paths: &AppPaths) -> LamaStatus {
    let model_path = model_path(paths);
    let runtime_path = torch_runtime::runtime_dir(paths);
    LamaStatus {
        model_exists: model_path.is_file(),
        model_path,
        runtime_exists: runtime_path.as_deref().is_some_and(torch_runtime::installed),
        runtime_path,
        command: LamaCommand::from_env().map(|command| command.program),
    }
}

/// Where the model runs (or will, before it has loaded).
pub fn device_label() -> &'static str {
    let loaded =
        MODEL.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_ref().map(|loaded| loaded.model.device());
    match loaded {
        Some(mtt_torch::Device::Mps) => "GPU (MPS)",
        Some(mtt_torch::Device::Cpu) => "CPU",
        None if cfg!(target_os = "macos") => "GPU (MPS)",
        None => "CPU",
    }
}

/// What the Python sidecar of earlier versions left under `<data>/tools`; no longer used.
pub fn legacy_python_runtime(paths: &AppPaths) -> Vec<PathBuf> {
    ["lama-manga-venv", "Er0mangaInpaint", "native", ".lama-runtime-ready.json"]
        .into_iter()
        .map(|name| paths.tools_dir.join(name))
        .filter(|path| path.exists())
        .collect()
}

impl Lama {
    /// The configured command, otherwise the model when it and the runtime have been
    /// downloaded.
    pub fn detect(paths: &AppPaths) -> Option<Self> {
        if let Some(command) = LamaCommand::from_env() {
            return Some(Self::Command(command));
        }
        let model = model_path(paths);
        let runtime = torch_runtime::runtime_dir(paths).filter(|dir| torch_runtime::installed(dir))?;
        model.is_file().then_some(Self::Torch { model, runtime })
    }

    /// Inpaints a crop. `mask` is coverage (white = inpaint); the result matches `source`
    /// outside it.
    pub fn inpaint(&self, source: &RgbaImage, mask: &GrayImage) -> Result<RgbaImage, String> {
        match self {
            Self::Torch { model, runtime } => {
                let model = torch_model(model, runtime)?;
                // One inference at a time: the GPU (or every CPU core) is busy anyway, and
                // LibTorch's MPS stream is shared.
                static RUNNING: Mutex<()> = Mutex::new(());
                let _running = RUNNING.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                crate::windows::inpaint_by_windows(source, mask, |image, mask| model.inpaint(image, mask))
            }
            Self::Command(command) => command.run(source, mask),
        }
    }
}

/// A model that has not been used for this long is unloaded (it holds a few hundred MB;
/// LibTorch itself stays loaded).
const MODEL_IDLE_LIMIT: Duration = Duration::from_secs(10 * 60);

struct LoadedModel {
    path: PathBuf,
    model: Arc<TorchLama>,
    last_used: Instant,
}

/// The model, loaded on first use and kept for later inpaints.
static MODEL: LazyLock<Mutex<Option<LoadedModel>>> = LazyLock::new(Mutex::default);

fn torch_model(path: &Path, runtime: &Path) -> Result<Arc<TorchLama>, String> {
    let mut guard = MODEL.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(loaded) = guard.as_mut().filter(|loaded| loaded.path == path) {
        loaded.last_used = Instant::now();
        return Ok(Arc::clone(&loaded.model));
    }
    let started = Instant::now();
    mtt_torch::load(runtime).map_err(|e| format!("LibTorch를 불러오지 못했습니다: {e}"))?;
    let model = Arc::new(TorchLama::load(path, TorchLama::default_device())?);
    log::info!("LaMa loaded on {:?} in {:.2}s", model.device(), started.elapsed().as_secs_f32());
    let was_loaded = guard.is_some();
    *guard = Some(LoadedModel { path: path.to_owned(), model: Arc::clone(&model), last_used: Instant::now() });
    if !was_loaded {
        start_idle_watch();
    }
    Ok(model)
}

/// Unloads the model once it has been idle for `MODEL_IDLE_LIMIT`.
fn start_idle_watch() {
    std::thread::spawn(|| {
        loop {
            std::thread::sleep(Duration::from_secs(30));
            let mut guard = MODEL.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            match guard.as_ref() {
                Some(loaded) if loaded.last_used.elapsed() >= MODEL_IDLE_LIMIT => {
                    log::info!("LaMa idle, unloading the model");
                    *guard = None;
                    return;
                }
                Some(_) => {}
                None => return,
            }
        }
    });
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

/// `startLamaModelDownload`: whatever is missing of the model (into
/// `models/lama-manga/`) and the LibTorch runtime.
pub fn download_lama(paths: &AppPaths, mut progress: impl FnMut(Download, u64, Option<u64>)) -> Result<(), String> {
    let model = model_path(paths);
    if !model.is_file() {
        let dir = model.parent().expect("model path has a parent");
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        write_model_readme(dir, &model);
        download(MODEL_URL, &model, |written, total| progress(Download::Model, written, total))?;
    }
    torch_runtime::download(paths, |written, total| progress(Download::Runtime, written, total))
}

fn write_model_readme(model_dir: &Path, model: &Path) {
    let eol = if cfg!(windows) { "\r\n" } else { "\n" };
    let text =
        ["Download lama-manga.safetensors from:", MODEL_URL, "", "Save it as:", &model.to_string_lossy(), ""].join(eol);
    let _ = std::fs::write(model_dir.join("README.txt"), text);
}

/// Downloads `url` to `output` via `<output>.download` (redirects followed).
pub fn download(url: &str, output: &Path, mut progress: impl FnMut(u64, Option<u64>)) -> Result<(), String> {
    let mut temp = output.as_os_str().to_owned();
    temp.push(".download");
    let temp = PathBuf::from(temp);
    let _ = std::fs::remove_file(&temp);
    let agent = ureq::Agent::new_with_config(ureq::Agent::config_builder().http_status_as_error(false).build());
    let mut response = agent
        .get(url)
        .header("User-Agent", "MangaTranslationTools")
        .call()
        .map_err(|e| format!("다운로드 실패: {url}: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("다운로드 실패 ({}): {url}", response.status().as_u16()));
    }
    let total = response.headers().get("content-length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse().ok());
    let mut reader = response.body_mut().with_config().limit(u64::MAX).reader();
    let mut file = std::fs::File::create(&temp).map_err(|e| e.to_string())?;
    let mut buffer = vec![0u8; 1 << 16];
    let mut written = 0u64;
    loop {
        let read = reader.read(&mut buffer).map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            format!("다운로드 중 오류: {e}")
        })?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read]).map_err(|e| e.to_string())?;
        written += read as u64;
        progress(written, total);
    }
    file.sync_all().map_err(|e| e.to_string())?;
    std::fs::rename(&temp, output).map_err(|e| e.to_string())
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
        assert!(!status.runtime_exists && !status.ready());
        assert!(Lama::detect(&paths).is_none() || status.command.is_some());
        assert!(legacy_python_runtime(&paths).is_empty());
        std::fs::create_dir_all(paths.tools_dir.join("lama-manga-venv")).unwrap();
        assert_eq!(legacy_python_runtime(&paths), vec![paths.tools_dir.join("lama-manga-venv")]);
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

    #[test]
    fn download_reports_http_errors() {
        // Port 9 (discard) is closed on test machines; the error must be a message, not a panic.
        let dir = tempfile::tempdir().unwrap();
        assert!(download("http://127.0.0.1:9/x", &dir.path().join("x"), |_, _| {}).is_err());
    }

    #[test]
    fn tail_keeps_end() {
        assert_eq!(tail("abcdef", 2), "def");
        assert_eq!(tail("ab", 10), "ab");
    }
}
