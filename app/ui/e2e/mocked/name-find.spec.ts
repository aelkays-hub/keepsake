// Name and find (ADR-040, step 3) against the mock backend: right-click a
// photo, name a face, find the person's other faces, review and confirm,
// Find more.

import { expect, test, type Page } from "@playwright/test";

const calls = (page: Page, cmd: string) =>
  page.evaluate(
    (c) => (window.__mock?.calls ?? []).filter((x) => x.cmd === c).map((x) => x.args),
    cmd,
  );

/** Right-click photos until one has faces; returns the window. */
async function openOnAFace(page: Page) {
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
    // Looked for first, then shown.
    await expect(
      win
        .getByTestId("nf-looking")
        .or(win.getByTestId("nf-face").first())
        .or(win.getByTestId("nf-none")),
    ).toBeVisible();
    await expect(win.getByTestId("nf-looking")).toHaveCount(0, { timeout: 5000 });
    if ((await win.getByTestId("nf-face").count()) > 0) return win;
    await expect(win.getByTestId("nf-none")).toBeVisible();
    await win.getByTestId("nf-close").click();
  }
  throw new Error("no photo with faces among the first tiles");
}

test("name a face, review what is found, confirm and find more", async ({ page }) => {
  await page.goto("/?items=60&faces=on");
  await expect(page.getByTestId("count")).toHaveText("60 items");
  const win = await openOnAFace(page);
  await expect(win.getByRole("heading")).toContainText("Name and find:");
  // A face is picked; nothing happens before a name is given.
  await expect(win.getByTestId("nf-face").first()).toHaveAttribute("aria-checked", "true");
  await expect(win.getByTestId("nf-start")).toBeDisabled();
  await win.getByTestId("nf-name").fill("Sara");
  await win.getByTestId("nf-start").click();
  expect(await calls(page, "faces_name")).toEqual([{ face: expect.any(Number), name: "Sara" }]);
  await expect(win.getByRole("heading")).toHaveText("Finding Sara");

  // Every photo is searched, then the suggestions are shown by year.
  await expect(win.getByTestId("nf-found")).toContainText("that may be Sara", { timeout: 15_000 });
  const faces = win.getByTestId("nf-suggestion");
  const n = await faces.count();
  expect(n).toBeGreaterThan(1);
  await expect(win.getByTestId("nf-review").locator("h3").first()).toHaveText(/^\d{4}$/);
  // One is not her.
  await faces.first().click();
  await expect(faces.first()).toHaveAttribute("aria-pressed", "true");
  await expect(win.getByTestId("nf-confirm")).toHaveText(`Confirm ${n - 1} faces`);
  await win.getByTestId("nf-confirm").click();
  await expect(win.getByTestId("nf-note")).toHaveText(
    `${n - 1} faces confirmed as Sara; 1 face marked as not Sara.`,
  );
  const decided = (await calls(page, "faces_decide"))[0] as { confirm: number[]; reject: number[] };
  expect(decided.confirm).toHaveLength(n - 1);
  expect(decided.reject).toHaveLength(1);

  // Find more: nothing new (the one she is not is not suggested again).
  await win.getByTestId("nf-more").click();
  await expect(win.getByTestId("nf-note")).toHaveText(
    "No more faces like Sara's were found (photos from 2012 to 2020 looked at).",
  );
  await win.getByTestId("nf-close").click();
  await expect(win).toHaveCount(0);

  // Next time, Sara is offered as a name.
  await openOnAFace(page);
  await expect(page.locator("#nf-people option[value='Sara']")).toHaveCount(1);
});

test("Name and find needs Recognise faces", async ({ page }) => {
  await page.goto("/?items=60");
  await expect(page.getByTestId("count")).toHaveText("60 items");
  const tiles = page.getByTestId("tile");
  for (let i = 0; i < 12; i += 1) {
    await tiles.nth(i).click({ button: "right" });
    const item = page.getByTestId("tile-menu-faces");
    if (await item.isDisabled()) {
      await page.keyboard.press("Escape");
      continue;
    }
    await item.click();
    break;
  }
  await expect(page.getByTestId("name-find").getByRole("alert")).toContainText(
    "Turn on Recognise faces first",
  );
});

test("the viewer opens Name and find for its photo", async ({ page }) => {
  await page.goto("/?items=60&faces=on");
  await expect(page.getByTestId("count")).toHaveText("60 items");
  const tiles = page.getByTestId("tile");
  for (let i = 0; i < 12; i += 1) {
    await tiles.nth(i).dblclick();
    const btn = page.getByTestId("viewer-faces");
    if ((await btn.count()) > 0) {
      await btn.click();
      await expect(page.getByTestId("name-find")).toBeVisible();
      return;
    }
    await page.keyboard.press("Escape");
  }
  throw new Error("no photo opened in the viewer");
});
