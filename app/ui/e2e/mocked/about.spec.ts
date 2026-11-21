// Tools → About: the version, the credits, and the licences (ADR-047).

import { expect, test, type Page } from "@playwright/test";

const calls = (page: Page, cmd: string) =>
  page.evaluate(
    (c) => (window.__mock?.calls ?? []).filter((x) => x.cmd === c).map((x) => x.args),
    cmd,
  );

async function openAbout(page: Page) {
  await page.goto("/?items=50");
  await expect(page.getByTestId("count")).toHaveText("50 items");
  await page.getByTestId("sidebar").getByRole("button", { name: "About" }).click();
  await expect(page.getByTestId("about-screen")).toBeVisible();
}

test("About shows the version, the credits and the licences", async ({ page }) => {
  await openAbout(page);
  await expect(page.getByTestId("about-version")).toContainText(/Keepsake \d+\.\d+\.\d+-mock\./);
  const credits = page.getByTestId("about-credits");
  await expect(credits).toContainText("not endorsed or certified by SmugMug");
  await expect(credits).toContainText("Open Government Licence – Canada");
  await credits.getByRole("button", { name: "Open its page" }).first().click();
  expect(await calls(page, "about_open")).toEqual([
    { url: "https://www.flickr.com/services/api/" },
  ]);

  await expect(page.getByTestId("licence-programs")).toContainText("FFmpeg and ffprobe");
  const licences = page.getByTestId("licences");
  await expect(
    licences.getByRole("heading", { name: "Built into Keepsake (3 packages)" }),
  ).toBeVisible();
  const rows = page.getByTestId("licence-row");
  await expect(rows).toHaveCount(3);
  await expect(rows.first()).toContainText("rusqlite");
  await expect(rows.first()).toContainText("Rust");
  await expect(rows.nth(2)).toContainText("JavaScript");
});

test("a package's licence opens, and Find narrows the list", async ({ page }) => {
  await openAbout(page);
  const rows = page.getByTestId("licence-row");
  await rows.nth(1).getByTestId("licence-show").click();
  const text = page.getByTestId("licence-text-row");
  await expect(text).toContainText("standard text of its licence");
  await expect(text.getByTestId("licence-text")).toContainText("the authors of ts-rs");
  await expect(text).toContainText("Source: https://github.com/Aleph-Alpha/ts-rs");
  await rows.nth(1).getByTestId("licence-show").click();
  await expect(text).toHaveCount(0);

  await page.getByTestId("licence-find").fill("REACT");
  await expect(rows).toHaveCount(1);
  await expect(rows.first()).toContainText("react");
  await page.getByTestId("licence-find").fill("gpl");
  await expect(page.getByTestId("licence-none")).toHaveText("Nothing matches “gpl”.");
});
