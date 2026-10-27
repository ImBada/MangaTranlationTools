//! Preparing the LaMa Python sidecar (`scripts/prepare-lama-runtime.cjs`, `lamaRuntime.ts`):
//! a venv with the required packages, the Er0mangaInpaint code, and the model download.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use mtt_core::paths::AppPaths;

pub const RUNTIME_VERSION: u32 = 1;
const EROMANGA_ZIP_URL: &str = "https://github.com/Er0manga/Er0mangaInpaint/archive/refs/heads/main.zip";
const PYTHON_PACKAGES: [&str; 19] = [
    "torch",
    "torchvision",
    "opencv-python-headless",
    "hydra-core==1.3.2",
    "omegaconf==2.3.0",
    "pytorch-lightning==1.4.2",
    "torchmetrics==0.6.0",
    "kornia==0.5.0",
    "PyYAML",
    "tqdm",
    "numpy",
    "safetensors",
    "albumentations==0.5.2",
    "scikit-image",
    "scipy",
    "pandas",
    "matplotlib",
    "easydict",
    "webdataset",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PythonCommand {
    pub program: String,
    pub args: Vec<String>,
}

impl PythonCommand {
    pub fn display(&self) -> String {
        std::iter::once(self.program.as_str()).chain(self.args.iter().map(String::as_str)).collect::<Vec<_>>().join(" ")
    }
}

/// `resolvePython`: `PYTHON_BIN`, then python3.11/python3/python (py launcher on Windows).
pub fn resolve_python() -> Option<PythonCommand> {
    let candidates: Vec<(String, Vec<String>)> = match std::env::var("PYTHON_BIN").ok().filter(|v| !v.trim().is_empty())
    {
        Some(explicit) => vec![(explicit.trim().to_owned(), vec![])],
        None if cfg!(windows) => {
            vec![("py".into(), vec!["-3.11".into()]), ("py".into(), vec!["-3".into()]), ("python".into(), vec![])]
        }
        None => vec![("python3.11".into(), vec![]), ("python3".into(), vec![]), ("python".into(), vec![])],
    };
    candidates.into_iter().find_map(|(program, args)| {
        let ok = Command::new(&program)
            .args(&args)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        ok.then_some(PythonCommand { program, args })
    })
}

pub fn python_install_command() -> &'static str {
    if cfg!(target_os = "macos") {
        "brew install python@3.11"
    } else if cfg!(windows) {
        "winget install Python.Python.3.11"
    } else {
        "sudo apt-get update && sudo apt-get install -y python3.11 python3.11-venv"
    }
}

pub fn python_install_help() -> &'static [&'static str] {
    if cfg!(target_os = "macos") {
        &[
            "Homebrew가 있으면 터미널에서 위 명령을 실행하세요.",
            "Homebrew가 없으면 https://www.python.org/downloads/macos/ 에서 Python 3.11 이상 macOS installer를 설치하세요.",
            "설치 후 터미널에서 `python3 --version`이 동작하는지 확인한 뒤 앱에서 새로고침을 누르세요.",
        ]
    } else if cfg!(windows) {
        &[
            "Windows 터미널에서 위 명령을 실행하세요.",
            "winget을 쓸 수 없으면 https://www.python.org/downloads/windows/ 에서 Python 3.11 이상 installer를 받고, 설치 중 Add python.exe to PATH를 켜세요.",
            "설치 후 새 터미널에서 `py -3.11 --version` 또는 `python --version`을 확인한 뒤 앱에서 새로고침을 누르세요.",
        ]
    } else {
        &[
            "Debian/Ubuntu 계열은 위 명령을 실행하세요.",
            "다른 배포판은 패키지 매니저로 Python 3.11 이상과 venv 모듈을 설치하세요.",
            "설치 후 `python3 --version`이 동작하는지 확인한 뒤 앱에서 새로고침을 누르세요.",
        ]
    }
}

fn venv_dir(paths: &AppPaths) -> PathBuf {
    paths.tools_dir.join("lama-manga-venv")
}

fn venv_python(paths: &AppPaths) -> PathBuf {
    let venv = venv_dir(paths);
    if cfg!(windows) { venv.join("Scripts").join("python.exe") } else { venv.join("bin").join("python") }
}

pub fn log_path(paths: &AppPaths) -> PathBuf {
    paths.logs_dir.join("lama-prepare.log")
}

fn model_path(paths: &AppPaths) -> PathBuf {
    paths.models_dir.join("lama-manga").join("lama-manga.safetensors")
}

/// Appends a line to `logs/lama-prepare.log` and forwards it to `progress`.
struct PrepareLog<'a> {
    file: Option<std::fs::File>,
    progress: &'a mut dyn FnMut(&str),
}

impl PrepareLog<'_> {
    fn line(&mut self, text: &str) {
        if let Some(file) = &mut self.file {
            let _ = writeln!(file, "{text}");
        }
        (self.progress)(text);
    }

    fn run(&mut self, program: &Path, args: &[&str], error: &str) -> Result<(), String> {
        self.line(&format!(
            "> {} {}",
            program.file_name().map(|n| n.to_string_lossy()).unwrap_or_default(),
            args.join(" ")
        ));
        let output = Command::new(program).args(args).output().map_err(|e| format!("{error} ({e})"))?;
        if let Some(file) = &mut self.file {
            let _ = file.write_all(&output.stdout);
            let _ = file.write_all(&output.stderr);
        }
        if !output.status.success() {
            return Err(format!("{error} ({} {})", program.display(), args.join(" ")));
        }
        Ok(())
    }
}

/// Runs the whole preparation (idempotent). `progress` receives log lines.
pub fn prepare(paths: &AppPaths, mut progress: impl FnMut(&str)) -> Result<(), String> {
    std::fs::create_dir_all(&paths.tools_dir).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&paths.logs_dir).map_err(|e| e.to_string())?;
    let model_dir = paths.models_dir.join("lama-manga");
    std::fs::create_dir_all(&model_dir).map_err(|e| e.to_string())?;
    let file = std::fs::OpenOptions::new().create(true).append(true).open(log_path(paths)).ok();
    let mut log = PrepareLog { file, progress: &mut progress };
    write_model_readme(&model_dir, &model_path(paths));

    let python = venv_python(paths);
    if !python.is_file() {
        let system = resolve_python()
            .ok_or_else(|| format!("Python 3을 찾을 수 없습니다. 설치 명령: {}", python_install_command()))?;
        let venv = venv_dir(paths);
        std::fs::create_dir_all(&venv).map_err(|e| e.to_string())?;
        let mut args: Vec<&str> = system.args.iter().map(String::as_str).collect();
        let venv_text = venv.to_string_lossy().into_owned();
        args.extend(["-m", "venv", venv_text.as_str()]);
        log.run(Path::new(&system.program), &args, "Python 가상환경 생성에 실패했습니다.")?;
    }

    let marker = paths.tools_dir.join(".lama-runtime-ready.json");
    let marker_ok = std::fs::read(&marker)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .is_some_and(|value| {
            value["runtimeVersion"].as_u64() == Some(u64::from(RUNTIME_VERSION))
                && value["pythonPath"].as_str().is_some_and(|p| Path::new(p).is_file())
        });
    if !marker_ok {
        log.run(
            &python,
            &["-m", "pip", "install", "--upgrade", "pip", "setuptools", "wheel"],
            "pip 업그레이드에 실패했습니다.",
        )?;
        let mut args = vec!["-m", "pip", "install"];
        args.extend(PYTHON_PACKAGES);
        log.run(&python, &args, "LaMa Python 패키지 설치에 실패했습니다.")?;
    }

    let code_dir = paths.tools_dir.join("Er0mangaInpaint");
    if !code_dir.join("saicinpainting").join("training").join("modules").join("ffc.py").is_file() {
        log.line("Er0mangaInpaint 코드 다운로드 중");
        install_eromanga_code(&code_dir)?;
    }

    let result = serde_json::json!({
        "ok": true,
        "runtimeVersion": RUNTIME_VERSION,
        "pythonPath": python.to_string_lossy(),
        "codeDir": code_dir.to_string_lossy(),
        "modelPath": model_path(paths).to_string_lossy(),
        "modelExists": model_path(paths).is_file(),
    });
    std::fs::write(&marker, serde_json::to_string_pretty(&result).unwrap_or_default()).map_err(|e| e.to_string())?;
    log.line("LaMa 런타임 준비 완료");
    Ok(())
}

fn write_model_readme(model_dir: &Path, model: &Path) {
    let eol = if cfg!(windows) { "\r\n" } else { "\n" };
    let text = [
        "Download lama-manga.safetensors from:",
        crate::lama::MODEL_URL,
        "",
        "Save it as:",
        &model.to_string_lossy(),
        "",
    ]
    .join(eol);
    let _ = std::fs::write(model_dir.join("README.txt"), text);
}

fn install_eromanga_code(code_dir: &Path) -> Result<(), String> {
    let temp = tempfile::Builder::new().prefix("manga-lama-code-").tempdir().map_err(|e| e.to_string())?;
    let zip_path = temp.path().join("eromanga.zip");
    download(EROMANGA_ZIP_URL, &zip_path, |_, _| {})?;
    let file = std::fs::File::open(&zip_path).map_err(|e| e.to_string())?;
    let mut archive =
        zip::ZipArchive::new(file).map_err(|e| format!("Er0mangaInpaint 압축 파일을 열 수 없습니다: {e}"))?;
    archive.extract(temp.path()).map_err(|e| e.to_string())?;
    let root = std::fs::read_dir(temp.path())
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .find(|entry| {
            entry.file_type().is_ok_and(|t| t.is_dir())
                && entry.file_name().to_string_lossy().starts_with("Er0mangaInpaint-")
        })
        .ok_or("Er0mangaInpaint 압축 파일 구조를 확인할 수 없습니다.")?;
    let _ = std::fs::remove_dir_all(code_dir);
    if let Some(parent) = code_dir.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::rename(root.path(), code_dir)
        .or_else(|_| copy_dir(&root.path(), code_dir))
        .map_err(|e| format!("Er0mangaInpaint 설치 실패: {e}"))
}

fn copy_dir(from: &Path, to: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let target = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
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

/// `startLamaModelDownload`.
pub fn download_model(paths: &AppPaths, progress: impl FnMut(u64, Option<u64>)) -> Result<(), String> {
    let model = model_path(paths);
    if model.is_file() {
        return Ok(());
    }
    if let Some(parent) = model.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    download(crate::lama::MODEL_URL, &model, progress)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_hints_exist_and_paths_follow_ts_layout() {
        assert!(!python_install_command().is_empty());
        assert_eq!(python_install_help().len(), 3);
        let paths = AppPaths::from_data_root("/data");
        assert!(venv_python(&paths).starts_with("/data/tools/lama-manga-venv"));
        assert_eq!(log_path(&paths), PathBuf::from("/data/logs/lama-prepare.log"));
    }

    #[test]
    fn download_reports_http_errors() {
        // Port 9 (discard) is closed on test machines; the error must be a message, not a panic.
        let dir = tempfile::tempdir().unwrap();
        assert!(download("http://127.0.0.1:9/x", &dir.path().join("x"), |_, _| {}).is_err());
    }
}
