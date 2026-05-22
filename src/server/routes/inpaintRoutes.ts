import express from "express";
import multer from "multer";
import {
  exportInpaintPsdRequest,
  importInpaintPsdRequest,
  inpaintPage,
  quickImportLastExportedInpaintPsdRequest,
  readInpaintPsdMeta,
  readLastImportedPsd,
  readOptionalPsdPageQuery,
  readPsdPageQuery,
  saveInpaintLayersRequest,
  saveInpaintMaskRequest,
  saveInpaintResultLayerRequest
} from "../inpaintRequests";
import { asyncHandler } from "../serverUtils";
import type {
  ExportInpaintPsdRequest,
  InpaintPageRequest,
  SaveInpaintLayersRequest,
  SaveInpaintMaskRequest,
  SaveInpaintResultLayerRequest
} from "../../shared/types";

export function createInpaintRoutes(upload: multer.Multer): express.Router {
  const router = express.Router();

  router.post("/api/inpaint/page", asyncHandler(async (req, res) => {
    res.json(await inpaintPage(req.body as InpaintPageRequest));
  }));

  router.post("/api/inpaint/mask", asyncHandler(async (req, res) => {
    res.json(await saveInpaintMaskRequest(req.body as SaveInpaintMaskRequest));
  }));

  router.post("/api/inpaint/result-layer", asyncHandler(async (req, res) => {
    res.json(await saveInpaintResultLayerRequest(req.body as SaveInpaintResultLayerRequest));
  }));

  router.post("/api/inpaint/layers", asyncHandler(async (req, res) => {
    res.json(await saveInpaintLayersRequest(req.body as SaveInpaintLayersRequest));
  }));

  router.post("/api/inpaint/psd/export", asyncHandler(async (req, res) => {
    const request = req.body as ExportInpaintPsdRequest;
    res.json(await exportInpaintPsdRequest(request));
  }));

  router.post("/api/inpaint/psd/import", upload.single("file"), asyncHandler(async (req, res) => {
    res.json(await importInpaintPsdRequest(req));
  }));

  router.post("/api/inpaint/psd/quick-load", asyncHandler(async (req, res) => {
    const chapterId = typeof req.body?.chapterId === "string" ? req.body.chapterId : "";
    const pageId = typeof req.body?.pageId === "string" ? req.body.pageId : "";
    res.json(await quickImportLastExportedInpaintPsdRequest(chapterId, pageId));
  }));

  router.get("/api/inpaint/psd/last-import", asyncHandler(async (req, res) => {
    const { chapterId, pageId } = readPsdPageQuery(req);
    const buffer = await readLastImportedPsd(chapterId, pageId);
    if (!buffer) {
      res.status(404).json({ error: "현재 페이지에서 마지막으로 가져온 PSD 파일이 없습니다." });
      return;
    }
    res.setHeader("Content-Type", "image/vnd.adobe.photoshop");
    res.setHeader("Content-Disposition", "attachment; filename=\"last-imported-inpaint.psd\"");
    res.end(buffer);
  }));

  const sendInpaintPsdMeta = asyncHandler(async (req, res) => {
    const pageQuery = readOptionalPsdPageQuery(req);
    if (!pageQuery) {
      res.json({ exists: false });
      return;
    }
    res.json(await readInpaintPsdMeta(pageQuery.chapterId, pageQuery.pageId));
  });

  router.get("/api/inpaint/psd/meta", sendInpaintPsdMeta);
  router.get("/api/inpaint/psd/last-import/meta", sendInpaintPsdMeta);

  return router;
}
