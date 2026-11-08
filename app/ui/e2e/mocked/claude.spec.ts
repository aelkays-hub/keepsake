// Tag with Claude (ADR-036): the key and model in Settings, the photo menu,
// the viewer button and the progress window, against the mock backend.

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

test("Settings: a key is checked, saved without being shown, and removed", async ({ page }) => {
  await open(page);
  await settings(page);
  const card = page.getByTestId("claude-settings");
  await expect(card.getByTestId("claude-key-state")).toHaveText("No API key is saved yet.");
  await expect(card.getByTestId("claude-key")).toHaveAttribute("type", "password");
  await card.getByTestId("claude-key").fill("not a key");
  await card.getByTestId("claude-key-save").click();
  await expect(card.getByRole("alert")).toContainText("does not look like a Claude API key");

  await card.getByTestId("claude-key").fill("sk-ant-api03-mock-key-123456");
  await card.getByTestId("claude-key").press("Enter");
  await expect(card.getByTestId("claude-key-state")).toContainText("An API key is saved");
  await expect(card.getByTestId("claude-key")).toHaveValue("");
  await expect(card.getByTestId("claude-saved")).toHaveText("The key is saved.");

  await expect(card.getByTestId("claude-model")).toHaveValue("claude-opus-5-5");
  await card.getByTestId("claude-model").selectOption("claude-haiku-5-5");
  expect(await calls(page, "ai_claude_set_model")).toEqual([{ model: "claude-haiku-5-5" }]);

  await card.getByTestId("claude-key-remove").click();
  await expect(card.getByTestId("claude-key-state")).toHaveText("No API key is saved yet.");
  expect(await calls(page, "ai_claude_set_key")).toEqual([
    { key: "not a key" },
    { key: "sk-ant-api03-mock-key-123456" },
    { key: null },
  ]);
});

test("right-click photos: Tag with Claude asks first, then tags them", async ({ page }) => {
  await open(page, "&claude=key");
  const tiles = page.getByTestId("tile");
  await tiles.nth(0).click();
  await tiles.nth(2).click({ modifiers: ["Shift"] });
  await tiles.nth(1).click({ button: "right" });
  const menu = page.getByTestId("tile-menu");
  await expect(menu).toBeVisible();
  await menu.getByTestId("tile-menu-claude").click();
  await expect(menu).toHaveCount(0);

  const win = page.getByTestId("ai-job");
  await expect(win.getByRole("heading")).toHaveText("Tagging with Claude: 3 photos");
  await expect(win.getByTestId("ai-job-status")).toHaveText(
    "Send 3 photos to Claude Opus 5.5 to be tagged? It costs about $0.06, charged by Anthropic.",
  );
  // Nothing is sent before Send.
  expect(await calls(page, "ai_claude_tag")).toEqual([]);
  await win.getByTestId("ai-job-send").click();
  await expect(win.getByTestId("ai-job-status")).toHaveText("3 of 3 photos tagged.");
  const sent = await calls(page, "ai_claude_tag");
  expect(sent).toHaveLength(1);
  expect((sent[0] as { selection: { ranges: number[][] } }).selection.ranges).toEqual([[0, 3]]);

  // The tags are keywords now.
  await win.getByTestId("ai-job-close").click();
  await expect(win).toHaveCount(0);
  await tiles.nth(1).click();
  await expect(page.getByTestId("inspector")).toContainText("AI|Sand Castle");
});

test("without a key the window points to Settings", async ({ page }) => {
  await open(page);
  await page.getByTestId("tile").first().click({ button: "right" });
  await page.getByTestId("tile-menu-claude").click();
  const win = page.getByTestId("ai-job");
  await expect(win.getByTestId("ai-job-status")).toContainText("Add your Claude API key");
  await expect(win.getByTestId("ai-job-send")).toHaveCount(0);
  await win.getByTestId("ai-job-settings").click();
  await expect(page.getByTestId("settings-screen")).toBeVisible();
  await expect(win).toHaveCount(0);
});

test("the viewer tags one photo, and a refused key says why", async ({ page }) => {
  await open(page, "&claude=bad");
  await page.getByTestId("tile").first().dblclick();
  await page.getByTestId("viewer-claude").click();
  const win = page.getByTestId("ai-job");
  await expect(win.getByTestId("ai-job-status")).toContainText("Send 1 photo to Claude Opus 5.5");
  await win.getByTestId("ai-job-send").click();
  await expect(win.getByTestId("ai-job-status")).toHaveText(
    "0 of 1 photos tagged. 1 photo could not be: Claude did not accept the API key: check it in Settings",
  );
  const sent = await calls(page, "ai_claude_tag");
  expect((sent[0] as { selection: { ids: string[] } }).selection.ids).toEqual([
    expect.stringMatching(/^a\d+$/),
  ]);
});

test("the Menu key opens the photo menu", async ({ page }) => {
  await open(page, "&claude=key");
  await page.getByTestId("tile").nth(4).click();
  await page.getByTestId("grid").press("Shift+F10");
  await expect(page.getByTestId("tile-menu")).toBeVisible();
  await expect(page.getByTestId("tile-menu").getByRole("menuitem", { name: "Open" })).toBeFocused();
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("tile-menu")).toHaveCount(0);
});
