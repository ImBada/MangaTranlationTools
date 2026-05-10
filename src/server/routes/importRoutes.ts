import express from "express";
import multer from "multer";
import { randomUUID } from "node:crypto";
import { createImport, previewUploadedFiles, type UploadedImportFile } from "../library";
import {
  cleanupUnreferencedUploadedImportFiles,
  cleanupUploadedImportFiles,
  collectImportPreviewSourcePaths
} from "../importUploads";
import { getActiveJob, recordJobEvent, setActiveJob, emitJobEvent } from "../jobState";
import { logError } from "../logger";
import { asyncHandler, isAbortError } from "../serverUtils";
import type { CreateImportRequest, ImportPreviewResult, ImportSourceKind, JobEvent } from "../../shared/types";

export function createImportRoutes(upload: multer.Multer, uploadDir: string): express.Router {
  const router = express.Router();

  router.post("/api/import/preview/:kind", upload.array("files"), asyncHandler(async (req, res) => {
    const kind = req.params.kind as ImportSourceKind;
    const relativePaths = parseRelativePaths(req.body.relativePaths);
    const files = ((req.files as Express.Multer.File[] | undefined) ?? []).map((file, index): UploadedImportFile => ({
      path: file.path,
      name: file.originalname,
      relativePath: relativePaths[index] || file.originalname
    }));
    let preview: ImportPreviewResult | undefined;
    try {
      preview = await previewUploadedFiles(kind, files);
      res.json(preview.chapters.length ? preview : null);
    } finally {
      await cleanupUnreferencedUploadedImportFiles(uploadDir, files.map((file) => file.path), preview?.chapters.length ? preview : undefined);
    }
  }));

  router.post("/api/import/discard", asyncHandler(async (req, res) => {
    const preview = (req.body?.preview ?? req.body) as ImportPreviewResult | undefined;
    await cleanupUploadedImportFiles(uploadDir, collectImportPreviewSourcePaths(preview));
    res.sendStatus(204);
  }));

  router.post("/api/import/create", asyncHandler(async (req, res) => {
    if (getActiveJob()) {
      res.status(409).json({ error: "이미 실행 중인 작업이 있습니다." });
      return;
    }

    const request = req.body as CreateImportRequest;
    const id = randomUUID();
    const abortController = new AbortController();
    setActiveJob({ id, abortController });
    const emit = (event: JobEvent) => {
      recordJobEvent(id, event);
      emitJobEvent(event);
    };
    let shouldCleanupUploadedSources = false;

    try {
      const result = await createImport(request, { jobId: id, signal: abortController.signal, emit });
      shouldCleanupUploadedSources = true;
      res.json(result);
    } catch (error) {
      const lastEvent = getActiveJob()?.id === id ? getActiveJob()?.lastEvent : undefined;
      if (isAbortError(error) || abortController.signal.aborted) {
        shouldCleanupUploadedSources = true;
        emit({
          id,
          kind: "library-import",
          status: "cancelled",
          progressText: "가져오기가 취소되었습니다.",
          phase: "cancelled",
          progressCurrent: lastEvent?.progressCurrent,
          progressTotal: lastEvent?.progressTotal,
          pageIndex: lastEvent?.pageIndex,
          pageTotal: lastEvent?.pageTotal
        });
        res.status(499).json({ error: "가져오기가 취소되었습니다." });
        return;
      }

      const message = error instanceof Error ? error.message : String(error);
      logError("Import job failed", { jobId: id, request, lastEvent, error });
      emit({
        id,
        kind: "library-import",
        status: "failed",
        progressText: "가져오기 실패",
        phase: "failed",
        detail: message
      });
      res.status(500).json({ error: message });
    } finally {
      setActiveJob(null);
      if (shouldCleanupUploadedSources) {
        await cleanupUploadedImportFiles(uploadDir, collectImportPreviewSourcePaths(request.preview));
      }
    }
  }));

  return router;
}

function parseRelativePaths(value: unknown): string[] {
  if (typeof value !== "string") {
    return [];
  }
  try {
    const parsed = JSON.parse(value);
    return Array.isArray(parsed) ? parsed.map(String) : [];
  } catch {
    return [];
  }
}
