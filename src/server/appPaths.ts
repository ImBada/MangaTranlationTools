import { mkdirSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";

export type AppPaths = {
  repoRoot: string;
  executableDir: string;
  dataRoot: string;
  settingsPath: string;
  libraryDir: string;
  logsDir: string;
  logFile: string;
  runtimeDir: string;
  toolsDir: string;
  hfHomeDir?: string;
  hfHubCacheDir?: string;
};

export function getAppPaths(): AppPaths {
  const repoRoot = process.env.MANGA_TRANSLATOR_APP_ROOT?.trim()
    ? resolve(process.env.MANGA_TRANSLATOR_APP_ROOT)
    : resolve(__dirname, "../..");
  const executableDir = dirname(process.execPath);
  const dataRoot = process.env.MANGA_TRANSLATOR_DATA_DIR?.trim() || defaultDataRoot();
  const libraryDir = join(dataRoot, "library");
  const logsDir = join(dataRoot, "logs");
  const runtimeDir = join(repoRoot, "out", "app-runtime");
  const toolsDir = join(repoRoot, "tools");
  const explicitHfHome = process.env.MANGA_TRANSLATOR_HF_HOME?.trim();
  const explicitHubCache = process.env.HF_HUB_CACHE?.trim() || process.env.HUGGINGFACE_HUB_CACHE?.trim();
  const hfHomeDir = explicitHfHome || undefined;
  const hfHubCacheDir = explicitHubCache || undefined;

  return {
    repoRoot,
    executableDir,
    dataRoot,
    settingsPath: join(dataRoot, "settings.json"),
    libraryDir,
    logsDir,
    logFile: join(logsDir, "app.log"),
    runtimeDir,
    toolsDir,
    hfHomeDir,
    hfHubCacheDir
  };
}

function defaultDataRoot(): string {
  return join(homedir(), "Documents", "MangaTranslationTools");
}

export function ensureWritableAppDirectories(): AppPaths {
  const paths = getAppPaths();
  mkdirSync(paths.libraryDir, { recursive: true });
  mkdirSync(paths.logsDir, { recursive: true });
  if (paths.hfHomeDir) {
    mkdirSync(paths.hfHomeDir, { recursive: true });
  }
  if (paths.hfHubCacheDir) {
    mkdirSync(paths.hfHubCacheDir, { recursive: true });
  }
  return paths;
}
