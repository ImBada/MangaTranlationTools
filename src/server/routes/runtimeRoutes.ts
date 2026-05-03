import express from "express";
import type { AppPaths } from "../appPaths";
import { getLamaRuntimeStatus, startLamaModelDownload, startLamaRuntimePrepare } from "../lamaRuntime";
import { asyncHandler } from "../serverUtils";

export function createRuntimeRoutes(appPaths: AppPaths): express.Router {
  const router = express.Router();

  router.get("/api/lama/status", asyncHandler(async (_req, res) => {
    res.json(getLamaRuntimeStatus(appPaths));
  }));

  router.post("/api/lama/prepare", asyncHandler(async (_req, res) => {
    res.json(startLamaRuntimePrepare(appPaths));
  }));

  router.post("/api/lama/model/download", asyncHandler(async (_req, res) => {
    res.json(startLamaModelDownload(appPaths));
  }));

  return router;
}
