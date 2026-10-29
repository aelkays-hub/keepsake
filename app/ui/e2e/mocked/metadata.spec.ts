// "All metadata" in the inspector: every tag of the item's file, grouped,
// with a filter, Copy and the JPEG/RAW switch.

import { expect, test, type Page } from "@playwright/test";

const calls = (page: Page, cmd: string) =>
  page.evaluate(
    (c) => (window.__mock?.calls ?? []).filter((x) => x.cmd === c).map((x) => x.args),
    cmd,
  );

// The mock's item kinds (src/mock/backend.ts): video, raw or photo by a hash.
const r = (i: number) => (i * 2654435761) % 1000;
const kindOf = (i: number) => (r(i) < 150 ? "video" : r(i) > 850 ? "raw" : "photo");
const online = (i: number) => i % 4 !== 3;
const find = (f: (i: number) => boolean) => {
  for (let i = 1; i < 2000; i++) if (f(i)) return i;
  throw new Error("no such item");
};
const pad = (i: number) => String(i).padStart(5, "0");

/** Show only the item named `DSC<i>` and select it. */
async function select(page: Page, i: number) {
  const name = `DSC${pad(i)}`;
  await page.getByTestId("search").fill(name);
  await expect(page.getByTestId("count")).toHaveText("1 item");
  // The previous search also showed one item: wait for this one.
  await expect(page.getByTestId("tile").first()).toHaveAttribute("title", new RegExp(name));
  await page.getByTestId("tile").first().click();
  await expect(page.getByTestId("inspector").locator(".name")).toContainText(name);
}

test.beforeEach(async ({ page }) => {
  await page.goto("/?items=2000");
  await expect(page.getByTestId("count")).toHaveText("2,000 items");
});

test("every tag, grouped; filter; stays open from item to item", async ({ page, context }) => {
  await context.grantPermissions(["clipboard-read", "clipboard-write"]);
  const a = find((x) => kindOf(x) === "photo" && online(x));
  await select(page, a);
  const box = page.getByTestId("all-metadata");
  // Closed until asked: nothing is read.
  expect(await calls(page, "metadata_all")).toEqual([]);
  await box.locator("summary").click();
  await expect(box.locator("summary")).toHaveText("All metadata (9)");
  await expect(box.locator("h3")).toHaveText([
    "Camera and image (IFD0)",
    "Exposure (EXIF)",
    "Sony maker notes",
    "XMP (dc)",
    "File",
  ]);
  await expect(box.getByRole("region", { name: "Exposure (EXIF)" })).toContainText(
    "Exposure Time1/250",
  );
  expect(await calls(page, "metadata_all")).toEqual([{ id: `a${a}`, member: "primary" }]);

  await box.getByTestId("meta-filter").fill("iso");
  await expect(box.locator("dt")).toHaveText(["ISO"]);
  await box.getByTestId("meta-filter").fill("nothing like this");
  await expect(box).toContainText("No tags match.");
  await box.getByTestId("meta-filter").fill("");

  await box.getByTestId("meta-copy").click();
  await expect(box.getByTestId("meta-copy")).toHaveText("Copied");
  const text = await page.evaluate(() => navigator.clipboard.readText());
  // Windows' clipboard turns line breaks into CR LF.
  expect(text).toMatch(/\[ExifIFD\]\r?\nExposureTime: 1\/250/);

  // The next item: still open, read again.
  const b = find((x) => x > a && kindOf(x) === "photo" && online(x));
  await select(page, b);
  await expect(box.locator("summary")).toHaveText("All metadata (9)");
  await expect
    .poll(async () => (await calls(page, "metadata_all")).at(-1))
    .toEqual({
      id: `a${b}`,
      member: "primary",
    });
  // …and after a reload too.
  await page.reload();
  await expect(page.getByTestId("count")).toHaveText("2,000 items");
  await select(page, a);
  await expect(page.getByTestId("all-metadata")).toHaveAttribute("open", "");
});

test("a RAW + JPEG pair can show either file's tags", async ({ page }) => {
  const i = find((x) => kindOf(x) === "raw" && x % 2 === 0 && online(x));
  await select(page, i);
  const box = page.getByTestId("all-metadata");
  await box.locator("summary").click();
  await expect(box.getByRole("button", { name: "JPEG" })).toHaveAttribute("aria-pressed", "true");
  await box.getByRole("button", { name: "RAW" }).click();
  await expect(box.getByRole("button", { name: "RAW" })).toHaveAttribute("aria-pressed", "true");
  await expect
    .poll(async () => (await calls(page, "metadata_all")).at(-1))
    .toEqual({
      id: `a${i}`,
      member: "raw",
    });
  await expect(box.locator(".all-meta-file")).toHaveText(/\.ARW$/);
});

test("an item on an unplugged drive says so, without reading", async ({ page }) => {
  const i = find((x) => kindOf(x) === "photo" && !online(x));
  await select(page, i);
  const box = page.getByTestId("all-metadata");
  await box.locator("summary").click();
  await expect(box).toContainText("Connect the drive with this item");
  expect(await calls(page, "metadata_all")).toEqual([]);
});
