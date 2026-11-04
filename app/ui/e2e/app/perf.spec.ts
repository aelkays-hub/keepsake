// Scale in the real app: 300,000 seeded items. Start-up, search, calendar
// and grid scrolling, against the prompt-12 limits:
//   start-up 3 s · search 300 ms · no dropped frames while scrolling.
//
// Run with the release app for real numbers (CI uses the debug app and
// KEEPSAKE_SCALE_SLACK=2):
//   pnpm tauri build --no-bundle && cargo build --release -p keepsake-cli
//   $env:KEEPSAKE_APP_PROFILE="release"; pnpm test:app -- perf.spec.ts
// Results are printed as a table for docs/PERFORMANCE.md.

import { expect, test, type Page } from "@playwright/test";
import * as fs from "node:fs";

import { App, exe, startVite, stopVite, World } from "./fixture";

const release = process.env.KEEPSAKE_APP_PROFILE === "release";
const N = Number(process.env.KEEPSAKE_UI_SEED ?? 300000);
const SLACK = Number(process.env.KEEPSAKE_SCALE_SLACK ?? (release ? 1 : 2));

test.skip(process.platform !== "win32", "WebView2 automation is Windows-only");
test.skip(
  !fs.existsSync(exe("keepsake-app", release ? "release" : "debug")),
  "build the app first",
);
test.skip(
  !fs.existsSync(exe("keepsake", "release")) && N > 50000,
  "needs a release CLI to seed 300,000 items",
);

test.beforeAll(async () => {
  if (!release) await startVite();
});
test.afterAll(() => {
  if (!release) stopVite();
});

const results: { what: string; value: string; limit: string; ok: boolean }[] = [];
function record(what: string, ms: number, limitMs: number) {
  results.push({
    what,
    value: `${Math.round(ms)} ms`,
    limit: `${limitMs} ms`,
    ok: ms <= limitMs * SLACK,
  });
}

/** The middle value of a few timings. */
function middle(ms: number[]): number {
  const s = [...ms].sort((a, b) => a - b);
  return s[Math.floor(s.length / 2)];
}

const invoke = <T>(page: Page, cmd: string) =>
  page.evaluate(
    (c) =>
      (
        window as unknown as { __TAURI_INTERNALS__: { invoke: (c: string) => Promise<unknown> } }
      ).__TAURI_INTERNALS__.invoke(c),
    cmd,
  ) as Promise<T>;

async function startupMs(page: Page): Promise<number> {
  let ms: number | null = null;
  await expect
    .poll(async () => (ms = await invoke<number | null>(page, "startup_ms")), { timeout: 60_000 })
    .not.toBeNull();
  return ms ?? 0;
}

/** Time from a change to the count line showing a new value. */
async function timeToCount(page: Page, act: () => Promise<void>): Promise<number> {
  await page.evaluate(() => {
    const w = window as unknown as { __t0: number; __t1: number | null };
    const el = document.querySelector('[data-testid="count"]');
    const before = el?.textContent;
    w.__t1 = null;
    const mo = new MutationObserver(() => {
      if (el?.textContent !== before && w.__t1 === null) {
        w.__t1 = performance.now();
        mo.disconnect();
      }
    });
    if (el) mo.observe(el, { childList: true, characterData: true, subtree: true });
    w.__t0 = performance.now();
  });
  await act();
  await expect
    .poll(() => page.evaluate(() => (window as unknown as { __t1: number | null }).__t1), {
      timeout: 30_000,
    })
    .not.toBeNull();
  return page.evaluate(() => {
    const w = window as unknown as { __t0: number; __t1: number };
    return w.__t1 - w.__t0;
  });
}

test("300,000 items: start-up, search, calendar and scrolling", async () => {
  test.setTimeout(900_000);
  const w = new World();
  w.cli(["init", w.p("lib")]);
  w.cacheHere();
  w.cli(["dev", "seed", "--assets", String(N)], fs.existsSync(exe("keepsake", "release")));
  const profile = { WEBVIEW2_USER_DATA_FOLDER: w.p("webview2-perf") };

  // Cold start: a new WebView2 profile (first launch after install).
  let app = new App(w, profile);
  try {
    await app.connect();
    let page = await app.page("main");
    const cold = await startupMs(page);
    record("start-up, first launch (new WebView2 profile)", cold, 3000);
    await app.close();

    // Warm start: the usual case.
    app = new App(w, profile);
    await app.connect();
    page = await app.page("main");
    const warm = await startupMs(page);
    record("start-up", warm, 3000);
    await expect(page.getByTestId("count")).toHaveText(`${N.toLocaleString("en-US")} items`);
    await expect(page.getByTestId("tile").first().locator("img")).toBeVisible();

    // Search: from the last keystroke to the new count on screen. This
    // includes the deliberate pause that waits for typing to stop. Each
    // search runs three times and is judged on the middle time: on a shared
    // CI machine single timings of one search swung from 190 to 620 ms. The
    // app keeps no search cache, so every repeat does the full work.
    for (const words of ["lake", "sunset lake", "dsc01234", "family"]) {
      const times: number[] = [];
      for (let i = 0; i < 3; i++) {
        times.push(await timeToCount(page, () => page.getByTestId("search").fill(words)));
        await page.getByTestId("search").fill("");
        await expect(page.getByTestId("count")).toHaveText(`${N.toLocaleString("en-US")} items`);
      }
      record(`search "${words}" (keystroke to results, middle of 3)`, middle(times), 300);
    }
    // The filter and the calendar are also timed three times, judged on the
    // middle time, for the same reason: "calendar: years" read 155, 265 and
    // 887 ms on three CI runs of nearly the same code.
    const filterMs: number[] = [];
    for (let i = 0; i < 3; i++) {
      filterMs.push(await timeToCount(page, () => page.getByTestId("rating").selectOption("4")));
      await page.getByRole("button", { name: "Clear filters" }).click();
      await expect(page.getByTestId("count")).toHaveText(`${N.toLocaleString("en-US")} items`);
    }
    record("filter: 4 stars or more (middle of 3)", middle(filterMs), 300);

    // Calendar: switch to it and wait for the year cells, then open a year.
    // "All items" in the sidebar resets the calendar to its years for the
    // next round; the calendar keeps no cache, so each round asks again.
    const yearsMs: number[] = [];
    const monthsMs: number[] = [];
    for (let i = 0; i < 3; i++) {
      const t0 = Date.now();
      await page.keyboard.press("Control+1");
      await expect(page.locator('[data-testid="cal-cell"]').first()).toBeVisible();
      yearsMs.push(Date.now() - t0);
      const t1 = Date.now();
      await page.locator('[data-testid="cal-cell"]').first().click();
      await expect(page.locator('[data-testid="cal-cell"]').nth(5)).toBeVisible();
      monthsMs.push(Date.now() - t1);
      await page.getByTestId("sidebar").getByRole("button", { name: "All items" }).click();
      await expect(page.getByTestId("grid")).toBeVisible();
    }
    record("calendar: years on screen (middle of 3)", middle(yearsMs), 300);
    record("calendar: a year's months on screen (middle of 3)", middle(monthsMs), 300);

    // Scrolling: 4 s of fast scrolling (60 px a frame, about 15 rows a
    // second), timing every frame. A frame is dropped when it takes more
    // than 1.5 times the screen's frame interval.
    const frames = await page.evaluate(async () => {
      const g = document.querySelector<HTMLElement>('[data-testid="grid"]');
      if (!g) return { deltas: [] as number[], long: 0 };
      let long = 0;
      const po = new PerformanceObserver((l) => (long += l.getEntries().length));
      try {
        po.observe({ type: "longtask", buffered: false });
      } catch {
        // not supported
      }
      const deltas: number[] = [];
      await new Promise<void>((done) => {
        let last = performance.now();
        const end = last + 4000;
        const step = (t: number) => {
          deltas.push(t - last);
          last = t;
          g.scrollTop += 60;
          if (t < end) requestAnimationFrame(step);
          else done();
        };
        requestAnimationFrame(step);
      });
      po.disconnect();
      return { deltas: deltas.slice(2), long };
    });
    const sorted = [...frames.deltas].sort((a, b) => a - b);
    const interval = sorted[Math.floor(sorted.length / 2)];
    const dropped = frames.deltas.filter((d) => d > interval * 1.5).length;
    const p99 = sorted[Math.floor(sorted.length * 0.99)];
    results.push({
      what: `scrolling: dropped frames (of ${frames.deltas.length}, ${interval.toFixed(1)} ms each)`,
      value: `${dropped} (slowest ${sorted[sorted.length - 1].toFixed(1)} ms, 99th ${p99.toFixed(1)} ms, ${frames.long} long tasks)`,
      limit: "0",
      ok: dropped <= Math.floor(frames.deltas.length * 0.01 * (SLACK - 1)),
    });
  } finally {
    await app.close();
    w.cleanup();
    console.log(
      [
        `\n${release ? "Release" : "Debug"} app, ${N.toLocaleString("en-US")} items, limits × ${SLACK}:`,
        "| Measure | Result | Limit | OK |",
        "|---|---|---|---|",
        ...results.map(
          (r) => `| ${r.what} | ${r.value} | ${r.limit} | ${r.ok ? "yes" : "**no**"} |`,
        ),
      ].join("\n"),
    );
  }
  const over = results.filter((r) => !r.ok);
  expect(over, JSON.stringify(over, null, 1)).toEqual([]);
});
