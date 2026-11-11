// An item's right-click menu: Show in Explorer / Finder (its original, with
// the file selected) and Open in…, from the grid, the details table and the
// viewer, with the mouse or the Menu key / Shift+F10.

import { expect, test, type Page } from "@playwright/test";

const calls = (page: Page, cmd: string) =>
  page.evaluate(
    (c) => (window.__mock?.calls ?? []).filter((x) => x.cmd === c).map((x) => x.args),
    cmd,
  );

async function setUp(page: Page) {
  await page.goto("/?items=2000");
  await expect(page.getByTestId("count")).toHaveText("2,000 items");
  await page.getByTestId("sidebar").getByRole("button", { name: "All items" }).click();
}

const online = (page: Page) => page.locator('[data-testid="tile"]:not(.offline)');

test("right-click a tile, then Show in Explorer", async ({ page }) => {
  await setUp(page);
  const tile = online(page).first();
  const name = await tile.getAttribute("title");
  await tile.click({ button: "right" });
  const menu = page.getByTestId("item-menu");
  await expect(menu).toBeVisible();
  await expect(menu).toHaveAttribute("aria-label", name ?? "");
  const reveal = menu.getByTestId("item-menu-reveal");
  await expect(reveal).toHaveText("Show in Explorer");
  await expect(reveal).toBeEnabled();
  // In the grid, Open comes first (then Show, Open in…, Tag with Claude).
  await expect(menu.getByTestId("item-menu-view")).toBeFocused();
  // Open in… is in the same menu: here only the system default is set up.
  await expect(menu.getByTestId("item-menu-open")).toHaveText(["Open with the system default"]);
  const id = (await calls(page, "original_place")).at(-1) as { id: string };
  await reveal.click();
  await expect(menu).toHaveCount(0);
  expect((await calls(page, "show_in_folder")).at(-1)).toEqual({ id: id.id });
  // The tile is selected and has focus again.
  await expect(tile).toHaveAttribute("aria-selected", "true");
  await expect(tile).toBeFocused();
});

test("with several selected, the menu acts on the right-clicked one", async ({ page }) => {
  await setUp(page);
  const a = online(page).nth(0);
  const b = online(page).nth(1);
  await a.click();
  await b.click({ modifiers: ["Control"] });
  await expect(page.locator('[data-testid="tile"][aria-selected="true"]')).toHaveCount(2);
  const name = await b.getAttribute("title");
  await b.click({ button: "right" });
  const menu = page.getByTestId("item-menu");
  await expect(menu).toHaveAttribute("aria-label", name ?? "");
  // The selection stays.
  await expect(page.locator('[data-testid="tile"][aria-selected="true"]')).toHaveCount(2);
  await menu.getByTestId("item-menu-reveal").click();
  const shown = (await calls(page, "show_in_folder")).at(-1) as { id: string };
  const asked = (await calls(page, "original_place")).at(-1) as { id: string };
  expect(shown.id).toBe(asked.id);
});

test("Menu key and Shift+F10 open it on the focused item; Escape closes it", async ({ page }) => {
  await setUp(page);
  const tile = online(page).first();
  await tile.click();
  await page.keyboard.press("Shift+F10");
  const menu = page.getByTestId("item-menu");
  const reveal = menu.getByTestId("item-menu-reveal");
  const view = menu.getByTestId("item-menu-view");
  await expect(view).toBeFocused();
  // Arrows stay in the menu (the grid's focus does not move).
  await page.keyboard.press("ArrowDown");
  await expect(reveal).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(menu.getByTestId("item-menu-open").first()).toBeFocused();
  await page.keyboard.press("ArrowUp");
  await expect(reveal).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  await expect(tile).toBeFocused();
  // The Menu key, Arrow Down, then Enter: shown.
  await page.keyboard.press("ContextMenu");
  await expect(view).toBeFocused();
  await page.keyboard.press("ArrowDown");
  await expect(reveal).toBeFocused();
  const before = (await calls(page, "show_in_folder")).length;
  await page.keyboard.press("Enter");
  await expect(menu).toHaveCount(0);
  await expect.poll(async () => (await calls(page, "show_in_folder")).length).toBe(before + 1);
  // A click elsewhere closes it too.
  await tile.click({ button: "right" });
  await expect(menu).toBeVisible();
  await page.getByTestId("count").click();
  await expect(menu).toHaveCount(0);
});

test("an item whose drive is not connected says so", async ({ page }) => {
  await setUp(page);
  const tile = page.locator('[data-testid="tile"].offline').first();
  await tile.click({ button: "right" });
  const menu = page.getByTestId("item-menu");
  await expect(menu.getByTestId("item-menu-reveal")).toBeDisabled();
  await expect(menu.getByTestId("item-menu-offline")).toHaveText("Drive not connected (Archive)");
  await expect(menu.getByTestId("item-menu-open")).toHaveCount(0);
  // Keys still work: Escape closes it.
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  expect(await calls(page, "show_in_folder")).toEqual([]);
});

test("in the details table", async ({ page }) => {
  await setUp(page);
  await page.keyboard.press("Control+3");
  const row = page.locator('[data-testid="drow"]:not(.offline)').first();
  await row.click({ button: "right" });
  const menu = page.getByTestId("item-menu");
  await expect(menu.getByTestId("item-menu-reveal")).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(row).toBeFocused();
  await page.keyboard.press("Shift+F10");
  await page.keyboard.press("Enter");
  await expect(menu).toHaveCount(0);
  await expect.poll(async () => (await calls(page, "show_in_folder")).length).toBe(1);
});

test("in the viewer: Escape closes the menu, not the viewer", async ({ page }) => {
  await setUp(page);
  await online(page).first().dblclick();
  const viewer = page.getByTestId("viewer");
  await expect(viewer).toBeVisible();
  await page.keyboard.press("ContextMenu");
  const menu = viewer.getByTestId("item-menu");
  await expect(menu.getByTestId("item-menu-reveal")).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  await expect(viewer).toBeVisible();
  // Right-click on the picture, then Show in Explorer.
  await viewer.locator(".viewer-main").click({ button: "right" });
  await menu.getByTestId("item-menu-reveal").click();
  await expect(viewer.getByTestId("viewer-note")).toHaveText("Shown in Explorer");
  await expect(viewer).toBeVisible();
  expect(await calls(page, "show_in_folder")).toHaveLength(1);
});

test("Ctrl+click toggles the selection and opens no menu", async ({ page }) => {
  await setUp(page);
  const a = online(page).nth(0);
  const b = online(page).nth(1);
  await a.click();
  await b.click({ modifiers: ["Control"] });
  await expect(page.locator('[data-testid="tile"][aria-selected="true"]')).toHaveCount(2);
  // On a Mac, Ctrl+click also fires contextmenu (primary button, Ctrl down):
  // send that here too, so every OS checks it.
  await b.evaluate((el) =>
    el.dispatchEvent(
      new MouseEvent("contextmenu", {
        bubbles: true,
        cancelable: true,
        ctrlKey: true,
        button: 0,
        clientX: 10,
        clientY: 10,
      }),
    ),
  );
  await expect(page.getByTestId("item-menu")).toHaveCount(0);
  // Keys still go to the grid, not to a menu.
  await expect(b).toBeFocused();
  await b.click({ modifiers: ["Control"] });
  await expect(b).toHaveAttribute("aria-selected", "false");
  await expect(page.locator('[data-testid="tile"][aria-selected="true"]')).toHaveCount(1);
  expect(await calls(page, "original_place")).toEqual([]);
  // A right-click still opens it.
  await b.click({ button: "right" });
  await expect(page.getByTestId("item-menu")).toBeVisible();
});
