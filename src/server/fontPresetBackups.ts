import { randomUUID } from "node:crypto";
import { existsSync } from "node:fs";
import { mkdir, readdir, unlink } from "node:fs/promises";
import { join } from "node:path";
import type {
  CreateFontPresetBackupRequest,
  FontPresetBackupSnapshot,
  FontPresetBackupSummary
} from "../shared/types";
import type { AppPaths } from "./appPaths";
import { readJsonFile, writeJsonFile } from "./libraryFileIO";

const BACKUP_FILE_EXTENSION = ".json";

export async function listFontPresetBackups(appPaths: AppPaths): Promise<FontPresetBackupSummary[]> {
  await ensureFontPresetBackupDir(appPaths);
  const filenames = await readdir(fontPresetBackupDir(appPaths));
  const backups = await Promise.all(
    filenames
      .filter((filename) => filename.endsWith(BACKUP_FILE_EXTENSION))
      .map((filename) => readFontPresetBackupFile(appPaths, filename.slice(0, -BACKUP_FILE_EXTENSION.length)).catch(() => null))
  );

  return backups
    .filter((backup): backup is FontPresetBackupSnapshot => Boolean(backup))
    .map(toFontPresetBackupSummary)
    .sort((a, b) => b.createdAt.localeCompare(a.createdAt));
}

export async function createFontPresetBackup(
  appPaths: AppPaths,
  request: CreateFontPresetBackupRequest
): Promise<FontPresetBackupSnapshot> {
  const now = new Date().toISOString();
  const backup: FontPresetBackupSnapshot = {
    id: randomUUID(),
    name: sanitizeBackupName(request.name),
    createdAt: now,
    fontPresets: Array.isArray(request.fontPresets) ? request.fontPresets : [],
    fontSizePresets: Array.isArray(request.fontSizePresets) ? request.fontSizePresets : [],
    fontPresetCount: Array.isArray(request.fontPresets) ? request.fontPresets.length : 0,
    fontSizePresetCount: Array.isArray(request.fontSizePresets) ? request.fontSizePresets.length : 0
  };

  await ensureFontPresetBackupDir(appPaths);
  await writeJsonFile(fontPresetBackupPath(appPaths, backup.id), backup);
  return backup;
}

export async function readFontPresetBackup(appPaths: AppPaths, backupId: string): Promise<FontPresetBackupSnapshot> {
  const backup = await readFontPresetBackupFile(appPaths, backupId);
  if (!backup) {
    throw new Error("폰트 프리셋 백업을 찾지 못했습니다.");
  }
  return backup;
}

export async function deleteFontPresetBackup(appPaths: AppPaths, backupId: string): Promise<FontPresetBackupSummary[]> {
  const path = fontPresetBackupPath(appPaths, backupId);
  if (!existsSync(path)) {
    throw new Error("폰트 프리셋 백업을 찾지 못했습니다.");
  }
  await unlink(path);
  return listFontPresetBackups(appPaths);
}

function toFontPresetBackupSummary(backup: FontPresetBackupSnapshot): FontPresetBackupSummary {
  return {
    id: backup.id,
    name: backup.name,
    createdAt: backup.createdAt,
    fontPresetCount: backup.fontPresets.length,
    fontSizePresetCount: backup.fontSizePresets.length
  };
}

async function readFontPresetBackupFile(appPaths: AppPaths, backupId: string): Promise<FontPresetBackupSnapshot | null> {
  assertBackupId(backupId);
  const path = fontPresetBackupPath(appPaths, backupId);
  if (!existsSync(path)) {
    return null;
  }
  const backup = await readJsonFile<FontPresetBackupSnapshot>(path);
  return {
    ...backup,
    fontPresets: Array.isArray(backup.fontPresets) ? backup.fontPresets : [],
    fontSizePresets: Array.isArray(backup.fontSizePresets) ? backup.fontSizePresets : [],
    fontPresetCount: Array.isArray(backup.fontPresets) ? backup.fontPresets.length : 0,
    fontSizePresetCount: Array.isArray(backup.fontSizePresets) ? backup.fontSizePresets.length : 0
  };
}

function sanitizeBackupName(name: string): string {
  const trimmed = name.trim();
  return trimmed || "폰트 프리셋 백업";
}

function assertBackupId(backupId: string): void {
  if (!/^[0-9a-f-]{36}$/iu.test(backupId)) {
    throw new Error("폰트 프리셋 백업 ID가 올바르지 않습니다.");
  }
}

async function ensureFontPresetBackupDir(appPaths: AppPaths): Promise<void> {
  await mkdir(fontPresetBackupDir(appPaths), { recursive: true });
}

function fontPresetBackupDir(appPaths: AppPaths): string {
  return join(appPaths.dataRoot, "font-preset-backups");
}

function fontPresetBackupPath(appPaths: AppPaths, backupId: string): string {
  assertBackupId(backupId);
  return join(fontPresetBackupDir(appPaths), `${backupId}${BACKUP_FILE_EXTENSION}`);
}
