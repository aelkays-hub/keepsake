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
  await expect(ai.getByTestId("ai-progress")).toContainText("items analysed");
  expect(await calls(page, "ai_enable")).toEqual([{ enabled: true, gpu: null }]);
  await ai.getByTestId("ai-enabled").uncheck();
  await expect(ai.getByTestId("ai-remove")).toBeVisible();
  await ai.getByTestId("ai-remove").click();
  await expect(ai.getByTestId("ai-remove")).toHaveCount(0);
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
  await expect(page.getByTestId("ai-tags")).toHaveCount(0);
  await expect(page.getByRole("list", { name: "Keywords" })).toContainText(second);
  const decided = (await calls(page, "ai_tag_decide")) as { tag: string; keep: boolean }[];
  expect(decided.map((d) => [d.tag, d.keep])).toEqual([
    [first, false],
    [second, true],
  ]);
});
