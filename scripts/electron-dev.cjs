const { spawn } = require("node:child_process");
const { join } = require("node:path");

const root = join(__dirname, "..");
const electronBin = require("electron");
const env = {
  ...process.env,
  MANGA_TRANSLATOR_APP_ROOT: root
};
delete env.ELECTRON_RUN_AS_NODE;

const child = spawn(electronBin, [join(root, "electron")], {
  cwd: root,
  env,
  stdio: "inherit",
  shell: false
});

child.on("exit", (code, signal) => {
  if (signal) {
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 0);
});
