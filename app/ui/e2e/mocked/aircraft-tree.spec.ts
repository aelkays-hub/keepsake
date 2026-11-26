// The sidebar tree By aircraft (ADR-053): airline › type › registration,
// the aircraft report's drill-down, each step A to Z with the unnamed
// last. Each line opens its photos as the report's lines do; the open
// lines are remembered.

import { expect, test, type Page } from "@playwright/test";

const tree = (page: Page) => page.getByTestId("sidebar").locator("button.cal-node");
const line = (page: Page, label: string) => tree(page).filter({ hasText: label });
const labels = (page: Page, depth: number) =>
  page.getByTestId("sidebar").locator(`button.cal-node.depth-${depth} .label`);
const opened = (page: Page) =>
  page.evaluate(() =>
    JSON.stringify((window.__mock?.calls ?? []).filter((c) => c.cmd === "view_open").at(-1)),
  );

test.beforeEach(async ({ page }) => {
  await page.goto("/?items=200");
  await expect(page.getByTestId("count")).toHaveText("200 items");
});

test("airline › type › registration, A to Z, each line opens its photos", async ({ page }) => {
  await page.getByTestId("tree-mode").getByRole("button", { name: "By aircraft" }).click();
  await expect(
    page.getByTestId("tree-mode").getByRole("button", { name: "By aircraft" }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(line(page, "Calendar")).toHaveCount(0);
  // The report lists them by photos (Air Canada, KLM, Interjet); here A to Z.
  await expect(labels(page, 0)).toHaveText(["Air Canada", "Interjet", "KLM"]);
  await expect(line(page, "Air Canada").locator(".count")).toHaveText("20");

  await line(page, "Air Canada").click();
  await expect(page.getByTestId("heading")).toHaveText("Air Canada");
  await expect(line(page, "Air Canada")).toHaveAttribute("aria-current", "page");
  await expect(line(page, "Air Canada")).toHaveAttribute("aria-expanded", "true");
  await expect.poll(() => opened(page)).toContain('"tags":["Aircraft|Airline|Air Canada"]');
  // Its types A to Z, the photos naming no type last.
  await expect(labels(page, 1)).toHaveText(["Boeing 777-300ER", "Boeing 787-9", "No type named"]);

  await line(page, "No type named").click();
  await expect(page.getByTestId("heading")).toHaveText("Air Canada › No type named");
  await expect.poll(() => opened(page)).toContain('"withoutTags":["Aircraft|Type"]');

  await line(page, "Boeing 787-9").click();
  // Boeing 787-9's registrations, then No type named's (opened above).
  await expect(labels(page, 2)).toHaveText([
    "C-FVLU",
    "No registration named",
    "No registration named",
  ]);
  await line(page, "C-FVLU").click();
  await expect(page.getByTestId("heading")).toHaveText("Air Canada › Boeing 787-9 › C-FVLU");
  await expect
    .poll(() => opened(page))
    .toContain(
      JSON.stringify([
        "Aircraft|Airline|Air Canada",
        "Aircraft|Type|Boeing 787-9",
        "Aircraft|Registration|C-FVLU",
      ]),
    );
});

test("a registration opens to its years and a year to its months", async ({ page }) => {
  // C-FVLU on ten photos (the mock report is fixed; the calendar counts
  // the photos with these keywords).
  await page.evaluate(() => {
    const kw = [
      "Aircraft|Airline|Air Canada",
      "Aircraft|Type|Boeing 787-9",
      "Aircraft|Registration|C-FVLU",
    ];
    for (const i of [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]) window.__mock?.keywords.set(i, kw);
  });
  await page.getByTestId("tree-mode").getByRole("button", { name: "By aircraft" }).click();
  await line(page, "Air Canada").click();
  await line(page, "Boeing 787-9").click();
  await line(page, "C-FVLU").click();
  await expect(line(page, "C-FVLU")).toHaveAttribute("aria-expanded", "true");
  // Its years; together its ten photos.
  await expect(labels(page, 3).first()).toBeVisible();
  const counts = await page
    .getByTestId("sidebar")
    .locator("button.cal-node.depth-3 .count")
    .allTextContents();
  expect(counts.map(Number).reduce((a, b) => a + b, 0)).toBe(10);

  const year = page.getByTestId("sidebar").locator("button.cal-node.depth-3").first();
  const yearLabel = ((await year.locator(".label").textContent()) ?? "").trim();
  const yearCount = Number(await year.locator(".count").textContent());
  await year.click();
  await expect(page.getByTestId("heading")).toHaveText(
    `Air Canada › Boeing 787-9 › C-FVLU › ${yearLabel}`,
  );
  await expect(page.getByTestId("count")).toHaveText(
    `${yearCount} item${yearCount === 1 ? "" : "s"}`,
  );
  const month = page.getByTestId("sidebar").locator("button.cal-node.depth-4").first();
  const monthLabel = ((await month.locator(".label").textContent()) ?? "").trim();
  const monthCount = Number(await month.locator(".count").textContent());
  await month.click();
  await expect(page.getByTestId("heading")).toHaveText(
    `Air Canada › Boeing 787-9 › C-FVLU › ${yearLabel} › ${monthLabel}`,
  );
  await expect(page.getByTestId("count")).toHaveText(
    `${monthCount} item${monthCount === 1 ? "" : "s"}`,
  );
});

test("the open lines are remembered after a restart", async ({ page }) => {
  await page.getByTestId("tree-mode").getByRole("button", { name: "By aircraft" }).click();
  await line(page, "KLM").click();
  await expect(labels(page, 1)).toHaveText(["Boeing 747"]);
  await page.reload();
  await expect(
    page.getByTestId("tree-mode").getByRole("button", { name: "By aircraft" }),
  ).toHaveAttribute("aria-pressed", "true");
  await expect(line(page, "KLM")).toHaveAttribute("aria-expanded", "true");
  await expect(labels(page, 1)).toHaveText(["Boeing 747"]);
});

test("with no aircraft keyword, the tree says how to get some", async ({ page }) => {
  await page.evaluate(() => {
    if (window.__mock) window.__mock.noAircraft = true;
  });
  await page.getByTestId("tree-mode").getByRole("button", { name: "By aircraft" }).click();
  await expect(page.getByTestId("tree-aircraft-empty")).toBeVisible();
});
