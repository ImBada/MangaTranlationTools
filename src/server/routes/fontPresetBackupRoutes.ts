import express from "express";
import type { AppPaths } from "../appPaths";
import { createFontPresetBackup, deleteFontPresetBackup, listFontPresetBackups, readFontPresetBackup } from "../fontPresetBackups";
import { asyncHandler } from "../serverUtils";

export function createFontPresetBackupRoutes(appPaths: AppPaths): express.Router {
  const router = express.Router();

  router.get("/api/font-preset-backups", asyncHandler(async (_req, res) => {
    res.json(await listFontPresetBackups(appPaths));
  }));

  router.post("/api/font-preset-backups", asyncHandler(async (req, res) => {
    res.json(await createFontPresetBackup(appPaths, req.body));
  }));

  router.get("/api/font-preset-backups/:backupId", asyncHandler(async (req, res) => {
    res.json(await readFontPresetBackup(appPaths, String(req.params.backupId)));
  }));

  router.delete("/api/font-preset-backups/:backupId", asyncHandler(async (req, res) => {
    res.json(await deleteFontPresetBackup(appPaths, String(req.params.backupId)));
  }));

  return router;
}
