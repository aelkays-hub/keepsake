// Read aircraft (ADR-041): the Settings card, the photo menu, the viewer
// button and the progress window, against the mock backend.

import { expect, test, type Page } from "@playwright/test";

const calls = (page: Page, cmd: string) =>
  page.evaluate(
    (c) => (window.__mock?.calls ?? []).filter((x) => x.cmd === c).map((x) => x.args),
    cmd,
  );

async function open(page: Page, query = "") {
  await page.goto(`/?items=1000${query}`);
  await expect(page.getByTestId("count")).toHaveText("1,000 items");
}

const settings = (page: Page) =>
  page.getByTestId("sidebar").getByRole("button", { name: "Settings" }).click();

test("Settings: turning it on downloads the text models, and off again", async ({ page }) => {
  await open(page);
  await settings(page);
  const card = page.getByTestId("aircraft-settings");
  await expect(card.getByRole("heading")).toHaveText("Read aircraft");
  await expect(card.getByTestId("aircraft-progress")).toHaveText("Off.");
  await expect(card).toContainText("downloads two text models once (95.9 MB)");
  await card.getByTestId("aircraft-enabled").check();
  expect(await calls(page, "aircraft_enable")).toEqual([{ enabled: true }]);
  await expect(card.getByTestId("aircraft-progress")).toContainText(
    "Ready, on the graphics card (DirectML).",
  );
  await expect(card.getByTestId("aircraft-progress")).toContainText("guessed from its look");

  // Off: the models can be deleted.
  await card.getByTestId("aircraft-enabled").uncheck();
  await expect(card.getByTestId("aircraft-progress")).toHaveText("Off.");
  await card.getByTestId("aircraft-remove").click();
  expect(await calls(page, "aircraft_remove_models")).toHaveLength(1);
  await expect(card).toContainText("downloads two text models once");
});

test("without On-device AI the type is only read when painted on", async ({ page }) => {
  await open(page, "&ai=off&aircraft=on");
  await settings(page);
  await expect(page.getByTestId("aircraft-progress")).toContainText("turn on On-device AI");
});

test("right-click photos: Read aircraft reads them and adds keywords", async ({ page }) => {
  await open(page, "&aircraft=on");
  const tiles = page.getByTestId("tile");
  await tiles.nth(0).click();
  await tiles.nth(2).click({ modifiers: ["Shift"] });
  await tiles.nth(1).click({ button: "right" });
  const menu = page.getByTestId("item-menu");
  await menu.getByTestId("tile-menu-aircraft").click();
  await expect(menu).toHaveCount(0);

  // It runs on this computer: nothing to ask, it starts at once.
  const win = page.getByTestId("ai-job");
  await expect(win.getByRole("heading")).toHaveText("Reading aircraft: 3 photos");
  await expect(win).toContainText("read at full size on this computer");
  await expect(win.getByTestId("ai-job-send")).toHaveCount(0);
  await expect(win.getByTestId("ai-job-status")).toHaveText("3 of 3 photos read.");
  const sent = await calls(page, "aircraft_read");
  expect(sent).toHaveLength(1);
  expect((sent[0] as { selection: { ranges: number[][] } }).selection.ranges).toEqual([[0, 3]]);
  // No duplicates to show for this kind of job.
  await expect(win.getByTestId("ai-job-show-dups")).toHaveCount(0);

  // What it found are keywords now, and the inspector says who gave them.
  await win.getByTestId("ai-job-close").click();
  await expect(win).toHaveCount(0);
  await tiles.nth(1).click();
  const inspector = page.getByTestId("inspector");
  await expect(inspector).toContainText("Aircraft|Registration|C-FJZS");
  const by = inspector.getByTestId("ai-source-aircraft");
  await expect(by).toContainText("Aircraft reader");
  await expect(by).toContainText("Aircraft|Type|Boeing 777-300ER");
});

test("when it is off, the window says so and opens Settings", async ({ page }) => {
  await open(page);
  await page.getByTestId("tile").first().click({ button: "right" });
  await page.getByTestId("tile-menu-aircraft").click();
  const win = page.getByTestId("ai-job");
  await expect(win.getByRole("alert")).toHaveText(
    "Could not start: Turn on Read aircraft in Settings first.",
  );
  await win.getByTestId("ai-job-settings").click();
  await expect(win).toHaveCount(0);
  await expect(page.getByTestId("aircraft-settings")).toBeVisible();
});

test("the viewer has Read aircraft for the photo shown; Stop keeps what was read", async ({
  page,
}) => {
  await open(page, "&aircraft=on");
  await page.getByTestId("tile").first().dblclick();
  await page.getByTestId("viewer-aircraft").click();
  const win = page.getByTestId("ai-job");
  await expect(win.getByRole("heading")).toContainText("Reading aircraft:");
  await win.getByTestId("ai-job-stop").click();
  await expect(win.getByTestId("ai-job-status")).toContainText("Stopped: 0 of 1 read.");
  expect(await calls(page, "aircraft_read_stop")).toHaveLength(1);
});
