// On-device AI (ADR-035): Settings, the Duplicates screen and AI tags in
// the inspector, against the mock backend.

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

const tool = (page: Page, name: string) =>
  page.getByTestId("sidebar").getByRole("button", { name, exact: true }).click();

test("duplicates: pick a keeper, the rest are not kept and can be hidden", async ({ page }) => {
  await open(page);
  await tool(page, "Duplicates");
  const screen = page.getByTestId("duplicates-screen");
  await expect(screen.getByTestId("dup-totals")).toHaveText("3 groups, 3 to decide");
  const groups = screen.getByTestId("dup-group");
  await expect(groups).toHaveCount(3);
  const first = groups.first();
  await expect(first.getByTestId("dup-member")).toHaveCount(3);
  await expect(first.getByTestId("dup-member").first()).toContainText("Suggested");

  // Click the second photo: it is kept, the others are not.
  await first.getByTestId("dup-member").nth(1).click();
  await expect(first.getByTestId("dup-member").nth(1)).toContainText("Kept");
  await expect(first.getByTestId("dup-member").nth(0)).toContainText("Not kept");
  await expect(screen.getByTestId("dup-totals")).toHaveText("3 groups, 2 to decide");
  expect(await calls(page, "ai_pick_keeper")).toEqual([
    { keeper: "a1", group: ["a0", "a1", "a2"] },
  ]);

  // Keyboard: N on the second group says "not duplicates"; it goes away.
  await screen.getByTestId("dup-groups").focus();
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("n");
  await expect(groups).toHaveCount(2);

  // Undo puts the first group back to undecided.
  await groups.first().getByTestId("dup-undo").click();
  await expect(groups.first().getByTestId("dup-member").first()).toContainText("Suggested");

  // Keep again, then hide the not-kept items in the library.
  await groups.first().getByTestId("dup-keep-suggested").click();
  await tool(page, "All items");
  await page.getByTestId("hide-not-kept").check();
  await expect(page.getByTestId("count")).toHaveText("998 items");
  await page.getByTestId("hide-not-kept").uncheck();
  await expect(page.getByTestId("count")).toHaveText("1,000 items");
});

test("duplicates with AI off point to Settings", async ({ page }) => {
  await open(page, "&ai=off");
  await tool(page, "Duplicates");
  await page.getByTestId("dup-open-settings").click();
  await expect(page.getByTestId("settings-screen")).toBeVisible();
  const ai = page.getByTestId("ai-settings");
  await expect(ai.getByTestId("ai-progress")).toHaveText("Off.");
  await expect(ai).toContainText("downloads the model once (371.8 MB)");
  await ai.getByTestId("ai-enabled").check();
  await expect(ai.getByTestId("ai-progress")).toContainText("analysed so far");
  expect(await calls(page, "ai_enable")).toEqual([{ enabled: true, gpu: null }]);
  await ai.getByTestId("ai-enabled").uncheck();
  await expect(ai.getByTestId("ai-remove")).toBeVisible();
  await ai.getByTestId("ai-remove").click();
  await expect(ai.getByTestId("ai-remove")).toHaveCount(0);
});

const calNode = (page: Page, key: string) =>
  page.getByTestId("sidebar").locator(`button[data-cal="${key}"]`);

test("right-click a year: find duplicates for those dates only", async ({ page }) => {
  await open(page);
  await calNode(page, "2015").click({ button: "right" });
  const menu = page.getByTestId("date-menu");
  await expect(menu.getByRole("menuitem")).toHaveText(["Show", "Find duplicates", "Tag photos"]);
  await menu.getByTestId("date-menu-dups").click();
  expect(await calls(page, "ai_analyze_range")).toEqual([
    { from: "2015-01-01", to: "2015-12-31", what: "duplicates" },
  ]);

  const screen = page.getByTestId("duplicates-screen");
  await expect(screen.getByTestId("dup-title")).toHaveText("Duplicates · 2015");
  // It shows the comparison running, then the groups of those dates.
  await expect(screen.getByTestId("dup-ai-status")).toHaveText(/compared\.$/, {
    timeout: 15_000,
  });
  await expect(screen.getByTestId("dup-group").first()).toBeVisible();
  expect((await calls(page, "ai_duplicates")).at(-1)).toEqual({
    from: "2015-01-01",
    to: "2015-12-31",
  });

  // Another year has none; "All analysed dates" shows every day again.
  await calNode(page, "2016").click({ button: "right" });
  await page.getByTestId("date-menu-dups").click();
  await expect(screen.getByTestId("dup-title")).toHaveText("Duplicates · 2016");
  await expect(screen.getByTestId("dup-none")).toBeVisible({ timeout: 15_000 });
  await screen.getByTestId("dup-all").click();
  await expect(screen.getByTestId("dup-title")).toHaveText("Duplicates");
  await expect(screen.getByTestId("dup-group").first()).toBeVisible();
});

test("right-click a year: tag its photos, separately from duplicates", async ({ page }) => {
  // Queueing takes a while (a busy drive): the menu closes at once and the
  // progress window says it is getting ready.
  await open(page, "&queueing=1500");
  await calNode(page, "2015").click({ button: "right" });
  await page.getByTestId("date-menu-tags").click();
  await expect(page.getByTestId("date-menu")).toHaveCount(0);
  const win = page.getByTestId("ai-job");
  await expect(win.getByRole("heading")).toHaveText("Tagging 2015");
  await expect(win.getByTestId("ai-job-status")).toHaveText("Getting ready…");
  expect(await calls(page, "ai_analyze_range")).toEqual([
    { from: "2015-01-01", to: "2015-12-31", what: "tags" },
  ]);
  // It stays in the library (on that year); the window has a progress bar.
  await expect(page.getByTestId("duplicates-screen")).toHaveCount(0);
  await expect(win.getByTestId("ai-job-bar")).toBeVisible();
  await expect(win.getByTestId("ai-job-status")).toContainText(/done…$/);
  await expect(win.getByTestId("ai-job-status")).toHaveText(/^[\d,]+ of [\d,]+ photos tagged\.$/, {
    timeout: 15_000,
  });
  expect((await calls(page, "ai_range_status")).every((c) => c.what === "tags")).toBe(true);
  // Finding duplicates was not asked for.
  expect(await calls(page, "ai_duplicates")).toEqual([]);
});

test("Tag photos on dates can be stopped", async ({ page }) => {
  await open(page);
  await calNode(page, "2015").click({ button: "right" });
  await page.getByTestId("date-menu-tags").click();
  const win = page.getByTestId("ai-job");
  await win.getByTestId("ai-job-stop").click();
  await expect(win.getByTestId("ai-job-status")).toContainText("Stopped:");
  expect(await calls(page, "ai_range_stop")).toEqual([
    { from: "2015-01-01", to: "2015-12-31", what: "tags" },
  ]);
});

test("the date menu explains when AI is off, and works from the keyboard", async ({ page }) => {
  await open(page, "&ai=off");
  await page.getByTestId("tile").first().click();
  await expect(page.getByTestId("ai-tags-none")).toHaveText("On-device AI is off (Settings).");
  await calNode(page, "2015").focus();
  await page.keyboard.press("Shift+F10");
  const menu = page.getByTestId("date-menu");
  await expect(menu).toBeVisible();
  await page.keyboard.press("ArrowDown");
  await page.keyboard.press("Enter");
  await expect(menu.getByRole("alert")).toContainText("Turn on On-device AI in Settings");
  await page.keyboard.press("Escape");
  await expect(menu).toHaveCount(0);

  // Tag photos says why in its window, until closed.
  await calNode(page, "2015").click({ button: "right" });
  await page.getByTestId("date-menu-tags").click();
  const win = page.getByTestId("ai-job");
  await expect(win.getByRole("alert")).toContainText(
    "Could not start: Turn on On-device AI in Settings",
  );
  await win.getByTestId("ai-job-close").click();
  await expect(win).toHaveCount(0);
});

test("Find duplicates says it is working while the dates are queued", async ({ page }) => {
  await open(page, "&queueing=1500");
  await calNode(page, "2015").click({ button: "right" });
  await page.getByTestId("date-menu-dups").click();
  await expect(page.getByTestId("menu-busy")).toHaveText("Working…");
  await expect(page.getByTestId("duplicates-screen")).toBeVisible();
  await expect(page.getByTestId("date-menu")).toHaveCount(0);
});

test("the toolbar's tag list shows only items with that tag", async ({ page }) => {
  await open(page);
  const tag = page.getByTestId("tag");
  await expect(tag.locator("option").first()).toHaveText("Any tag");
  // Each of the 6 mock AI tags is on a third of the items.
  await expect(tag.locator("option")).toHaveCount(7);
  await expect(tag.locator("option", { hasText: "Dog" })).toHaveText("Dog (334)");
  await tag.selectOption("Dog");
  await expect(page.getByTestId("count")).toHaveText("334 items");
  expect((await calls(page, "view_open")).at(-1)).toMatchObject({ query: { tag: "Dog" } });
  // ✕ Clear shows everything again.
  await page.getByRole("button", { name: "Clear filters" }).click();
  await expect(tag).toHaveValue("");
  await expect(page.getByTestId("count")).toHaveText("1,000 items");
});

test("AI tags in the inspector: remove one, keep one as a keyword", async ({ page }) => {
  await open(page);
  await page.getByTestId("tile").first().click();
  const tags = page.getByTestId("ai-tags");
  const chips = tags.getByRole("listitem");
  await expect(chips).toHaveCount(2);
  const [first, second] = (await chips.allInnerTexts()).map((t) => t.replace(/[✓✕\s]+$/u, ""));
  await tags.getByRole("button", { name: `Remove AI tag ${first}` }).click();
  await expect(chips).toHaveCount(1);
  await tags.getByRole("button", { name: `Keep ${second} as a keyword` }).click();
  // None left: the section says so instead of vanishing.
  await expect(page.getByTestId("ai-tags-none")).toHaveText(/None/);
  await expect(page.getByRole("list", { name: "Keywords" })).toContainText(second);
  const decided = (await calls(page, "ai_tag_decide")) as { tag: string; keep: boolean }[];
  expect(decided.map((d) => [d.tag, d.keep])).toEqual([
    [first, false],
    [second, true],
  ]);
});

test("right-click No date: tag the photos with no date", async ({ page }) => {
  await open(page);
  await calNode(page, "undated").click({ button: "right" });
  const menu = page.getByTestId("date-menu");
  // No Find duplicates: those are of one day.
  await expect(menu.getByRole("menuitem")).toHaveText(["Show", "Tag photos"]);
  await menu.getByTestId("date-menu-tags").click();
  const win = page.getByTestId("ai-job");
  await expect(win.getByRole("heading")).toHaveText("Tagging No date");
  await expect(win).toContainText("Photos and videos with no date.");
  await expect(win.getByTestId("ai-job-status")).toHaveText(/photos tagged\.$/, {
    timeout: 15_000,
  });
  expect(await calls(page, "ai_analyze_undated")).toEqual([{ what: "tags" }]);
  expect(await calls(page, "ai_analyze_range")).toEqual([]);
});

test("the Calendar entry has no date menu", async ({ page }) => {
  await open(page);
  await calNode(page, "cal").click({ button: "right" });
  await expect(page.getByTestId("date-menu")).toHaveCount(0);
});
