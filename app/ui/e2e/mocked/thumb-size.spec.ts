// Small, Medium and Large thumbnails in the grid: the toolbar switch, the
// + and − keys, the 1024 px thumbnail for Large, and remembering the choice.

import { expect, test, type Page } from "@playwright/test";

async function openLibrary(page: Page, items = 2000) {
  await page.goto(`/?items=${items}`);
  await expect(page.getByTestId("count")).toHaveText(`${items.toLocaleString("en-US")} items`);
  await expect(page.getByTestId("tile").first()).toBeVisible();
}

const tileWidth = (page: Page) =>
  page
    .getByTestId("tile")
    .first()
    .evaluate((e) => Math.round(e.getBoundingClientRect().width));

/** Tiles in the first row. */
const perRow = (page: Page) => page.locator(".grid-row").first().getByTestId("tile").count();

test("the switch sets Small, Medium and Large, and it is remembered", async ({ page }) => {
  await openLibrary(page);
  const grid = page.getByTestId("grid");
  const sizes = page.getByTestId("thumb-size");
  // Medium to begin with.
  await expect(grid).toHaveAttribute("data-size", "medium");
  await expect(sizes.getByRole("button", { name: "Medium thumbnails" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  expect(await tileWidth(page)).toBe(176);
  const medium = await perRow(page);

  await page.getByTestId("thumb-large").click();
  await expect(grid).toHaveAttribute("data-size", "large");
  await expect.poll(() => tileWidth(page)).toBe(280);
  expect(await perRow(page)).toBeLessThan(medium);
  // Large tiles use the 1024 px thumbnail.
  await expect(page.getByTestId("tile").first().locator("img")).toHaveAttribute("src", /#large$/);

  await page.getByTestId("thumb-small").click();
  await expect.poll(() => tileWidth(page)).toBe(120);
  expect(await perRow(page)).toBeGreaterThan(medium);
  await expect(page.getByTestId("tile").first().locator("img")).not.toHaveAttribute(
    "src",
    /#large$/,
  );

  // Remembered after a restart.
  await page.reload();
  await expect(page.getByTestId("tile").first()).toBeVisible();
  await expect(grid).toHaveAttribute("data-size", "small");
  expect(await tileWidth(page)).toBe(120);
});

test("+ and − in the grid change the size, and the focused item stays in view", async ({
  page,
}) => {
  await openLibrary(page);
  await page.getByTestId("tile").first().click();
  // Go well down the grid.
  for (let i = 0; i < 5; i++) await page.keyboard.press("PageDown");
  const focused = page.locator(".tile.focused");
  const index = await focused.getAttribute("data-index");
  await page.keyboard.press("+");
  await expect(page.getByTestId("grid")).toHaveAttribute("data-size", "large");
  await page.keyboard.press("+");
  // Stays at Large.
  await expect(page.getByTestId("grid")).toHaveAttribute("data-size", "large");
  await expect(page.locator(`.tile.focused[data-index="${index}"]`)).toBeInViewport();
  await page.keyboard.press("-");
  await page.keyboard.press("-");
  await expect(page.getByTestId("grid")).toHaveAttribute("data-size", "small");
  await expect(page.locator(`.tile.focused[data-index="${index}"]`)).toBeInViewport();
  // + and − did not rate or move anything.
  await expect(page.locator(".tile.focused")).toHaveAttribute("data-index", String(index));
});

test("the switch is only in the grid", async ({ page }) => {
  await openLibrary(page);
  await expect(page.getByTestId("thumb-size")).toBeVisible();
  await page.keyboard.press("Control+3");
  await expect(page.getByTestId("thumb-size")).toHaveCount(0);
  await page.keyboard.press("Control+1");
  await expect(page.getByTestId("thumb-size")).toHaveCount(0);
});
