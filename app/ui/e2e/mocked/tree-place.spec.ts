// The sidebar's tree keeps its place: a right-click action that changes the
// library (Tag photos, Scan again) no longer sends it back to the top, and
// after a restart it comes back scrolled to where it was, with the same
// levels open.

import { expect, test, type Page } from "@playwright/test";

const side = (page: Page) => page.getByTestId("sidebar");
const scrollTop = (page: Page) => side(page).evaluate((el) => el.scrollTop);

async function open(page: Page) {
  // A short window, so the tree scrolls.
  await page.setViewportSize({ width: 1280, height: 420 });
  await page.goto("/?items=5000");
  await expect(page.getByTestId("count")).toHaveText("5,000 items");
}

/** Open years until the tree is tall, and scroll it to the middle. */
async function tallDateTree(page: Page) {
  for (const y of ["2016", "2017", "2018", "2019"]) {
    await side(page).locator(`button[data-cal="${y}"] .twisty`).click();
    await expect(side(page).locator(`button[data-cal="${y}-01"]`)).toBeVisible();
  }
  await side(page).evaluate((el) => (el.scrollTop = 600));
  await expect.poll(() => scrollTop(page)).toBe(600);
}

/** The first entry of `selector` fully on screen (below the toolbar). */
async function onScreen(page: Page, selector: string) {
  const box = await side(page).boundingBox();
  for (const b of await side(page).locator(selector).all()) {
    const r = await b.boundingBox();
    if (r && box && r.y > box.y + 4 && r.y + r.height < box.y + box.height - 4) return r;
  }
  throw new Error(`nothing on screen for ${selector}`);
}

test("Tag photos from a month's menu leaves the tree where it was", async ({ page }) => {
  await open(page);
  await tallDateTree(page);
  const entries = await side(page).locator("button[data-entry]").count();
  // Watch the tree while the job runs: it must never shrink.
  await side(page).evaluate((el) => {
    const w = window as unknown as { fewest: number };
    w.fewest = el.querySelectorAll("button[data-entry]").length;
    new MutationObserver(() => {
      w.fewest = Math.min(w.fewest, el.querySelectorAll("button[data-entry]").length);
    }).observe(el, { childList: true, subtree: true });
  });
  const r = await onScreen(page, 'button[data-cal*="-"]');
  await page.mouse.click(r.x + 60, r.y + r.height / 2, { button: "right" });
  await page.getByTestId("date-menu-tags").click();
  const win = page.getByTestId("ai-job");
  await expect(win.getByTestId("ai-job-status")).toHaveText(/photos tagged\.$/, {
    timeout: 30_000,
  });
  // The library changed (the tree's counts load again) but nothing moved.
  expect(await scrollTop(page)).toBe(600);
  expect(await page.evaluate(() => (window as unknown as { fewest: number }).fewest)).toBe(entries);
});

test("after a restart the tree is open and scrolled as it was left", async ({ page }) => {
  await open(page);
  await tallDateTree(page);
  await side(page).evaluate((el) => (el.scrollTop = 450));
  await page.waitForTimeout(100);
  await page.reload();
  await expect(page.getByTestId("count")).toHaveText("5,000 items");
  await expect(side(page).locator('button[data-cal="2019-01"]')).toBeVisible();
  await expect.poll(() => scrollTop(page)).toBe(450);
  // The other tree remembers its own place.
  await side(page).getByRole("button", { name: "By folder" }).click();
  await expect.poll(() => scrollTop(page)).toBe(0);
  await side(page).getByRole("button", { name: "By date" }).click();
  await expect.poll(() => scrollTop(page)).toBe(450);
});

test("Scan again from a folder's menu leaves the folder tree where it was", async ({ page }) => {
  await open(page);
  await side(page).getByRole("button", { name: "By folder" }).click();
  // Open the connected drive and two of its years.
  const drive = side(page).locator("button.cal-node").filter({ hasText: "Photos A" }).first();
  await drive.locator(".twisty").click();
  for (const y of ["2016", "2017"]) {
    const f = side(page).locator(`button[data-folder="${y}"]`).first();
    await f.locator(".twisty").click();
    await expect(side(page).locator(`button[data-folder="${y}/${y}-01"]`).first()).toBeVisible();
  }
  const most = await side(page).evaluate((el) => el.scrollHeight - el.clientHeight);
  const at = Math.min(300, most);
  expect(at).toBeGreaterThan(50);
  await side(page).evaluate((el, t) => (el.scrollTop = t), at);
  const r = await onScreen(page, 'button[data-folder*="/"]');
  await page.mouse.click(r.x + 60, r.y + r.height / 2, { button: "right" });
  await page.getByTestId("folder-menu-rescan").click();
  await expect(page.getByTestId("folder-menu")).toHaveCount(0);
  // The scan's end reloads the tree's counts.
  await page.waitForTimeout(1500);
  expect(await scrollTop(page)).toBe(at);
});
