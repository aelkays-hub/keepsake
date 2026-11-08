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

test("right-click a folder: Scan again scans that folder and shows the progress", async ({
  page,
}) => {
  await page.getByTestId("tree-mode").getByRole("button", { name: "By folder" }).click();
  await node(page, "Photos A").click();
  const year = folder(page, "2016");
  await expect(year).toBeVisible();

  // Not on other entries.
  await page.getByTestId("sidebar").getByRole("button", { name: "All items" }).click({
    button: "right",
  });
  await expect(page.getByTestId("folder-menu")).toHaveCount(0);

  await year.click({ button: "right" });
  const menu = page.getByTestId("folder-menu");
  await expect(menu).toBeVisible();
  await expect(menu.getByRole("menuitem", { name: "Show" })).toBeFocused();
  await menu.getByRole("menuitem", { name: "Scan again" }).click();
  await expect(menu).toHaveCount(0);
  await expect
    .poll(async () => (await calls(page, "folder_rescan")).at(-1))
    .toEqual({ volume: "vOn", path: "2016" });
  // The Folders screen shows it running, then what was found.
  await expect(page.getByTestId("folders-screen")).toBeVisible();
  await expect(page.getByTestId("folder-outcome")).toContainText("Scanned Photos A: 12 items");

  // The volume itself: the whole folder.
  await page.getByTestId("tree-mode").getByRole("button", { name: "By folder" }).click();
  await node(page, "Photos A").click({ button: "right" });
  await page.getByTestId("folder-menu-rescan").click();
  await expect
    .poll(async () => (await calls(page, "folder_rescan")).at(-1))
    .toEqual({ volume: "vOn", path: null });
});

test("the folder menu from the keyboard, and on a drive that is not connected", async ({
  page,
}) => {
  await page.getByTestId("tree-mode").getByRole("button", { name: "By folder" }).click();
  const archive = node(page, "Archive (unplugged)");
  await archive.focus();
  await page.keyboard.press("Shift+F10");
  const menu = page.getByTestId("folder-menu");
  await expect(menu).toBeVisible();
  await expect(page.getByTestId("folder-menu-rescan")).toBeDisabled();
  await expect(menu).toContainText("not connected");
  // Arrows stay in the menu; Escape closes it and goes back to the folder.
  await page.keyboard.press("ArrowDown");
  await expect(menu).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  await expect(archive).toBeFocused();

  // Show from the menu opens the folder.
  await node(page, "Photos A").focus();
  await page.keyboard.press("ContextMenu");
  await expect(menu.getByRole("menuitem", { name: "Show" })).toBeFocused();
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("heading")).toHaveText("Photos A");
  // A click elsewhere closes it.
  await node(page, "Photos A").click({ button: "right" });
  await expect(menu).toBeVisible();
  await page.getByTestId("count").click();
  await expect(menu).toHaveCount(0);
});

test("a scan that cannot start says why in the menu", async ({ page }) => {
  await page.goto("/?items=2000&rescanBusy=1");
  await expect(page.getByTestId("count")).toHaveText("2,000 items");
  await page.getByTestId("tree-mode").getByRole("button", { name: "By folder" }).click();
  await node(page, "Photos A").click({ button: "right" });
  await page.getByTestId("folder-menu-rescan").click();
  await expect(page.getByTestId("folder-menu-problem")).toContainText("already running");
  await expect(page.getByTestId("folders-screen")).toHaveCount(0);
});

test("right-click a folder: tag it and the folders in it, with a progress window", async ({
  page,
}) => {
  await page.getByTestId("tree-mode").getByRole("button", { name: "By folder" }).click();
  await node(page, "Photos A").click();
  const year = folder(page, "2016");
  const n = await countOf(page, "2016");
  await year.click({ button: "right" });
  const menu = page.getByTestId("folder-menu");
  await expect(menu.getByRole("menuitem")).toHaveText([
    "Show",
    "Scan again",
    "Find duplicates",
    "Tag photos",
  ]);
  await page.getByTestId("folder-menu-tags").click();
  await expect(menu).toHaveCount(0);
  const win = page.getByTestId("ai-job");
  await expect(win.getByRole("heading")).toHaveText("Tagging Photos A › 2016");
  await expect(win.getByTestId("ai-job-status")).toContainText(/done…$/);
  expect((await calls(page, "ai_analyze_folder")).at(-1)).toEqual({
    volume: "vOn",
    folder: "2016",
    what: "tags",
  });
  // Hide: a small button in the corner brings it back.
  await win.getByTestId("ai-job-hide").click();
  await expect(win).toHaveCount(0);
  await page.getByTestId("ai-job-pill").click();
  await expect(win.getByTestId("ai-job-status")).toHaveText(
    `${n.toLocaleString("en-US")} of ${n.toLocaleString("en-US")} photos tagged.`,
    { timeout: 15_000 },
  );
  await win.getByTestId("ai-job-close").click();
  await expect(win).toHaveCount(0);
});

test("a folder's duplicate search can be stopped; what was done stays", async ({ page }) => {
  await page.getByTestId("tree-mode").getByRole("button", { name: "By folder" }).click();
  await node(page, "Photos A").click();
  await folder(page, "2016").click({ button: "right" });
  await page.getByTestId("folder-menu-dups").click();
  const win = page.getByTestId("ai-job");
  await expect(win.getByRole("heading")).toHaveText("Finding duplicates in Photos A › 2016");
  await win.getByTestId("ai-job-stop").click();
  await expect(win.getByTestId("ai-job-status")).toContainText("Stopped:");
  expect((await calls(page, "ai_folder_stop")).at(-1)).toMatchObject({ what: "duplicates" });
  await win.getByTestId("ai-job-show-dups").click();
  await expect(page.getByTestId("duplicates-screen")).toBeVisible();
});
