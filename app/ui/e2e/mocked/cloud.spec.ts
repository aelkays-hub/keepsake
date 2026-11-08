// Tag with AI (ADR-036, ADR-038): each provider's key and model in
// Settings, the photo menu, the viewer button, choosing the AI and the
// progress window, against the mock backend.

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
  const card = page.getByTestId("cloud-claude");
  await expect(card.getByTestId("cloud-key-state")).toContainText("No API key is saved yet.");
  await expect(card.getByTestId("cloud-key")).toHaveAttribute("type", "password");
  await card.getByTestId("cloud-key").fill("not a key");
  await card.getByTestId("cloud-key-save").click();
  await expect(card.getByRole("alert")).toContainText("does not look like a Claude API key");

  await card.getByTestId("cloud-key").fill("sk-ant-api03-mock-key-123456");
  await card.getByTestId("cloud-key").press("Enter");
  await expect(card.getByTestId("cloud-key-state")).toContainText("An API key is saved");
  await expect(card.getByTestId("cloud-key")).toHaveValue("");
  await expect(card.getByTestId("cloud-saved")).toHaveText("The key is saved.");

  await expect(card.getByTestId("cloud-model")).toHaveValue("claude-opus-5-5");
  await card.getByTestId("cloud-model").selectOption("claude-haiku-5-5");
  expect(await calls(page, "ai_cloud_set_model")).toEqual([{ model: "claude-haiku-5-5" }]);

  await card.getByTestId("cloud-key-remove").click();
  await expect(card.getByTestId("cloud-key-state")).toContainText("No API key is saved yet.");
  expect(await calls(page, "ai_cloud_set_key")).toEqual([
    { providerId: "claude", key: "not a key" },
    { providerId: "claude", key: "sk-ant-api03-mock-key-123456" },
    { providerId: "claude", key: null },
  ]);
});

test("Settings: each provider has its own key, model and note", async ({ page }) => {
  await open(page);
  await settings(page);
  const openai = page.getByTestId("cloud-openai");
  const gemini = page.getByTestId("cloud-gemini");
  await expect(openai.getByRole("heading")).toHaveText("ChatGPT (OpenAI)");
  await expect(gemini.getByRole("heading")).toHaveText("Gemini (Google)");
  // A Claude key is not taken as a ChatGPT one.
  await openai.getByTestId("cloud-key").fill("sk-ant-api03-mock-key-123456");
  await openai.getByTestId("cloud-key-save").click();
  await expect(openai.getByRole("alert")).toContainText("does not look like a ChatGPT API key");
  await openai.getByTestId("cloud-key").fill("sk-proj-mock-key-1234567890");
  await openai.getByTestId("cloud-key-save").click();
  await expect(openai.getByTestId("cloud-key-state")).toContainText("An API key is saved");
  await expect(page.getByTestId("cloud-claude").getByTestId("cloud-key-state")).toContainText(
    "No API key",
  );
  await gemini.getByTestId("cloud-model").selectOption("gemini-3.5-flash-lite");
  await expect(gemini.getByTestId("cloud-model")).toHaveValue("gemini-3.5-flash-lite");
  await expect(gemini.getByTestId("cloud-note")).toContainText("billing turned on");
  await expect(openai.getByTestId("cloud-note")).toContainText("30 days");
});

test("right-click photos: Tag with AI asks first, then tags them", async ({ page }) => {
  await open(page, "&claude=key");
  const tiles = page.getByTestId("tile");
  await tiles.nth(0).click();
  await tiles.nth(2).click({ modifiers: ["Shift"] });
  await tiles.nth(1).click({ button: "right" });
  const menu = page.getByTestId("tile-menu");
  await expect(menu).toBeVisible();
  await menu.getByTestId("tile-menu-cloud").click();
  await expect(menu).toHaveCount(0);

  const win = page.getByTestId("ai-job");
  await expect(win.getByRole("heading")).toHaveText("Tagging with Claude: 3 photos");
  await expect(win.getByTestId("ai-job-status")).toHaveText(
    "Send 3 photos to Claude Opus 5.5 to be tagged? It costs about $0.06, charged by Anthropic.",
  );
  // One AI with a key: nothing to choose.
  await expect(win.getByTestId("ai-job-provider")).toHaveCount(0);
  // Nothing is sent before Send.
  expect(await calls(page, "ai_cloud_tag")).toEqual([]);
  await win.getByTestId("ai-job-send").click();
  await expect(win.getByTestId("ai-job-status")).toHaveText("3 of 3 photos tagged.");
  const sent = await calls(page, "ai_cloud_tag");
  expect(sent).toHaveLength(1);
  const first = sent[0] as { selection: { ranges: number[][] }; providerId: string };
  expect(first.selection.ranges).toEqual([[0, 3]]);
  expect(first.providerId).toBe("claude");

  // The tags are keywords now.
  await win.getByTestId("ai-job-close").click();
  await expect(win).toHaveCount(0);
  await tiles.nth(1).click();
  await expect(page.getByTestId("inspector")).toContainText("AI|Sand Castle");
});

test("with several keys the user picks the AI, and it is offered first next time", async ({
  page,
}) => {
  await open(page, "&claude=key&openai=key&gemini=key");
  await page.getByTestId("tile").first().click({ button: "right" });
  await page.getByTestId("tile-menu-cloud").click();
  const win = page.getByTestId("ai-job");
  const pick = win.getByTestId("ai-job-provider");
  await expect(pick).toHaveValue("claude");
  await expect(pick.locator("option")).toHaveText([
    "Claude (Claude Opus 5.5)",
    "ChatGPT (GPT-6.1 Sol)",
    "Gemini (Gemini 3.6 Flash)",
    "All (Claude, ChatGPT and Gemini)",
  ]);
  await pick.selectOption("gemini");
  await expect(win.getByRole("heading")).toContainText("Tagging with Gemini:");
  await expect(win.getByTestId("ai-job-status")).toContainText(
    "to Gemini 3.6 Flash to be tagged? It costs under 1¢, charged by Google.",
  );
  await expect(win.getByTestId("ai-job-privacy")).toContainText("free-tier key");
  await pick.selectOption("openai");
  await expect(win.getByTestId("ai-job-status")).toContainText("charged by OpenAI.");
  await win.getByTestId("ai-job-send").click();
  await expect(win.getByTestId("ai-job-status")).toHaveText("1 of 1 photos tagged.");
  expect((await calls(page, "ai_cloud_tag"))[0]).toMatchObject({ providerId: "openai" });
  expect(await calls(page, "ai_cloud_set_default")).toEqual([{ providerId: "openai" }]);
  // Status and Stop go to the same provider.
  for (const c of await calls(page, "ai_cloud_status"))
    expect(c).toMatchObject({ providerId: "openai" });
  await win.getByTestId("ai-job-close").click();

  await page.getByTestId("tile").nth(3).click({ button: "right" });
  await page.getByTestId("tile-menu-cloud").click();
  await expect(win.getByTestId("ai-job-provider")).toHaveValue("openai");
});

test("without a key the window points to Settings", async ({ page }) => {
  await open(page);
  await page.getByTestId("tile").first().click({ button: "right" });
  await page.getByTestId("tile-menu-cloud").click();
  const win = page.getByTestId("ai-job");
  await expect(win.getByTestId("ai-job-status")).toContainText(
    "Add an API key for Claude, ChatGPT or Gemini",
  );
  await expect(win.getByTestId("ai-job-send")).toHaveCount(0);
  await win.getByTestId("ai-job-settings").click();
  await expect(page.getByTestId("settings-screen")).toBeVisible();
  await expect(win).toHaveCount(0);
});

test("the viewer tags one photo, and a refused key says why", async ({ page }) => {
  await open(page, "&gemini=bad");
  await page.getByTestId("tile").first().dblclick();
  await page.getByTestId("viewer-cloud").click();
  const win = page.getByTestId("ai-job");
  // Claude is the default but has no key: the AI with one is offered.
  await expect(win.getByTestId("ai-job-status")).toContainText("Send 1 photo to Gemini 3.6 Flash");
  await win.getByTestId("ai-job-send").click();
  await expect(win.getByTestId("ai-job-status")).toHaveText(
    "0 of 1 photos tagged. 1 photo could not be: Gemini did not accept the API key: check it in Settings",
  );
  const sent = await calls(page, "ai_cloud_tag");
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

test("All sends the photos to every AI with a key, and the inspector shows who gave what", async ({
  page,
}) => {
  await open(page, "&claude=key&openai=key");
  await page.getByTestId("tile").first().click();
  await page.getByTestId("tile").first().click({ button: "right" });
  await page.getByTestId("tile-menu-cloud").click();
  const win = page.getByTestId("ai-job");
  const pick = win.getByTestId("ai-job-provider");
  await pick.selectOption("all");
  await expect(win.getByRole("heading")).toContainText("Tagging with Claude and ChatGPT:");
  await expect(win.getByTestId("ai-job-status")).toHaveText(
    "Send 1 photo to Claude Opus 5.5 and GPT-6.1 Sol to be tagged? Each one tags them. " +
      "It costs about $0.03 in all, charged by Anthropic and OpenAI.",
  );
  await expect(win.getByTestId("ai-job-privacy")).toContainText("30 days");
  await win.getByTestId("ai-job-send").click();
  await expect(win.getByTestId("ai-job-status")).toHaveText(
    "Photos tagged: Claude 1 of 1, ChatGPT 1 of 1.",
  );
  const sent = (await calls(page, "ai_cloud_tag")) as { providerId: string }[];
  expect(sent.map((c) => c.providerId)).toEqual(["claude", "openai"]);
  expect(await calls(page, "ai_cloud_set_default")).toEqual([{ providerId: "all" }]);
  await win.getByTestId("ai-job-close").click();

  // Each AI has its own row; Beach, which both gave, is in both.
  const by = page.getByTestId("ai-sources");
  await expect(by.getByTestId("ai-source-claude")).toContainText("Claude");
  await expect(
    by.getByRole("list", { name: "Keywords from Claude" }).getByRole("listitem"),
  ).toHaveText(["AI|Sand Castle", "Beach"]);
  await expect(
    by.getByRole("list", { name: "Keywords from ChatGPT" }).getByRole("listitem"),
  ).toHaveText(["Beach", "Sky"]);
  await expect(by.getByTestId("ai-source-gemini")).toHaveCount(0);

  // Next time All is offered first.
  await page.getByTestId("tile").nth(3).click({ button: "right" });
  await page.getByTestId("tile-menu-cloud").click();
  await expect(win.getByTestId("ai-job-provider")).toHaveValue("all");
});
