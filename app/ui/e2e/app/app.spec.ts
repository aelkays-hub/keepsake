// Core flows against the real app (Windows): real catalog, real XMP files,
// real thumbnails, the real device watcher (fed fake card events), and a
// real import.

import { expect, test } from "@playwright/test";
import * as fs from "node:fs";
import * as path from "node:path";

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

test("first run creates a library", async () => {
  const w = new World();
  const app = new App(w);
  try {
    await app.connect();
    const page = await app.page("main");
    await expect(page.getByRole("heading", { name: "Welcome to Keepsake" })).toBeVisible();
    await page.getByTestId("library-path").fill(w.p("lib"));
    await page.getByTestId("create-library").click();
    await expect(page.getByTestId("count")).toHaveText("0 items");
    expect(fs.existsSync(path.join(w.p("lib"), "catalog.db"))).toBe(true);
    expect(fs.readFileSync(path.join(w.config, "config.toml"), "utf8")).toContain(
      "default_library",
    );
  } finally {
    await app.close();
    w.cleanup();
  }
});

test("library: thumbnails, rating writes XMP, calendar, search, details, offline", async () => {
  const w = new World();
  w.cli(["init", w.p("lib")]);
  w.cacheHere();
  const card = w.media("card");
  w.cli(["ingest", card, "--dest", w.p("A"), "--no-probe", "--unsafe-allow-same-disk"]);
  w.cli(["renders", "run", "--ignore-battery"]);
  const app = new App(w);
  try {
    await app.connect();
    const page = await app.page("main");
    await expect(page.getByTestId("count")).toHaveText("7 items");
    // Thumbnails come from the cache through the asset protocol.
    const img = page.getByTestId("tile").first().locator("img");
    await expect(img).toBeVisible();
    await expect
      .poll(() => img.evaluate((i: HTMLImageElement) => i.naturalWidth))
      .toBeGreaterThan(0);
    expect(await img.getAttribute("src")).toMatch(/asset\.localhost|asset:/);

    // Rate with the keyboard: the XMP sidecar beside the original changes.
    await page.getByTestId("search").fill("DSC01002");
    await expect(page.getByTestId("count")).toHaveText("1 item");
    await page.getByTestId("tile").first().click();
    await page.keyboard.press("4");
    await page.keyboard.press("6");
    const xmp = path.join(w.p("A"), "0000-undated", "0000-00-00", "DSC01002.JPG.xmp");
    await expect
      .poll(() => (fs.existsSync(xmp) ? fs.readFileSync(xmp, "utf8") : ""))
      .toMatch(/Rating[^0-9]*4/);
    await expect.poll(() => fs.readFileSync(xmp, "utf8")).toContain("Red");
    await expect(
      page.getByTestId("inspector").getByRole("radio", { name: "4 stars" }),
    ).toHaveAttribute("aria-checked", "true");
    await page.getByRole("button", { name: "Clear filters" }).click();
    await expect(page.getByTestId("count")).toHaveText("7 items");

    // Filters: videos only, then 4 stars or more.
    await page.getByTestId("type").selectOption("video");
    await expect(page.getByTestId("count")).toHaveText("1 item");
    await page.getByTestId("type").selectOption("");
    await page.getByTestId("rating").selectOption("4");
    await expect(page.getByTestId("count")).toHaveText("1 item");
    await page.getByRole("button", { name: "Clear filters" }).click();

    // Details: sort by name.
    await page.keyboard.press("Control+3");
    await page.getByTestId("sort-name").click();
    await expect(page.getByTestId("drow").first().locator(".name")).toHaveText(/C0001\.MP4/);

    // Calendar: the files have no capture dates (no probing), so only "No date".
    await page.keyboard.press("Control+1");
    await expect(page.locator('[data-testid="cal-cell"][data-key="undated"]')).toBeVisible();

    // Offline: the destination drive goes away; thumbnails stay, badged.
    await page.keyboard.press("Control+2");
    fs.renameSync(w.p("A"), w.p("A-unplugged"));
    await page.reload();
    await expect(page.getByTestId("count")).toHaveText("7 items");
    await expect(page.locator(".tile.offline").first()).toBeVisible();
    await expect(page.locator(".tile.offline img").first()).toBeVisible();
    expect(await page.locator(".tile:not(.offline)").count()).toBe(0);
  } finally {
    await app.close();
    w.cleanup();
  }
});

test("a card insert opens the import window; import ends SAFE TO FORMAT", async () => {
  const w = new World();
  w.cli(["init", w.p("lib")]);
  w.cacheHere();
  const card = w.media("card", 4);
  const events = w.p("events.jsonl");
  fs.writeFileSync(
    events,
    JSON.stringify({
      event: "arrived",
      mount: card,
      identity: { fs_serial: "AB12-CD34", fs_type: "exFAT", label: "A7IV" },
    }) + "\n",
  );
  const app = new App(w, { KEEPSAKE_FAKE_EVENTS: events });
  try {
    await app.connect();
    const imp = await app.page("import");
    await expect(imp.getByTestId("import-summary")).toContainText("5 new items");
    await expect(imp.getByTestId("card-tile").first().locator("img")).toBeVisible();
    await imp.getByTestId("select-none").click();
    await expect(imp.getByTestId("import-summary")).toContainText("0 selected");
    await imp.getByTestId("select-all").click();
    await imp.getByTestId("dest-a").fill(w.p("A"));
    await imp.getByTestId("dest-b").fill(w.p("B"));
    await expect(imp.getByTestId("check")).toContainText("needs");
    await imp.getByTestId("start-import").click();
    await expect(imp.getByTestId("result")).toContainText("SAFE TO FORMAT", { timeout: 120_000 });
    // The main window shows the imported items.
    const main = await app.page("main");
    await expect(main.getByTestId("count")).toHaveText("5 items");
    expect(
      fs.readdirSync(w.p("B"), { recursive: true }).some((f) => String(f).endsWith("C0001.MP4")),
    ).toBe(true);
  } finally {
    await app.close();
    w.cleanup();
  }
});

test("300,000 items stay virtualized in the real app", async () => {
  // Seeding 300,000 items is setup, not what is measured: on a slow CI
  // runner it alone took over four minutes (the timings below have their
  // own limits).
  test.setTimeout(600_000);
  const n = Number(process.env.KEEPSAKE_UI_SEED ?? 300000);
  test.skip(
    !fs.existsSync(exe("keepsake", "release")) && n > 50000,
    "needs a release CLI to seed 300,000 items",
  );
  const w = new World();
  w.cli(["init", w.p("lib")]);
  w.cacheHere();
  w.cli(["dev", "seed", "--assets", String(n)], fs.existsSync(exe("keepsake", "release")));
  const app = new App(w);
  try {
    await app.connect();
    const page = await app.page("main");
    await expect(page.getByTestId("count")).toHaveText(`${n.toLocaleString("en-US")} items`, {
      timeout: 60_000,
    });
    expect(await page.getByTestId("tile").count()).toBeLessThan(400);
    await page.getByTestId("tile").first().click();
    const t0 = Date.now();
    await page.keyboard.press("End");
    const last = page.locator(`[data-testid="tile"][data-index="${n - 1}"]`);
    await expect(last.locator("img")).toBeVisible();
    const ms = Date.now() - t0;
    console.log(`End → last tile with thumbnail: ${ms} ms`);
    expect(ms).toBeLessThan(5000);
    await expect(page.locator(".tile.offline").first()).toBeVisible();
    await page.keyboard.press("Control+a");
    await expect(page.getByTestId("selected")).toHaveText(`${n.toLocaleString("en-US")} selected`);
    expect(await page.getByTestId("tile").count()).toBeLessThan(400);
  } finally {
    await app.close();
    w.cleanup();
  }
});
