import { createWriteStream, existsSync, mkdirSync, openSync, closeSync, rmSync } from "node:fs";
import { rename, rm } from "node:fs/promises";
import https from "node:https";
import { join } from "node:path";
import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import type { AppPaths } from "./appPaths";
import type { LamaRuntimeStatus } from "../shared/types";

const modelUrl = "https://huggingface.co/mayocream/lama-manga/resolve/main/lama-manga.safetensors";

let prepareProcess: ChildProcess | null = null;
let modelDownload: Promise<void> | null = null;
let lastError: string | undefined;

export function getLamaRuntimeStatus(paths: AppPaths): LamaRuntimeStatus {
  const lama = resolveLamaPaths(paths);
  const python = resolvePython();
  return {
    pythonAvailable: Boolean(python),
    pythonCommand: python ? [python.command, ...python.args].join(" ") : null,
    pythonInstallCommand: pythonInstallCommand(),
    pythonInstallHelp: pythonInstallHelp(),
    runtimeReady: existsSync(lama.pythonPath) && existsSync(lama.codeReadyPath),
    runtimePreparing: Boolean(prepareProcess),
    modelExists: existsSync(lama.modelPath),
    modelDownloading: Boolean(modelDownload),
    modelPath: lama.modelPath,
    modelUrl,
    logPath: lama.logPath,
    ...(lastError ? { lastError } : {})
  };
}

export function configureLamaEnvironment(paths: AppPaths): void {
  if (process.env.MANGA_TRANSLATOR_LAMA_COMMAND?.trim()) {
    return;
  }

  const lama = resolveLamaPaths(paths);
  process.env.MANGA_TRANSLATOR_LAMA_CODE_DIR ||= lama.codeDir;
  process.env.MANGA_TRANSLATOR_LAMA_WEIGHTS ||= lama.modelPath;

  if (!existsSync(lama.pythonPath)) {
    return;
  }

  process.env.MANGA_TRANSLATOR_LAMA_COMMAND = lama.pythonPath;
  process.env.MANGA_TRANSLATOR_LAMA_ARGS = JSON.stringify([
    lama.scriptPath,
    "--input",
    "{source}",
    "--mask",
    "{mask}",
    "--output",
    "{output}",
    "--weights",
    lama.modelPath
  ]);
}

export function startLamaRuntimePrepare(paths: AppPaths): LamaRuntimeStatus {
  const lama = resolveLamaPaths(paths);
  const python = resolvePython();
  if (!python) {
    lastError = `Python 3을 찾을 수 없습니다. 설치 명령: ${pythonInstallCommand()}`;
    return getLamaRuntimeStatus(paths);
  }
  if (prepareProcess) {
    return getLamaRuntimeStatus(paths);
  }
  if (!existsSync(lama.prepareScriptPath)) {
    lastError = `LaMa 준비 스크립트를 찾을 수 없습니다: ${lama.prepareScriptPath}`;
    return getLamaRuntimeStatus(paths);
  }

  mkdirSync(paths.logsDir, { recursive: true });
  const logFd = openSync(lama.logPath, "a");
  prepareProcess = spawn(process.execPath, [lama.prepareScriptPath, "--data-dir", paths.dataRoot], {
    env: { ...process.env, ELECTRON_RUN_AS_NODE: "1" },
    stdio: ["ignore", logFd, logFd],
    windowsHide: true
  });
  prepareProcess.on("exit", (code) => {
    closeSync(logFd);
    if (code === 0) {
      lastError = undefined;
      configureLamaEnvironment(paths);
    } else {
      lastError = `LaMa 환경 준비가 실패했습니다. 로그: ${lama.logPath}`;
    }
    prepareProcess = null;
  });
  prepareProcess.on("error", (error) => {
    closeSync(logFd);
    lastError = error.message;
    prepareProcess = null;
  });
  return getLamaRuntimeStatus(paths);
}

export function startLamaModelDownload(paths: AppPaths): LamaRuntimeStatus {
  const lama = resolveLamaPaths(paths);
  if (modelDownload) {
    return getLamaRuntimeStatus(paths);
  }
  if (existsSync(lama.modelPath)) {
    return getLamaRuntimeStatus(paths);
  }

  mkdirSync(lama.modelDir, { recursive: true });
  const tempPath = `${lama.modelPath}.download`;
  rmSync(tempPath, { force: true });
  modelDownload = download(modelUrl, tempPath)
    .then(async () => {
      await rename(tempPath, lama.modelPath);
      lastError = undefined;
    })
    .catch(async (error: unknown) => {
      await rm(tempPath, { force: true }).catch(() => undefined);
      lastError = error instanceof Error ? error.message : String(error);
    })
    .finally(() => {
      modelDownload = null;
    });
  return getLamaRuntimeStatus(paths);
}

function resolveLamaPaths(paths: AppPaths) {
  const assetRoot = paths.repoRoot.endsWith(".asar") ? paths.repoRoot.replace(/\.asar$/u, ".asar.unpacked") : paths.repoRoot;
  const toolsDir = join(paths.dataRoot, "tools");
  const modelDir = join(paths.dataRoot, "models", "lama-manga");
  return {
    pythonPath: process.platform === "win32"
      ? join(toolsDir, "lama-manga-venv", "Scripts", "python.exe")
      : join(toolsDir, "lama-manga-venv", "bin", "python"),
    codeDir: join(toolsDir, "Er0mangaInpaint"),
    codeReadyPath: join(toolsDir, "Er0mangaInpaint", "saicinpainting", "training", "modules", "ffc.py"),
    modelDir,
    modelPath: join(modelDir, "lama-manga.safetensors"),
    scriptPath: join(assetRoot, "scripts", "lama-inpaint.py"),
    prepareScriptPath: join(assetRoot, "scripts", "prepare-lama-runtime.cjs"),
    logPath: join(paths.logsDir, "lama-prepare.log")
  };
}

function resolvePython(): { command: string; args: string[] } | null {
  const explicit = process.env.PYTHON_BIN?.trim();
  const candidates = explicit
    ? [{ command: explicit, args: [] }]
    : process.platform === "win32"
      ? [
          { command: "py", args: ["-3.11"] },
          { command: "py", args: ["-3"] },
          { command: "python", args: [] }
        ]
      : [
          { command: "python3.11", args: [] },
          { command: "python3", args: [] },
          { command: "python", args: [] }
        ];

  return candidates.find((candidate) => {
    const result = spawnSync(candidate.command, [...candidate.args, "--version"], { stdio: "ignore", shell: false });
    return result.status === 0;
  }) ?? null;
}

function pythonInstallCommand(): string {
  if (process.platform === "darwin") {
    return "brew install python@3.11";
  }
  if (process.platform === "win32") {
    return "winget install Python.Python.3.11";
  }
  return "sudo apt-get update && sudo apt-get install -y python3.11 python3.11-venv";
}

function pythonInstallHelp(): string[] {
  if (process.platform === "darwin") {
    return [
      "Homebrew가 있으면 터미널에서 위 명령을 실행하세요.",
      "Homebrew가 없으면 https://www.python.org/downloads/macos/ 에서 Python 3.11 이상 macOS installer를 설치하세요.",
      "설치 후 터미널에서 `python3 --version`이 동작하는지 확인한 뒤 앱에서 새로고침을 누르세요."
    ];
  }
  if (process.platform === "win32") {
    return [
      "Windows 터미널에서 위 명령을 실행하세요.",
      "winget을 쓸 수 없으면 https://www.python.org/downloads/windows/ 에서 Python 3.11 이상 installer를 받고, 설치 중 Add python.exe to PATH를 켜세요.",
      "설치 후 새 터미널에서 `py -3.11 --version` 또는 `python --version`을 확인한 뒤 앱에서 새로고침을 누르세요."
    ];
  }
  return [
    "Debian/Ubuntu 계열은 위 명령을 실행하세요.",
    "다른 배포판은 패키지 매니저로 Python 3.11 이상과 venv 모듈을 설치하세요.",
    "설치 후 `python3 --version`이 동작하는지 확인한 뒤 앱에서 새로고침을 누르세요."
  ];
}

function download(url: string, outputPath: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const request = https.get(url, (response) => {
      if ([301, 302, 303, 307, 308].includes(response.statusCode || 0) && response.headers.location) {
        response.resume();
        download(response.headers.location, outputPath).then(resolve, reject);
        return;
      }
      if (response.statusCode !== 200) {
        response.resume();
        reject(new Error(`다운로드 실패 (${response.statusCode}): ${url}`));
        return;
      }
      const file = createWriteStream(outputPath);
      response.pipe(file);
      file.on("finish", () => file.close((error) => (error ? reject(error) : resolve())));
      file.on("error", reject);
    });
    request.on("error", reject);
  });
}
