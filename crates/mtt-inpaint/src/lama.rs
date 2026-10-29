//! LaMa: `<data>/models/lama-manga/lama-manga.safetensors` run in-process on LibTorch
//! (`lama_torch.rs`, `torch_runtime.rs`), or a command set with
//! `MANGA_TRANSLATOR_LAMA_COMMAND`.

use std::borrow::Cow;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

use image::buffer::ConvertBuffer;
use image::{GrayImage, RgbaImage};
use mtt_core::paths::AppPaths;

use crate::lama_torch::TorchLama;
use crate::model_cache::{self, ModelCache};
use crate::torch_runtime::{self, RuntimeStatus};

pub const MODEL_URL: &str = "https://huggingface.co/mayocream/lama-manga/resolve/main/lama-manga.safetensors";

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

/// `startLamaModelDownload`: whatever is missing of the model (into
/// `models/lama-manga/`) and the LibTorch runtime (CUDA's when the GPU allows,
/// `torch_runtime::download`).
pub fn download_lama(paths: &AppPaths, mut progress: impl FnMut(Download, u64, Option<u64>)) -> Result<(), String> {
    let model = model_path(paths);
    if !model.is_file() {
        let dir = model.parent().expect("model path has a parent");
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        write_model_readme(dir, MODEL_URL, &model);
        download(MODEL_URL, &model, |written, total| progress(Download::Model, written, total))?;
    }
    torch_runtime::download(paths, |written, total| progress(Download::Runtime, written, total))
}

/// A note in the model's folder on where the file comes from, for a manual download.
pub(crate) fn write_model_readme(model_dir: &Path, url: &str, model: &Path) {
    let eol = if cfg!(windows) { "\r\n" } else { "\n" };
    let name = model.file_name().unwrap_or_default().to_string_lossy();
    let text = [&format!("Download {name} from:"), url, "", "Save it as:", &model.to_string_lossy(), ""].join(eol);
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
        assert!(download("http://127.0.0.1:9/x", &dir.path().join("x"), |_, _| {}).is_err());
    }

    #[test]
    fn tail_keeps_end() {
        assert_eq!(tail("abcdef", 2), "def");
        assert_eq!(tail("ab", 10), "ab");
    }
}
