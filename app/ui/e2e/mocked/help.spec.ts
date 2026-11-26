// Tools → Help (how-to topics, shortcuts, get help) and the version in Settings.

import { expect, test, type Page } from "@playwright/test";

const calls = (page: Page, cmd: string) =>
  page.evaluate(
    (c) => (window.__mock?.calls ?? []).filter((x) => x.cmd === c).map((x) => x.args),
    cmd,
  );

async function open(page: Page, name: string) {
  await page.goto("/?items=50");
  await expect(page.getByTestId("count")).toHaveText("50 items");
  await page.getByTestId("sidebar").getByRole("button", { name, exact: true }).click();
}

test("Help comes right after Settings in Tools", async ({ page }) => {
  await page.goto("/?items=50");
  await expect(page.getByTestId("count")).toHaveText("50 items");
  const names = await page
    .getByTestId("sidebar")
    .getByRole("button")
    .evaluateAll((bs) => bs.map((b) => (b.textContent ?? "").trim()));
  const i = names.findIndex((n) => n.startsWith("Settings"));
  expect(i).toBeGreaterThanOrEqual(0);
  expect(names[i + 1]).toMatch(/^Help/);
});

test("Help finds a topic, opens its steps, lists the shortcuts and copies the report", async ({
  page,
}) => {
  await open(page, "Help");
  await expect(page.getByTestId("help-screen")).toBeVisible();
  await expect(page.getByTestId("help-version")).toHaveText(/^Keepsake \d+\.\d+\.\d+-mock$/);
  const topics = page.getByTestId("help-topic");
  expect(await topics.count()).toBeGreaterThan(10);

  await page.getByTestId("help-find").fill("time");
  await expect(topics.first()).toContainText("Fix the time");
  await expect(topics.first()).toContainText("Ctrl+T");

  await page.getByTestId("help-find").fill("no such topic");
  await expect(topics).toHaveCount(0);
  await expect(page.getByTestId("help-none")).toBeVisible();

  await expect(page.getByTestId("help-shortcuts")).toContainText("Ctrl+F");
  await page.getByTestId("help-copy-report").click();
  await expect(page.getByTestId("help-get").getByRole("status")).toHaveText(
    "Copied to the clipboard.",
  );
  expect((await calls(page, "diagnostic_report")).length).toBe(1);
});

test("Settings shows the version", async ({ page }) => {
  await open(page, "Settings");
  await expect(page.getByTestId("settings-version")).toHaveText(/^Keepsake \d+\.\d+\.\d+-mock$/);
});
