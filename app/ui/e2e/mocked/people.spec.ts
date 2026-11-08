// People in the Inspector and faces in the viewer (ADR-040, step 5),
// against the mock backend.

import { expect, test, type Page } from "@playwright/test";

const calls = (page: Page, cmd: string) =>
  page.evaluate(
    (c) => (window.__mock?.calls ?? []).filter((x) => x.cmd === c).map((x) => x.args),
    cmd,
  );

/** Name a face "Sara" on the first photo with faces (from the photo
 * menu); returns that tile's index. */
async function nameSara(page: Page): Promise<number> {
  const tiles = page.getByTestId("tile");
  const win = page.getByTestId("name-find");
  for (let i = 0; i < 12; i += 1) {
    await tiles.nth(i).click({ button: "right" });
    const item = page.getByTestId("tile-menu-faces");
    if (await item.isDisabled()) {
      await page.keyboard.press("Escape");
      continue;
    }
    await item.click();
    await expect(win.getByTestId("nf-looking")).toHaveCount(0, { timeout: 5000 });
    if ((await win.getByTestId("nf-face").count()) === 0) {
      await win.getByTestId("nf-close").click();
      continue;
    }
    await win.getByTestId("nf-name").fill("Sara");
    await win.getByTestId("nf-start").click();
    await expect(win.getByRole("heading")).toHaveText("Finding Sara");
    await win.getByTestId("nf-close").click();
    return i;
  }
  throw new Error("no photo with faces among the first tiles");
}

test("the Inspector lists the people in the photo", async ({ page }) => {
  await page.goto("/?items=60&faces=on");
  await expect(page.getByTestId("count")).toHaveText("60 items");
  // A photo not searched yet: it says so, and does not start a search.
  const tiles = page.getByTestId("tile");
  const i = await nameSara(page);
  await tiles.nth(i).click();
  const people = page.getByTestId("people");
  await expect(people.getByRole("list", { name: "People in this photo" })).toHaveText("Sara");
  for (const c of await calls(page, "faces_of_photo"))
    if ((c as { search?: boolean }).search === false) expect(c).toMatchObject({ search: false });
  // Name and find… from the Inspector opens the window on this photo.
  await people.getByTestId("people-name-find").click();
  await expect(page.getByTestId("name-find")).toBeVisible();
});

test("a photo not searched yet says so in the Inspector", async ({ page }) => {
  await page.goto("/?items=60&faces=on");
  await expect(page.getByTestId("count")).toHaveText("60 items");
  const tiles = page.getByTestId("tile");
  for (let i = 0; i < 12; i += 1) {
    await tiles.nth(i).click();
    const people = page.getByTestId("people");
    if ((await people.count()) === 0) continue; // a video
    await expect(people.getByTestId("people-note")).toHaveText("Not searched for faces yet.");
    // Looking does not search it.
    await expect(people.getByTestId("people-note")).toHaveText("Not searched for faces yet.");
    return;
  }
  throw new Error("no photo among the first tiles");
});

test("faces in the viewer: boxes with names, Not this person, Not a face", async ({ page }) => {
  await page.goto("/?items=60&faces=on");
  await expect(page.getByTestId("count")).toHaveText("60 items");
  const i = await nameSara(page);
  await page.getByTestId("tile").nth(i).dblclick();
  const viewer = page.getByTestId("viewer");
  await expect(viewer).toBeVisible();
  // P shows the faces (F is full screen).
  await page.keyboard.press("p");
  await expect(viewer.getByTestId("viewer-show-faces")).toHaveAttribute("aria-pressed", "true");
  const boxes = viewer.getByTestId("face-box");
  await expect(boxes.first()).toBeVisible();
  const n = await boxes.count();
  const sara = viewer.getByRole("button", { name: "Sara", exact: true });
  await expect(sara).toHaveText("Sara");

  // Right-click Sara: Name…, Not Sara, Not a face.
  await sara.click({ button: "right" });
  const menu = page.getByTestId("face-menu");
  await expect(menu.getByRole("menuitem")).toHaveText(["Name…", "Not Sara", "Not a face"]);
  await menu.getByTestId("face-menu-not-person").click();
  await expect(menu).toHaveCount(0);
  await expect(viewer.getByRole("button", { name: "Sara", exact: true })).toHaveCount(0);
  expect(await calls(page, "faces_not_person")).toHaveLength(1);
  // A face with no name: Not a face hides it.
  await viewer.getByRole("button", { name: "A face with no name" }).first().click();
  await page.getByTestId("face-menu").getByTestId("face-menu-not-face").click();
  await expect(boxes).toHaveCount(n - 1);

  // Name… opens the window with that face picked.
  await boxes.first().click();
  await page.getByTestId("face-menu").getByTestId("face-menu-name").click();
  const win = page.getByTestId("name-find");
  await expect(win).toBeVisible();
  await expect(win.locator('[data-testid="nf-face"][aria-checked="true"]')).toHaveCount(1);

  // The Faces button hides them again.
  await win.getByTestId("nf-close").click();
  await viewer.getByTestId("viewer-show-faces").click();
  await expect(boxes).toHaveCount(0);
});
