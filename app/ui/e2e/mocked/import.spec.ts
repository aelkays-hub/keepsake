// The import window against the mock backend.

import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.goto(`/?items=100#/import?mount=${encodeURIComponent("E:\\")}`);
  await expect(page.getByTestId("import")).toBeVisible();
});

test("lists new items grouped by day, with select all and none", async ({ page }) => {
  await expect(page.getByTestId("import-summary")).toContainText("11 new items since last import (12 on the card)");
  await expect(page.getByTestId("import-summary")).toContainText("11 selected");
  await expect(page.getByRole("heading", { name: /2024-06-15/ })).toBeVisible();
  await expect(page.getByRole("heading", { name: /2024-06-16/ })).toBeVisible();
  await expect(page.getByTestId("card-tile").first().locator("img")).toBeVisible();
  await page.getByTestId("select-none").click();
  await expect(page.getByTestId("import-summary")).toContainText("0 selected");
  await page.getByTestId("dest-a").fill("F:\\Photos");
  await page.getByTestId("dest-b").fill("G:\\Photos");
  await expect(page.getByTestId("check")).toHaveText("nothing selected");
  await expect(page.getByTestId("start-import")).toBeDisabled();
  await page.getByTestId("select-all").click();
  await expect(page.getByTestId("import-summary")).toContainText("12 selected");
});

test("a preset fills destinations; import shows progress and SAFE TO FORMAT", async ({ page }) => {
  await page.getByTestId("preset").selectOption("Two drives");
  await expect(page.getByTestId("dest-a")).toHaveValue("F:\\Photos");
  await expect(page.getByTestId("dest-b")).toHaveValue("G:\\Photos");
  await expect(page.getByTestId("check")).toContainText("needs");
  await expect(page.getByText("→ 2024/2024-06-15/DSC01001.JPG")).toBeVisible();
  await page.getByTestId("start-import").click();
  await expect(page.getByTestId("progress")).toBeVisible();
  await expect(page.getByTestId("result")).toContainText("SAFE TO FORMAT");
  await expect(page.getByTestId("result")).toContainText("11 assets");
});

test("one destination is never safe to format; same disk is refused", async ({ page }) => {
  await page.getByTestId("dest-a").fill("F:\\Photos");
  await page.getByTestId("dest-b").fill("F:\\Backup");
  await expect(page.getByTestId("check")).toContainText("same disk");
  await expect(page.getByTestId("start-import")).toBeDisabled();
  await page.getByTestId("use-b").uncheck();
  await expect(page.getByText("With one destination the card will not be safe to format.")).toBeVisible();
  await page.getByTestId("start-import").click();
  await expect(page.getByTestId("result")).toContainText("NOT safe to format");
  await expect(page.getByTestId("result")).toContainText("only one destination");
});
