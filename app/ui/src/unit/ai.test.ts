import { describe, expect, it } from "vitest";

import type { AiStatus } from "../api";
import { aiProgress, dayTitle, memberBadge } from "../ai";

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
      "1,200 of 2,000 items analysed, 800 to go (on the graphics card (DirectML)).",
    );
    expect(aiProgress(status({ pending: 0, analysed: 2000 }))).toBe(
      "2,000 of 2,000 items analysed.",
    );
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
