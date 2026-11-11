// Recognise faces (ADR-040, step 2): the Settings card against the mock
// backend: turning it on, searching every photo, stopping, and deleting all
// face data (asked first).

import { expect, test, type Page } from "@playwright/test";

const calls = (page: Page, cmd: string) =>
  page.evaluate(
    (c) => (window.__mock?.calls ?? []).filter((x) => x.cmd === c).map((x) => x.args),
    cmd,
  );

async function settings(page: Page, query = "") {
  await page.goto(`/?items=1000${query}`);
  await expect(page.getByTestId("count")).toHaveText("1,000 items");
  await page.getByTestId("sidebar").getByRole("button", { name: "Settings" }).click();
  return page.getByTestId("faces-settings");
}

test("turning it on, searching every photo and deleting all face data", async ({ page }) => {
  const card = await settings(page);
  await expect(card.getByRole("heading")).toHaveText("Recognise faces");
  await expect(card.getByTestId("faces-progress")).toHaveText("Off.");
  await expect(card).toContainText("downloads two face models once");
  await card.getByTestId("faces-enabled").check();
  expect(await calls(page, "faces_enable")).toEqual([{ enabled: true }]);
  await expect(card.getByTestId("faces-progress")).toContainText(/^0 of [\d,]+ photos searched/);

  // Every photo (not the videos) is to be searched.
  await card.getByTestId("faces-scan-all").click();
  await expect(card.getByTestId("faces-note")).toHaveText(/^Searching [\d,]+ photos…$/);
  await expect(card.getByTestId("faces-stop")).toBeVisible();
  await expect(card.getByTestId("faces-progress")).toContainText(
    /faces in [\d,]+ of [\d,]+ photos searched/,
    {
      timeout: 15_000,
    },
  );
  await expect(card.getByTestId("faces-scan-all")).toHaveText("Every photo is searched");

  // Deleting is asked first; Cancel keeps everything.
  await card.getByTestId("faces-delete").click();
  const ask = card.getByTestId("faces-delete-ask");
  await expect(ask).toContainText(/Forget [\d,]+ faces and 0 people\?/);
  await ask.getByTestId("faces-delete-no").click();
  expect(await calls(page, "faces_delete_all")).toEqual([]);
  await card.getByTestId("faces-delete").click();
  await card.getByTestId("faces-delete-yes").click();
  await expect(card.getByTestId("faces-note")).toHaveText("All face data was deleted.");
  await expect(card.getByTestId("faces-progress")).toContainText(/^0 of [\d,]+ photos searched/);
});

test("Stop ends a search", async ({ page }) => {
  const card = await settings(page, "&faces=on");
  await card.getByTestId("faces-scan-all").click();
  await card.getByTestId("faces-stop").click();
  await expect(card.getByTestId("faces-note")).toHaveText("Stopped.");
  expect(await calls(page, "faces_stop")).toHaveLength(1);
});

test("faces need On-device AI", async ({ page }) => {
  const card = await settings(page, "&ai=off");
  await expect(card.getByTestId("faces-progress")).toHaveText("Needs On-device AI (just above).");
  await expect(card.getByTestId("faces-enabled")).toBeDisabled();
});
