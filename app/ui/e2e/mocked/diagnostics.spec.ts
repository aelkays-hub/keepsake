// Local error reporting: the diagnostic report, the crash notice, the
// error screen, and UI errors reaching the log. Nothing is sent anywhere.

import { expect, test, type Page } from "@playwright/test";

const calls = (page: Page, cmd: string) =>
  page.evaluate((c) => (window.__mock?.calls ?? []).filter((x) => x.cmd === c).map((x) => x.args), cmd);

test.beforeEach(async ({ context }) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
});

async function open(page: Page, query = "") {
  await page.goto(`/?items=500${query}`);
  await expect(page.getByTestId("count")).toHaveText("500 items");
}

test("Settings copies the diagnostic report to the clipboard", async ({ page }) => {
  await open(page);
  await page.keyboard.press("Control+,");
  const card = page.getByTestId("diagnostics");
  await card.getByTestId("copy-report").click();
  await expect(card.getByTestId("diagnostics-status")).toHaveText("Copied to the clipboard.");
  const clip = await page.evaluate(() => navigator.clipboard.readText());
  expect(clip).toContain("Keepsake diagnostic report");
  expect(clip).toContain("keepsake-app");
  await card.getByText("What was copied").click();
  await expect(card.getByTestId("report-text")).toContainText("Tools:");
  await card.getByRole("button", { name: "Open log folder" }).click();
  await expect(card.getByTestId("diagnostics-status")).toContainText("logs");
});

test("after a crash, a notice offers the report once", async ({ page }) => {
  await open(page, "&crash=1");
  const banner = page.getByTestId("crash-banner");
  await expect(banner).toContainText("Keepsake closed unexpectedly last time.");
  await expect(banner).toContainText("crash-20261025-221500-4242.txt");
  await banner.getByRole("button", { name: "Copy diagnostic report" }).click();
  await expect(banner.getByRole("status")).toContainText("Copied to the clipboard");
  await banner.getByRole("button", { name: "Dismiss" }).click();
  await expect(banner).toHaveCount(0);
  expect(await calls(page, "crash_dismiss")).toHaveLength(1);
});

test("a UI failure shows the error screen and is logged", async ({ page }) => {
  await open(page, "&broken=details");
  await page.getByTestId("tile").first().click();
  await expect(page.getByRole("heading", { name: "Something went wrong" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "Something went wrong" })).toBeFocused();
  await expect(page.getByRole("button", { name: "Reload" })).toBeVisible();
  await page.getByRole("button", { name: "Copy diagnostic report" }).click();
  await expect(page.getByRole("status")).toHaveText("Copied to the clipboard.");
  const logged = await calls(page, "log_ui_error");
  expect(logged.some((l) => String((l as { message: string }).message).startsWith("render:"))).toBe(true);
});

test("uncaught errors and failed promises go to the local log", async ({ page }) => {
  await open(page);
  await page.evaluate(() => {
    setTimeout(() => {
      throw new Error("boom from a timer");
    });
    void Promise.reject(new Error("nobody caught this"));
  });
  await expect
    .poll(async () => (await calls(page, "log_ui_error")).map((l) => (l as { message: string }).message))
    .toEqual(expect.arrayContaining(["uncaught: boom from a timer", "unhandled promise: nobody caught this"]));
});
