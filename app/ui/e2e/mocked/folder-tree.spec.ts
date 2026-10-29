// The sidebar tree by folder: volumes, then their folders as on disk; a
// folder shows everything in it and below. The mock's layout is
// `<year>/<year-month>` on "Photos A" (connected items) and the archive.

import { expect, test, type Page } from "@playwright/test";

const calls = (page: Page, cmd: string) =>
  page.evaluate(
    (c) => (window.__mock?.calls ?? []).filter((x) => x.cmd === c).map((x) => x.args),
    cmd,
  );

const node = (page: Page, label: string | RegExp) =>
  page.getByTestId("sidebar").locator("button.cal-node").filter({ hasText: label });

/** A folder node by its path on the volume. */
const folder = (page: Page, path: string) =>
  page.getByTestId("sidebar").locator(`button[data-folder="${path}"]`).first();

const countOf = async (page: Page, path: string) =>
  Number(((await folder(page, path).locator(".count").textContent()) ?? "").replace(/,/g, ""));

test.beforeEach(async ({ page }) => {
  await page.goto("/?items=2000");
  await expect(page.getByTestId("count")).toHaveText("2,000 items");
});

test("switch to folders: volumes, then their folders; a folder shows all below", async ({
  page,
}) => {
  const sidebar = page.getByTestId("sidebar");
  const modes = page.getByTestId("tree-mode");
  await expect(modes.getByRole("button", { name: "By date" })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await expect(sidebar.getByRole("heading", { name: "Volumes" })).toBeVisible();
  await modes.getByRole("button", { name: "By folder" }).click();
  // The volumes are the tree's roots now.
  await expect(sidebar.getByRole("heading", { name: "Volumes" })).toHaveCount(0);
  await expect(node(page, "Photos A")).toBeVisible();
  await expect(node(page, "Archive (unplugged)")).toBeVisible();
  await expect(node(page, "Calendar")).toHaveCount(0);

  await node(page, "Photos A").click();
  await expect(page.getByTestId("heading")).toHaveText("Photos A");
  await expect(node(page, "Photos A")).toHaveAttribute("aria-expanded", "true");
  const year = folder(page, "2016");
  await expect(year).toBeVisible();
  const n = await countOf(page, "2016");
  expect(n).toBeGreaterThan(0);

  await year.click();
  await expect(page.getByTestId("heading")).toHaveText("Photos A › 2016");
  await expect(page.getByTestId("count")).toHaveText(`${n.toLocaleString("en-US")} items`);
  await expect
    .poll(async () => (await calls(page, "view_open")).at(-1))
    .toMatchObject({
      query: { volume: "vOn", folder: "2016" },
    });
  // Its months, one level down.
  const month = folder(page, "2016/2016-03");
  await expect(month).toBeVisible();
  await month.click();
  await expect(page.getByTestId("heading")).toHaveText("Photos A › 2016 › 2016-03");
  await expect(month).toHaveAttribute("aria-current", "page");

  // Filters change the counts.
  await page.getByTestId("type").selectOption("video");
  await expect.poll(() => countOf(page, "2016")).toBeLessThan(n);

  // The choice is kept.
  await page.reload();
  await expect(page.getByTestId("count")).toHaveText("2,000 items");
  await expect(
    page.getByTestId("tree-mode").getByRole("button", { name: "By folder" }),
  ).toHaveAttribute("aria-pressed", "true");
  await page.getByTestId("tree-mode").getByRole("button", { name: "By date" }).click();
  await expect(node(page, "Calendar")).toBeVisible();
  await expect(sidebar.getByRole("heading", { name: "Volumes" })).toBeVisible();
});

test("folders with the keyboard: right opens, left closes and goes up", async ({ page }) => {
  await page.getByTestId("tree-mode").getByRole("button", { name: "By folder" }).click();
  const root = node(page, "Photos A");
  await root.focus();
  await page.keyboard.press("ArrowRight");
  await expect(root).toHaveAttribute("aria-expanded", "true");
  await page.keyboard.press("ArrowRight");
  await expect(page.locator(":focus")).toHaveAttribute("data-folder", /^\d{4}$/);
  const first = await page.locator(":focus").getAttribute("data-folder");
  await page.keyboard.press("ArrowRight");
  await expect(folder(page, `${first}/${first}-01`)).toBeVisible();
  await page.keyboard.press("ArrowLeft");
  await expect(page.locator(`button[data-folder="${first}/${first}-01"]`)).toHaveCount(0);
  await page.keyboard.press("ArrowLeft");
  await expect(root).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("heading")).toHaveText("Photos A");
});
