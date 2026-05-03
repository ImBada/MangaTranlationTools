import express from "express";
import { saveRenderedPage } from "../library";
import { asyncHandler } from "../serverUtils";
import type { RenderPageRequest } from "../../shared/types";

export function createRenderRoutes(): express.Router {
  const router = express.Router();

  router.post("/api/render/page", asyncHandler(async (req, res) => {
    const request = req.body as RenderPageRequest;
    res.json(await saveRenderedPage(request.chapterId, request.pageId, request.dataUrl));
  }));

  return router;
}
