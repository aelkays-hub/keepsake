// Zoom speed in the real app, on a 6000 x 4000 JPEG (a camera-size picture):
//   - the full size is ready in the background soon after the photo opens
//     (an upright JPEG is copied, not decoded and encoded again);
//   - each zoom slider step reaches the screen within a few frames.
//
// CI uses the debug app and KEEPSAKE_SCALE_SLACK=2; see perf.spec.ts for
// running against the release app.

import { expect, test } from "@playwright/test";
import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";

import { App, exe, startVite, stopVite, vendor, World } from "./fixture";

const release = process.env.KEEPSAKE_APP_PROFILE === "release";
const SLACK = Number(process.env.KEEPSAKE_SCALE_SLACK ?? (release ? 1 : 2));
const ffmpeg = path.join(vendor, process.platform === "win32" ? "ffmpeg.exe" : "ffmpeg");

/** From opening the photo to its full size on screen, with no zoom asked for. */
const FULL_READY_MS = 3000;
/** From a slider step to the frame that shows it (90th percentile). */
const STEP_P90_MS = 50;

test.skip(process.platform !== "win32", "WebView2 automation is Windows-only");
test.skip(
  !fs.existsSync(exe("keepsake-app", release ? "release" : "debug")),
  "build the app first",
);
test.skip(!fs.existsSync(ffmpeg), "needs vendor/ffmpeg (run the setup script)");

test.beforeAll(async () => {
  if (!release) await startVite();
});
test.afterAll(() => {
  if (!release) stopVite();
});

test("zoom on a 6000 x 4000 JPEG: full size soon, and slider steps within a few frames", async () => {
  test.setTimeout(300_000);
  const w = new World();
  w.cli(["init", w.p("lib")]);
  w.cacheHere();
  const dir = w.p("Big");
  fs.mkdirSync(dir, { recursive: true });
  execFileSync(ffmpeg, [
    "-y",
    "-v",
    "error",
    "-f",
    "lavfi",
    "-i",
    "testsrc2=size=6000x4000",
    "-frames:v",
    "1",
    "-q:v",
    "2",
    path.join(dir, "BIG.JPG"),
  ]);
  w.cli(["add-folder", dir, "--no-probe"]);
  w.cli(["renders", "run", "--ignore-battery"]);
  const app = new App(w);
  try {
    await app.connect();
    const page = await app.page("main");
    await expect(page.getByTestId("count")).toHaveText("1 item");
    await page.getByTestId("tile").first().dblclick();
    const t0 = Date.now();
    const img = page.getByTestId("viewer-image");
    await expect(img).toHaveAttribute("data-full", "1", { timeout: 60_000 });
    const ready = Date.now() - t0;
    await expect(page.getByTestId("viewer-stage")).toHaveAttribute("data-zoom", "fit");

    const steps = await page.evaluate(async () => {
      const s = document.querySelector<HTMLInputElement>('[data-testid="zoom-slider"]');
      const set = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
      if (!s || !set) return [];
      const painted = () =>
        new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
      const out: number[] = [];
      for (let v = 20; v <= 1000; v += 20) {
        const t = performance.now();
        set.call(s, String(v));
        s.dispatchEvent(new Event("input", { bubbles: true }));
        await painted();
        out.push(performance.now() - t);
      }
      return out.sort((a, b) => a - b);
    });
    expect(steps.length).toBe(50);
    const p90 = steps[Math.floor(steps.length * 0.9)];
    console.log(
      `\n${release ? "Release" : "Debug"} app, limits × ${SLACK}:\n` +
        `| full size ready (background) | ${ready} ms | ${FULL_READY_MS} ms |\n` +
        `| zoom slider step, median | ${steps[steps.length >> 1].toFixed(1)} ms | |\n` +
        `| zoom slider step, 90th | ${p90.toFixed(1)} ms | ${STEP_P90_MS} ms |\n` +
        `| zoom slider step, slowest | ${steps[steps.length - 1].toFixed(1)} ms | |`,
    );
    // The slider's far end is the highest zoom.
    expect(
      Number(await page.getByTestId("viewer-stage").getAttribute("data-zoom")),
    ).toBeGreaterThan(100);
    expect(ready).toBeLessThanOrEqual(FULL_READY_MS * SLACK);
    expect(p90).toBeLessThanOrEqual(STEP_P90_MS * SLACK);
  } finally {
    await app.close();
    w.cleanup();
  }
});
