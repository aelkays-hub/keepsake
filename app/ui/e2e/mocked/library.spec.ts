// Core flows against the mock backend (300,000 synthetic items).

import { expect, test, type Page } from "@playwright/test";

const calls = (page: Page, cmd: string) =>
  page.evaluate((c) => (window.__mock?.calls ?? []).filter((x) => x.cmd === c).map((x) => x.args), cmd);

async function openLibrary(page: Page, items = 300000) {
  await page.goto(`/?items=${items}`);
  await expect(page.getByTestId("count")).toHaveText(`${items.toLocaleString("en-US")} items`);
  await expect(page.getByTestId("tile").first()).toBeVisible();
}

test("first run creates a library and shows the grid", async ({ page }) => {
  await page.goto("/?library=none&items=500");
  await expect(page.getByRole("heading", { name: "Welcome to Keepsake" })).toBeVisible();
  await page.getByTestId("library-path").fill("D:\\Keepsake");
  await page.getByTestId("create-library").click();
  await expect(page.getByTestId("count")).toHaveText("500 items");
  expect(await calls(page, "library_create")).toEqual([{ path: "D:\\Keepsake" }]);
});

test("the grid stays virtualized at 300,000 items and reaches the end", async ({ page }) => {
  await openLibrary(page);
  const tiles = await page.getByTestId("tile").count();
  expect(tiles).toBeGreaterThan(10);
  expect(tiles).toBeLessThan(400);
  // Keyboard: End jumps to the last item, which is rendered with a thumbnail.
  await page.getByTestId("tile").first().click();
  await page.keyboard.press("End");
  const last = page.locator('[data-testid="tile"][data-index="299999"]');
  await expect(last).toBeVisible();
  await expect(last.locator("img")).toHaveAttribute("src", /^data:image/);
  expect(await page.getByTestId("tile").count()).toBeLessThan(400);
  // Scrolling back to the top with Home.
  await page.keyboard.press("Home");
  await expect(page.locator('[data-testid="tile"][data-index="0"]')).toBeVisible();
});

test("multi-select with mouse and keyboard, then rate the selection", async ({ page }) => {
  await openLibrary(page);
  const tile = (i: number) => page.locator(`[data-testid="tile"][data-index="${i}"]`);
  await tile(0).click();
  await tile(5).click({ modifiers: ["Shift"] });
  await expect(page.getByTestId("selected")).toHaveText("6 selected");
  await tile(8).click({ modifiers: ["Control"] });
  await expect(page.getByTestId("selected")).toHaveText("7 selected");
  await page.keyboard.press("Shift+ArrowRight");
  await expect(tile(9)).toHaveAttribute("aria-selected", "true");
  await page.keyboard.press("Control+a");
  await expect(page.getByTestId("selected")).toHaveText("300,000 selected");
  await page.keyboard.press("4");
  await expect.poll(() => calls(page, "edit_rating")).toContainEqual({
    selection: { ids: [], view: expect.any(Number), ranges: [[0, 300000]] },
    rating: 4,
  });
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("selected")).toHaveCount(0);
});

test("keyboard rating and label show in the inspector", async ({ page }) => {
  await openLibrary(page, 2000);
  await page.getByTestId("tile").first().click();
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("3");
  const insp = page.getByTestId("inspector");
  await expect(insp.getByRole("radio", { name: "3 stars" })).toHaveAttribute("aria-checked", "true");
  await page.keyboard.press("6");
  await expect(insp.getByRole("radio", { name: "Red" })).toHaveAttribute("aria-checked", "true");
  await page.keyboard.press("0");
  await expect(insp.getByRole("radio", { name: "3 stars" })).toHaveAttribute("aria-checked", "false");
});

test("inspector edits comment and keywords", async ({ page }) => {
  await openLibrary(page, 2000);
  await page.getByTestId("tile").nth(2).click();
  const insp = page.getByTestId("inspector");
  await insp.getByTestId("comment").fill("Sunset at the lake");
  await insp.getByTestId("keyword-input").click();
  await expect.poll(() => calls(page, "edit_comment")).toHaveLength(1);
  await insp.getByTestId("keyword-input").fill("Places|Lake");
  await page.keyboard.press("Enter");
  await expect(insp.getByRole("list", { name: "Keywords" })).toContainText("Places|Lake");
  await insp.getByRole("button", { name: "Remove Places|Lake" }).click();
  await expect(insp.getByRole("list", { name: "Keywords" })).not.toContainText("Places|Lake");
});

test("calendar drills from year to day and opens the day's grid", async ({ page }) => {
  await openLibrary(page, 20000);
  await page.getByTestId("mode-calendar").click();
  const cells = page.getByTestId("cal-cell");
  await expect(cells.first()).toBeVisible();
  const years = await cells.allTextContents();
  expect(years.some((t) => t.startsWith("2018"))).toBe(true);
  await page.locator('[data-testid="cal-cell"][data-key="2018"]').click();
  await expect(page.getByRole("grid", { name: "Months" })).toBeVisible();
  await page.locator('[data-testid="cal-cell"][data-key="2018-06"]').click();
  await expect(page.getByRole("grid", { name: "Days" })).toBeVisible();
  const day = page.locator('[data-testid="cal-cell"]:not([disabled])').first();
  const key = await day.getAttribute("data-key");
  const label = (await day.getAttribute("aria-label")) ?? "";
  const n = Number(label.split(": ")[1].split(" ")[0].replace(/,/g, ""));
  await day.click();
  await expect(page.getByTestId("heading")).toContainText(`June › ${Number(key?.slice(8))}`);
  await expect(page.getByTestId("count")).toHaveText(`${n} item${n === 1 ? "" : "s"}`);
  // Backspace from the grid goes back to the month.
  await page.getByTestId("tile").first().click();
  await page.keyboard.press("Backspace");
  await expect(page.getByRole("grid", { name: "Days" })).toBeVisible();
});

test("search with words and filters narrows the grid", async ({ page }) => {
  await openLibrary(page, 20000);
  await page.getByTestId("search").fill("DSC00010");
  await expect(page.getByTestId("count")).toHaveText(/^\d{1,2} items?$/);
  await page.getByRole("button", { name: "Clear filters" }).click();
  await expect(page.getByTestId("count")).toHaveText("20,000 items");
  await page.getByTestId("type").selectOption("video");
  await page.getByTestId("rating").selectOption("3");
  await page.getByTestId("camera").selectOption("ILCE-7M4");
  await page.getByTestId("from").fill("2016-01-01");
  await page.getByTestId("to").fill("2019-12-31");
  const shown = async () => Number(((await page.getByTestId("count").textContent()) ?? "").split(" ")[0].replace(/,/g, ""));
  await expect.poll(shown).toBeLessThan(2000);
  expect(await shown()).toBeGreaterThan(0);
  const q = (await calls(page, "view_open")).pop() as { query: Record<string, unknown> };
  expect(q.query).toMatchObject({ kind: "video", minRating: 3, camera: "ILCE-7M4", from: "2016-01-01", to: "2019-12-31" });
  await expect(page.locator(".badge", { hasText: "▶" }).first()).toBeVisible();
});

test("details view sorts by column", async ({ page }) => {
  await openLibrary(page, 5000);
  await page.keyboard.press("Control+3");
  await expect(page.getByTestId("details")).toBeVisible();
  await page.getByTestId("sort-name").click();
  await expect(page.getByTestId("sort-name")).toHaveAttribute("aria-sort", "ascending");
  await expect(page.getByTestId("drow").first().locator(".name")).toHaveText(/^C\d{5}\.MP4/);
  await page.getByTestId("sort-name").click();
  await expect(page.getByTestId("sort-name")).toHaveAttribute("aria-sort", "descending");
  await expect(page.getByTestId("drow").first().locator(".name")).toContainText("DSC");
  await page.getByTestId("sort-size").click();
  await expect(page.getByTestId("sort-size")).toHaveAttribute("aria-sort", "descending");
  await expect
    .poll(async () => (await calls(page, "view_open")).pop())
    .toMatchObject({ sort: { field: "size", descending: true } });
});

test("offline items show their cached thumbnail and a badge", async ({ page }) => {
  await openLibrary(page, 2000);
  const off = page.locator(".tile.offline").first();
  await expect(off).toBeVisible();
  await expect(off.locator(".badge.off")).toHaveText("offline");
  await expect(off.locator("img")).toBeVisible();
  // The unplugged volume in the sidebar lists exactly those.
  await page.getByTestId("sidebar").getByRole("button", { name: /Archive/ }).click();
  await expect(page.getByTestId("heading")).toHaveText("Archive (unplugged)");
  await expect(page.getByTestId("count")).toHaveText("500 items");
  await expect(page.locator(".tile.offline").first()).toBeVisible();
  expect(await page.locator(".tile:not(.offline)").count()).toBe(0);
});

test("keyboard: sidebar arrows, view switching, search shortcut", async ({ page }) => {
  await openLibrary(page, 2000);
  await page.getByTestId("sidebar").getByRole("button", { name: "All items" }).focus();
  await page.keyboard.press("ArrowDown");
  await expect(page.getByTestId("sidebar").getByRole("button", { name: "Calendar" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("calendar")).toBeVisible();
  await page.keyboard.press("Control+2");
  await expect(page.getByTestId("grid")).toBeVisible();
  await page.getByTestId("grid").focus();
  await page.keyboard.press("Control+f");
  await expect(page.getByTestId("search")).toBeFocused();
});

test("themes follow the OS", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "light" });
  await openLibrary(page, 100);
  const bg = () => page.evaluate(() => getComputedStyle(document.body).backgroundColor);
  expect(await bg()).toBe("rgb(246, 246, 247)");
  await page.emulateMedia({ colorScheme: "dark" });
  expect(await bg()).toBe("rgb(23, 23, 26)");
});
