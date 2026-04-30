import { readFileSync } from "node:fs";
import https from "node:https";
import { join } from "node:path";
import type { AppPaths } from "./appPaths";
import type { UpdateStatus } from "../shared/types";

const latestReleaseUrl = "https://api.github.com/repos/ImBada/MangaTranlationTools/releases/latest";
const cacheTtlMs = 30 * 60 * 1000;

let cachedStatus: UpdateStatus | null = null;

type GitHubRelease = {
  tag_name?: unknown;
  name?: unknown;
  html_url?: unknown;
};

export async function getUpdateStatus(paths: AppPaths, options: { refresh?: boolean } = {}): Promise<UpdateStatus> {
  if (!options.refresh && cachedStatus && Date.now() - Date.parse(cachedStatus.checkedAt) < cacheTtlMs) {
    return cachedStatus;
  }

  const currentVersion = readCurrentVersion(paths);
  const checkedAt = new Date().toISOString();

  try {
    const release = await fetchJson<GitHubRelease>(latestReleaseUrl);
    const latestVersion = typeof release.tag_name === "string" ? normalizeVersion(release.tag_name) : null;
    const releaseUrl = typeof release.html_url === "string" ? release.html_url : null;
    const releaseName = typeof release.name === "string" ? release.name : null;
    cachedStatus = {
      currentVersion,
      latestVersion,
      updateAvailable: latestVersion ? compareVersions(latestVersion, currentVersion) > 0 : false,
      checkedAt,
      releaseUrl,
      releaseName
    };
  } catch (error) {
    if (error instanceof HttpStatusError && error.statusCode === 404) {
      cachedStatus = {
        currentVersion,
        latestVersion: null,
        updateAvailable: false,
        checkedAt,
        releaseUrl: null,
        releaseName: null
      };
      return cachedStatus;
    }
    cachedStatus = {
      currentVersion,
      latestVersion: null,
      updateAvailable: false,
      checkedAt,
      releaseUrl: null,
      releaseName: null,
      error: error instanceof Error ? error.message : String(error)
    };
  }

  return cachedStatus;
}

class HttpStatusError extends Error {
  constructor(public readonly statusCode: number | undefined) {
    super(`업데이트 확인 실패 (${statusCode ?? "unknown"})`);
  }
}

function readCurrentVersion(paths: AppPaths): string {
  try {
    const packageJson = JSON.parse(readFileSync(join(paths.repoRoot, "package.json"), "utf8")) as { version?: unknown };
    return typeof packageJson.version === "string" ? packageJson.version : "0.0.0";
  } catch {
    return "0.0.0";
  }
}

function normalizeVersion(version: string): string {
  return version.trim().replace(/^v/iu, "");
}

function compareVersions(left: string, right: string): number {
  const leftParts = normalizeVersion(left).split(/[.-]/u).map((part) => Number.parseInt(part, 10) || 0);
  const rightParts = normalizeVersion(right).split(/[.-]/u).map((part) => Number.parseInt(part, 10) || 0);
  const length = Math.max(leftParts.length, rightParts.length, 3);
  for (let index = 0; index < length; index += 1) {
    const delta = (leftParts[index] ?? 0) - (rightParts[index] ?? 0);
    if (delta !== 0) {
      return delta;
    }
  }
  return 0;
}

function fetchJson<T>(url: string): Promise<T> {
  return new Promise((resolve, reject) => {
    const request = https.get(
      url,
      {
        headers: {
          Accept: "application/vnd.github+json",
          "User-Agent": "MangaTranslationTools"
        }
      },
      (response) => {
        let body = "";
        response.setEncoding("utf8");
        response.on("data", (chunk) => {
          body += chunk;
        });
        response.on("end", () => {
          if (response.statusCode !== 200) {
            reject(new HttpStatusError(response.statusCode));
            return;
          }
          try {
            resolve(JSON.parse(body) as T);
          } catch (error) {
            reject(error);
          }
        });
      }
    );
    request.on("error", reject);
    request.setTimeout(5000, () => {
      request.destroy(new Error("업데이트 확인 시간이 초과되었습니다."));
    });
  });
}
