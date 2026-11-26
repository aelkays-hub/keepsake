// The sidebar tree By face (ADR-053): each person named in the photos,
// then the photos with a face that has no name, by year and month. Each
// node opens exactly its photos; the tree shown is remembered.

import { expect, test, type Page } from "@playwright/test";

const node = (page: Page, label: string | RegExp) =>
  page.getByTestId("sidebar").locator("button.cal-node").filter({ hasText: label });

const countOf = async (page: Page, label: string | RegExp) =>
  Number(
    ((await node(page, label).first().locator(".count").textContent()) ?? "").replace(/,/g, ""),
  );

const items = (n: number) => `${n.toLocaleString("en-US")} item${n === 1 ? "" : "s"}`;

test.beforeEach(async ({ page }) => {
  await page.goto("/?items=2000");
  await expect(page.getByTestId("count")).toHaveText("2,000 items");
  // Sara in six photos, Tom in three (one with Sara, one undated: item 499
  // has no date), and four photos with an unnamed face (one also Sara's).
  await page.evaluate(() => {
    const m = window.__mock;
    if (!m) throw new Error("no mock");
    for (let i = 0; i < 6; i++) m.keywords.set(i, ["People|Sara"]);
    m.keywords.set(5, ["People|Sara", "People|Tom"]);
    m.keywords.set(6, ["People|Tom"]);
    m.keywords.set(499, ["People|Tom"]);
    for (const i of [0, 20, 21, 22]) m.unnamedFaces.add(i);
  });
});

test("by face, then year and month: each node opens exactly its photos", async ({ page }) => {
  await page.getByTestId("tree-mode").getByRole("button", { name: "By face" }).click();
  await expect(
    page.getByTestId("tree-mode").getByRole("button", { name: "By face" }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(page.getByTestId("tree-aircraft")).toHaveCount(0);
  await expect(node(page, "Calendar")).toHaveCount(0);
  expect(await countOf(page, "Sara")).toBe(6);
  expect(await countOf(page, "Tom")).toBe(3);
  expect(await countOf(page, "Unnamed faces")).toBe(4);
  // The unnamed faces come last.
  await expect(page.getByTestId("sidebar").locator("button.depth-0").last()).toContainText(
    "Unnamed faces",
  );

  await node(page, "Tom").click();
  await expect(page.getByTestId("heading")).toHaveText("Tom");
  await expect(page.getByTestId("count")).toHaveText(items(3));
  await expect(node(page, "Tom")).toHaveAttribute("aria-current", "page");
  const years = page.getByTestId("sidebar").locator("button.depth-1");
  await expect(years.last()).toContainText("No date");
  await years.last().click();
  await expect(page.getByTestId("heading")).toHaveText("Tom › No date");
  await expect(page.getByTestId("count")).toHaveText(items(1));

  await node(page, "Unnamed faces").click();
  await expect(page.getByTestId("heading")).toHaveText("Unnamed faces");
  await expect(page.getByTestId("count")).toHaveText(items(4));
  // Its first year (Tom's years are still open above it).
  const year = node(page, "Unnamed faces").locator(
    "xpath=following::button[contains(@class, 'depth-1')][1]",
  );
  const yearLabel = ((await year.locator(".label").textContent()) ?? "").trim();
  const yearCount = Number(((await year.locator(".count").textContent()) ?? "").replace(/,/g, ""));
  await year.click();
  await expect(page.getByTestId("heading")).toHaveText(`Unnamed faces › ${yearLabel}`);
  await expect(page.getByTestId("count")).toHaveText(items(yearCount));
  const month = year.locator("xpath=following::button[contains(@class, 'depth-2')][1]");
  const monthCount = Number(
    ((await month.locator(".count").textContent()) ?? "").replace(/,/g, ""),
  );
  await month.click();
  await expect(page.getByTestId("count")).toHaveText(items(monthCount));
});

test("the face tree is remembered after a restart", async ({ page }) => {
  await page.getByTestId("tree-mode").getByRole("button", { name: "By face" }).click();
  await node(page, "Sara").click();
  await expect(node(page, "Sara")).toHaveAttribute("aria-expanded", "true");
  await page.reload();
  await expect(
    page.getByTestId("tree-mode").getByRole("button", { name: "By face" }),
  ).toHaveAttribute("aria-pressed", "true");
  // The people are the mock's, so they are given again.
  await page.evaluate(() => window.__mock?.keywords.set(1, ["People|Sara"]));
  await page.getByTestId("tree-mode").getByRole("button", { name: "By date" }).click();
  await expect(node(page, "Calendar")).toBeVisible();
  await page.getByTestId("tree-mode").getByRole("button", { name: "By face" }).click();
  await expect(node(page, "Sara")).toHaveAttribute("aria-expanded", "true");
});

test("with no face named or found, the tree says how to get some", async ({ page }) => {
  await page.evaluate(() => {
    window.__mock?.keywords.clear();
    window.__mock?.unnamedFaces.clear();
  });
  await page.getByTestId("tree-mode").getByRole("button", { name: "By face" }).click();
  await expect(page.getByTestId("tree-face-empty")).toBeVisible();
});
