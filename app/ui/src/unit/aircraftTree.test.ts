import { describe, expect, it } from "vitest";

import {
  dateQuery,
  isAircraftTree,
  nameQuery,
  nameTreeNodes,
  sameQuery,
  unnamedLabel,
} from "../aircraftTree";
import type { NameTreeNode } from "../bindings/NameTreeNode";

const tree: NameTreeNode[] = [
  {
    name: "KLM",
    photos: 5,
    years: [
      {
        year: 2019,
        photos: 3,
        months: [
          { month: 1, photos: 2 },
          { month: 3, photos: 1 },
        ],
      },
      { year: null, photos: 2, months: [] },
    ],
  },
  { name: null, photos: 1, years: [{ year: 2024, photos: 1, months: [{ month: 2, photos: 1 }] }] },
];

describe("aircraft trees", () => {
  it("knows its kinds", () => {
    expect(["airline", "type", "registration"].every(isAircraftTree)).toBe(true);
    expect(isAircraftTree("date")).toBe(false);
    expect(unnamedLabel("type")).toBe("No type named");
  });

  it("shows only the names until one is opened", () => {
    const n = nameTreeNodes("airline", tree, new Set());
    expect(n.map((x) => [x.label, x.depth, x.count, x.expanded])).toEqual([
      ["KLM", 0, 5, false],
      ["No airline named", 0, 1, false],
    ]);
  });

  it("opens a name to its years and a year to its months", () => {
    const n = nameTreeNodes("airline", tree, new Set(["KLM", "KLM\u001f2019"]));
    expect(n.map((x) => [x.label, x.depth, x.count, x.expanded])).toEqual([
      ["KLM", 0, 5, true],
      ["2019", 1, 3, true],
      ["January", 2, 2, undefined],
      ["March", 2, 1, undefined],
      ["No date", 1, 2, undefined],
      ["No airline named", 0, 1, false],
    ]);
    expect(n[2].title).toBe("KLM › 2019 › January");
  });

  it("finds exactly a node's photos", () => {
    expect(nameQuery("registration", "PH-BFA")).toEqual({ tags: ["Aircraft|Registration|PH-BFA"] });
    expect(nameQuery("type", null)).toEqual({
      tags: ["Aircraft"],
      withoutTags: ["Aircraft|Type"],
    });
    expect(dateQuery("airline", "KLM", 2019)).toEqual({
      tags: ["Aircraft|Airline|KLM"],
      from: "2019-01-01",
      to: "2019-12-31",
    });
    // A month ends on its own last day (a leap year here).
    expect(dateQuery("airline", "KLM", 2024, 2)).toMatchObject({
      from: "2024-02-01",
      to: "2024-02-29",
    });
    expect(dateQuery("airline", null, null)).toEqual({
      tags: ["Aircraft"],
      withoutTags: ["Aircraft|Airline"],
      undated: true,
    });
    const open = nameTreeNodes("airline", tree, new Set(["", "\u001f2024"]));
    expect(open[open.length - 1].query).toEqual({
      tags: ["Aircraft"],
      withoutTags: ["Aircraft|Airline"],
      from: "2024-02-01",
      to: "2024-02-29",
    });
  });

  it("compares queries whatever their order", () => {
    expect(sameQuery({ tags: ["a"], from: "x" }, { from: "x", tags: ["a"], undated: false })).toBe(
      true,
    );
    expect(sameQuery({ tags: ["a"] }, { tags: ["b"] })).toBe(false);
  });
});
