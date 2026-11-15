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

test("a row's name opens its items, and the heading leads back", async ({ page }) => {
  const screen = await open(page, 1000);
  const table = screen.getByTestId("report-table");
  const check = async (by: string, row: number) => {
    await screen.getByTestId(`report-by-${by}`).click();
    const tr = table.locator("tbody tr").nth(row);
    const name = (await tr.locator("th button").textContent()) ?? "";
    const items = num(await tr.locator("td").nth(2).textContent());
    await tr.locator("th button").click();
    await expect(page.getByTestId("heading")).toContainText(name);
    await expect(page.getByTestId("count")).toHaveText(
      `${items.toLocaleString("en-US")} item${items === 1 ? "" : "s"}`,
    );
    // Back to the report, on the same breakdown.
    await page.getByTestId("heading").getByRole("button", { name: "Report" }).click();
    await expect(screen.getByTestId(`report-by-${by}`)).toHaveAttribute("aria-pressed", "true");
  };
  await check("year", 0);
  await check("month", 1);
  await check("camera", 0);
  await check("lens", 2);
  await check("type", 1);
  await check("drive", 0);
});

test("a row opens its items without the filters set before", async ({ page }) => {
  await page.goto("/?items=1000");
  await page.getByTestId("type").selectOption("video");
  await page.getByTestId("search").fill("zzz-nothing");
  await expect(page.getByTestId("count")).toHaveText("0 items");
  await page.getByTestId("sidebar").getByRole("button", { name: "Report", exact: true }).click();
  const screen = page.getByTestId("report-screen");
  const tr = screen.getByTestId("report-table").locator("tbody tr").first();
  const items = num(await tr.locator("td").nth(2).textContent());
  await tr.locator("th button").click();
  await expect(page.getByTestId("count")).toHaveText(`${items.toLocaleString("en-US")} items`);
  await expect(page.getByTestId("search")).toHaveValue("");
  await expect(page.getByTestId("type")).toHaveValue("");
});

test("the aircraft report: totals, years, tables, find and export", async ({ page }) => {
  const screen = await open(page, 200);
  await screen.getByTestId("report-kind-aircraft").click();
  await expect(screen.getByTestId("aircraft-total-photos")).toHaveText("120");
  await expect(screen.getByTestId("aircraft-total-shots")).toHaveText("110 distinct shots");
  await expect(screen.getByTestId("aircraft-total-airlines")).toHaveText("3");
  await expect(screen.getByTestId("aircraft-total-registrations")).toHaveText("4");
  await expect(screen.getByTestId("aircraft-total-waiting")).toHaveText("7");
  await expect(screen.getByTestId("aircraft-named")).toContainText(
    "40 of 120 aircraft photos (33%) name an airline, type or registration.",
  );
  // Years oldest first, undated last.
  const years = screen.getByTestId("aircraft-years").locator("tbody th");
  await expect(years).toHaveText(["2018", "2019", "2024", "Undated"]);
  await expect(screen.getByTestId("aircraft-years").locator("thead th")).toHaveText([
    "Year",
    "Bar",
    "Photos",
    "Named",
  ]);

  const table = screen.getByTestId("aircraft-table");
  await expect(table.locator("thead th").first()).toHaveText("Airline");
  await expect(table.locator("tbody tr").first().locator("th")).toHaveText("Air Canada");
  await screen.getByTestId("aircraft-by-registrations").click();
  await expect(table.locator("thead th").first()).toHaveText("Registration");
  await expect(table.locator("tbody tr")).toHaveCount(4);
  // Find looks in the name, airline and type.
  await screen.getByTestId("aircraft-find").fill("interjet");
  await expect(table.locator("tbody th")).toHaveText(["XA-MBA"]);
  await screen.getByTestId("aircraft-find").fill("concorde");
  await expect(screen.getByTestId("aircraft-none")).toHaveText("Nothing matches “concorde”.");

  await screen.getByTestId("aircraft-export").click();
  await expect(screen.getByTestId("aircraft-saved")).toBeVisible();
  expect(await calls(page, "aircraft_report_csv")).toHaveLength(1);
});

test("an aircraft row opens its photos by keyword, and back shows the same table", async ({
  page,
}) => {
  const screen = await open(page, 200);
  await screen.getByTestId("report-kind-aircraft").click();
  await screen.getByTestId("aircraft-by-types").click();
  await screen.getByTestId("aircraft-table").getByRole("button", { name: "Boeing 747" }).click();
  await expect(page.getByTestId("heading")).toContainText("Type Boeing 747");
  // The library asks for the keyword's photos (wait: a slow runner can
  // show the heading before the request is made).
  await expect
    .poll(async () => JSON.stringify(await calls(page, "view_open")))
    .toContain("Aircraft|Type|Boeing 747");
  await page.getByTestId("heading").getByRole("button", { name: "Report" }).click();
  await expect(screen.getByTestId("report-kind-aircraft")).toHaveAttribute("aria-pressed", "true");
  await expect(screen.getByTestId("aircraft-by-types")).toHaveAttribute("aria-pressed", "true");
});

test("report columns sort when their heading is clicked, Unknown last", async ({ page }) => {
  const screen = await open(page, 1000);
  const table = screen.getByTestId("report-table");
  const col = async (i: number) =>
    (
      await table
        .locator("tbody tr")
        .locator(i === 0 ? "th" : `td:nth-of-type(${i})`)
        .allTextContents()
    ).map((t) => t.trim());
  const nums = async (i: number) => (await col(i)).map((t) => num(t));

  // Photos: most first, then fewest first; Unknown stays last.
  await screen.getByTestId("report-sort-photos").click();
  await expect(table.locator("thead th").nth(1)).toHaveAttribute("aria-sort", "descending");
  let rows = await col(0);
  expect(rows.at(-1)).toBe("Unknown");
  let photos = (await nums(1)).slice(0, -1);
  expect(photos).toEqual([...photos].sort((a, b) => b - a));
  await screen.getByTestId("report-sort-photos").click();
  await expect(table.locator("thead th").nth(1)).toHaveAttribute("aria-sort", "ascending");
  photos = (await nums(1)).slice(0, -1);
  expect(photos).toEqual([...photos].sort((a, b) => a - b));
  expect((await col(0)).at(-1)).toBe("Unknown");

  // Year: oldest first on the first click.
  await screen.getByTestId("report-sort-group").click();
  rows = (await col(0)).slice(0, -1);
  expect(rows).toEqual([...rows].sort());

  // The sort is kept on another breakdown.
  await screen.getByTestId("report-by-camera").click();
  await expect(table.locator("thead th").first()).toHaveAttribute("aria-sort", "ascending");

  // Aircraft: by registrations, then by name Z–A.
  await screen.getByTestId("report-kind-aircraft").click();
  const air = screen.getByTestId("aircraft-table");
  await screen.getByTestId("aircraft-sort-registrations").click();
  // Ties keep their order (KLM and Interjet have one each).
  await expect(air.locator("tbody th")).toHaveText(["Air Canada", "KLM", "Interjet"]);
  await screen.getByTestId("aircraft-sort-name").click();
  await screen.getByTestId("aircraft-sort-name").click();
  await expect(air.locator("tbody th")).toHaveText(["KLM", "Interjet", "Air Canada"]);
  // Years: most photos first; Undated stays last.
  await screen.getByTestId("aircraft-years-sort-photos").click();
  await expect(screen.getByTestId("aircraft-years").locator("tbody th")).toHaveText([
    "2019",
    "2018",
    "2024",
    "Undated",
  ]);
});
