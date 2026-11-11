// Edit suggestions (ADR-042): the photo menu, the viewer button and the
// window (measured suggestions, asking an AI, Open in Photoshop / Topaz
// Photo), against the mock backend.

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

/** Right-click the first photo (not a video) and choose Edit suggestions. */
async function fromMenu(page: Page) {
  const tiles = page.getByTestId("tile");
  await tiles.nth(0).click({ button: "right" });
  const menu = page.getByTestId("item-menu");
  const item = menu.getByTestId("tile-menu-edit-advice");
  await expect(item).toHaveText("Edit suggestions…");
  await item.click();
  await expect(menu).toHaveCount(0);
  const win = page.getByTestId("edit-advice");
  await expect(win.getByRole("heading", { level: 2 })).toContainText("Edit suggestions:");
  return win;
}

test("right-click a photo: suggestions measured here, with Open in buttons", async ({ page }) => {
  await open(page, "&apps=ps");
  const win = await fromMenu(page);
  await expect(win.getByTestId("ea-summary")).toContainText("Keepsake measured the photo");
  const items = win.getByTestId("ea-rules").getByTestId("advice-item");
  await expect(items).toHaveCount(2);
  await expect(items.nth(0)).toContainText("Brighten the photo");
  await expect(items.nth(0)).toContainText("Photoshop: Camera Raw > Light: Exposure +1.8");
  await expect(items.nth(1)).toContainText("Topaz Photo: Denoise");
  await win.getByText("What Keepsake measured").click();
  await expect(win.getByTestId("ea-measured")).toContainText("Dark (middle tone 18%)");

  // No AI key: it says where to add one, and nothing is sent.
  await expect(win.getByTestId("ea-no-key")).toContainText("Settings → Tag with AI");

  // Open in Photoshop opens the original with Photoshop.
  await win.getByTestId("ea-open-photoshop").click();
  await expect(win.getByTestId("ea-note")).toHaveText("Opened with Photoshop");
  const opened = await calls(page, "open_external");
  expect(opened).toHaveLength(1);
  expect(String((opened[0] as { app: string }).app)).toContain("Photoshop.exe");
  await win.getByTestId("ea-open-topaz").click();
  await expect(win.getByTestId("ea-note")).toHaveText("Opened with Topaz Photo");

  // Kept: opening again does not measure again.
  await win.getByTestId("ea-close").click();
  await expect(win).toHaveCount(0);
  await fromMenu(page);
  const asked = (await calls(page, "edit_suggestions")) as { again: boolean }[];
  expect(asked.map((a) => a.again)).toEqual([false, false]);
  await page.getByTestId("ea-again").click();
  await expect(page.getByTestId("ea-summary")).toBeVisible();
  expect(((await calls(page, "edit_suggestions")) as { again: boolean }[]).at(-1)?.again).toBe(
    true,
  );
});

test("without Photoshop or Topaz set up the buttons say where to add them", async ({ page }) => {
  await open(page);
  const win = await fromMenu(page);
  await expect(win.getByTestId("ea-summary")).toBeVisible();
  await expect(win.getByTestId("ea-open-photoshop")).toBeDisabled();
  await expect(win.getByTestId("ea-open-topaz")).toHaveAttribute(
    "title",
    "Add Topaz Photo for this file type in Settings → Open in",
  );
});

test("Ask Claude: shows the cost, waits for the answer, keeps it", async ({ page }) => {
  await open(page, "&claude=key&gemini=key");
  const win = await fromMenu(page);
  await expect(win.getByTestId("ea-ask-line")).toContainText("to Claude, by Anthropic (about 2¢");
  await expect(win.getByTestId("ea-ask-line")).toContainText("The original is not sent.");
  await expect(win.getByTestId("ea-provider")).toHaveValue("claude");
  await win.getByTestId("ea-ask").click();
  expect(await calls(page, "edit_suggestions_ask")).toEqual([
    { id: expect.any(String), providerId: "claude" },
  ]);
  await expect(win.getByTestId("ea-asking")).toHaveText(/Asking Claude…/);
  await expect(win.getByTestId("ea-ai-title")).toHaveText("From Claude (claude-opus-5-5)");
  await expect(win.getByTestId("ea-asking")).toHaveCount(0);
  const ai = win.getByTestId("ea-ai").getByTestId("advice-item");
  await expect(ai).toHaveCount(2);
  await expect(ai.nth(1)).toContainText("Remove the bin on the right");
  await expect(ai.nth(1)).toContainText("Topaz Photo: Remove: brush over the bin");
  await expect(win.getByTestId("ea-ask")).toHaveText("Ask Claude again");

  // Kept: shown at once next time.
  await win.getByTestId("ea-close").click();
  const again = await fromMenu(page);
  await expect(again.getByTestId("ea-ai-title")).toHaveText("From Claude (claude-opus-5-5)");
  expect(await calls(page, "edit_suggestions_ask")).toHaveLength(1);
});

test("a refused key says why; Stop stops a question", async ({ page }) => {
  await open(page, "&gemini=bad");
  const win = await fromMenu(page);
  await win.getByTestId("ea-ask").click();
  await expect(win.getByTestId("ea-asking")).toHaveText(
    "Gemini could not answer: Gemini did not accept the API key: check it in Settings.",
  );
  await win.getByTestId("ea-ask").click();
  await win.getByTestId("ea-stop").click();
  expect(await calls(page, "edit_suggestions_stop")).toHaveLength(1);
  await expect(win.getByTestId("ea-stop")).toHaveCount(0);
});

test("a photo with no preview yet is measured once it has one", async ({ page }) => {
  await open(page, "&suggestWait=1");
  const win = await fromMenu(page);
  await expect(win.getByTestId("ea-looking")).toContainText("Making the preview first");
  await expect(win.getByTestId("ea-summary")).toBeVisible();
});

test("the viewer has an Edit suggestions button", async ({ page }) => {
  await open(page);
  await page.getByTestId("tile").nth(0).dblclick();
  await page.getByTestId("viewer-edit-advice").click();
  await expect(page.getByTestId("edit-advice")).toBeVisible();
  await page.keyboard.press("Escape");
  await expect(page.getByTestId("edit-advice")).toHaveCount(0);
});
