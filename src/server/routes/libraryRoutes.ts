import express from "express";
import {
  deleteChapter,
  deletePage,
  deleteWork,
  listLibrary,
  openChapter,
  patchChapterSnapshot,
  readPageImageAsset,
  renameChapter,
  renameWork,
  reorderChapters,
  reorderPages,
  saveChapterLastOpenedPage,
  saveChapterSnapshot
} from "../library";
import { isPageImageLayer } from "../inpaintRequests";
import { asyncHandler } from "../serverUtils";

export function createLibraryRoutes(): express.Router {
  const router = express.Router();

  router.get("/api/library", asyncHandler(async (_req, res) => {
    res.json(await listLibrary());
  }));

  router.get("/api/library/chapters/:chapterId", asyncHandler(async (req, res) => {
    res.json(await openChapter(String(req.params.chapterId)));
  }));

  router.get("/api/library/chapters/:chapterId/pages/:pageId/images/:layer", asyncHandler(async (req, res) => {
    const layer = String(req.params.layer);
    if (!isPageImageLayer(layer)) {
      res.status(404).json({ error: "요청한 이미지 레이어를 찾지 못했습니다." });
      return;
    }
    const asset = await readPageImageAsset(String(req.params.chapterId), String(req.params.pageId), layer);
    res.setHeader("Content-Type", asset.mime);
    res.setHeader("Cache-Control", "no-store");
    res.setHeader("Last-Modified", new Date(asset.updatedAt).toUTCString());
    res.end(asset.buffer);
  }));

  router.post("/api/library/chapters", asyncHandler(async (req, res) => {
    const body = req.body;
    const chapter = body?.chapter ?? body;
    const dirtyPageIds = Array.isArray(body?.dirtyPageIds) ? body.dirtyPageIds.filter((id: unknown) => typeof id === "string") : undefined;
    res.json(await saveChapterSnapshot(chapter, { dirtyPageIds }));
  }));

  router.post("/api/library/chapters/:chapterId/last-opened-page", asyncHandler(async (req, res) => {
    res.json(await saveChapterLastOpenedPage(String(req.params.chapterId), String(req.body?.pageId ?? "")));
  }));

  router.post("/api/library/chapters/:chapterId/patch", asyncHandler(async (req, res) => {
    res.json(await patchChapterSnapshot(String(req.params.chapterId), req.body));
  }));

  router.post("/api/library/works/:workId/rename", asyncHandler(async (req, res) => {
    res.json(await renameWork(String(req.params.workId), String(req.body.title ?? "")));
  }));

  router.post("/api/library/chapters/:chapterId/rename", asyncHandler(async (req, res) => {
    res.json(await renameChapter(String(req.params.chapterId), String(req.body.title ?? "")));
  }));

  router.delete("/api/library/works/:workId", asyncHandler(async (req, res) => {
    res.json(await deleteWork(String(req.params.workId)));
  }));

  router.delete("/api/library/chapters/:chapterId", asyncHandler(async (req, res) => {
    res.json(await deleteChapter(String(req.params.chapterId)));
  }));

  router.post("/api/library/works/:workId/reorder-chapters", asyncHandler(async (req, res) => {
    res.json(await reorderChapters(String(req.params.workId), req.body.chapterIds));
  }));

  router.post("/api/library/chapters/:chapterId/reorder-pages", asyncHandler(async (req, res) => {
    res.json(await reorderPages(String(req.params.chapterId), req.body.pageIds));
  }));

  router.delete("/api/library/chapters/:chapterId/pages/:pageId", asyncHandler(async (req, res) => {
    res.json(await deletePage(String(req.params.chapterId), String(req.params.pageId)));
  }));

  return router;
}
