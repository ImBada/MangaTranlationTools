const http = require("node:http");
const { join } = require("node:path");
const { spawn, spawnSync } = require("node:child_process");
const { prepareRuntimeAssets } = require("./prepare-runtime.cjs");

const root = join(__dirname, "..");
const backendUrl = "http://127.0.0.1:3000/api/health";
const rendererUrl = "http://127.0.0.1:5173";
const children = [];

function runSync(command, args) {
  const result = spawnSync(command, args, { cwd: root, stdio: "inherit", shell: false });
  if (result.status !== 0) {
    process.exit(result.status ?? 1);
  }
}

function spawnChild(command, args, env = {}) {
  const child = spawn(command, args, {
    cwd: root,
    stdio: "inherit",
    shell: false,
    env: { ...process.env, ...env }
  });
  children.push(child);
  child.on("exit", () => {
    for (const other of children) {
      if (other !== child && other.exitCode === null && other.signalCode === null) {
        other.kill();
      }
    }
  });
  return child;
}

async function waitForUrl(url, timeoutMs = 30000) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    if (await canReach(url)) {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 400));
  }
  throw new Error(`Timed out waiting for ${url}`);
}

function canReach(url) {
  return new Promise((resolve) => {
    const req = http.get(url, (res) => {
      res.resume();
      resolve(res.statusCode >= 200 && res.statusCode < 500);
    });
    req.on("error", () => resolve(false));
    req.setTimeout(1000, () => {
      req.destroy();
      resolve(false);
    });
  });
}

function nodeBin(packageName, ...parts) {
  return join(root, "node_modules", packageName, ...parts);
}

process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);

function shutdown() {
  for (const child of children) {
    if (child.exitCode === null && child.signalCode === null) {
      child.kill();
    }
  }
  process.exit(0);
}

(async () => {
  prepareRuntimeAssets({ root, outputDir: join(root, "out", "app-runtime") });
  runSync(process.execPath, [nodeBin("typescript", "bin", "tsc"), "-p", "tsconfig.server.json"]);
  spawnChild(process.execPath, ["out/main/index.js"]);
  await waitForUrl(backendUrl);
  spawnChild(process.execPath, [nodeBin("vite", "bin", "vite.js"), "--config", "vite.renderer.config.ts", "--host", "127.0.0.1"]);
  await waitForUrl(rendererUrl);
  console.log(`\nWeb app: ${rendererUrl}\nAPI:     http://127.0.0.1:3000\n`);
})().catch((error) => {
  console.error(error);
  shutdown();
});
