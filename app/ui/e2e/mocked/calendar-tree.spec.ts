// The calendar as a tree in the sidebar: years, months, days.

import { expect, test, type Page } from "@playwright/test";

const node = (page: Page, key: string) =>
  page.getByTestId("sidebar").locator(`button[data-cal="${key}"]`);
const children = (page: Page, prefix: string) =>
  page.getByTestId("sidebar").locator(`button[data-cal^="${prefix}-"]`);

async function openLibrary(page: Page) {
  await page.goto("/?items=2000");
  await expect(page.getByTestId("count")).toHaveText("2,000 items");
}

test("years show under Calendar; a year opens its months, a month its days", async ({ page }) => {
  await openLibrary(page);
  await expect(node(page, "cal")).toHaveAttribute("aria-expanded", "true");
  // The mock spans 2015 to 2024, plus items without a date.
  for (const y of ["2015", "2020", "2024"]) await expect(node(page, y)).toBeVisible();
  await expect(node(page, "undated")).toContainText("No date");
  await expect(node(page, "2020")).toHaveAttribute("aria-expanded", "false");
  await expect(children(page, "2020")).toHaveCount(0);

  // A year: the calendar shows its months, and the tree opens it.
  await node(page, "2020").click();
  await expect(page.getByRole("group", { name: "Months" })).toBeVisible();
  await expect(node(page, "2020")).toHaveAttribute("aria-current", "page");
  await expect(node(page, "2020")).toHaveAttribute("aria-expanded", "true");
  const months = children(page, "2020");
  await expect(months.first()).toBeVisible();
  const month = (await months.first().getAttribute("data-cal")) ?? "";
  expect(month).toMatch(/^2020-\d\d$/);

  // A month: its days.
  await node(page, month).click();
  await expect(page.getByRole("group", { name: "Days" })).toBeVisible();
  const days = page.getByTestId("sidebar").locator(`button[data-cal^="${month}-"]`);
  await expect(days.first()).toBeVisible();
  const day = (await days.first().getAttribute("data-cal")) ?? "";
  expect(day).toMatch(/^2020-\d\d-\d\d$/);
  await expect(node(page, day)).not.toHaveAttribute("aria-expanded");

  // A day: its items in the grid.
  await node(page, day).click();
  await expect(page.getByTestId("grid")).toBeVisible();
  await expect(page.getByTestId("heading")).toContainText(`› ${Number(day.slice(8))}`);
  await expect(node(page, day)).toHaveAttribute("aria-current", "page");
  const n = Number((await node(page, day).locator(".count").textContent()) ?? "0");
  await expect(page.getByTestId("count")).toHaveText(`${n} item${n === 1 ? "" : "s"}`);
});

test("the arrow only opens and closes; the tree follows the calendar view", async ({ page }) => {
  await openLibrary(page);
  await node(page, "2018").locator(".twisty").click();
  await expect(node(page, "2018")).toHaveAttribute("aria-expanded", "true");
  await expect(children(page, "2018").first()).toBeVisible();
  // Still the grid of all items: the arrow opened nothing.
  await expect(page.getByTestId("grid")).toBeVisible();
  await node(page, "2018").locator(".twisty").click();
  await expect(children(page, "2018")).toHaveCount(0);

  // Moving in the calendar view opens the tree to match.
  await page.keyboard.press("Control+1");
  await page.getByTestId("cal-cell").filter({ hasText: "2022" }).click();
  await expect(page.getByRole("group", { name: "Months" })).toBeVisible();
  await expect(node(page, "2022")).toHaveAttribute("aria-expanded", "true");
  await expect(node(page, "2022")).toHaveAttribute("aria-current", "page");
});

test("with the keyboard: Right opens or goes in, Left goes up or closes", async ({ page }) => {
  await openLibrary(page);
  await node(page, "2021").focus();
  await page.keyboard.press("ArrowRight");
  await expect(node(page, "2021")).toHaveAttribute("aria-expanded", "true");
  await expect(children(page, "2021").first()).toBeVisible();
  await page.keyboard.press("ArrowRight");
  await expect(children(page, "2021").first()).toBeFocused();
  await page.keyboard.press("ArrowLeft");
  await expect(node(page, "2021")).toBeFocused();
  await page.keyboard.press("ArrowLeft");
  await expect(node(page, "2021")).toHaveAttribute("aria-expanded", "false");
  await page.keyboard.press("Enter");
  await expect(page.getByRole("group", { name: "Months" })).toBeVisible();
});

test("No date shows the items without a capture date", async ({ page }) => {
  await openLibrary(page);
  const n = Number((await node(page, "undated").locator(".count").textContent()) ?? "0");
  expect(n).toBeGreaterThan(0);
  await node(page, "undated").click();
  await expect(page.getByTestId("count")).toHaveText(`${n} item${n === 1 ? "" : "s"}`);
});
