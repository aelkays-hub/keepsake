// The sidebar tree By aircraft (ADR-053): airline, type or registration,
// then year and month, from the aircraft keywords. Each node opens exactly
// its photos; the kind shown is remembered.

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
  // Ten KLM photos (two registrations), one undated KLM photo (item 499 has
  // no date), and one with only a built year: it names no airline.
  await page.evaluate(() => {
    const kw = window.__mock?.keywords;
    if (!kw) throw new Error("no mock");
    for (let i = 0; i < 10; i++)
      kw.set(i, [
        "Aircraft|Airline|KLM",
        "Aircraft|Type|Boeing 747",
        `Aircraft|Registration|${i < 6 ? "PH-BFA" : "PH-BVA"}`,
      ]);
    kw.set(499, ["Aircraft|Airline|KLM"]);
    kw.set(1500, ["Aircraft|Built|2014"]);
  });
});

test("by airline, then year and month: each node opens exactly its photos", async ({ page }) => {
  await page.getByTestId("tree-mode").getByRole("button", { name: "By aircraft" }).click();
  const kinds = page.getByTestId("tree-aircraft");
  await expect(kinds.getByRole("button", { name: "Airline" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(node(page, "Calendar")).toHaveCount(0);
  await expect(node(page, "KLM")).toHaveAttribute("aria-expanded", "false");
  expect(await countOf(page, "KLM")).toBe(11);
  expect(await countOf(page, "No airline named")).toBe(1);

  await node(page, "KLM").click();
  await expect(page.getByTestId("heading")).toHaveText("KLM");
  await expect(page.getByTestId("count")).toHaveText(items(11));
  await expect(node(page, "KLM")).toHaveAttribute("aria-current", "page");
  await expect(node(page, "KLM")).toHaveAttribute("aria-expanded", "true");

  // Its years: the dated ones first, then the undated photo.
  const years = page.getByTestId("sidebar").locator("button.depth-1");
  await expect(years.last()).toContainText("No date");
  const yearLabel = ((await years.first().locator(".label").textContent()) ?? "").trim();
  const yearCount = Number(
    ((await years.first().locator(".count").textContent()) ?? "").replace(/,/g, ""),
  );
  await years.first().click();
  await expect(page.getByTestId("heading")).toHaveText(`KLM › ${yearLabel}`);
  await expect(page.getByTestId("count")).toHaveText(items(yearCount));

  const month = page.getByTestId("sidebar").locator("button.depth-2").first();
  const monthLabel = ((await month.locator(".label").textContent()) ?? "").trim();
  const monthCount = Number(
    ((await month.locator(".count").textContent()) ?? "").replace(/,/g, ""),
  );
  expect(monthCount).toBeGreaterThan(0);
  await month.click();
  await expect(page.getByTestId("heading")).toHaveText(`KLM › ${yearLabel} › ${monthLabel}`);
  await expect(page.getByTestId("count")).toHaveText(items(monthCount));

  await years.last().click();
  await expect(page.getByTestId("count")).toHaveText(items(1));

  await node(page, "No airline named").click();
  await expect(page.getByTestId("count")).toHaveText(items(1));
});

test("by type or registration; the kind and open nodes are remembered", async ({ page }) => {
  await page.getByTestId("tree-mode").getByRole("button", { name: "By aircraft" }).click();
  const kinds = page.getByTestId("tree-aircraft");
  await kinds.getByRole("button", { name: "Registration" }).click();
  expect(await countOf(page, "PH-BFA")).toBe(6);
  expect(await countOf(page, "PH-BVA")).toBe(4);
  expect(await countOf(page, "No registration named")).toBe(2);
  await node(page, "PH-BVA").click();
  await expect(page.getByTestId("count")).toHaveText(items(4));

  await kinds.getByRole("button", { name: "Type" }).click();
  expect(await countOf(page, "Boeing 747")).toBe(10);
  await expect(node(page, "PH-BVA")).toHaveCount(0);

  // After a restart: the type tree again (the keywords are the mock's, so
  // they are given again).
  await page.reload();
  await expect(
    page.getByTestId("tree-aircraft").getByRole("button", { name: "Type" }),
  ).toHaveAttribute("aria-pressed", "true");
  // Back to Registration: PH-BVA is still open.
  await page.getByTestId("tree-aircraft").getByRole("button", { name: "Registration" }).click();
  await page.evaluate(() => window.__mock?.keywords.set(7, ["Aircraft|Registration|PH-BVA"]));
  await page.getByTestId("tree-mode").getByRole("button", { name: "By date" }).click();
  await expect(node(page, "Calendar")).toBeVisible();
  await expect(page.getByTestId("tree-aircraft")).toHaveCount(0);
  await page.getByTestId("tree-mode").getByRole("button", { name: "By aircraft" }).click();
  await expect(
    page.getByTestId("tree-aircraft").getByRole("button", { name: "Registration" }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(node(page, "PH-BVA")).toHaveAttribute("aria-expanded", "true");
});

test("with no aircraft keyword, the tree says how to get some", async ({ page }) => {
  await page.evaluate(() => window.__mock?.keywords.clear());
  await page.getByTestId("tree-mode").getByRole("button", { name: "By aircraft" }).click();
  await expect(page.getByTestId("tree-aircraft-empty")).toBeVisible();
});
