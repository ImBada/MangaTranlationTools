const { join } = require("node:path");
const { spawn } = require("node:child_process");
const { prepareRuntimeAssets } = require("./prepare-runtime.cjs");

const root = join(__dirname, "..");
prepareRuntimeAssets({ root, outputDir: join(root, "out", "app-runtime") });

const child = spawn(process.execPath, [join(root, "out", "server", "index.js")], {
  cwd: root,
  stdio: "inherit",
  shell: false
});

child.on("exit", (code) => {
  process.exit(code ?? 0);
});
