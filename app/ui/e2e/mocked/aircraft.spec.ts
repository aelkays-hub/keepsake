// Read aircraft (ADR-041): the Settings card, and the review window that
// every Read aircraft opens (the photo menu, the viewer, a folder, dates),
// against the mock backend.

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

test("right-click photos: the review window reads them; only what is accepted is kept", async ({
  page,
}) => {
  await open(page, "&aircraft=on");
  const tiles = page.getByTestId("tile");
  await tiles.nth(0).click();
  await tiles.nth(2).click({ modifiers: ["Shift"] });
  await tiles.nth(1).click({ button: "right" });
  const menu = page.getByTestId("item-menu");
  await menu.getByTestId("tile-menu-aircraft").click();
  await expect(menu).toHaveCount(0);

  const win = page.getByTestId("aircraft-review");
  await expect(win.getByRole("heading")).toHaveText("Read aircraft: 3 photos");
  const sent = await calls(page, "aircraft_read");
  expect(sent).toHaveLength(1);
  expect((sent[0] as { selection: { ranges: number[][] } }).selection.ranges).toEqual([[0, 3]]);
  await expect(win.getByTestId("ar-progress")).toHaveText("3 of 3 photos read.");
  const photos = win.getByTestId("ar-photo");
  await expect(photos).toHaveCount(3);
  await expect(win.getByTestId("ar-waiting")).toHaveText("3 photos waiting for review");

  // The saved levels: 80, 85, 70. Each photo offers the registration (96 %)
  // and, with no painted type sure enough (66 %), the type from its look.
  await expect(win.getByTestId("ar-typePct")).toHaveValue("70");
  const first = photos.first();
  await expect(first).toContainText("Registration C-FJZS 96%");
  await expect(first).toContainText("Type Boeing 777 (its look)");
  await expect(first.getByTestId("ar-also")).toContainText("Type Boeing 777-300ER 66%");

  // Lower the type: the painted type is offered instead, at once, and the
  // levels are kept for next time. Nothing is read again.
  await win.getByTestId("ar-typePct").fill("60");
  await expect(first).toContainText("Type Boeing 777-300ER 66%");
  await expect(first).not.toContainText("(its look)");
  await expect
    .poll(async () => (await calls(page, "aircraft_set_confidence")).at(-1))
    .toEqual({ confidence: { registrationPct: 80, airlinePct: 85, typePct: 60 } });
  expect(await calls(page, "aircraft_read")).toHaveLength(1);

  // Untick the type and accept: the type does not become a keyword.
  await first.getByTestId("ar-mark").last().uncheck();
  await first.getByTestId("ar-accept").click();
  await expect(photos).toHaveCount(2);
  const accepted = (await calls(page, "aircraft_accept"))[0] as { keywords: string[] };
  expect(accepted.keywords).toContain("Aircraft|Registration|C-FJZS");
  expect(accepted.keywords).not.toContain("Aircraft|Type|Boeing 777-300ER");
  // Dismiss one; the last is accepted with Accept all shown.
  await photos.first().getByTestId("ar-dismiss").click();
  await expect(photos).toHaveCount(1);
  expect(await calls(page, "aircraft_dismiss")).toHaveLength(1);
  await win.getByTestId("ar-accept-all").click();
  await expect(win.getByTestId("ar-none-waiting")).toBeVisible();
  await win.getByTestId("ar-all").check();
  await expect(photos).toHaveCount(3);
  await expect(win.getByTestId("ar-state")).toHaveText(["Accepted", "Dismissed", "Accepted"]);

  // What was accepted are keywords now, and the inspector says who gave them.
  await win.getByTestId("ar-close").click();
  await expect(win).toHaveCount(0);
  await tiles.nth(0).click();
  const inspector = page.getByTestId("inspector");
  await expect(inspector).toContainText("Aircraft|Registration|C-FJZS");
  await expect(inspector.getByTestId("ai-source-aircraft")).toContainText("Aircraft reader");
  // Settings shows the levels as last set.
  await settings(page);
  await expect(page.getByTestId("aircraft-confidence")).toContainText(
    "registration 80%, airline 85%, type 60%",
  );
});

test("when it is off, the window says so and opens Settings", async ({ page }) => {
  await open(page);
  await page.getByTestId("tile").first().click({ button: "right" });
  await page.getByTestId("tile-menu-aircraft").click();
  const win = page.getByTestId("aircraft-review");
  await expect(win.getByTestId("ar-problem")).toHaveText(
    "Turn on Read aircraft in Settings first.",
  );
  await win.getByTestId("ar-settings").click();
  await expect(win).toHaveCount(0);
  await expect(page.getByTestId("aircraft-settings")).toBeVisible();
});

test("the viewer has Read aircraft for the photo shown; Stop keeps what was read", async ({
  page,
}) => {
  await open(page, "&aircraft=on");
  await page.getByTestId("tile").first().dblclick();
  await page.getByTestId("viewer-aircraft").click();
  const win = page.getByTestId("aircraft-review");
  await expect(win.getByRole("heading")).toContainText("Read aircraft:");
  await win.getByTestId("ar-stop").click();
  await expect(win.getByTestId("ar-progress")).toContainText("Stopped: 0 of 1 read.");
  expect(await calls(page, "aircraft_read_stop")).toHaveLength(1);
});

const node = (page: Page, label: string | RegExp) =>
  page.getByTestId("sidebar").locator("button.cal-node").filter({ hasText: label });

test("right-click a folder: Read aircraft reviews its aircraft photos", async ({ page }) => {
  await page.goto("/?items=2000&aircraft=on");
  await expect(page.getByTestId("count")).toHaveText("2,000 items");
  await page.getByTestId("tree-mode").getByRole("button", { name: "By folder" }).click();
  await node(page, "Photos A").click();
  await page
    .getByTestId("sidebar")
    .locator('button[data-folder="2016"]')
    .first()
    .click({ button: "right" });
  const menu = page.getByTestId("folder-menu");
  await menu.getByTestId("folder-menu-aircraft").click();
  await expect(menu).toHaveCount(0);
  const win = page.getByTestId("aircraft-review");
  await expect(win.getByRole("heading")).toHaveText("Read aircraft: Photos A › 2016");
  expect((await calls(page, "ai_analyze_folder")).at(-1)).toEqual({
    volume: "vOn",
    folder: "2016",
    what: "aircraft",
  });
  expect((await calls(page, "aircraft_target_ids")).at(-1)).toEqual({
    volume: "vOn",
    folder: "2016",
    from: null,
    to: null,
  });
  // Only its aircraft photos, each offered for review.
  await expect(win.getByTestId("ar-progress")).toHaveText(/^[\d,]+ of [\d,]+ photos read\.$/, {
    timeout: 15_000,
  });
  await expect(win.getByTestId("ar-photo").first()).toContainText("C-FJZS");
  await win.getByTestId("ar-close").click();
  await expect(win).toHaveCount(0);
});

test("right-click a year: Read aircraft; when it is off the window opens Settings", async ({
  page,
}) => {
  await open(page, "&aircraft=on");
  await node(page, "2015").click({ button: "right" });
  await page.getByTestId("date-menu-aircraft").click();
  const win = page.getByTestId("aircraft-review");
  await expect(win.getByRole("heading")).toHaveText("Read aircraft: 2015");
  expect(await calls(page, "ai_analyze_range")).toEqual([
    { from: "2015-01-01", to: "2015-12-31", what: "aircraft" },
  ]);
  await expect(win.getByTestId("ar-progress")).toHaveText(/^[\d,]+ of [\d,]+ photos read\.$/, {
    timeout: 15_000,
  });

  // Off: it says so.
  await open(page);
  await node(page, "2015").click({ button: "right" });
  await page.getByTestId("date-menu-aircraft").click();
  await expect(page.getByTestId("ar-problem")).toHaveText(
    "Turn on Read aircraft in Settings first.",
  );
  await page.getByTestId("ar-settings").click();
  await expect(page.getByTestId("aircraft-settings")).toBeVisible();
});
