// Upload to Flickr (ADR-046): the Settings card (key, secret, sign-in with
// the code Flickr shows) and the upload window (privacy, tags, progress),
// against the mock backend. Nothing is sent anywhere.

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

/** Right-click the first photo tile (not a video) and choose Upload to Flickr…. */
async function uploadWindow(page: Page, extra = 0) {
  const tiles = page.getByTestId("tile");
  let first = -1;
  for (let i = 0; i < 20 && first < 0; i++) {
    await tiles.nth(i).click({ button: "right" });
    const item = page.getByTestId("tile-menu-flickr");
    if (await item.isEnabled()) first = i;
    else await page.keyboard.press("Escape");
  }
  expect(first).toBeGreaterThanOrEqual(0);
  if (extra > 0) {
    await page.keyboard.press("Escape");
    await tiles.nth(first).click();
    await tiles.nth(first + extra).click({ modifiers: ["Shift"] });
    await tiles.nth(first).click({ button: "right" });
  }
  await page.getByTestId("tile-menu-flickr").click();
  return page.getByTestId("flickr-window");
}

test("Settings: save the key and secret, sign in with Flickr's code, test, sign out", async ({
  page,
}) => {
  await open(page);
  await settings(page);
  const card = page.getByTestId("flickr-settings");
  await expect(card.getByRole("heading")).toHaveText("Flickr");
  await expect(card.getByTestId("flickr-state")).toHaveText("No API key is saved yet.");
  await card.getByTestId("flickr-key").fill("0123456789abcdef0123456789abcdef");
  await expect(card.getByTestId("flickr-key-save")).toBeDisabled();
  await card.getByTestId("flickr-secret").fill("0123456789abcdef");
  await card.getByTestId("flickr-key-save").click();
  await expect(card.getByTestId("flickr-state")).toHaveText(
    "The API key is saved. Sign in to Flickr next.",
  );
  // The key is not shown again.
  await expect(card.getByTestId("flickr-key")).toHaveValue("");

  await card.getByTestId("flickr-sign-in").click();
  expect(await calls(page, "flickr_sign_in_start")).toHaveLength(1);
  await expect(card.getByTestId("flickr-sign-in-url")).toContainText(
    "https://www.flickr.com/services/oauth/authorize?oauth_token=",
  );
  await card.getByTestId("flickr-code").fill("000-000-000");
  await card.getByTestId("flickr-code-save").click();
  await expect(card.getByTestId("flickr-problem")).toContainText("did not accept the sign-in");
  await card.getByTestId("flickr-code").fill("123-456-789");
  await card.getByTestId("flickr-code-save").click();
  await expect(card.getByTestId("flickr-state")).toContainText("Signed in as keepsake-tester.");

  await card.getByTestId("flickr-test").click();
  await expect(card.getByTestId("flickr-said")).toHaveText(
    "Flickr says you are signed in as keepsake-tester.",
  );
  await card.getByTestId("flickr-sign-out").click();
  await expect(card.getByTestId("flickr-state")).toHaveText(
    "The API key is saved. Sign in to Flickr next.",
  );
});

test("Upload: not signed in, the window says so and uploads nothing", async ({ page }) => {
  await open(page, "&flickr=app");
  const win = await uploadWindow(page);
  await expect(win.getByTestId("fl-signed-out")).toContainText("Sign in to Flickr");
  await expect(win.getByTestId("fl-upload")).toBeDisabled();
  await win.getByTestId("fl-close").click();
  await expect(win).toBeHidden();
});

test("Upload: choose public, adjust the tags, see each photo uploaded", async ({ page }) => {
  await open(page, "&flickr=signed");
  const win = await uploadWindow(page);
  await expect(win.getByTestId("fl-account")).toContainText("keepsake-tester");
  const item = win.getByTestId("fl-item").first();
  const tags = item.getByTestId("fl-tags");
  // The keyword's last part, and the camera and lens by default.
  await expect(tags).toContainText("C-FEJC");
  await expect(tags).toContainText("Sony ILCE-1");
  await expect(tags).toContainText("FE 200-600mm F5.6-6.3 G OSS");
  // Private until chosen.
  await expect(win.getByTestId("fl-privacy-private")).toBeChecked();
  await win.getByTestId("fl-privacy-public").check();

  // Machine tags are an option.
  await win.getByTestId("fl-opt-machine").check();
  await expect(tags).toContainText("aircraft:registration=C-FEJC");
  // Remove one, add two.
  await item.getByRole("button", { name: "Remove tag FE 200-600mm F5.6-6.3 G OSS" }).click();
  await expect(tags).not.toContainText("FE 200-600mm");
  await item.getByTestId("fl-tag-add").fill("Air Canada, YYZ");
  await item.getByTestId("fl-tag-add-button").click();
  await expect(tags).toContainText("Air Canada");

  await expect(win.getByTestId("fl-upload")).toHaveText("Upload 1 photo");
  await win.getByTestId("fl-upload").click();
  const sent = (await calls(page, "flickr_upload")) as {
    items: { tags: string[]; title: string }[];
    privacy: string;
  }[];
  expect(sent).toHaveLength(1);
  expect(sent[0].privacy).toBe("public");
  expect(sent[0].items[0].tags).toEqual([
    "C-FEJC",
    "aircraft:registration=C-FEJC",
    "Sony",
    "Sony ILCE-1",
    "Air Canada",
    "YYZ",
  ]);
  await expect(win.getByTestId("fl-progress")).toHaveText("Done: 1 of 1 uploaded.");
  await expect(item.getByTestId("fl-item-state")).toContainText("Uploaded.");
  await item.getByTestId("fl-item-open").click();
  const opened = (await calls(page, "flickr_open")) as { url: string }[];
  expect(opened[0].url).toMatch(/^https:\/\/www\.flickr\.com\/photos\//);
});

test("Upload: a photo already on Flickr is not ticked; a refusal is explained", async ({
  page,
}) => {
  // Find the first photo's id, then open again with it uploaded.
  await open(page, "&flickr=signed");
  let win = await uploadWindow(page);
  const id = await win
    .getByTestId("fl-item")
    .first()
    .locator("strong")
    .innerText()
    .then((n) => n.replace(/\.ARW$/, ""));
  await open(page, `&flickr=signed&flickrDone=${id}`);
  win = await uploadWindow(page);
  const item = win.getByTestId("fl-item").first();
  await expect(item.getByTestId("fl-already")).toContainText(
    "On Flickr already (public, 2026-11-01)",
  );
  await expect(item.getByTestId("fl-tick")).not.toBeChecked();
  await expect(win.getByTestId("fl-upload")).toBeDisabled();
  await win.getByTestId("fl-close").click();

  await open(page, `&flickr=signed&flickrFail=${id}`);
  win = await uploadWindow(page);
  await win.getByTestId("fl-upload").click();
  await expect(win.getByTestId("fl-progress")).toHaveText("Done: 0 of 1 uploaded, 1 not.");
  await expect(win.getByTestId("fl-item").first().getByTestId("fl-item-state")).toContainText(
    "upload limit is reached",
  );
});

test("Upload: several photos at once, with a tag for all", async ({ page }) => {
  await open(page, "&flickr=signed");
  const win = await uploadWindow(page, 2);
  const items = win.getByTestId("fl-item");
  await expect(items).toHaveCount(3);
  const ticked = await win
    .getByTestId("fl-tick")
    .evaluateAll((els) => els.filter((e) => (e as HTMLInputElement).checked).length);
  expect(ticked).toBeGreaterThanOrEqual(1);
  await expect(win.getByTestId("fl-upload")).toHaveText(
    ticked === 1 ? "Upload 1 photo" : `Upload ${ticked} photos`,
  );
  await win.getByTestId("fl-tag-all").fill("Spotting");
  await win.getByTestId("fl-tag-all-add").click();
  await win.getByTestId("fl-upload").click();
  const sent = (await calls(page, "flickr_upload")) as { items: { tags: string[] }[] }[];
  expect(sent[0].items).toHaveLength(ticked);
  for (const it of sent[0].items) expect(it.tags).toContain("Spotting");
  await expect(win.getByTestId("fl-progress")).toHaveText(`Done: ${ticked} of ${ticked} uploaded.`);
});
