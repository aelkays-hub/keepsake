// Viewer, time shift, backup, Open in and settings against the real app
// (Windows): real full-size decoding, a real proxy, XMP files on disk, a
// real backup and verification, a real external program.

import { expect, test, type Page } from "@playwright/test";
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";

import { App, exe, startVite, stopVite, vendor, World } from "./fixture";

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

const exiftool = path.join(vendor, "exiftool", "exiftool.exe");
const sha = (f: string) => createHash("sha256").update(fs.readFileSync(f)).digest("hex");

/** Every file under `dir` (relative paths). */
const files = (dir: string) =>
  (fs.readdirSync(dir, { recursive: true }) as string[]).filter((f) =>
    fs.statSync(path.join(dir, f)).isFile(),
  );

/** A library with the test card imported into `A`. With `dated`, the
 * photos say they were taken 2024-06-15 13:02:11. */
function library(w: World, dated: boolean, render = true): string {
  w.cli(["init", w.p("lib")]);
  w.cacheHere();
  const card = w.media("card");
  if (dated) {
    execFileSync(exiftool, [
      "-q",
      "-overwrite_original",
      "-DateTimeOriginal=2024:06:15 13:02:11",
      path.join(card, "DCIM", "100MSDCF"),
    ]);
  }
  const args = ["ingest", card, "--dest", w.p("A"), "--unsafe-allow-same-disk"];
  w.cli(dated ? args : [...args, "--no-probe"]);
  if (render) w.cli(["renders", "run", "--ignore-battery"]);
  return card;
}

async function only(page: Page, name: string) {
  await page.getByTestId("search").fill(name);
  await expect(page.getByTestId("count")).toHaveText("1 item");
  await expect(page.getByTestId("tile").first()).toHaveAttribute("title", new RegExp(name));
}

test("viewer: a real JPEG at 100%, and frame stepping on a real proxy", async () => {
  const w = new World();
  library(w, false);
  const app = new App(w);
  try {
    await app.connect();
    const page = await app.page("main");
    await expect(page.getByTestId("count")).toHaveText("7 items");
    // DSC01005.JPG is 1280 × 800 (the preview of it is the same size, so
    // the check is that the full-size rendition is made and shown).
    await only(page, "DSC01005");
    await page.getByTestId("tile").first().dblclick();
    await expect(page.getByTestId("viewer")).toBeVisible();
    await page.keyboard.press("z");
    const img = page.getByTestId("viewer-image");
    await expect(img).toHaveAttribute("data-full", "1");
    await expect.poll(() => img.evaluate((i: HTMLImageElement) => i.naturalWidth)).toBe(1280);
    await expect(page.getByTestId("viewer-stage")).toHaveAttribute("data-zoom", "100");
    expect(files(w.p("cache")).some((f) => f.startsWith("preview"))).toBe(true);
    await page.keyboard.press("Escape");

    await only(page, "C0001");
    await page.getByTestId("tile").first().dblclick();
    const video = page.getByTestId("viewer-video");
    await expect(video).toBeVisible();
    await expect
      .poll(() => video.evaluate((v: HTMLVideoElement) => v.readyState))
      .toBeGreaterThanOrEqual(1);
    const tc = page.getByTestId("timecode");
    await expect(tc).toHaveText("00:00:00:00");
    await page.keyboard.press(".");
    await expect(tc).toHaveText("00:00:00:01");
    await page.keyboard.press(".");
    await expect(tc).toHaveText("00:00:00:02");
    await page.keyboard.press(",");
    await expect(tc).toHaveText("00:00:00:01");
    await page.keyboard.press("l");
    await expect(tc).toHaveText("00:00:01:01");
  } finally {
    await app.close();
    w.cleanup();
  }
});

test("time shift writes the XMP sidecars and leaves the originals alone", async () => {
  const w = new World();
  library(w, true, false);
  const dest = w.p("A");
  const originals = files(dest).filter((f) => f.endsWith(".JPG"));
  expect(originals).toHaveLength(6);
  const before = Object.fromEntries(originals.map((f) => [f, sha(path.join(dest, f))]));
  const app = new App(w);
  try {
    await app.connect();
    const page = await app.page("main");
    await page.getByTestId("type").selectOption("photo");
    await expect(page.getByTestId("count")).toHaveText("6 items");
    await page.getByTestId("tile").first().click();
    await page.keyboard.press("Control+a");
    await page.keyboard.press("Control+t");
    const d = page.getByTestId("time-shift");
    await expect(d.getByTestId("ts-before")).toHaveText("2024-06-15 13:02:11");
    await d.getByTestId("ts-correct").fill("2024-06-15T14:32:11");
    await expect(d.getByTestId("ts-offset")).toHaveText("+1 h 30 min");
    await expect(d.getByTestId("ts-preview").locator("tbody tr")).toHaveCount(6);
    await expect(d.getByTestId("ts-preview")).toContainText("2024-06-15 14:32:11");
    await d.getByTestId("ts-apply").click();
    await expect(d).toHaveCount(0);
    // Every photo's sidecar has the new time and the shift.
    for (const f of originals) {
      const xmp = path.join(dest, `${f}.xmp`);
      await expect
        .poll(() => (fs.existsSync(xmp) ? fs.readFileSync(xmp, "utf8") : ""))
        .toContain("2024-06-15T14:32:11");
      expect(fs.readFileSync(xmp, "utf8")).toMatch(/CaptureShiftSeconds[^0-9-]*5400/);
    }
    // The originals are byte-for-byte what was imported.
    for (const f of originals) expect(sha(path.join(dest, f))).toBe(before[f]);
    // The inspector shows the new time.
    await page.getByTestId("tile").first().click();
    await expect(page.getByTestId("inspector")).toContainText("2024-06-15 14:32");
  } finally {
    await app.close();
    w.cleanup();
  }
});

test("backup copies and verifies, and both are in the history", async () => {
  const w = new World();
  library(w, false, false);
  const app = new App(w);
  try {
    await app.connect();
    const page = await app.page("main");
    await expect(page.getByTestId("count")).toHaveText("7 items");
    await page.getByTestId("sidebar").getByRole("button", { name: "Backup" }).click();
    const screen = page.getByTestId("backup-screen");
    await screen.getByTestId("backup-dest").fill(w.p("backup"));
    await screen.getByTestId("backup-start").click();
    await expect(screen.getByTestId("backup-outcome")).toContainText("Backup finished", {
      timeout: 120_000,
    });
    const copied = files(w.p("backup"));
    expect(copied.filter((f) => /DSC010\d\d\.JPG$/.test(f))).toHaveLength(6);
    expect(copied.some((f) => f.endsWith("C0001.MP4"))).toBe(true);
    expect(fs.existsSync(path.join(w.p("backup"), ".keepsake-library", "catalog.db"))).toBe(true);
    const dest = screen.getByTestId("backup-destinations").locator("li");
    await expect(dest).toHaveCount(1);
    await expect(dest.getByTestId("last-verified")).toHaveText("never");

    await dest.getByTestId("verify-now").click();
    await expect(screen.getByTestId("backup-outcome")).toContainText("Verification finished", {
      timeout: 120_000,
    });
    await expect(dest.getByTestId("last-verified")).not.toHaveText("never");
    await expect(screen.getByTestId("history").locator("tbody tr")).toHaveCount(2);
    const history = JSON.parse(fs.readFileSync(path.join(w.p("lib"), "history.json"), "utf8"));
    expect(history.map((r: { kind: string }) => r.kind)).toEqual(["verify", "backup"]);
    expect(history[0].problems).toBe(0);
    expect(history[0].files).toBeGreaterThanOrEqual(7);

    // A second backup copies nothing new.
    await screen.getByRole("button", { name: "Back up now" }).click();
    await expect(screen.getByTestId("backup-outcome")).toContainText("0 B copied this time", {
      timeout: 120_000,
    });
  } finally {
    await app.close();
    w.cleanup();
  }
});

test("Open in runs the program set for the type; settings are saved", async () => {
  const w = new World();
  library(w, false, false);
  // A stand-in "application" that writes down what it was asked to open.
  const record = w.p("opened.txt");
  const program = w.p("open-with.cmd");
  fs.writeFileSync(program, `@echo off\r\necho %~1> "${record}"\r\n`);
  const app = new App(w);
  try {
    await app.connect();
    const page = await app.page("main");
    await expect(page.getByTestId("count")).toHaveText("7 items");
    await page.keyboard.press("Control+,");
    const screen = page.getByTestId("settings-screen");
    await screen.getByTestId("app-ext").fill("jpg");
    await screen.getByTestId("app-path").fill(program);
    await screen.getByTestId("workers").selectOption("2");
    await screen.getByTestId("cache-limit").fill("30");
    await screen.getByTestId("settings-save").click();
    await expect(screen.getByTestId("settings-saved")).toHaveText("Saved.");
    const config = fs.readFileSync(path.join(w.config, "config.toml"), "utf8");
    expect(config).toMatch(/\[open\.apps\][\s\S]*jpg = /);
    expect(config).toContain("open-with.cmd");
    expect(config).toMatch(/workers = 2/);
    expect(config).toMatch(new RegExp(`max_bytes = ${30 * 1024 ** 3}`));

    await page.getByTestId("sidebar").getByRole("button", { name: "All items" }).click();
    await only(page, "DSC01003");
    await page.getByTestId("tile").first().click();
    await page.keyboard.press("Control+o");
    await expect.poll(() => (fs.existsSync(record) ? fs.readFileSync(record, "utf8") : "")).toMatch(
      /DSC01003\.JPG/,
    );
    const opened = fs.readFileSync(record, "utf8").trim();
    expect(fs.existsSync(opened)).toBe(true);
    // The imported copy in A (temp paths may be spelled short or long).
    const real = (p: string) => fs.realpathSync.native(p).toLowerCase();
    expect(real(opened).startsWith(real(w.p("A")))).toBe(true);

    // Settings survive a restart.
    await app.close();
    const again = new App(w);
    try {
      await again.connect();
      const p2 = await again.page("main");
      await expect(p2.getByTestId("count")).toHaveText("7 items");
      await p2.keyboard.press("Control+,");
      await expect(p2.getByTestId("open-apps")).toContainText(".jpg");
      await expect(p2.getByTestId("workers")).toHaveValue("2");
    } finally {
      await again.close();
    }
  } finally {
    await app.close();
    w.cleanup();
  }
});
