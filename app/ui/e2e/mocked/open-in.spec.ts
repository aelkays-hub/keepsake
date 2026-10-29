// Open in…: several applications per file type (Settings), a menu in the
// inspector and the viewer, Ctrl+O for the first one.

import { expect, test, type Page } from "@playwright/test";

const calls = (page: Page, cmd: string) =>
  page.evaluate(
    (c) => (window.__mock?.calls ?? []).filter((x) => x.cmd === c).map((x) => x.args),
    cmd,
  );

// The mock's items (src/mock/backend.ts): a JPEG photo that is online.
const r = (i: number) => (i * 2654435761) % 1000;
const photo = (() => {
  for (let i = 1; i < 2000; i++) if (r(i) >= 150 && r(i) <= 850 && i % 4 !== 3) return i;
  throw new Error("no photo");
})();
const name = `DSC${String(photo).padStart(5, "0")}`;

const PS = "C:\\Program Files\\Adobe\\Adobe Photoshop 2026\\Photoshop.exe";
const TOPAZ = "C:\\Program Files\\Topaz Labs LLC\\Topaz Photo\\Topaz Photo.exe";

async function setUp(page: Page) {
  await page.goto("/?items=2000");
  await expect(page.getByTestId("count")).toHaveText("2,000 items");
  await page.keyboard.press("Control+,");
  const screen = page.getByTestId("settings-screen");
  for (const app of [PS, TOPAZ]) {
    await screen.getByTestId("app-ext").fill("arw, JPG");
    await screen.getByTestId("app-path").fill(app);
    await screen.getByTestId("settings-save").click();
    await expect(screen.getByTestId("settings-saved")).toHaveText("Saved.");
  }
  const rows = screen.getByTestId("open-apps").locator("tbody tr");
  // arw: Photoshop, Topaz; jpg: Photoshop, Topaz; and the new row.
  await expect(rows).toHaveCount(5);
  await expect(rows.nth(0)).toContainText(".arw (Ctrl+O)");
  await expect(rows.nth(0)).toContainText("Photoshop.exe");
  await expect(rows.nth(1)).toContainText("Topaz Photo.exe");
  await page.getByTestId("sidebar").getByRole("button", { name: "All items" }).click();
  await page.getByTestId("search").fill(name);
  await expect(page.getByTestId("count")).toHaveText("1 item");
  await page.getByTestId("tile").first().click();
}

test("the inspector's Open in menu lists the apps for the type, default first", async ({
  page,
}) => {
  await setUp(page);
  const inspector = page.getByTestId("inspector");
  const button = inspector.getByTestId("open-in");
  await expect(button).toHaveAttribute("aria-expanded", "false");
  await button.click();
  const menu = inspector.getByRole("menu", { name: "Open in" });
  const items = menu.getByRole("menuitem");
  await expect(items).toHaveText(["PhotoshopCtrl+O", "Topaz Photo", "System default"]);
  await expect(items.first()).toBeFocused();
  // Keyboard: down to Topaz Photo, Enter.
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  await expect(menu).toHaveCount(0);
  await expect(button).toBeFocused();
  await expect(inspector.getByTestId("opened")).toHaveText("Opened with Topaz Photo");
  expect((await calls(page, "open_external")).at(-1)).toEqual({
    id: `a${photo}`,
    member: "primary",
    app: TOPAZ,
  });
  // Ctrl+O opens the default.
  await page.getByTestId("tile").first().click();
  await page.keyboard.press("Control+o");
  await expect
    .poll(async () => (await calls(page, "open_external")).at(-1))
    .toEqual({
      id: `a${photo}`,
      member: "primary",
      app: null,
    });
  // The system default is in the menu too; Escape closes it.
  await button.click();
  await menu.getByRole("menuitem", { name: "System default" }).click();
  await expect(inspector.getByTestId("opened")).toHaveText("Opened with the system default");
  await button.click();
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
});

test("in the viewer, Escape closes the menu but not the viewer", async ({ page }) => {
  await setUp(page);
  await page.keyboard.press("Enter");
  const viewer = page.getByTestId("viewer");
  await expect(viewer).toBeVisible();
  await viewer.getByTestId("open-in").click();
  const menu = viewer.getByRole("menu", { name: "Open in" });
  await expect(menu.getByRole("menuitem")).toHaveCount(3);
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);
  await expect(viewer).toBeVisible();
  await viewer.getByTestId("open-in").click();
  await menu.getByRole("menuitem", { name: /Photoshop/ }).click();
  await expect(page.getByTestId("viewer-note")).toHaveText("Opened with Photoshop");
  // Ctrl+O in the viewer: the default.
  await page.keyboard.press("Control+o");
  await expect
    .poll(async () => (await calls(page, "open_external")).at(-1))
    .toEqual({
      id: `a${photo}`,
      member: "primary",
      app: null,
    });
});
