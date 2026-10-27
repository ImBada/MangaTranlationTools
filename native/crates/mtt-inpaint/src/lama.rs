//! LaMa through the Python sidecar prepared by the TypeScript app (`lamaRuntime.ts`):
//! `<data>/tools/lama-manga-venv` + `<data>/tools/Er0mangaInpaint` +
//! `<data>/models/lama-manga/lama-manga.safetensors`. The sidecar script is embedded.

use std::path::{Path, PathBuf};
use std::process::Command;

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
        Some(Self { program: PathBuf::from(program.trim()), args, env: Vec::new() })
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
        })
    }

    /// Inpaints a crop. `mask` is coverage (white = inpaint).
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
        let result = image::open(&output_path).map_err(|e| format!("LaMa 결과를 읽지 못했습니다: {e}"))?.to_rgba8();
        Ok(if result.dimensions() == source.dimensions() {
            result
        } else {
            image::imageops::resize(&result, source.width(), source.height(), image::imageops::FilterType::Triangle)
        })
    }
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

    #[test]
    fn command_runs_and_substitutes_placeholders() {
        // A fake engine that copies the source to the output.
        let dir = tempfile::tempdir().unwrap();
        let command = LamaCommand {
            program: PathBuf::from("/bin/cp"),
            args: vec!["{source}".into(), "{output}".into()],
            env: Vec::new(),
        };
        let source = RgbaImage::from_pixel(4, 3, image::Rgba([1, 2, 3, 255]));
        let result = command.run(&source, &GrayImage::new(4, 3)).unwrap();
        assert_eq!(result, source);
        drop(dir);
    }

    #[test]
    fn tail_keeps_end() {
        assert_eq!(tail("abcdef", 2), "def");
        assert_eq!(tail("ab", 10), "ab");
    }
}
