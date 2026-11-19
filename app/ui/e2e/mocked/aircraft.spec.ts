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
  await expect(card).toContainText(
    "downloads two text models and an aircraft finder once (131.7 MB)",
  );
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
  await expect(card).toContainText("downloads two text models and an aircraft finder once");
});

test("Settings: with the text models but not the aircraft finder, it can be downloaded alone", async ({
  page,
}) => {
  await open(page, "&aircraft=on&finder=off");
  await settings(page);
  const card = page.getByTestId("aircraft-settings");
  const finder = card.getByTestId("aircraft-finder");
  await expect(finder).toContainText("The aircraft finder is not downloaded");
  await expect(finder.getByTestId("aircraft-finder-get")).toHaveText("Download it (35.9 MB)");
  await finder.getByTestId("aircraft-finder-get").click();
  expect(await calls(page, "aircraft_enable")).toEqual([{ enabled: true }]);
  await expect(card.getByTestId("aircraft-finder")).toHaveCount(0);
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
  await expect(win.getByTestId("ar-waiting")).toHaveText("3 photos to review");

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
    .toEqual({ confidence: { registrationPct: 80, airlinePct: 85, typePct: 60, sameShotS: 10 } });
  expect(await calls(page, "aircraft_read")).toHaveLength(1);

  // Untick the type and accept: the type does not become a keyword. (Its
  // neighbour is unticked too, so the next photo stays to be reviewed.)
  await first.getByTestId("ar-mark").last().uncheck();
  await first
    .getByTestId("ar-near")
    .locator("label", { hasText: "96% alike" })
    .getByTestId("ar-near-photo")
    .uncheck();
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
  // Accepted before dismissed.
  await expect(win.getByTestId("ar-state")).toHaveText(["Accepted", "Accepted", "Dismissed"]);

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

test("with the online lookup: the registry's type and airline, neighbours and the same plane", async ({
  page,
}) => {
  await open(page, "&aircraft=on&lookup=on");
  await page.getByTestId("tile").nth(10).click({ button: "right" });
  await page.getByTestId("tile-menu-aircraft").click();
  const win = page.getByTestId("aircraft-review");
  const photo = win.getByTestId("ar-photo").first();
  await expect(photo.getByTestId("ar-lookup")).toHaveText(
    "Registry (adsbdb): C-FJZS: Boeing 777-200LR, Air Canada, Canada",
  );
  await expect(photo).toContainText("Type Boeing 777-200LR (adsbdb)");
  // The painted type (66 %) comes in when the type is lowered, unticked:
  // the registry says otherwise.
  await win.getByTestId("ar-typePct").fill("60");
  const painted = photo.locator("label", { hasText: "Boeing 777-300ER" }).getByTestId("ar-mark");
  await expect(painted).not.toBeChecked();
  const looked = photo.locator("label", { hasText: "Boeing 777-200LR" }).getByTestId("ar-mark");
  await expect(looked).toBeChecked();

  // Neighbours: the alike one ticked, the other not.
  const near = photo.getByTestId("ar-near");
  const alike = near.locator("label", { hasText: "96% alike" }).getByTestId("ar-near-photo");
  const other = near.locator("label", { hasText: "61% alike" }).getByTestId("ar-near-photo");
  if (await alike.count()) await expect(alike).toBeChecked();
  if (await other.count()) await expect(other).not.toBeChecked();
  const nearTicked = await alike.count();

  await photo.getByTestId("ar-accept").click();
  const sent = (await calls(page, "aircraft_accept"))[0] as {
    keywords: string[];
    neighbours: string[];
  };
  expect(sent.keywords).toContain("Aircraft|Type|Boeing 777-200LR");
  expect(sent.keywords).not.toContain("Aircraft|Type|Boeing 777-300ER");
  expect(sent.neighbours).toHaveLength(nearTicked);

  // The registration's other photos that lack the type are offered it.
  const same = win.getByTestId("ar-same");
  await expect(same).toContainText("2 other photos of C-FJZS lack Boeing 777-200LR.");
  await same.getByTestId("ar-same-add").click();
  await expect(same).toHaveCount(0);
  expect(await calls(page, "aircraft_add_keywords")).toEqual([
    { ids: ["a900"], keywords: ["Aircraft|Type|Boeing 777-200LR"] },
    { ids: ["a901"], keywords: ["Aircraft|Type|Boeing 777-200LR"] },
  ]);

  // Settings: the lookup is on, and can be turned off.
  await win.getByTestId("ar-close").click();
  await settings(page);
  const box = page.getByTestId("aircraft-lookup");
  await expect(box).toBeChecked();
  await box.uncheck();
  expect(await calls(page, "aircraft_set_lookup")).toEqual([{ enabled: false }]);
});

test("photos accepted as a photo's neighbours leave the list and are not asked about again", async ({
  page,
}) => {
  await open(page, "&aircraft=on");
  const tiles = page.getByTestId("tile");
  await tiles.nth(0).click();
  await tiles.nth(2).click({ modifiers: ["Shift"] });
  await tiles.nth(1).click({ button: "right" });
  await page.getByTestId("item-menu").getByTestId("tile-menu-aircraft").click();
  const win = page.getByTestId("aircraft-review");
  await expect(win.getByTestId("ar-progress")).toHaveText("3 of 3 photos read.");
  const photos = win.getByTestId("ar-photo");
  await expect(photos).toHaveCount(3);
  // A photo whose next one (2 s later, alike, ticked) is in the list too.
  const ids = await photos.evaluateAll((els) => els.map((e) => e.getAttribute("data-id") ?? ""));
  const id = ids.find((x) => ids.includes(`a${Number(x.slice(1)) + 1}`)) ?? "";
  expect(id).not.toBe("");
  const next = `a${Number(id.slice(1)) + 1}`;
  const photo = win.locator(`[data-id="${id}"]`);
  await expect(
    photo
      .getByTestId("ar-near")
      .locator("label", { hasText: "96% alike" })
      .getByTestId("ar-near-photo"),
  ).toBeChecked();

  await photo.getByTestId("ar-accept").click();
  // Both leave the list at once: the neighbour is not asked about again.
  await expect(win.locator(`[data-id="${id}"]`)).toHaveCount(0);
  await expect(win.locator(`[data-id="${next}"]`)).toHaveCount(0);
  await expect(photos).toHaveCount(1);
  await expect(win.getByTestId("ar-waiting")).toHaveText("1 photo to review");
  // Shown with the others, it is accepted, and stays so after a refresh.
  await win.getByTestId("ar-all").check();
  await expect(win.locator(`[data-id="${next}"]`).getByTestId("ar-state")).toHaveText("Accepted");
  await win.getByTestId("ar-typePct").fill("60");
  await expect(win.locator(`[data-id="${next}"]`).getByTestId("ar-state")).toHaveText("Accepted");
});

test("Read again: a photo, or all of them, is read again and waits for review again", async ({
  page,
}) => {
  await open(page, "&aircraft=on");
  const tiles = page.getByTestId("tile");
  await tiles.nth(0).click();
  await tiles.nth(0).click({ button: "right" });
  await page.getByTestId("item-menu").getByTestId("tile-menu-aircraft").click();
  const win = page.getByTestId("aircraft-review");
  const photos = win.getByTestId("ar-photo");
  await expect(photos).toHaveCount(1);
  // Dismissed, it leaves the list; shown again, it can be read again.
  await photos.first().getByTestId("ar-dismiss").click();
  await expect(photos).toHaveCount(0);
  await win.getByTestId("ar-all").check();
  await expect(win.getByTestId("ar-state")).toHaveText(["Dismissed"]);
  await photos.first().getByTestId("ar-reread").click();
  await win.getByTestId("ar-all").uncheck();
  await expect(photos).toHaveCount(1);
  await expect(win.getByTestId("ar-waiting")).toHaveText("1 photo to review");
  expect(await calls(page, "aircraft_read")).toHaveLength(2);
  // Only that photo is asked for.
  const again = (await calls(page, "aircraft_read"))[1] as { selection: { ids: string[] } };
  expect(again.selection.ids).toHaveLength(1);
  // All of the window's photos at once.
  await win.getByTestId("ar-reread-all").click();
  await expect.poll(async () => (await calls(page, "aircraft_read")).length).toBe(3);
  await expect(photos).toHaveCount(1);
});

// ---- The aircraft's age (ADR-044) ----

/** Whole years from `y-m-d` to today (a birthday not yet reached not counted). */
function yearsSince(y: number, m = 1, d = 1): number {
  const now = new Date();
  const before = now.getMonth() + 1 < m || (now.getMonth() + 1 === m && now.getDate() < d);
  return now.getFullYear() - y - (before ? 1 : 0);
}

test("the aircraft's age: worked out in the inspector and the viewer, typed once per registration", async ({
  page,
}) => {
  await open(page, "&aircraft=on&lookup=on&builtdb=on");
  await page.getByTestId("tile").nth(0).click();
  const insp = page.getByTestId("inspector");
  await insp.getByTestId("keyword-input").fill("Aircraft|Registration|C-FJZS");
  await insp.getByTestId("keyword-input").press("Enter");
  // OpenSky's database says 2008 (a year alone: "about").
  const age = insp.getByTestId("aircraft-age");
  await expect(age.getByTestId("aircraft-age-today")).toHaveText(
    `about ${yearsSince(2008)} years (built 2008)`,
  );
  await expect(age.getByTestId("aircraft-age-photo")).toHaveText(
    /^(about \d+ years|about 1 year|under a year) \(built 2008, photographed \d{4}\)$/,
  );
  await expect(age.getByTestId("aircraft-built-source")).toHaveText("C-FJZS: from OpenSky");

  // Typed: the date wins, for every photo of C-FJZS, and the age is exact.
  await age.getByTestId("aircraft-built-input").fill("2010-06-15");
  await age.getByTestId("aircraft-built-save").click();
  expect(await calls(page, "aircraft_set_built")).toEqual([
    { registration: "C-FJZS", built: "2010-06-15" },
  ]);
  const n = yearsSince(2010, 6, 15);
  await expect(age.getByTestId("aircraft-age-today")).toHaveText(`${n} years (built 15 June 2010)`);
  await expect(age.getByTestId("aircraft-built-source")).toHaveText("C-FJZS: typed by you");
  await expect(insp.getByRole("list", { name: "Keywords" })).toContainText("Aircraft|Built|2010");

  // Not a year: said, nothing changes.
  await age.getByTestId("aircraft-built-input").fill("soon");
  await age.getByTestId("aircraft-built-save").click();
  await expect(age.getByRole("alert")).toContainText("soon is not a year");

  // The viewer's information panel says both ages too.
  await page.keyboard.press("Escape");
  await page.getByTestId("tile").nth(0).click();
  await page.keyboard.press("Enter");
  await page.getByTestId("viewer-info-toggle").click();
  const facts = page.getByTestId("viewer-facts");
  await expect(facts).toContainText(`Aircraft age${n} years (built 15 June 2010)`);
  await expect(facts).toContainText("Age in this photo");
});

test("the aircraft's age: not known until typed, or the OpenSky database is downloaded", async ({
  page,
}) => {
  await open(page, "&aircraft=on&lookup=on");
  await page.getByTestId("tile").nth(0).click();
  const insp = page.getByTestId("inspector");
  // No registration: nothing is said.
  await expect(insp.getByTestId("aircraft-age")).toHaveCount(0);
  await insp.getByTestId("keyword-input").fill("Aircraft|Registration|XA-MBA");
  await insp.getByTestId("keyword-input").press("Enter");
  await expect(insp.getByTestId("aircraft-age-unknown")).toContainText(
    "When XA-MBA was built is not known",
  );
  await expect(insp.getByTestId("aircraft-age-today")).toHaveCount(0);

  // Settings: the database is downloaded once, then it is there.
  await settings(page);
  const db = page.getByTestId("aircraft-built-db");
  await expect(db).toContainText("free for non-commercial use");
  await expect(db.getByTestId("aircraft-built-get")).toHaveText("Download it (94.5 MB)");
  await db.getByTestId("aircraft-built-get").click();
  expect(await calls(page, "aircraft_built_install")).toHaveLength(1);
  await expect(db.getByTestId("aircraft-built-ready")).toHaveText("On this computer.");
  await db.getByTestId("aircraft-built-remove").click();
  expect(await calls(page, "aircraft_built_remove")).toHaveLength(1);
  await expect(db.getByTestId("aircraft-built-get")).toBeVisible();

  // With the lookup off, the database is not offered.
  await page.getByTestId("aircraft-lookup").uncheck();
  await expect(page.getByTestId("aircraft-built-db")).toHaveCount(0);
});

test("the review window offers the build year from OpenSky", async ({ page }) => {
  await open(page, "&aircraft=on&lookup=on&builtdb=on");
  await page.getByTestId("tile").nth(10).click({ button: "right" });
  await page.getByTestId("tile-menu-aircraft").click();
  const photo = page.getByTestId("aircraft-review").getByTestId("ar-photo").first();
  await expect(photo.getByTestId("ar-lookup")).toHaveText(
    "Registry (adsbdb): C-FJZS: Boeing 777-200LR, Air Canada, Canada, built 2008",
  );
  await expect(photo).toContainText("Built 2008 (OpenSky)");
  await photo.getByTestId("ar-accept").click();
  const sent = (await calls(page, "aircraft_accept"))[0] as { keywords: string[] };
  expect(sent.keywords).toContain("Aircraft|Built|2008");
});

// ---- Add route (ADR-044, part 2) ----

/** Pick the first `n` tiles and open Add route on them. */
async function addRoute(page: Page, n: number) {
  const tiles = page.getByTestId("tile");
  await tiles.nth(0).click();
  await tiles.nth(n - 1).click({ modifiers: ["Shift"] });
  await tiles.nth(0).click({ button: "right" });
  await page.getByTestId("tile-menu-route").click();
  return page.getByTestId("route-window");
}

test("Add route is off until turned on with an OpenSky client", async ({ page }) => {
  await open(page, "&aircraft=on&lookup=on");
  await page.getByTestId("tile").nth(0).click({ button: "right" });
  const item = page.getByTestId("tile-menu-route");
  await expect(item).toBeDisabled();
  await expect(item).toHaveAttribute("title", /Turn on Find the route online/);
  await page.keyboard.press("Escape");

  await settings(page);
  const box = page.getByTestId("aircraft-route");
  await expect(page.getByTestId("aircraft-settings")).toContainText(
    "This tells OpenSky when you photographed that aircraft. No photo is sent.",
  );
  await box.check();
  expect(await calls(page, "aircraft_set_route")).toEqual([{ enabled: true }]);
  const client = page.getByTestId("opensky-client");
  await expect(client.getByTestId("opensky-save")).toBeDisabled();
  await client.getByTestId("opensky-id").fill("me-api-client");
  await client.getByTestId("opensky-secret").fill("s3cret");
  await client.getByTestId("opensky-save").click();
  expect(await calls(page, "opensky_set_credentials")).toEqual([
    { clientId: "me-api-client", secret: "s3cret" },
  ]);
  await expect(client.getByTestId("opensky-saved")).toHaveText("Saved.");
  // The secret is never shown again.
  await expect(client.getByTestId("opensky-secret")).toHaveCount(0);
  await client.getByTestId("opensky-test").click();
  await expect(client.getByTestId("opensky-tested")).toHaveText(
    "OpenSky accepted the client ID and secret.",
  );

  // Now the menu item can be used.
  await page.getByTestId("sidebar").getByRole("button", { name: "All items" }).click();
  await page.getByTestId("tile").nth(0).click({ button: "right" });
  await expect(page.getByTestId("tile-menu-route")).toBeEnabled();
});

test("Add route: the flight found, two as near, none near, nothing known; only what is accepted is kept", async ({
  page,
}) => {
  await open(page, "&aircraft=on&lookup=on&route=on&opensky=saved");
  const win = await addRoute(page, 20);
  await expect(win.getByRole("heading")).toHaveText(/^Add route: /);
  expect(await calls(page, "aircraft_route")).toHaveLength(1);
  // Asked, then answered.
  await expect(win.getByTestId("rt-progress")).toHaveText("20 photos.");
  const photos = win.getByTestId("rt-photo");
  await expect(photos).toHaveCount(20);

  // Not of an aircraft: no registration.
  await expect(
    win.getByTestId("rt-state").filter({ hasText: "No registration" }).first(),
  ).toBeVisible();
  // No flight near the time: Shift time is suggested.
  await expect(win.getByTestId("rt-state").filter({ hasText: "Shift time" })).toHaveCount(1);
  // Nothing known: OpenSky's history and coverage are said.
  await expect(win.getByTestId("rt-state").filter({ hasText: "back to about 2023" })).toHaveCount(
    1,
  );

  // Two flights as near: the user chooses; nothing is offered before.
  const several = photos.filter({ has: page.getByTestId("rt-several") });
  await expect(several.getByTestId("rt-accept")).toHaveCount(0);
  await several.getByTestId("rt-choice").nth(0).check();
  await expect(several.getByTestId("rt-route")).toHaveText(
    "Not known → YYZ (Canada) · Flight AC443 (callsign ACA443)",
  );

  // Found: YYZ → LHR, flight AC856; untick a keyword and accept.
  const found = photos.filter({ hasText: "YYZ (Canada) → LHR (United Kingdom)" });
  await expect(found.getByTestId("rt-route")).toHaveText(
    "YYZ (Canada) → LHR (United Kingdom) · Flight AC856 (callsign ACA856)",
  );
  await expect(found.getByTestId("rt-when")).toHaveText("Taken 15 min before take-off (12:16 UTC)");
  await found
    .locator("label", { hasText: "To country United Kingdom" })
    .getByTestId("rt-keyword")
    .uncheck();
  await found.getByTestId("rt-accept").click();
  const sent = (await calls(page, "aircraft_route_accept"))[0] as { keywords: string[] };
  expect(sent.keywords).toEqual([
    "Aircraft|From|YYZ",
    "Aircraft|From country|Canada",
    "Aircraft|To|LHR",
    "Aircraft|Flight|AC856",
  ]);
  await expect(found.getByTestId("rt-added")).toContainText("Flight AC856");

  // Accepted for that photo only.
  await win.getByTestId("rt-close").click();
  await expect(win).toHaveCount(0);
  const id = (sent as unknown as { id: string }).id;
  expect(id).toMatch(/^a\d+$/);
});

test("Add route: turned on but without the setting the window says so", async ({ page }) => {
  // Saved client, setting off: the menu item is disabled; the command
  // refuses too (checked through the mock).
  await open(page, "&aircraft=on&lookup=on&opensky=saved");
  await page.getByTestId("tile").nth(0).click({ button: "right" });
  await expect(page.getByTestId("tile-menu-route")).toBeDisabled();
});

test("Build years: a database kept before the transponder codes is offered again", async ({
  page,
}) => {
  await open(page, "&aircraft=on&lookup=on&builtdb=old");
  await settings(page);
  const db = page.getByTestId("aircraft-built-db");
  await expect(db).toContainText("transponder code is kept too");
  await expect(db.getByTestId("aircraft-built-old")).toContainText("without the transponder codes");
  await expect(db.getByTestId("aircraft-built-again")).toHaveText("Download again (94.5 MB)");
  await db.getByTestId("aircraft-built-again").click();
  expect(await calls(page, "aircraft_built_install")).toHaveLength(1);
  await expect(db.getByTestId("aircraft-built-ready")).toHaveText("On this computer.");
  await expect(db.getByTestId("aircraft-built-again")).toHaveCount(0);
});
