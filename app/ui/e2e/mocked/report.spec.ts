// Tools → Report: totals, every breakdown, Unknown groups, unplugged drives
// and the CSV export.

import { expect, test, type Page } from "@playwright/test";

const calls = (page: Page, cmd: string) =>
  page.evaluate(
    (c) => (window.__mock?.calls ?? []).filter((x) => x.cmd === c).map((x) => x.args),
    cmd,
  );

const num = (s: string | null) => Number((s ?? "").replace(/,/g, ""));

async function open(page: Page, items: number) {
  await page.goto(`/?items=${items}`);
  await expect(page.getByTestId("count")).toHaveText(`${items.toLocaleString("en-US")} items`);
  await page.getByTestId("sidebar").getByRole("button", { name: "Report", exact: true }).click();
  const screen = page.getByTestId("report-screen");
  await expect(screen).toBeVisible();
  return screen;
}

test("totals and breakdowns by year, month, camera, lens, type and drive", async ({ page }) => {
  const screen = await open(page, 1000);
  const photos = num(await screen.getByTestId("report-total-photos").textContent());
  const videos = num(await screen.getByTestId("report-total-videos").textContent());
  const files = num(await screen.getByTestId("report-total-files").textContent());
  expect(photos + videos).toBe(1000);
  expect(videos).toBeGreaterThan(0);
  // RAW+JPEG pairs: one photo, two files.
  expect(files).toBeGreaterThan(1000);
  await expect(screen.getByTestId("report-total-bytes")).toHaveText(/GB$/);

  const table = screen.getByTestId("report-table");
  const body = table.locator("tbody tr");
  // Years newest first; undated items last, as Unknown.
  await expect(table.locator("thead th").first()).toHaveText("Year");
  await expect(body.first().locator("th")).toHaveText("2024");
  await expect(body.last().locator("th")).toHaveText("Unknown");
  // The footer is the grand total.
  await expect(table.locator("tfoot td").nth(2)).toHaveText(
    (photos + videos).toLocaleString("en-US"),
  );

  await screen.getByTestId("report-by-month").click();
  await expect(screen.getByTestId("report-by-month")).toHaveAttribute("aria-pressed", "true");
  await expect(body.first().locator("th")).toHaveText(/^[A-Z][a-z]+ 2024$/);

  await screen.getByTestId("report-by-camera").click();
  await expect(body).toHaveCount(4);
  await expect(table).toContainText("ILCE-7M4");

  await screen.getByTestId("report-by-lens").click();
  await expect(table).toContainText("FE 24-70mm F2.8 GM II");

  await screen.getByTestId("report-by-type").click();
  await expect(body.locator("th")).toHaveText(["JPG", "MP4", "ARW"]);
  await expect(screen.getByText("counts as an item under both of its file types")).toBeVisible();

  await screen.getByTestId("report-by-drive").click();
  await expect(body).toHaveCount(2);
  await expect(table).toContainText("Archive (unplugged) (unplugged)");
  await expect(table.locator("tfoot th")).toHaveText("All drives");

  await screen.getByTestId("report-by-day").click();
  await expect(body.first().locator("th")).toHaveText(/^\d{1,2} [A-Z][a-z]+ 2024$/);
});

test("export writes the CSV where the save dialog says", async ({ page }) => {
  const screen = await open(page, 200);
  await screen.getByTestId("report-export").click();
  await expect(screen.getByTestId("report-saved")).toHaveText(
    "Saved to C:\\Users\\me\\Documents\\Keepsake report.csv",
  );
  expect(await calls(page, "library_report_csv")).toEqual([
    { path: "C:\\Users\\me\\Documents\\Keepsake report.csv" },
  ]);
  // Refresh asks the backend again.
  const before = (await calls(page, "library_report")).length;
  await screen.getByTestId("report-refresh").click();
  await expect.poll(async () => (await calls(page, "library_report")).length).toBe(before + 1);
});

test("an empty library says so", async ({ page }) => {
  await page.goto("/?items=0");
  await page.getByTestId("sidebar").getByRole("button", { name: "Report", exact: true }).click();
  const screen = page.getByTestId("report-screen");
  await expect(screen.getByTestId("report-total-photos")).toHaveText("0");
  await expect(screen.getByText("The library is empty.")).toBeVisible();
});
