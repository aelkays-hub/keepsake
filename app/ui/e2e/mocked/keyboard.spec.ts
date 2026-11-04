// Every MVP flow with the keyboard alone: no clicks. `tabTo` presses Tab
// until the wanted control has focus, so it also proves the control can be
// reached by keyboard.

import { expect, test, type Locator, type Page } from "@playwright/test";

const calls = (page: Page, cmd: string) =>
  page.evaluate(
    (c) => (window.__mock?.calls ?? []).filter((x) => x.cmd === c).map((x) => x.args),
    cmd,
  );

async function tabTo(page: Page, target: Locator, max = 60, back = false) {
  for (let i = 0; i < max; i++) {
    if (
      await target.evaluate(
        (el) => el === document.activeElement || el.contains(document.activeElement),
      )
    )
      return;
    await page.keyboard.press(back ? "Shift+Tab" : "Tab");
  }
  throw new Error(`could not reach ${target} with ${back ? "Shift+Tab" : "Tab"}`);
}

const focused = (page: Page) =>
  page.evaluate(() => {
    const el = document.activeElement as HTMLElement | null;
    return {
      testid: el?.closest("[data-testid]")?.getAttribute("data-testid") ?? null,
      index: el?.getAttribute("data-index") ?? null,
      label: el?.getAttribute("aria-label") ?? el?.textContent?.trim() ?? "",
      inDialog: !!el?.closest('[role="dialog"]'),
    };
  });

async function open(page: Page, items = 2000) {
  await page.goto(`/?items=${items}`);
  await expect(page.getByTestId("count")).toHaveText(`${items.toLocaleString("en-US")} items`);
  await expect(page.getByTestId("tile").first()).toBeVisible();
}

test("first run: create a library with the keyboard", async ({ page }) => {
  await page.goto("/?library=none&items=50");
  await tabTo(page, page.getByTestId("library-path"));
  await page.keyboard.type("D:\\Keepsake");
  await tabTo(page, page.getByTestId("create-library"));
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("count")).toHaveText("50 items");
});

test("grid: move, select, rate, and each tile is announced", async ({ page }) => {
  await open(page);
  await tabTo(page, page.getByTestId("grid"));
  await page.keyboard.press("ArrowRight");
  let f = await focused(page);
  // Focus is on the tile itself, with a spoken description.
  expect(f.testid).toBe("tile");
  expect(f.label).toMatch(/^(DSC|C)\d{5}\.(JPG|ARW|MP4), (photo|RAW photo|video), \d{4}-\d\d-\d\d/);
  await page.keyboard.press("Shift+ArrowRight");
  await page.keyboard.press("Shift+ArrowRight");
  await expect(page.getByTestId("selected")).toHaveText("3 selected");
  await page.keyboard.press("5");
  await expect.poll(async () => (await calls(page, "edit_rating")).length).toBe(1);
  f = await focused(page);
  expect(f.label).toContain("5 stars");
  await page.keyboard.press("PageDown");
  await page.keyboard.press("End");
  // The last tile is drawn only after the grid scrolls there, so focus
  // reaches it a moment later.
  await expect.poll(async () => (await focused(page)).index).toBe("1999");
});

test("viewer: open, move, close, and focus comes back", async ({ page }) => {
  await open(page);
  await tabTo(page, page.getByTestId("grid"));
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("viewer")).toBeVisible();
  await page.keyboard.press("ArrowRight");
  await expect(page.getByTestId("viewer-position")).toHaveText("2 of 2,000");
  // Tab stays inside the viewer.
  for (let i = 0; i < 12; i++) {
    await page.keyboard.press("Tab");
    expect((await focused(page)).inDialog).toBe(true);
  }
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("viewer")).toHaveCount(0);
  const f = await focused(page);
  expect(f.testid).toBe("tile");
  expect(f.index).toBe("1");
});

test("calendar and details views", async ({ page }) => {
  await open(page, 20000);
  await tabTo(page, page.getByTestId("grid"));
  await page.keyboard.press("Control+1");
  await expect(page.getByTestId("calendar")).toBeVisible();
  await tabTo(page, page.getByTestId("cal-cell").first());
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("group", { name: "Months" })).toBeVisible();
  await tabTo(page, page.getByRole("group", { name: "Months" }));
  await page.keyboard.press("Enter");
  await expect(page.getByRole("group", { name: "Days" })).toBeVisible();
  await page.keyboard.press("Backspace");
  await expect(page.getByRole("group", { name: "Months" })).toBeVisible();
  await page.keyboard.press("Control+3");
  await tabTo(page, page.getByTestId("details"));
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("ArrowDown");
  expect(await focused(page)).toMatchObject({ testid: "drow", index: "1" });
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("viewer")).toBeVisible();
  await page.keyboard.press("Escape");
  expect(await focused(page)).toMatchObject({ testid: "drow", index: "1" });
});

test("search, filters and the inspector", async ({ page }) => {
  await open(page);
  await tabTo(page, page.getByTestId("grid"));
  await page.keyboard.press("Control+f");
  await page.keyboard.type("DSC00010");
  await expect(page.getByTestId("count")).toHaveText("1 item");
  await tabTo(page, page.getByTestId("type"));
  await page.keyboard.press("ArrowDown"); // the next type in the list
  await expect
    .poll(async () => (await calls(page, "view_open")).at(-1))
    .toMatchObject({
      query: { text: "DSC00010" },
    });
  await tabTo(page, page.getByRole("button", { name: "Clear filters" }));
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("count")).toHaveText("2,000 items");
  // F6 moves between the sidebar, the items and the inspector.
  await tabTo(page, page.getByTestId("grid"));
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("F6");
  expect((await focused(page)).testid).toBe("inspector");
  await tabTo(page, page.getByTestId("comment"));
  await page.keyboard.type("Written without a mouse");
  await page.keyboard.press("Tab");
  await expect.poll(async () => (await calls(page, "edit_comment")).length).toBe(1);
});

test("time shift with the keyboard; Tab stays in the dialog", async ({ page }) => {
  await open(page);
  await tabTo(page, page.getByTestId("grid"));
  await page.keyboard.press("ArrowRight");
  await page.keyboard.press("Shift+ArrowRight");
  await page.keyboard.press("Control+t");
  const d = page.getByTestId("time-shift");
  await expect(d).toBeVisible();
  await expect(d.getByTestId("ts-reference")).toBeFocused();
  await tabTo(page, d.getByTestId("ts-correct"));
  const before = (await d.getByTestId("ts-correct").inputValue()).slice(0, 19);
  const later = new Date(Date.parse(`${before}Z`) + 3600_000).toISOString().slice(0, 19);
  await d.getByTestId("ts-correct").fill(later); // typed into the focused field
  await expect(d.getByTestId("ts-offset")).toHaveText("+1 h");
  for (let i = 0; i < 10; i++) {
    await page.keyboard.press("Tab");
    expect((await focused(page)).inDialog).toBe(true);
  }
  await tabTo(page, d.getByTestId("ts-apply"));
  await page.keyboard.press("Enter");
  await expect(d).toHaveCount(0);
  expect(await calls(page, "time_shift_apply")).toHaveLength(1);
});

test("backup and verify with the keyboard", async ({ page }) => {
  await open(page);
  await tabTo(page, page.getByTestId("sidebar").getByRole("button", { name: "All items" }));
  // Past the calendar tree's years, then Events, Volumes and Folders.
  for (let i = 0; i < 40; i++) {
    if ((await focused(page)).label === "Backup") break;
    await page.keyboard.press("ArrowDown");
  }
  await page.keyboard.press("Enter");
  const screen = page.getByTestId("backup-screen");
  await tabTo(page, screen.getByTestId("backup-dest"));
  await page.keyboard.type("E:\\Backup");
  await tabTo(page, screen.getByTestId("backup-start"));
  await page.keyboard.press("Enter");
  await expect(screen.getByTestId("backup-outcome")).toContainText("Backup finished");
  await tabTo(page, screen.getByTestId("verify-now"));
  await page.keyboard.press("Enter");
  await expect(screen.getByTestId("backup-outcome")).toContainText("Verification finished");
});

test("settings with the keyboard", async ({ page }) => {
  await open(page);
  await tabTo(page, page.getByTestId("grid"));
  await page.keyboard.press("Control+,");
  const screen = page.getByTestId("settings-screen");
  await tabTo(page, screen.getByTestId("cache-limit"));
  await page.keyboard.press("ControlOrMeta+a"); // Ctrl+A, or Cmd+A on macOS
  await page.keyboard.type("40");
  await tabTo(page, screen.getByTestId("settings-save"));
  await page.keyboard.press("Enter");
  await expect(screen.getByTestId("settings-saved")).toHaveText("Saved.");
  expect((await calls(page, "settings_set"))[0]).toMatchObject({
    update: { cacheMaxBytes: 40 * 1024 ** 3 },
  });
  await tabTo(page, screen.getByTestId("copy-report"));
});

test("the import window with the keyboard", async ({ page }) => {
  await page.goto(`/?items=100#/import?mount=${encodeURIComponent("E:\\")}`);
  await expect(page.getByTestId("import")).toBeVisible();
  await tabTo(page, page.getByTestId("select-none"));
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("import-summary")).toContainText("0 selected");
  // Items can be ticked one by one with Space.
  await tabTo(page, page.getByTestId("card-tile").nth(1));
  await page.keyboard.press("Space");
  await expect(page.getByTestId("import-summary")).toContainText("1 selected");
  await tabTo(page, page.getByTestId("select-all"), 60, true);
  await page.keyboard.press("Enter");
  await tabTo(page, page.getByTestId("dest-a"));
  await page.keyboard.type("F:\\Photos");
  await tabTo(page, page.getByTestId("dest-b"));
  await page.keyboard.type("G:\\Photos");
  await tabTo(page, page.getByTestId("start-import"));
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("result")).toContainText("SAFE TO FORMAT");
});
