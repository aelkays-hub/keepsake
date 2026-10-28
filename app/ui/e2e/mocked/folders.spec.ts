// Adding a folder of photos that are already on disk (Folders screen), from
// an empty library and from the sidebar.

import { expect, test, type Page } from "@playwright/test";

const calls = (page: Page, cmd: string) =>
  page.evaluate(
    (c) => (window.__mock?.calls ?? []).filter((x) => x.cmd === c).map((x) => x.args),
    cmd,
  );

test("an empty library offers to add a folder, and shows it once added", async ({ page }) => {
  await page.goto("/?items=0");
  await expect(page.getByTestId("count")).toHaveText("0 items");
  await page.getByTestId("empty-add-folder").click();
  const screen = page.getByTestId("folders-screen");
  await expect(screen).toBeVisible();
  await expect(screen.getByText("No folders or drives yet.")).toBeVisible();

  const views = (await calls(page, "view_open")).length;
  await screen.getByTestId("folder-path").fill("E:\\Pictures\\2025");
  await screen.getByTestId("folder-add").click();
  const outcome = screen.getByTestId("folder-outcome");
  await expect(outcome).toContainText("Added 2025: 38 items");
  await expect(outcome).toContainText("Nothing in the folder was changed.");
  await expect(outcome).toContainText("12 took ratings or keywords from .xmp files.");
  expect(await calls(page, "folder_add")).toEqual([{ path: "E:\\Pictures\\2025" }]);
  // The library view and the sidebar are refreshed.
  await expect.poll(async () => (await calls(page, "view_open")).length).toBeGreaterThan(views);
  const list = screen.getByTestId("folder-list").locator("li");
  await expect(list).toHaveCount(1);
  await expect(list.first()).toContainText("E:\\Pictures\\2025");
  await expect(list.first()).toContainText("38 items");
  await expect(page.getByTestId("sidebar").getByRole("button", { name: /\b2025\b/ })).toBeVisible();

  // Scan again uses the same folder.
  await list.first().getByTestId("folder-rescan").click();
  await expect(outcome).toContainText("Added 2025");
  expect(await calls(page, "folder_add")).toHaveLength(2);
  expect((await calls(page, "folder_add"))[1]).toEqual({ path: "E:\\Pictures\\2025" });
  await expect(list).toHaveCount(1);

  await outcome.getByRole("button", { name: "Show them" }).click();
  await expect(page.getByTestId("heading")).toHaveText("2025");
});

test("a folder that is not there is reported, and nothing runs", async ({ page }) => {
  await page.goto("/?items=200");
  await expect(page.getByTestId("count")).toHaveText("200 items");
  await page.getByTestId("sidebar").getByRole("button", { name: "Folders" }).click();
  const screen = page.getByTestId("folders-screen");
  await expect(screen.getByTestId("folder-add")).toBeDisabled();
  await screen.getByTestId("folder-path").fill("E:\\missing");
  await screen.getByTestId("folder-add").click();
  await expect(screen.getByTestId("folder-status")).toContainText("is not a folder");
  await expect(screen.getByTestId("folder-outcome")).toHaveCount(0);
});

test("with the keyboard: type the folder and press Enter", async ({ page }) => {
  await page.goto("/?items=200");
  await expect(page.getByTestId("count")).toHaveText("200 items");
  await page.getByTestId("sidebar").getByRole("button", { name: "Folders" }).focus();
  await page.keyboard.press("Enter");
  const path = page.getByTestId("folder-path");
  for (let i = 0; i < 20 && !(await path.evaluate((el) => el === document.activeElement)); i++)
    await page.keyboard.press("Tab");
  await expect(path).toBeFocused();
  await page.keyboard.type("D:\\Photos");
  await page.keyboard.press("Enter");
  await expect(page.getByTestId("folder-outcome")).toContainText("Added Photos");
});
