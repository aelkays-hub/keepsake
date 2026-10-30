// Viewer, time shift, backup and settings against the mock backend.

import { expect, test, type Page } from "@playwright/test";

const calls = (page: Page, cmd: string) =>
  page.evaluate(
    (c) => (window.__mock?.calls ?? []).filter((x) => x.cmd === c).map((x) => x.args),
    cmd,
  );

// The mock's item kinds (src/mock/backend.ts): video, raw or photo by a hash.
const r = (i: number) => (i * 2654435761) % 1000;
const kindOf = (i: number) => (r(i) < 150 ? "video" : r(i) > 850 ? "raw" : "photo");
const online = (i: number) => i % 4 !== 3;
const find = (f: (i: number) => boolean) => {
  for (let i = 1; i < 2000; i++) if (f(i)) return i;
  throw new Error("no such item");
};
const pad = (i: number) => String(i).padStart(5, "0");

async function openLibrary(page: Page, items = 2000) {
  await page.goto(`/?items=${items}`);
  await expect(page.getByTestId("count")).toHaveText(`${items.toLocaleString("en-US")} items`);
  await expect(page.getByTestId("tile").first()).toBeVisible();
}

/** Narrow the grid to one item by its name and open it in the viewer. */
async function viewOne(page: Page, name: string) {
  await page.getByTestId("search").fill(name);
  await expect
    .poll(async () => (await calls(page, "view_open")).at(-1))
    .toMatchObject({ query: { text: name } });
  await expect(page.getByTestId("count")).toHaveText("1 item");
  await page.getByTestId("tile").first().dblclick();
  await expect(page.getByTestId("viewer")).toBeVisible();
  await expect(page.getByTestId("viewer-name")).toHaveText(new RegExp(name));
}

const tile = (page: Page, i: number) => page.locator(`[data-testid="tile"][data-index="${i}"]`);

test("Enter opens the viewer; arrows move through the view, or the selection", async ({ page }) => {
  await openLibrary(page);
  await tile(page, 0).click();
  await page.keyboard.press("Enter");
  const pos = page.getByTestId("viewer-position");
  await expect(pos).toHaveText("1 of 2,000");
  await page.keyboard.press("ArrowRight");
  await expect(pos).toHaveText("2 of 2,000");
  await page.keyboard.press("End");
  await expect(pos).toHaveText("2,000 of 2,000");
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("viewer")).toHaveCount(0);
  // Back in the grid, on the last item the viewer showed.
  await expect(tile(page, 1999)).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("Home");
  // Two items selected: the viewer steps between those two only.
  await tile(page, 0).click();
  await tile(page, 5).click({ modifiers: ["Control"] });
  await page.keyboard.press("Enter");
  await expect(pos).toHaveText("6 of 2,000 · 2 selected");
  await page.keyboard.press("ArrowLeft");
  await expect(pos).toHaveText("1 of 2,000 · 2 selected");
  await page.keyboard.press("ArrowLeft");
  await expect(pos).toHaveText("1 of 2,000 · 2 selected");
  await page.keyboard.press("ArrowRight");
  await expect(pos).toHaveText("6 of 2,000 · 2 selected");
  // Closing keeps the selection.
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("selected")).toHaveText("2 selected");
});

test("Z zooms to a full-size picture at 100% and arrows pan", async ({ page }) => {
  const i = find((x) => kindOf(x) === "photo" && online(x));
  await openLibrary(page);
  await viewOne(page, `DSC${pad(i)}`);
  const stage = page.getByTestId("viewer-stage");
  const img = page.getByTestId("viewer-image");
  await expect(stage).toHaveAttribute("data-zoom", "fit");
  await expect(img).toHaveAttribute("data-full", "0");
  await page.keyboard.press("z");
  await expect(stage).toHaveAttribute("data-zoom", "100");
  await expect(img).toHaveAttribute("data-full", "1");
  expect(await calls(page, "full_image")).toEqual([{ id: `a${i}`, member: "primary" }]);
  // One picture pixel per screen pixel: the 4000 px picture overflows.
  await expect
    .poll(() =>
      img.evaluate((e: HTMLImageElement) =>
        Math.round(e.getBoundingClientRect().width * devicePixelRatio),
      ),
    )
    .toBe(4000);
  const left = await stage.evaluate((e) => e.scrollLeft);
  await page.keyboard.press("ArrowRight");
  await expect.poll(() => stage.evaluate((e) => e.scrollLeft)).toBeGreaterThan(left);
  // Arrows pan while zoomed: still the same item.
  await expect(page.getByTestId("viewer-position")).toHaveText("1 of 1");
  await page.keyboard.press("z");
  await expect(stage).toHaveAttribute("data-zoom", "fit");
  // A click zooms in too, and the full size is not fetched twice.
  await img.click();
  await expect(stage).toHaveAttribute("data-zoom", "100");
  expect(await calls(page, "full_image")).toHaveLength(1);
});

test("the zoom slider, + and −, Fit and 100%", async ({ page }) => {
  const i = find((x) => kindOf(x) === "photo" && online(x));
  await openLibrary(page);
  await viewOne(page, `DSC${pad(i)}`);
  const stage = page.getByTestId("viewer-stage");
  const img = page.getByTestId("viewer-image");
  const level = page.getByTestId("zoom-level");
  const slider = page.getByTestId("zoom-slider");
  /** Picture pixels per screen pixel × 100. */
  const shownPct = () =>
    img.evaluate((e: HTMLImageElement) =>
      Math.round((e.getBoundingClientRect().width * devicePixelRatio * 100) / 4000),
    );
  await expect(level).toHaveText("Fit");
  await expect(slider).toHaveValue("0");

  // The slider's end: 400%, from the full-size picture.
  await slider.fill("1000");
  await expect(stage).toHaveAttribute("data-zoom", "400");
  await expect(img).toHaveAttribute("data-full", "1");
  await expect(level).toHaveText("400%");
  await expect.poll(shownPct).toBe(400);
  expect(await calls(page, "full_image")).toHaveLength(1);

  // − steps down; + steps up.
  await page.getByTestId("zoom-out").click();
  await expect(stage).toHaveAttribute("data-zoom", "320");
  await page.keyboard.press("-");
  await expect(stage).toHaveAttribute("data-zoom", "256");
  await page.keyboard.press("+");
  await expect(stage).toHaveAttribute("data-zoom", "320");
  await expect.poll(shownPct).toBe(320);

  // 100%: one picture pixel per screen pixel.
  await page.getByTestId("zoom-100").click();
  await expect(stage).toHaveAttribute("data-zoom", "100");
  await expect.poll(shownPct).toBe(100);

  // Fit: the whole picture in the window, with its percentage.
  await page.getByTestId("zoom-fit").click();
  await expect(stage).toHaveAttribute("data-zoom", "fit");
  await expect(slider).toHaveValue("0");
  await expect(level).toHaveText(/^Fit \(\d+%\)$/);
  const box = await stage.boundingBox();
  const r = await img.boundingBox();
  expect(r!.width).toBeLessThanOrEqual(box!.width + 1);
  expect(r!.height).toBeLessThanOrEqual(box!.height + 1);

  // The middle of the slider is between Fit and 400%.
  const fit = Number((await level.textContent())?.match(/\d+/)?.[0]);
  expect(fit).toBeLessThan(100);
  await slider.fill("500");
  const mid = Number(await stage.getAttribute("data-zoom"));
  expect(mid).toBeGreaterThan(fit);
  expect(mid).toBeLessThan(400);

  // On the slider, arrows move the slider, not to another item or the picture.
  await slider.focus();
  await page.keyboard.press("ArrowRight");
  await expect(slider).toHaveValue("501");
  await expect(page.getByTestId("viewer")).toBeVisible();
  // The full size was fetched once.
  expect(await calls(page, "full_image")).toHaveLength(1);
});

test("the wheel zooms in and out around the pointer", async ({ page }) => {
  const i = find((x) => kindOf(x) === "photo" && online(x));
  await openLibrary(page);
  await viewOne(page, `DSC${pad(i)}`);
  const stage = page.getByTestId("viewer-stage");
  const box = (await stage.boundingBox())!;
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  for (let n = 0; n < 6; n++) await page.mouse.wheel(0, -200);
  await expect(page.getByTestId("viewer-image")).toHaveAttribute("data-full", "1");
  await expect.poll(async () => Number(await stage.getAttribute("data-zoom"))).toBeGreaterThan(0);
  const z = Number(await stage.getAttribute("data-zoom"));
  // Zoomed on the middle: the picture is scrolled to its middle.
  const mid = await stage.evaluate((e) => (e.scrollLeft + e.clientWidth / 2) / e.scrollWidth);
  expect(mid).toBeGreaterThan(0.4);
  expect(mid).toBeLessThan(0.6);
  for (let n = 0; n < 2; n++) await page.mouse.wheel(0, 200);
  await expect.poll(async () => Number(await stage.getAttribute("data-zoom"))).toBeLessThan(z);
  for (let n = 0; n < 30; n++) await page.mouse.wheel(0, 200);
  await expect(stage).toHaveAttribute("data-zoom", "fit");
  // Still the same item.
  await expect(page.getByTestId("viewer-position")).toHaveText("1 of 1");
});

test("a RAW + JPEG pair is one item with a JPEG | RAW toggle", async ({ page }) => {
  const i = find((x) => kindOf(x) === "raw" && x % 2 === 0 && online(x));
  await openLibrary(page);
  await viewOne(page, `DSC${pad(i)}`);
  const tabs = page.getByRole("tablist", { name: "Show" });
  await expect(tabs.getByRole("tab", { name: "JPEG" })).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("r");
  await expect(tabs.getByRole("tab", { name: "RAW" })).toHaveAttribute("aria-selected", "true");
  await expect(page.getByTestId("viewer-image")).toHaveAttribute("data-member", "raw");
  await expect(page.getByTestId("viewer-image")).toHaveAttribute("data-full", "1");
  expect(await calls(page, "full_image")).toEqual([{ id: `a${i}`, member: "raw" }]);
  await page.keyboard.press("r");
  await expect(tabs.getByRole("tab", { name: "JPEG" })).toHaveAttribute("aria-selected", "true");
  // A RAW alone has no toggle.
  const solo = find((x) => kindOf(x) === "raw" && x % 2 === 1 && online(x));
  await page.keyboard.press("Escape");
  await viewOne(page, `DSC${pad(solo)}`);
  await expect(page.getByRole("tablist", { name: "Show" })).toHaveCount(0);
});

test("video: frame stepping, jumps and timecode from the start timecode", async ({ page }) => {
  const i = find((x) => kindOf(x) === "video" && online(x));
  await openLibrary(page);
  await viewOne(page, `C${pad(i)}`);
  const video = page.getByTestId("viewer-video");
  await expect
    .poll(() => video.evaluate((v: HTMLVideoElement) => v.readyState))
    .toBeGreaterThanOrEqual(1);
  const tc = page.getByTestId("timecode");
  await expect(tc).toHaveText("10:00:00:00");
  await page.keyboard.press(".");
  await expect(tc).toHaveText("10:00:00:01");
  await expect(page.getByTestId("frame")).toHaveText("frame 1");
  await page.keyboard.press(".");
  await page.keyboard.press(".");
  await expect(tc).toHaveText("10:00:00:03");
  await page.keyboard.press(",");
  await expect(tc).toHaveText("10:00:00:02");
  await page.keyboard.press("l");
  await expect(tc).toHaveText("10:00:01:02");
  await page.keyboard.press("j");
  await expect(tc).toHaveText("10:00:00:02");
  // Space plays and pauses. (Headless Chromium on the macOS runner may not
  // advance playback time, so the state is checked, not the position.)
  await page.keyboard.press("Space");
  await expect.poll(() => video.evaluate((v: HTMLVideoElement) => v.paused)).toBe(false);
  await expect(page.getByRole("button", { name: "Pause" })).toBeVisible();
  await page.keyboard.press("Space");
  await expect.poll(() => video.evaluate((v: HTMLVideoElement) => v.paused)).toBe(true);
  // The scrub bar seeks.
  await page.getByRole("slider", { name: "Position" }).fill("1.5");
  await expect(tc).toHaveText("10:00:01:12");
});

test("rate, label and open the item shown in the viewer", async ({ page }) => {
  const i = find((x) => kindOf(x) === "photo" && online(x));
  await openLibrary(page);
  await viewOne(page, `DSC${pad(i)}`);
  await page.keyboard.press("4");
  await expect
    .poll(() => calls(page, "edit_rating"))
    .toEqual([{ selection: { ids: [`a${i}`], view: null, ranges: [] }, rating: 4 }]);
  await page.keyboard.press("8");
  await expect
    .poll(() => calls(page, "edit_label"))
    .toEqual([{ selection: { ids: [`a${i}`], view: null, ranges: [] }, label: "Green" }]);
  await page.keyboard.press("Control+o");
  await expect(page.getByTestId("viewer-note")).toHaveText("Opened with the system default");
  expect(await calls(page, "open_external")).toEqual([
    { id: `a${i}`, member: "primary", app: null },
  ]);
});

test("time shift: a reference photo and the correct time give the offset for all", async ({
  page,
}) => {
  await openLibrary(page);
  await tile(page, 0).click();
  await tile(page, 4).click({ modifiers: ["Shift"] });
  await page.keyboard.press("Control+t");
  const d = page.getByTestId("time-shift");
  await expect(d).toBeVisible();
  // The focused item is the reference by default.
  const before = (await d.getByTestId("ts-before").textContent()) ?? "";
  await expect(d.getByTestId("ts-offset")).toHaveText("no change");
  expect(before).toMatch(/^\d{4}-\d\d-\d\d \d\d:\d\d:\d\d$/);
  const later = new Date(Date.parse(`${before.replace(" ", "T")}Z`) + 3600_000 + 90_000);
  await d.getByTestId("ts-correct").fill(later.toISOString().slice(0, 19));
  await expect(d.getByTestId("ts-offset")).toHaveText("+1 h 1 min 30 s");
  const rows = d.getByTestId("ts-preview").locator("tbody tr");
  await expect(rows).toHaveCount(5);
  await expect(d.getByTestId("ts-apply")).toHaveText("Shift 5 items");
  const views = (await calls(page, "view_open")).length;
  await d.getByTestId("ts-apply").click();
  await expect(d).toHaveCount(0);
  const apply = await calls(page, "time_shift_apply");
  expect(apply).toEqual([
    { selection: { ids: [], view: expect.any(Number), ranges: [[0, 5]] }, offsetS: 3690 },
  ]);
  // The view is re-sorted with the new times.
  await expect.poll(async () => (await calls(page, "view_open")).length).toBeGreaterThan(views);
  // Escape closes without changes.
  await tile(page, 0).click();
  await page.keyboard.press("Control+t");
  await expect(d).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(d).toHaveCount(0);
  expect(await calls(page, "time_shift_apply")).toHaveLength(1);
});

test("backup: run to a folder, see it in the history, then verify it", async ({ page }) => {
  await openLibrary(page);
  await page.getByTestId("sidebar").getByRole("button", { name: "Backup" }).click();
  const screen = page.getByTestId("backup-screen");
  await expect(screen.getByText("No backups yet.")).toBeVisible();
  await screen.getByTestId("backup-dest").fill("E:\\Keepsake backup");
  await screen.getByTestId("backup-start").click();
  await expect(screen.getByTestId("backup-outcome")).toContainText("Backup finished");
  expect(await calls(page, "backup_run")).toEqual([{ dest: "E:\\Keepsake backup", label: null }]);
  const dests = screen.getByTestId("backup-destinations").locator("li");
  await expect(dests).toHaveCount(1);
  await expect(dests.first()).toContainText("Keepsake backup");
  await expect(dests.first().getByTestId("last-verified")).toHaveText("never");
  await dests.first().getByTestId("verify-now").click();
  await expect(screen.getByTestId("backup-outcome")).toContainText("Verification finished");
  await expect(dests.first().getByTestId("last-verified")).not.toHaveText("never");
  const history = screen.getByTestId("history").locator("tbody tr");
  await expect(history).toHaveCount(2);
  await expect(history.nth(0)).toContainText("Verify");
  await expect(history.nth(1)).toContainText("Backup");
});

test("settings: cache, workers and Open in are saved; old cache files can be removed", async ({
  page,
}) => {
  await openLibrary(page);
  await page.keyboard.press("Control+,");
  const screen = page.getByTestId("settings-screen");
  await expect(screen).toBeVisible();
  await expect(screen.getByTestId("settings-library")).toHaveText("D:\\Keepsake");
  await screen.getByTestId("cache-dir").fill("E:\\Cache");
  await screen.getByTestId("cache-limit").fill("50");
  await screen.getByTestId("workers").selectOption("2");
  await screen.getByTestId("app-ext").fill(".ARW");
  await screen.getByTestId("app-path").fill("C:\\Apps\\Raw Editor.exe");
  await screen.getByTestId("settings-save").click();
  await expect(screen.getByTestId("settings-saved")).toHaveText("Saved.");
  expect(await calls(page, "settings_set")).toEqual([
    {
      update: {
        cacheDir: "E:\\Cache",
        cacheMaxBytes: 50 * 1024 ** 3,
        workers: 2,
        proxyWorkers: 1,
        pauseOnBattery: true,
        openApps: [{ ext: ".ARW", app: "C:\\Apps\\Raw Editor.exe" }],
        import: {
          auto: false,
          destination: null,
          template: "{yyyy}/{month}/{yyyy}-{mm}-{dd}/{orig}",
        },
      },
    },
  ]);
  await expect(screen.getByTestId("open-apps")).toContainText(".arw");
  await screen.getByTestId("old-cache").getByRole("button", { name: "Remove old files" }).click();
  await expect(screen.getByTestId("settings-saved")).toHaveText(
    "Removed 2.5 GB from the old cache.",
  );
  // A limit below 256 MB is refused.
  await screen.getByTestId("cache-limit").fill("0.1");
  await screen.getByTestId("settings-save").click();
  await expect(screen.getByRole("alert")).toContainText("at least 256 MB");
});

test("settings: import presets can be deleted and created", async ({ page }) => {
  await openLibrary(page);
  await page.getByTestId("sidebar").getByRole("button", { name: "Settings" }).click();
  const presets = page.getByTestId("presets").locator("li");
  await expect(presets).toHaveCount(1);
  await page.getByRole("button", { name: "Delete Two drives" }).click();
  await expect(presets).toHaveCount(0);
  await page.getByRole("button", { name: "New preset…" }).click();
  const ed = page.getByTestId("preset-edit");
  await ed.getByLabel("Name", { exact: true }).fill("Travel");
  await ed.getByLabel("First destination").fill("F:\\Travel");
  await ed.getByRole("button", { name: "Save preset" }).click();
  await expect(presets).toHaveCount(1);
  await expect(presets.first()).toContainText("Travel");
  expect(await calls(page, "preset_save")).toEqual([
    {
      preset: {
        name: "Travel",
        destinations: ["F:\\Travel"],
        template: "{yyyy}/{yyyy}-{mm}-{dd}/{orig}",
        event: null,
      },
    },
  ]);
  // Back to the library from the sidebar.
  await page.getByTestId("sidebar").getByRole("button", { name: "All items" }).click();
  await expect(page.getByTestId("grid")).toBeVisible();
});
