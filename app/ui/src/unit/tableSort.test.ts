import { describe, expect, it } from "vitest";

import { ariaSort, nextSort, sortRows, type SortValue } from "../tableSort";

type Row = { name: string | null; n: number | null };
const rows: Row[] = [
  { name: "b", n: 2 },
  { name: null, n: 9 },
  { name: "A10", n: null },
  { name: "a2", n: 2 },
  { name: "c", n: 5 },
];
const value = (r: Row, col: string): SortValue => (col === "n" ? r.n : r.name);
const names = (rs: Row[]) => rs.map((r) => r.name);

describe("report table sort", () => {
  it("starts numbers most first and text A–Z, and turns over on the same column", () => {
    expect(nextSort(null, "n", true)).toEqual({ col: "n", desc: true });
    expect(nextSort(null, "name", false)).toEqual({ col: "name", desc: false });
    expect(nextSort({ col: "n", desc: true }, "n", true)).toEqual({ col: "n", desc: false });
    expect(nextSort({ col: "n", desc: false }, "name", false)).toEqual({
      col: "name",
      desc: false,
    });
    expect(ariaSort({ col: "n", desc: true }, "n")).toBe("descending");
    expect(ariaSort({ col: "n", desc: false }, "n")).toBe("ascending");
    expect(ariaSort({ col: "n", desc: false }, "name")).toBe("none");
  });

  it("sorts numbers and text, empty values last either way, ties in order", () => {
    expect(sortRows(rows, null, value)).toBe(rows);
    expect(names(sortRows(rows, { col: "n", desc: true }, value))).toEqual([
      null,
      "c",
      "b",
      "a2",
      "A10",
    ]);
    expect(names(sortRows(rows, { col: "n", desc: false }, value))).toEqual([
      "b",
      "a2",
      "c",
      null,
      "A10",
    ]);
    // Text ignores case and reads numbers as numbers (a2 before A10).
    expect(names(sortRows(rows, { col: "name", desc: false }, value))).toEqual([
      "a2",
      "A10",
      "b",
      "c",
      null,
    ]);
    expect(names(sortRows(rows, { col: "name", desc: true }, value))).toEqual([
      "c",
      "b",
      "A10",
      "a2",
      null,
    ]);
  });

  it("keeps pinned rows (Unknown) at the end", () => {
    const pinned = (r: Row) => r.name === null;
    expect(names(sortRows(rows, { col: "n", desc: true }, value, pinned)).at(-1)).toBeNull();
    expect(names(sortRows(rows, { col: "n", desc: false }, value, pinned)).at(-1)).toBeNull();
  });
});
