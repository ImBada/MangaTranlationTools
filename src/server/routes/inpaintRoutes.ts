import express from "express";
import multer from "multer";
import { sanitizePsdFileBasename } from "../inpaintPsd";
import {
  exportInpaintPsdRequest,
  importInpaintPsdRequest,
  inpaintPage,
  readLastImportedPsd,
  readLastImportedPsdMeta,
  readOptionalPsdPageQuery,
  readPsdPageQuery,
  saveInpaintMaskRequest,
  saveInpaintResultLayerRequest
} from "../inpaintRequests";
import { asyncHandler } from "../serverUtils";
import type {
  ExportInpaintPsdRequest,
  InpaintPageRequest,
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

  router.post("/api/inpaint/psd/export", asyncHandler(async (req, res) => {
    const request = req.body as ExportInpaintPsdRequest;
    const buffer = await exportInpaintPsdRequest(request);
    const filename = `${sanitizePsdFileBasename(request.pageName || request.pageId, request.pageId || "inpaint")}-inpaint.psd`;
    res.setHeader("Content-Type", "image/vnd.adobe.photoshop");
    res.setHeader("Content-Disposition", `attachment; filename="inpaint.psd"; filename*=UTF-8''${encodeURIComponent(filename)}`);
    res.end(buffer);
  }));

  router.post("/api/inpaint/psd/import", upload.single("file"), asyncHandler(async (req, res) => {
    res.json(await importInpaintPsdRequest(req));
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

  router.get("/api/inpaint/psd/last-import/meta", asyncHandler(async (req, res) => {
    const pageQuery = readOptionalPsdPageQuery(req);
    if (!pageQuery) {
      res.json({ exists: false });
      return;
    }
    res.json(await readLastImportedPsdMeta(pageQuery.chapterId, pageQuery.pageId));
  }));

  return router;
}
