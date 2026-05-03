import express from "express";
import type { AppPaths } from "../appPaths";
import { getAppSettings, resetAppSettings, saveAppSettings } from "../settingsStore";
import { listSystemFonts } from "../systemFonts";
import { getUpdateStatus } from "../updateCheck";
import { writeLog } from "../logger";
import { testModelSettings } from "../modelTest";
import { asyncHandler } from "../serverUtils";

export function createSettingsRoutes(appPaths: AppPaths): express.Router {
  const router = express.Router();

  router.get("/api/update/status", asyncHandler(async (req, res) => {
    res.json(await getUpdateStatus(appPaths, { refresh: req.query.refresh === "1" }));
  }));

  router.post("/api/logs/write", (req, res) => {
    const { level, message, detail } = req.body as { level: "debug" | "info" | "warn" | "error"; message: string; detail?: unknown };
    writeLog(level, `client: ${message}`, detail);
    res.json({ logged: true });
  });

  router.get("/api/settings", asyncHandler(async (_req, res) => {
    res.json(await getAppSettings());
  }));

  router.get("/api/fonts", asyncHandler(async (_req, res) => {
    res.json(await listSystemFonts());
  }));

  router.post("/api/settings", asyncHandler(async (req, res) => {
    res.json(await saveAppSettings(req.body));
  }));

  router.post("/api/settings/reset", asyncHandler(async (_req, res) => {
    res.json(await resetAppSettings());
  }));

  router.post("/api/settings/test-model", asyncHandler(async (req, res) => {
    res.json(await testModelSettings(req.body, appPaths));
  }));

  return router;
}
