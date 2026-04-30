const http = require("node:http");
const net = require("node:net");
const path = require("node:path");
const { app, BrowserWindow, dialog, shell } = require("electron");

const preferredPort = Number(process.env.MANGA_TRANSLATOR_PORT || 3000);
let mainWindow = null;
let serverUrl = null;

app.setName("MangaTranslationTools");

const hasLock = !app.isPackaged || app.requestSingleInstanceLock();
if (!hasLock) {
  app.quit();
} else {
  app.on("second-instance", () => {
    if (!mainWindow) {
      return;
    }
    if (mainWindow.isMinimized()) {
      mainWindow.restore();
    }
    mainWindow.focus();
  });

  app.whenReady().then(startApp).catch((error) => {
    void showStartupError(error);
  });

  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0 && serverUrl) {
      createMainWindow(serverUrl);
    }
  });

  app.on("window-all-closed", () => {
    if (process.platform !== "darwin") {
      app.quit();
    }
  });
}

process.on("SIGINT", () => {
  app.exit(0);
});

process.on("SIGTERM", () => {
  app.exit(0);
});

async function startApp() {
  const port = process.env.MANGA_TRANSLATOR_PORT ? preferredPort : await findFreePort(preferredPort);
  const appRoot = process.env.MANGA_TRANSLATOR_APP_ROOT?.trim() || app.getAppPath();
  const dataRoot = process.env.MANGA_TRANSLATOR_DATA_DIR?.trim() || path.join(app.getPath("documents"), "MangaTranslationTools");
  serverUrl = `http://127.0.0.1:${port}`;

  process.env.MANGA_TRANSLATOR_PORT = String(port);
  process.env.MANGA_TRANSLATOR_APP_ROOT ||= appRoot;
  process.env.MANGA_TRANSLATOR_DATA_DIR ||= dataRoot;
  process.env.NODE_ENV ||= "production";

  require(path.join(appRoot, "out", "server", "index.js"));
  await waitForUrl(`${serverUrl}/api/health`, 30000);
  createMainWindow(serverUrl);
}

function createMainWindow(url) {
  mainWindow = new BrowserWindow({
    width: 1440,
    height: 960,
    minWidth: 1100,
    minHeight: 720,
    title: "MangaTranslationTools",
    backgroundColor: "#101214",
    webPreferences: {
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true
    }
  });

  mainWindow.webContents.setWindowOpenHandler(({ url: targetUrl }) => {
    void shell.openExternal(targetUrl);
    return { action: "deny" };
  });

  mainWindow.webContents.on("will-navigate", (event, targetUrl) => {
    if (!targetUrl.startsWith(url)) {
      event.preventDefault();
      void shell.openExternal(targetUrl);
    }
  });

  mainWindow.on("closed", () => {
    mainWindow = null;
  });

  void mainWindow.loadURL(url);
}

function findFreePort(port) {
  return new Promise((resolve, reject) => {
    const server = net.createServer();
    server.once("error", (error) => {
      if (error.code === "EADDRINUSE") {
        resolve(findFreePort(0));
        return;
      }
      reject(error);
    });
    server.listen(port, "127.0.0.1", () => {
      const address = server.address();
      server.close(() => {
        resolve(typeof address === "object" && address ? address.port : port);
      });
    });
  });
}

async function waitForUrl(url, timeoutMs) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    if (await canReach(url)) {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 300));
  }
  throw new Error(`Timed out waiting for ${url}`);
}

function canReach(url) {
  return new Promise((resolve) => {
    const request = http.get(url, (response) => {
      response.resume();
      resolve(response.statusCode >= 200 && response.statusCode < 500);
    });
    request.on("error", () => resolve(false));
    request.setTimeout(1000, () => {
      request.destroy();
      resolve(false);
    });
  });
}

async function showStartupError(error) {
  const message = error instanceof Error ? error.message : String(error);
  await dialog.showErrorBox("MangaTranslationTools 시작 실패", message);
  app.quit();
}
