import { describe, expect, it } from "vitest";

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
