import express from "express";
import multer from "multer";
import { mkdirSync } from "node:fs";
import { createServer } from "node:http";
import { join } from "node:path";
import { ensureWritableAppDirectories } from "./appPaths";
import { configureLamaEnvironment } from "./lamaRuntime";
import { cleanupLegacyLogs, getLibraryRoot } from "./library";
import { abortActiveJobForShutdown } from "./jobState";
import { getLogPath, logError, logInfo, resetAppLog } from "./logger";
import { createImportRoutes } from "./routes/importRoutes";
import { createInpaintRoutes } from "./routes/inpaintRoutes";
import { createJobRoutes } from "./routes/jobRoutes";
import { createFontPresetBackupRoutes } from "./routes/fontPresetBackupRoutes";
import { createLibraryRoutes } from "./routes/libraryRoutes";
import { createRenderRoutes } from "./routes/renderRoutes";
import { createRuntimeRoutes } from "./routes/runtimeRoutes";
import { createSettingsRoutes } from "./routes/settingsRoutes";

const appPaths = ensureWritableAppDirectories();
configureLamaEnvironment(appPaths);
const serverPort = Number(process.env.PORT || process.env.MANGA_TRANSLATOR_PORT || 3000);
const uploadDir = join(appPaths.dataRoot, "uploads");
mkdirSync(uploadDir, { recursive: true });
resetAppLog();

logInfo("Web server starting", {
  cwd: process.cwd(),
  logPath: getLogPath(),
  libraryPath: getLibraryRoot(),
  settingsPath: appPaths.settingsPath,
  dataRoot: appPaths.dataRoot,
  runtimeDir: appPaths.runtimeDir,
  port: serverPort
});

process.on("uncaughtException", (error) => {
  logError("Uncaught exception", error);
});

process.on("unhandledRejection", (reason) => {
  logError("Unhandled rejection", reason);
});

const app = express();
const upload = multer({ dest: uploadDir });

app.use(express.json({ limit: "120mb" }));
app.use((req, res, next) => {
  res.setHeader("Access-Control-Allow-Origin", req.headers.origin || "*");
  res.setHeader("Access-Control-Allow-Headers", "Content-Type");
  res.setHeader("Access-Control-Allow-Methods", "GET,POST,DELETE,OPTIONS");
  if (req.method === "OPTIONS") {
    res.sendStatus(204);
    return;
  }
  next();
});

app.get("/api/health", (_req, res) => {
  res.json({ ok: true });
});

app.use(createSettingsRoutes(appPaths));
app.use(createFontPresetBackupRoutes(appPaths));
app.use(createLibraryRoutes());
app.use(createRenderRoutes());
app.use(createInpaintRoutes(upload));
app.use(createRuntimeRoutes(appPaths));
app.use(createImportRoutes(upload));
app.use(createJobRoutes());

app.use(express.static(join(appPaths.repoRoot, "out", "client")));

app.use((error: unknown, _req: express.Request, res: express.Response, _next: express.NextFunction) => {
  const message = error instanceof Error ? error.message : String(error);
  logError("HTTP request failed", error);
  res.status(500).json({ error: message });
});

const httpServer = createServer(app);
httpServer.listen(serverPort, "127.0.0.1", async () => {
  await cleanupLegacyLogs();
  logInfo("Manga translator web app ready", { url: `http://127.0.0.1:${serverPort}` });
});

process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);

function shutdown(): void {
  abortActiveJobForShutdown();
  httpServer.close(() => process.exit(0));
}
