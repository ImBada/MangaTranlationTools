import express from "express";
import { startAnalysis } from "../analysisJob";
import { addEventClient, cancelActiveJob, removeEventClient } from "../jobState";
import { asyncHandler } from "../serverUtils";
import type { StartAnalysisRequest } from "../../shared/types";

export function createJobRoutes(): express.Router {
  const router = express.Router();

  router.post("/api/jobs/start-analysis", asyncHandler(async (req, res) => {
    res.json(await startAnalysis(req.body as StartAnalysisRequest));
  }));

  router.post("/api/jobs/cancel", asyncHandler(async (_req, res) => {
    res.json({ cancelled: await cancelActiveJob() });
  }));

  router.get("/api/jobs/events", (req, res) => {
    res.writeHead(200, {
      "Content-Type": "text/event-stream",
      "Cache-Control": "no-cache, no-transform",
      Connection: "keep-alive"
    });
    res.write("\n");
    addEventClient(res);
    req.on("close", () => {
      removeEventClient(res);
    });
  });

  return router;
}
