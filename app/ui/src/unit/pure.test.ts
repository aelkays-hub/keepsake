import { describe, expect, it } from "vitest";

import * as fmt from "../format";
import { actionFor, moveIndex } from "../keys";
import * as sel from "../selection";

const key = (
  k: string,
  mods: Partial<{ shiftKey: boolean; ctrlKey: boolean; metaKey: boolean; altKey: boolean }> = {},
) => ({
  key: k,
  shiftKey: false,
  ctrlKey: false,
  metaKey: false,
  altKey: false,
  ...mods,
});

describe("selection ranges", () => {
  it("merges and sizes", () => {
    expect(
      sel.normalize([
        [5, 8],
        [0, 2],
        [7, 10],
        [2, 3],
      ]),
    ).toEqual([
      [0, 3],
      [5, 10],
    ]);
    expect(
      sel.size({
        ranges: [
          [0, 3],
          [5, 10],
        ],
        anchor: null,
        focus: null,
      }),
    ).toBe(8);
  });

  it("toggles single items in and out of ranges", () => {
    let s = sel.all(10);
    s = sel.toggle(s, 4);
    expect(s.ranges).toEqual([
      [0, 4],
      [5, 10],
    ]);
    expect(sel.has(s, 4)).toBe(false);
    s = sel.toggle(s, 4);
    expect(s.ranges).toEqual([[0, 10]]);
  });

  it("extends from the anchor in either direction", () => {
    const s = sel.only(10);
    expect(sel.extend(s, 13).ranges).toEqual([[10, 14]]);
    expect(sel.extend(s, 7).ranges).toEqual([[7, 11]]);
    expect(sel.extend(s, 7).anchor).toBe(10);
  });

  it("selects 300,000 items as one range and clamps to a smaller view", () => {
    const s = sel.all(300_000);
    expect(s.ranges).toEqual([[0, 300_000]]);
    expect(sel.size(s)).toBe(300_000);
    expect(sel.clamp(s, 100).ranges).toEqual([[0, 100]]);
    expect(sel.all(0)).toEqual(sel.emptySel);
  });
});

describe("keyboard map", () => {
  it("maps ratings, labels, views and moves", () => {
    expect(actionFor(key("4"))).toEqual({ kind: "rate", stars: 4 });
    expect(actionFor(key("0"))).toEqual({ kind: "rate", stars: 0 });
    expect(actionFor(key("6"))).toEqual({ kind: "label", label: "Red" });
    expect(actionFor(key("9"))).toEqual({ kind: "label", label: "Blue" });
    expect(actionFor(key("a", { ctrlKey: true }))).toEqual({ kind: "selectAll" });
    expect(actionFor(key("A", { metaKey: true }))).toEqual({ kind: "selectAll" });
    expect(actionFor(key("3", { ctrlKey: true }))).toEqual({ kind: "view", mode: "details" });
    expect(actionFor(key("ArrowDown", { shiftKey: true }))).toEqual({
      kind: "move",
      by: "down",
      extend: true,
    });
    expect(actionFor(key("Enter"))).toEqual({ kind: "open" });
    expect(actionFor(key("x"))).toBeNull();
    expect(actionFor(key("4", { altKey: true }))).toBeNull();
  });

  it("moves within the grid and stops at the edges", () => {
    expect(moveIndex(null, "right", 10, 4, 3)).toBe(0);
    expect(moveIndex(5, "down", 10, 4, 3)).toBe(9);
    expect(moveIndex(9, "down", 10, 4, 3)).toBe(9);
    expect(moveIndex(1, "up", 10, 4, 3)).toBe(0);
    expect(moveIndex(3, "end", 10, 4, 3)).toBe(9);
    expect(moveIndex(0, "pageDown", 100, 4, 3)).toBe(12);
    expect(moveIndex(0, "left", 0, 4, 3)).toBeNull();
  });
});

describe("formatting", () => {
  it("formats sizes, durations and dates", () => {
    expect(fmt.bytes(24_000_000)).toBe("24.0 MB");
    expect(fmt.bytes(1_200_000_000)).toBe("1.2 GB");
    expect(fmt.bytes(null)).toBe("—");
    expect(fmt.duration(12_400)).toBe("0:12");
    expect(fmt.duration(3_725_000)).toBe("1:02:05");
    expect(fmt.dateTime("2024-06-15T14:02:11")).toBe("2024-06-15 14:02");
    expect(fmt.count(300000)).toBe("300,000");
    expect(fmt.plural(1, "item")).toBe("1 item");
    expect(fmt.plural(2, "item")).toBe("2 items");
  });

  it("knows calendar months", () => {
    expect(fmt.daysIn(2024, 2)).toBe(29);
    expect(fmt.daysIn(2023, 2)).toBe(28);
    expect(fmt.monthName(6)).toBe("June");
    // 1 June 2024 was a Saturday (Monday = 0).
    expect(fmt.firstWeekday(2024, 6)).toBe(5);
  });
});
