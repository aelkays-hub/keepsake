import { describe, expect, it } from "vitest";

import type { AiStatus } from "../api";
import { aiProgress, calRange, dayTitle, memberBadge, rangeProgress } from "../ai";

const status = (p: Partial<AiStatus>): AiStatus => ({
  enabled: true,
  gpu: true,
  modelInstalled: true,
  modelBytes: 371807752,
  runtimeFound: true,
  accel: "graphics card (DirectML)",
  problem: null,
  installing: null,
  analysed: 1200,
  assets: 2000,
  pending: 800,
  failed: 0,
  ...p,
});

describe("AI progress", () => {
  it("says what the AI is doing", () => {
    expect(aiProgress(status({ enabled: false }))).toBe("Off.");
    expect(aiProgress(status({ problem: "no model" }))).toBe("Not running: no model");
    expect(aiProgress(status({ accel: null }))).toBe("Starting…");
    expect(aiProgress(status({}))).toBe(
      "Ready, on the graphics card (DirectML). 1,200 items analysed so far; 800 waiting.",
    );
    expect(aiProgress(status({ pending: 0, analysed: 1 }))).toBe(
      "Ready, on the graphics card (DirectML). 1 item analysed so far.",
    );
  });
});

describe("date ranges", () => {
  it("covers a year, a month or a day", () => {
    expect(calRange(null, null, null)).toBeNull();
    expect(calRange(2026, null, null)).toEqual({
      from: "2026-01-01",
      to: "2026-12-31",
      title: "2026",
    });
    expect(calRange(2026, 7, null)).toEqual({
      from: "2026-07-01",
      to: "2026-07-31",
      title: "July 2026",
    });
    expect(calRange(2026, 7, 4)).toEqual({
      from: "2026-07-04",
      to: "2026-07-04",
      title: "Sat 4 July 2026",
    });
  });

  it("says how far the analysis has got", () => {
    expect(rangeProgress({ items: 0, analysed: 0, pending: 0 })).toBe(
      "No RAW photos on these dates.",
    );
    expect(rangeProgress({ items: 120, analysed: 34, pending: 86 })).toBe(
      "Comparing: 34 of 120 done…",
    );
    expect(rangeProgress({ items: 120, analysed: 120, pending: 0 })).toBe(
      "120 of 120 RAW photos compared.",
    );
    expect(rangeProgress({ items: 0, analysed: 0, pending: 0 }, "tags")).toBe(
      "No photos on these dates.",
    );
    expect(rangeProgress({ items: 9, analysed: 4, pending: 5 }, "tags")).toBe(
      "Tagging: 4 of 9 done…",
    );
    expect(rangeProgress({ items: 9, analysed: 9, pending: 0 }, "tags")).toBe("9 of 9 tagged.");
  });
});

describe("duplicates", () => {
  it("titles a day", () => {
    expect(dayTitle("2026-07-04")).toBe("Sat 4 July 2026");
  });

  it("badges members", () => {
    expect(memberBadge({ id: "a", choice: null }, "a")).toBe("Suggested");
    expect(memberBadge({ id: "b", choice: null }, "a")).toBe("");
    expect(memberBadge({ id: "a", choice: "keeper" }, "a")).toBe("Kept");
    expect(memberBadge({ id: "b", choice: "not_kept" }, "a")).toBe("Not kept");
    expect(memberBadge({ id: "b", choice: "not_duplicate" }, "a")).toBe("");
  });
});
