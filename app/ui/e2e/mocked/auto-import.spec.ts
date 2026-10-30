// A card inserted with "Copy new photos and videos automatically" on: the
// import window fills in the destination and layout from Settings and
// starts by itself. Off, it waits for you. A missing drive stops it.

import { expect, test, type Page } from "@playwright/test";

const calls = (page: Page, cmd: string) =>
  page.evaluate(
    (c) => (window.__mock?.calls ?? []).filter((x) => x.cmd === c).map((x) => x.args),
    cmd,
  );

const YMD = "{yyyy}/{month}/{yyyy}-{mm}-{dd}/{orig}";
const card = `#/import?mount=${encodeURIComponent("E:\\")}&auto=1`;

/** Record `window.close()` (a test page cannot really close itself). */
const watchClose = (page: Page) =>
  page.addInitScript(() => {
    (window as unknown as { closed_: boolean }).closed_ = false;
    window.close = () => {
      (window as unknown as { closed_: boolean }).closed_ = true;
    };
  });
const wasClosed = (page: Page) =>
  page.evaluate(() => (window as unknown as { closed_: boolean }).closed_);

test("on: new items are copied by themselves, by year, month and day", async ({ page }) => {
  await watchClose(page);
  await page.goto(`/?items=100&autoImport=1${card}`);
  const banner = page.getByTestId("auto-banner");
  await expect(banner).toContainText("E:\\Pictures");
  await expect(banner).toContainText("year, month and day");
  await expect(page.getByTestId("result")).toContainText("NOT safe to format");
  const starts = await calls(page, "import_start");
  expect(starts).toHaveLength(1);
  expect(starts[0]).toMatchObject({
    request: { destinations: ["E:\\Pictures"], template: YMD, event: null },
  });
  // Only the new items (the mock card has one already in the library).
  expect((starts[0] as { request: { keys: string[] } }).request.keys).toHaveLength(11);
  await expect(page.getByTestId("result")).toContainText("Copied 11 items to E:\\Pictures");
  // Then the window closes by itself.
  await expect(page.getByTestId("auto-close")).toContainText("Closing in");
  await expect.poll(() => wasClosed(page), { timeout: 8000 }).toBe(true);
});

test("Keep open stops the automatic close", async ({ page }) => {
  await watchClose(page);
  await page.goto(`/?items=100&autoImport=1${card}`);
  await page.getByTestId("keep-open").click();
  await expect(page.getByTestId("auto-close")).toHaveCount(0);
  await page.waitForTimeout(6000);
  expect(await wasClosed(page)).toBe(false);
});

test("off: the window waits for you", async ({ page }) => {
  await page.goto(`/?items=100${card}`);
  await expect(page.getByTestId("import")).toBeVisible();
  await expect(page.getByTestId("start-import")).toBeVisible();
  await page.waitForTimeout(1000);
  await expect(page.getByTestId("auto-banner")).toHaveCount(0);
  expect(await calls(page, "import_start")).toEqual([]);
});

test("a destination drive that is not connected stops it, with the reason", async ({ page }) => {
  await watchClose(page);
  await page.goto(`/?items=100&autoImport=Z${card}`);
  await expect(page.getByTestId("auto-note")).toContainText(
    "Not imported automatically: destination Z:\\Pictures does not exist",
  );
  expect(await calls(page, "import_start")).toEqual([]);
  // It stays open, so the reason can be read.
  await expect(page.getByTestId("auto-close")).toHaveCount(0);
  await page.waitForTimeout(6000);
  expect(await wasClosed(page)).toBe(false);
});

test("settings: turn it on, with where to copy and the folder layout", async ({ page }) => {
  await page.goto("/?items=100");
  await expect(page.getByTestId("count")).toHaveText("100 items");
  await page.keyboard.press("Control+,");
  const box = page.getByTestId("card-import");
  await box.getByTestId("import-auto").check();
  await page.getByTestId("settings-save").click();
  await expect(page.getByRole("alert")).toContainText("choose where cards are copied");
  await box.getByTestId("import-dest").fill("E:\\Pictures");
  await expect(box.getByTestId("import-layout")).toHaveValue(YMD);
  const now = new Date();
  const month = now.toLocaleString("en-US", { month: "long" });
  await expect(box.getByTestId("import-example")).toContainText(
    `E:\\Pictures\\${now.getFullYear()}\\${month}\\${now.getFullYear()}-`,
  );
  await page.getByTestId("settings-save").click();
  await expect(page.getByTestId("settings-saved")).toHaveText("Saved.");
  expect((await calls(page, "settings_set")).at(-1)).toMatchObject({
    update: { import: { auto: true, destination: "E:\\Pictures", template: YMD } },
  });
  // Another layout.
  await box.getByTestId("import-layout").selectOption({ label: "Year › Day" });
  await expect(box.getByTestId("import-template")).toHaveValue("{yyyy}/{yyyy}-{mm}-{dd}/{orig}");
});
