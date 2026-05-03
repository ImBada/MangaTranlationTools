import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "tests/e2e",
  testMatch: "**/*.e2e.ts",
  fullyParallel: false,
  workers: 1,
  timeout: 120_000,
  expect: {
    timeout: 15_000
  },
  reporter: [["list"]],
  outputDir: ".tmp/playwright-results",
  use: {
    screenshot: "only-on-failure",
    trace: "retain-on-failure"
  }
});
