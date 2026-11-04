// Start-up: while the library opens (a catalog upgrade can take a minute on a
// slow drive) the window shows that it is opening instead of staying blank.

import { expect, test } from "@playwright/test";

test("a slow library open shows Opening, then a note, then the library", async ({ page }) => {
  await page.goto("/?items=100&opening=5000");
  await expect(page.getByTestId("opening")).toContainText("Opening your library");
  await expect(page.getByTestId("opening-slow")).toHaveCount(0);
  await expect(page.getByTestId("opening-slow")).toContainText("catalog is upgraded", {
    timeout: 4500,
  });
  await expect(page.getByTestId("count")).toHaveText("100 items", { timeout: 10000 });
  await expect(page.getByTestId("opening")).toHaveCount(0);
});

test("a quick open goes straight to the library", async ({ page }) => {
  await page.goto("/?items=100");
  await expect(page.getByTestId("count")).toHaveText("100 items");
  await expect(page.getByTestId("opening")).toHaveCount(0);
});
