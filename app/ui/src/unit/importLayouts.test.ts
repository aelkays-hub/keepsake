import { describe, expect, it } from "vitest";

import { LAYOUTS, layoutExample } from "../importLayouts";

describe("card import layouts", () => {
  const d = new Date(2026, 9, 30, 14, 5);
  it("shows where a photo goes", () => {
    expect(layoutExample(LAYOUTS[0].template, d)).toBe("2026\\October\\2026-10-30\\A7400123.ARW");
    expect(layoutExample(LAYOUTS[1].template, d)).toBe("2026\\2026-10-30\\A7400123.ARW");
    expect(layoutExample("{yyyy}/{month}/{orig}", new Date(2025, 0, 3), "/")).toBe(
      "2025/January/A7400123.ARW",
    );
  });
  it("leaves unknown tokens visible", () => {
    expect(layoutExample("{yyyy}/{nope}/{orig}", d)).toBe("2026\\{nope}\\A7400123.ARW");
  });
});
