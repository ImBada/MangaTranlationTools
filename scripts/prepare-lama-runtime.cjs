const { createWriteStream, existsSync, mkdirSync, rmSync, writeFileSync } = require("node:fs");
const { mkdtemp, readdir, rename, rm } = require("node:fs/promises");
const https = require("node:https");
const os = require("node:os");
const { basename, join } = require("node:path");
const { spawnSync } = require("node:child_process");
const AdmZip = require("adm-zip");

const runtimeVersion = 1;
const eromangaZipUrl = "https://github.com/Er0manga/Er0mangaInpaint/archive/refs/heads/main.zip";
const modelUrl = "https://huggingface.co/mayocream/lama-manga/resolve/main/lama-manga.safetensors";
const pythonPackages = [
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
  "webdataset"
];

async function main() {
  const options = parseArgs(process.argv.slice(2));
  const dataDir = options.dataDir || process.env.MANGA_TRANSLATOR_DATA_DIR || process.cwd();
  const toolsDir = join(dataDir, "tools");
  const venvDir = join(toolsDir, "lama-manga-venv");
  const codeDir = join(toolsDir, "Er0mangaInpaint");
  const modelDir = join(dataDir, "models", "lama-manga");
  const modelPath = join(modelDir, "lama-manga.safetensors");
  const markerPath = join(toolsDir, ".lama-runtime-ready.json");

  mkdirSync(toolsDir, { recursive: true });
  mkdirSync(modelDir, { recursive: true });
  writeModelReadme(modelDir, modelPath);

  const pythonPath = await ensureVenv(venvDir);
  await ensurePythonPackages(pythonPath, markerPath);
  await ensureEromangaCode(codeDir);

  const result = {
    ok: true,
    runtimeVersion,
    pythonPath,
    codeDir,
    modelPath,
    modelExists: existsSync(modelPath)
  };
  writeFileSync(markerPath, JSON.stringify(result, null, 2));
  console.log(JSON.stringify(result));
}

async function ensureVenv(venvDir) {
  const pythonPath = venvPythonPath(venvDir);
  if (existsSync(pythonPath)) {
    return pythonPath;
  }
  mkdirSync(venvDir, { recursive: true });
  const python = resolvePython();
  run(python.command, [...python.args, "-m", "venv", venvDir], "Python 가상환경 생성에 실패했습니다.");
  return pythonPath;
}

async function ensurePythonPackages(pythonPath, markerPath) {
  const marker = readMarker(markerPath);
  if (marker?.runtimeVersion === runtimeVersion && existsSync(marker.pythonPath || "")) {
    return;
  }
  run(pythonPath, ["-m", "pip", "install", "--upgrade", "pip", "setuptools", "wheel"], "pip 업그레이드에 실패했습니다.");
  run(pythonPath, ["-m", "pip", "install", ...pythonPackages], "LaMa Python 패키지 설치에 실패했습니다.");
}

async function ensureEromangaCode(codeDir) {
  if (existsSync(join(codeDir, "saicinpainting", "training", "modules", "ffc.py"))) {
    return;
  }
  rmSync(codeDir, { recursive: true, force: true });
  const tempDir = await mkdtemp(join(os.tmpdir(), "manga-lama-code-"));
  const zipPath = join(tempDir, "eromanga.zip");
  try {
    await download(eromangaZipUrl, zipPath);
    new AdmZip(zipPath).extractAllTo(tempDir, true);
    const extractedRoot = (await readdir(tempDir, { withFileTypes: true }))
      .find((entry) => entry.isDirectory() && entry.name.startsWith("Er0mangaInpaint-"));
    if (!extractedRoot) {
      throw new Error("Er0mangaInpaint 압축 파일 구조를 확인할 수 없습니다.");
    }
    mkdirSync(join(codeDir, ".."), { recursive: true });
    await rename(join(tempDir, extractedRoot.name), codeDir);
  } finally {
    await rm(tempDir, { recursive: true, force: true });
  }
}

function resolvePython() {
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

  for (const candidate of candidates) {
    const result = spawnSync(candidate.command, [...candidate.args, "--version"], { encoding: "utf8", shell: false });
    if (result.status === 0) {
      return candidate;
    }
  }
  throw new Error("Python 3을 찾을 수 없습니다. Python 3.11 설치 후 다시 실행하세요.");
}

function venvPythonPath(venvDir) {
  return process.platform === "win32" ? join(venvDir, "Scripts", "python.exe") : join(venvDir, "bin", "python");
}

function readMarker(markerPath) {
  if (!existsSync(markerPath)) {
    return null;
  }
  try {
    return require(markerPath);
  } catch {
    return null;
  }
}

function writeModelReadme(modelDir, modelPath) {
  writeFileSync(
    join(modelDir, "README.txt"),
    [
      "Download lama-manga.safetensors from:",
      modelUrl,
      "",
      "Save it as:",
      modelPath,
      ""
    ].join(os.EOL)
  );
}

function download(url, outputPath) {
  return new Promise((resolve, reject) => {
    const request = https.get(url, (response) => {
      if ([301, 302, 303, 307, 308].includes(response.statusCode || 0) && response.headers.location) {
        response.resume();
        download(response.headers.location, outputPath).then(resolve, reject);
        return;
      }
      if (response.statusCode !== 200) {
        response.resume();
        reject(new Error(`Download failed (${response.statusCode}) for ${url}`));
        return;
      }
      const file = createWriteStream(outputPath);
      response.pipe(file);
      file.on("finish", () => {
        file.close(resolve);
      });
      file.on("error", reject);
    });
    request.on("error", reject);
  });
}

function run(command, args, errorMessage) {
  console.log(`> ${basename(command)} ${args.join(" ")}`);
  const result = spawnSync(command, args, { stdio: "inherit", shell: false });
  if (result.status !== 0) {
    throw new Error(`${errorMessage} (${command} ${args.join(" ")})`);
  }
}

function parseArgs(args) {
  const options = {};
  for (let index = 0; index < args.length; index += 1) {
    if (args[index] === "--data-dir") {
      options.dataDir = args[index + 1];
      index += 1;
    }
  }
  return options;
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exit(1);
});
