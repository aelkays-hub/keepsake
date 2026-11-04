// UI tests.
//
// - "mocked": the UI in Chromium against the in-memory backend
//   (src/mock/backend.ts), served by Vite with VITE_KEEPSAKE_MOCK=1. Runs on
//   Windows and macOS (WKWebView cannot be automated, so this is the macOS
//   coverage).
// - "app": the real Tauri app on Windows. WebView2 is Chromium, so
//   Playwright connects over its debugging port; see e2e/app/fixture.ts.

import { defineConfig } from "@playwright/test";

const app = process.argv.some((a) => a.includes("--project=app"));

export default defineConfig({
  testDir: "e2e",
  timeout: 90_000,
  expect: { timeout: 10_000 },
  fullyParallel: false,
  workers: 1,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  use: { trace: "retain-on-failure" },
  projects: [
    {
      name: "mocked",
      testDir: "e2e/mocked",
      use: {
        browserName: "chromium",
        baseURL: "http://localhost:1421",
        viewport: { width: 1280, height: 800 },
      },
    },
    {
      name: "app",
      testDir: "e2e/app",
      timeout: 240_000,
    },
  ],
  webServer: app
    ? undefined
    : {
        command: "pnpm exec vite --port 1421 --strictPort",
        env: { VITE_KEEPSAKE_MOCK: "1" },
        url: "http://localhost:1421",
        reuseExistingServer: !process.env.CI,
        timeout: 60_000,
      },
});
