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
  // A slow CI runner (Windows) can take more than 10 s to show the first
  // screen (the item count): twice as long there, as each spec's open()
  // waits for it (failed twice on main after 0.6.0: edit-advice, then
  // auto-import, each passing elsewhere).
  expect: { timeout: process.env.CI ? 20_000 : 10_000 },
  // On CI a test that fails is run once more: the macOS runner fails a
  // different mocked test on each run (tree-place, then keyboard on 0.7.0's
  // PR, each passing on the other run); a real failure fails both times.
  retries: process.env.CI ? 1 : 0,
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
