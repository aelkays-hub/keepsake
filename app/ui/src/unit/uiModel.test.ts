import { describe, expect, it } from "vitest";

import { emptyQuery } from "../api";
import { effectiveQuery, initial, reduce } from "../uiModel";

describe("folder section", () => {
  it("narrows the query to the volume and folder", () => {
    const s = reduce(initial, {
      t: "section",
      section: { kind: "folder", id: "v1", path: "2025/May", title: "Pictures › 2025 › May" },
    });
    expect(s.screen).toBe("library");
    const q = effectiveQuery(s);
    expect(q.volume).toBe("v1");
    expect(q.folder).toBe("2025/May");
  });

  it("a volume section has no folder", () => {
    const s = reduce(initial, {
      t: "section",
      section: { kind: "volume", id: "v1", title: "Pictures" },
    });
    const q = effectiveQuery(s);
    expect(q.volume).toBe("v1");
    expect(q.folder).toBeUndefined();
  });
});

describe("report section", () => {
  it("shows a report row's items with no other filters", () => {
    const filtered = {
      ...initial,
      mode: "calendar" as const,
      screen: "report" as const,
      filters: { ...emptyQuery, camera: "ZV-1", text: "beach" },
      sort: { field: "size" as const, descending: false },
    };
    const s = reduce(filtered, {
      t: "open",
      section: {
        kind: "report",
        title: "Year 2024",
        query: { from: "2024-01-01", to: "2024-12-31" },
      },
    });
    expect(s.screen).toBe("library");
    expect(s.mode).toBe("grid");
    expect(s.sort).toEqual(initial.sort);
    expect(effectiveQuery(s)).toEqual({ ...emptyQuery, from: "2024-01-01", to: "2024-12-31" });
  });
});
