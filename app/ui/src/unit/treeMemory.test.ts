import { beforeEach, describe, expect, it, vi } from "vitest";

import { keepOpenLevels, keepScroll, MAX_OPEN, openLevels, scrollOf } from "../treeMemory";

/** A stand-in for the browser's storage. */
function storage(): Storage {
  const m = new Map<string, string>();
  return {
    getItem: (k) => m.get(k) ?? null,
    setItem: (k, v) => void m.set(k, String(v)),
    removeItem: (k) => void m.delete(k),
    clear: () => m.clear(),
    key: (i) => [...m.keys()][i] ?? null,
    get length() {
      return m.size;
    },
  };
}

describe("tree memory", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", storage());
  });

  it("remembers each tree's open levels and scroll on its own", () => {
    expect(openLevels("date", ["cal"])).toEqual(new Set(["cal"]));
    expect(scrollOf("folder")).toBe(0);
    keepOpenLevels("date", new Set(["cal", "2024", "2024-06"]));
    keepScroll("date", 512.6);
    keepScroll("folder", 40);
    expect(openLevels("date", ["cal"])).toEqual(new Set(["cal", "2024", "2024-06"]));
    expect(openLevels("folder", [])).toEqual(new Set());
    expect(scrollOf("date")).toBe(513);
    expect(scrollOf("folder")).toBe(40);
    // Every level closed is remembered too.
    keepOpenLevels("date", new Set());
    expect(openLevels("date", ["cal"])).toEqual(new Set());
  });

  it("keeps the latest levels when there are very many", () => {
    const many = new Set(Array.from({ length: MAX_OPEN + 50 }, (_, i) => `k${i}`));
    keepOpenLevels("folder", many);
    const back = openLevels("folder", []);
    expect(back.size).toBe(MAX_OPEN);
    expect(back.has(`k${MAX_OPEN + 49}`)).toBe(true);
    expect(back.has("k0")).toBe(false);
  });

  it("starts at the top when the storage is unreadable or cannot be used", () => {
    localStorage.setItem("keepsake.tree.date.open", "{not json");
    localStorage.setItem("keepsake.tree.date.scroll", "-5");
    expect(openLevels("date", ["cal"])).toEqual(new Set(["cal"]));
    expect(scrollOf("date")).toBe(0);
    vi.stubGlobal("localStorage", {
      getItem: () => {
        throw new Error("blocked");
      },
      setItem: () => {
        throw new Error("blocked");
      },
    });
    expect(openLevels("folder", ["x"])).toEqual(new Set(["x"]));
    expect(scrollOf("folder")).toBe(0);
    expect(() => keepScroll("folder", 10)).not.toThrow();
    expect(() => keepOpenLevels("folder", new Set(["a"]))).not.toThrow();
  });
});
