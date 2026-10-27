// Logs, crash reports and the diagnostic report in the real app (Windows).

import { expect, test } from "@playwright/test";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { spawnSync } from "node:child_process";

import { App, exe, startVite, stopVite, World } from "./fixture";

test.skip(
  process.platform !== "win32",
  "WebView2 automation is Windows-only (macOS runs the mocked suite)",
);
test.skip(
  !fs.existsSync(exe("keepsake-app")),
  "build first: cargo build -p keepsake-app -p keepsake-cli",
);

test.beforeAll(async () => {
  await startVite();
});
test.afterAll(() => stopVite());

test("a crash is reported locally at the next start; the report has real facts", async () => {
  const w = new World();
  w.cli(["init", w.p("lib")]);
  w.cacheHere();
  const logs = w.env.KEEPSAKE_LOG_DIR ?? "";
  // A real crash, in the CLI, written to the same log folder.
  const crash = spawnSync(exe("keepsake"), ["dev", "crash"], { env: w.env });
  expect(crash.status).not.toBe(0);
  expect(fs.readdirSync(logs).some((f) => f.startsWith("crash-"))).toBe(true);
  const app = new App(w);
  try {
    await app.connect();
    const page = await app.page("main");
    await expect(page.getByTestId("count")).toHaveText("0 items");
    // This run's log file exists and says it started.
    await expect
      .poll(() =>
        fs
          .readdirSync(logs)
          .filter((f) => f.startsWith("keepsake-") && f.endsWith(".log"))
          .map((f) => fs.readFileSync(path.join(logs, f), "utf8"))
          .join("\n"),
      )
      .toContain("Keepsake started");

    const banner = page.getByTestId("crash-banner");
    await expect(banner).toContainText("Keepsake closed unexpectedly last time.");
    const report: string = await page.evaluate(() =>
      // The same command the Copy button uses.
      (window as unknown as { __TAURI_INTERNALS__: { invoke: (c: string) => Promise<string> } })
        .__TAURI_INTERNALS__.invoke("diagnostic_report"),
    );
    expect(report).toContain("Program: keepsake-app");
    expect(report).toMatch(/exiftool\s+\d+\.\d+/);
    expect(report).toMatch(/ffprobe\s+ffprobe version/);
    expect(report).toContain("dcraw emulator");
    expect(report).toContain("schema ");
    expect(report).toContain("crash requested with `keepsake dev crash`");
    expect(report).toContain("Keepsake started");
    // The home folder is shown as ~ (the temp folder is under it).
    const home = os.homedir();
    expect(report.toLowerCase()).not.toContain(home.toLowerCase());

    await banner.getByRole("button", { name: "Dismiss" }).click();
    await expect(banner).toHaveCount(0);
    expect(fs.readdirSync(logs).some((f) => f.endsWith(".seen.txt"))).toBe(true);
    // Not shown again.
    await page.reload();
    await expect(page.getByTestId("count")).toHaveText("0 items");
    await page.waitForTimeout(500);
    await expect(page.getByTestId("crash-banner")).toHaveCount(0);
  } finally {
    await app.close();
    w.cleanup();
  }
});
