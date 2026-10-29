// Automated accessibility checks (axe-core): every MVP screen, in the light
// and dark themes, against WCAG 2.1 A and AA: contrast, names and labels,
// roles, structure. Keyboard use is covered in keyboard.spec.ts.

import AxeBuilder from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";

const WCAG = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"];

async function check(page: Page, what: string) {
  const r = await new AxeBuilder({ page }).withTags(WCAG).analyze();
  const problems = r.violations.map(
    (v) =>
      `${what}: ${v.id} (${v.impact}) ${v.help}\n` +
      v.nodes
        .slice(0, 5)
        .map(
          (n) => `    ${n.target.join(" ")}: ${n.failureSummary?.split("\n").slice(1).join("; ")}`,
        )
        .join("\n"),
  );
  return problems;
}

async function open(page: Page, items = 2000, extra = "") {
  await page.goto(`/?items=${items}${extra}`);
  await expect(page.getByTestId("count")).toHaveText(`${items.toLocaleString("en-US")} items`);
  await expect(page.getByTestId("tile").first().locator("img")).toBeVisible();
}

for (const scheme of ["light", "dark"] as const) {
  test(`every screen passes axe (${scheme})`, async ({ page }) => {
    test.setTimeout(180_000);
    await page.emulateMedia({ colorScheme: scheme });
    const all: string[] = [];

    await page.goto("/?library=none&items=10");
    await expect(page.getByRole("heading", { name: "Welcome to Keepsake" })).toBeVisible();
    all.push(...(await check(page, "first run")));

    await open(page, 2000, "&crash=1");
    await expect(page.getByTestId("crash-banner")).toBeVisible();
    await page.getByTestId("tile").nth(3).click();
    await expect(page.getByTestId("inspector")).toBeVisible();
    all.push(...(await check(page, "grid, inspector and crash notice")));
    await page.getByTestId("crash-banner").getByRole("button", { name: "Dismiss" }).click();

    await page.keyboard.press("Control+3");
    await expect(page.getByTestId("drow").first()).toBeVisible();
    all.push(...(await check(page, "details")));

    await page.keyboard.press("Control+1");
    await expect(page.getByTestId("cal-cell").first()).toBeVisible();
    all.push(...(await check(page, "calendar years")));
    await page.getByTestId("cal-cell").first().click();
    await expect(page.getByRole("group", { name: "Months" })).toBeVisible();
    all.push(...(await check(page, "calendar months")));

    await page.keyboard.press("Control+2");
    await page.getByTestId("tile").first().dblclick();
    await expect(page.getByTestId("viewer-image")).toBeVisible();
    all.push(...(await check(page, "viewer (photo)")));
    await page.getByTestId("viewer").getByTestId("open-in").click();
    await expect(page.getByRole("menu", { name: "Open in" }).getByRole("menuitem")).toHaveCount(1);
    all.push(...(await check(page, "Open in menu")));
    await page.keyboard.press("Escape");
    await expect(page.getByRole("menu", { name: "Open in" })).toHaveCount(0);
    await page.keyboard.press("Escape");

    await page.getByTestId("type").selectOption("video");
    await expect(page.getByTestId("count")).not.toHaveText("2,000 items");
    await expect(page.locator(".tile:not(.offline) img").first()).toBeVisible();
    await page.locator(".tile:not(.offline)").first().dblclick();
    await expect(page.getByTestId("viewer-video")).toBeVisible();
    all.push(...(await check(page, "viewer (video)")));
    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: "Clear filters" }).click();

    await page.getByTestId("tile").first().click();
    await page
      .getByTestId("tile")
      .nth(2)
      .click({ modifiers: ["Shift"] });
    await page.keyboard.press("Control+t");
    await expect(page.getByTestId("ts-preview")).toBeVisible();
    all.push(...(await check(page, "time shift")));
    await page.keyboard.press("Escape");

    await page.getByTestId("sidebar").getByRole("button", { name: "Backup" }).click();
    await page.getByTestId("backup-dest").fill("E:\\Backup");
    await page.getByTestId("backup-start").click();
    await expect(page.getByTestId("backup-outcome")).toContainText("Backup finished");
    all.push(...(await check(page, "backup")));

    await page.getByTestId("sidebar").getByRole("button", { name: "Folders" }).click();
    await page.getByTestId("folder-path").fill("E:\\Pictures\\2025");
    await page.getByTestId("folder-add").click();
    await expect(page.getByTestId("folder-outcome")).toContainText("Added 2025");
    all.push(...(await check(page, "folders")));

    await page.getByTestId("sidebar").getByRole("button", { name: "Settings" }).click();
    await page
      .getByTestId("copy-report")
      .click()
      .catch(() => undefined);
    await expect(page.getByTestId("settings-screen")).toBeVisible();
    all.push(...(await check(page, "settings")));

    await page.goto(`/?items=100#/import?mount=${encodeURIComponent("E:\\")}`);
    await expect(page.getByTestId("card-tile").first().locator("img")).toBeVisible();
    all.push(...(await check(page, "import window")));

    await page.goto("/?items=500&broken=details");
    await page.getByTestId("tile").first().click();
    await expect(page.getByRole("heading", { name: "Something went wrong" })).toBeVisible();
    all.push(...(await check(page, "error screen")));

    expect(all, all.join("\n")).toEqual([]);
  });
}
