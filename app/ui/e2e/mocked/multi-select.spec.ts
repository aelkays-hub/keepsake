// Selecting several items without modifier keys (tick-boxes, Select mode,
// dragging a rectangle) and editing them together in the inspector.

import { expect, test, type Page } from "@playwright/test";

const calls = (page: Page, cmd: string) =>
  page.evaluate(
    (c) => (window.__mock?.calls ?? []).filter((x) => x.cmd === c).map((x) => x.args),
    cmd,
  );

const tile = (page: Page, i: number) => page.locator(`[data-testid="tile"][data-index="${i}"]`);

test.beforeEach(async ({ page }) => {
  await page.goto("/?items=200");
  await expect(page.getByTestId("count")).toHaveText("200 items");
  await expect(tile(page, 0)).toBeVisible();
});

test("the tick-box on a tile adds and removes it", async ({ page }) => {
  await tile(page, 0).click();
  await tile(page, 2).getByTestId("tile-check").click();
  await tile(page, 4).getByTestId("tile-check").click();
  await expect(page.getByTestId("selected")).toHaveText("3 selected");
  await expect(tile(page, 2)).toHaveAttribute("aria-selected", "true");
  await expect(page.getByTestId("multi")).toContainText("3 items selected");
  // Again: out of the selection.
  await tile(page, 2).getByTestId("tile-check").click();
  await expect(page.getByTestId("selected")).toHaveText("2 selected");
  await expect(tile(page, 2)).toHaveAttribute("aria-selected", "false");
  // A plain click on a tile still selects just that one.
  await tile(page, 7).click();
  await expect(page.getByTestId("selected")).toHaveText("1 selected");
});

test("in Select mode a plain click adds; Esc leaves the mode", async ({ page }) => {
  await page.getByTestId("select-mode").click();
  await expect(page.getByTestId("select-mode")).toHaveAttribute("aria-pressed", "true");
  for (const i of [1, 3, 5]) await tile(page, i).click();
  await expect(page.getByTestId("selected")).toHaveText("3 selected");
  await tile(page, 3).click();
  await expect(page.getByTestId("selected")).toHaveText("2 selected");
  await page.getByTestId("select-all").click();
  await expect(page.getByTestId("selected")).toHaveText("200 selected");
  await page.getByTestId("select-none").click();
  await expect(page.getByTestId("selected")).toHaveCount(0);
  await tile(page, 0).click();
  await page.getByTestId("grid").press("Escape");
  await expect(page.getByTestId("select-mode")).toHaveAttribute("aria-pressed", "false");
  await expect(page.getByTestId("selected")).toHaveCount(0);
});

test("dragging a rectangle selects the tiles inside it", async ({ page }) => {
  const a = await tile(page, 0).boundingBox();
  const b = await tile(page, 1).boundingBox();
  if (!a || !b) throw new Error("tiles not laid out");
  await page.mouse.move(a.x + a.width / 2, a.y + a.height / 2);
  await page.mouse.down();
  await page.mouse.move(b.x + b.width / 2, b.y + b.height / 2, { steps: 5 });
  await expect(page.getByTestId("marquee")).toBeVisible();
  await page.mouse.up();
  await expect(page.getByTestId("marquee")).toHaveCount(0);
  await expect(page.getByTestId("selected")).toHaveText("2 selected");
  await expect(tile(page, 0)).toHaveAttribute("aria-selected", "true");
  await expect(tile(page, 1)).toHaveAttribute("aria-selected", "true");

  // With Ctrl held, a second rectangle adds to the first.
  const c = await tile(page, 3).boundingBox();
  if (!c) throw new Error("tile 3 not laid out");
  await page.keyboard.down("Control");
  await page.mouse.move(c.x + c.width / 2, c.y + c.height / 2);
  await page.mouse.down();
  await page.mouse.move(c.x + c.width / 2 + 10, c.y + c.height / 2 + 10, { steps: 3 });
  await page.mouse.up();
  await page.keyboard.up("Control");
  await expect(page.getByTestId("selected")).toHaveText("3 selected");
});

test("the inspector edits every selected item, and shows what they share", async ({ page }) => {
  await tile(page, 10).click();
  await tile(page, 11).getByTestId("tile-check").click();
  await tile(page, 12).getByTestId("tile-check").click();
  await expect(page.getByTestId("multi")).toContainText("3 items selected");
  const insp = page.getByTestId("inspector");

  // Rating: all three.
  await insp.getByRole("radio", { name: "4 stars" }).click();
  await expect(insp.getByRole("radio", { name: "4 stars" })).toHaveAttribute(
    "aria-checked",
    "true",
  );
  await expect(insp.getByTestId("rating-mixed")).toHaveCount(0);

  // A keyword for all three.
  await insp.getByTestId("keyword-input").fill("Places|Lake");
  await insp.getByTestId("keyword-input").press("Enter");
  await expect(insp.getByRole("listitem").filter({ hasText: "Places|Lake" })).toBeVisible();

  // The comment box works with several selected, and sets them all.
  const comment = insp.getByTestId("comment");
  await expect(comment).toBeEnabled();
  await comment.fill("Summer trip");
  await comment.press("Control+Enter");
  await expect.poll(async () => (await calls(page, "edit_comment")).length).toBe(1);
  const [c] = await calls(page, "edit_comment");
  expect(c.comment).toBe("Summer trip");
  expect((c.selection as { ranges: number[][] }).ranges).toEqual([[10, 13]]);

  // One more item: the keyword is now on 3 of 4, the rating is mixed.
  await tile(page, 13).getByTestId("tile-check").click();
  await expect(page.getByTestId("multi")).toContainText("4 items selected");
  const chip = insp.getByRole("listitem").filter({ hasText: "Places|Lake" });
  await expect(chip).toContainText("3 of 4");
  await expect(comment).toHaveAttribute("placeholder", /Different comments/);
  // "+" puts it on all four.
  await chip.getByRole("button", { name: "Add Places|Lake to all" }).click();
  await expect(chip).not.toContainText("of 4");

  // Back to one item: its own values.
  await tile(page, 11).click();
  await expect(page.getByTestId("multi")).toHaveCount(0);
  await expect(comment).toHaveValue("Summer trip");
});
