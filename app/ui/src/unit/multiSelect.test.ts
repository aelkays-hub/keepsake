import { describe, expect, it } from "vitest";

import type { SelectionSummary } from "../api";
import { keywordChips } from "../edits";
import * as sel from "../selection";
import { initial, reduce } from "../uiModel";

// 4 columns of 100 px tiles with 8 px gaps: tile (row r, col c) covers
// x 8+108c … 108+108c and y 8+108r … 108+108r.
const g: sel.GridGeom = { cols: 4, count: 10, tile: 100, gap: 8 };

describe("drag rectangle", () => {
  it("selects the tiles it touches, one range per row", () => {
    expect(sel.inRect(g, 50, 50, 120, 120)).toEqual([
      [0, 2],
      [4, 6],
    ]);
  });

  it("is the same whichever corner the drag starts from", () => {
    expect(sel.inRect(g, 120, 120, 50, 50)).toEqual(sel.inRect(g, 50, 50, 120, 120));
  });

  it("touches nothing in a gap or before the first tile", () => {
    expect(sel.inRect(g, 0, 0, 5, 5)).toEqual([]);
    expect(sel.inRect(g, 109, 50, 115, 60)).toEqual([]);
  });

  it("stops at the last item of a short last row", () => {
    expect(sel.inRect(g, 0, 0, 1000, 1000)).toEqual([[0, 10]]);
    // The last row holds items 8 and 9 only.
    expect(sel.inRect(g, 0, 230, 1000, 1000)).toEqual([[8, 10]]);
    expect(sel.inRect(g, 300, 230, 1000, 1000)).toEqual([]);
  });

  it("is empty for an empty view", () => {
    expect(sel.inRect({ ...g, count: 0 }, 0, 0, 500, 500)).toEqual([]);
  });

  it("adds to a selection with union", () => {
    const s = sel.union(sel.only(9), sel.inRect(g, 50, 50, 120, 60), 0);
    expect(s.ranges).toEqual([
      [0, 2],
      [9, 10],
    ]);
    expect(s.focus).toBe(0);
    expect(s.anchor).toBe(9);
  });
});

describe("select mode", () => {
  it("turns on and off", () => {
    expect(initial.selecting).toBe(false);
    const on = reduce(initial, { t: "selecting", on: true });
    expect(on.selecting).toBe(true);
    expect(reduce(on, { t: "selecting", on: false }).selecting).toBe(false);
  });
});

describe("keyword chips", () => {
  const summary: SelectionSummary = {
    count: 3,
    rating: null,
    label: null,
    labelMixed: true,
    comment: null,
    commentMixed: false,
    keywords: [
      { keyword: "People|Ann", count: 1 },
      { keyword: "Places|Lake", count: 3 },
    ],
  };

  it("lists one item's own keywords", () => {
    expect(keywordChips(false, ["A", "B"], summary)).toEqual([
      { k: "A", of: null },
      { k: "B", of: null },
    ]);
  });

  it("puts shared keywords first, then partial ones with a count", () => {
    expect(keywordChips(true, ["ignored"], summary)).toEqual([
      { k: "Places|Lake", of: null },
      { k: "People|Ann", of: "1 of 3" },
    ]);
  });

  it("shows nothing until the summary arrives", () => {
    expect(keywordChips(true, ["A"], null)).toEqual([]);
  });
});
