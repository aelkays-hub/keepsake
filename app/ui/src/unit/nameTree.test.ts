import { describe, expect, it } from "vitest";

import {
  aircraftTreeNodes,
  faceDateQuery,
  faceQuery,
  faceTreeNodes,
  isNameTree,
  sameQuery,
  sortGroups,
} from "../nameTree";
import type { AircraftReportGroup } from "../bindings/AircraftReportGroup";
import type { NameTreeNode } from "../bindings/NameTreeNode";

const g = (
  name: string | null,
  photos: number,
  children: AircraftReportGroup[] = [],
): AircraftReportGroup => ({ name, photos, children });

// As the report gives them: most photos first.
const fleet: AircraftReportGroup[] = [
  g("KLM", 12, [g("Boeing 777", 4, [g("PH-BVA", 4)]), g("Boeing 747", 8, [g("PH-BFA", 8)])]),
  g(null, 3, [g(null, 3, [g("C-GZQX", 3)])]),
  g("air Canada", 5, [g(null, 2, [g(null, 2)]), g("Airbus A330", 3, [g("C-GFAF", 3)])]),
];

const people: NameTreeNode[] = [
  { name: "tom", photos: 9, years: [{ year: 2019, photos: 9, months: [{ month: 3, photos: 9 }] }] },
  { name: null, photos: 4, years: [{ year: 2021, photos: 4, months: [{ month: 6, photos: 4 }] }] },
  {
    name: "Sara",
    photos: 2,
    years: [
      { year: 2019, photos: 1, months: [{ month: 1, photos: 1 }] },
      { year: 2022, photos: 1, months: [{ month: 5, photos: 1 }] },
      { year: null, photos: 1, months: [] },
    ],
  },
];

describe("trees by name", () => {
  it("knows its trees", () => {
    expect(isNameTree("aircraft")).toBe(true);
    expect(isNameTree("face")).toBe(true);
    expect(isNameTree("date")).toBe(false);
  });

  it("sorts every step A to Z, any case, the unnamed last", () => {
    const s = sortGroups(fleet);
    expect(s.map((x) => x.name)).toEqual(["air Canada", "KLM", null]);
    expect(s[0].children.map((x) => x.name)).toEqual(["Airbus A330", null]);
    expect(s[1].children.map((x) => x.name)).toEqual(["Boeing 747", "Boeing 777"]);
  });

  it("by aircraft: airlines, then an open one's types, then an open type's registrations", () => {
    expect(aircraftTreeNodes(fleet, new Set()).map((n) => [n.label, n.count, n.expanded])).toEqual([
      ["air Canada", 5, false],
      ["KLM", 12, false],
      ["No airline named", 3, false],
    ]);
    const klm = aircraftTreeNodes(fleet, new Set(["KLM\u001f", "KLM\u001fBoeing 747\u001f"]));
    expect(klm.map((n) => [n.label, n.depth, n.count, n.expanded])).toEqual([
      ["air Canada", 0, 5, false],
      ["KLM", 0, 12, true],
      ["Boeing 747", 1, 8, true],
      ["PH-BFA", 2, 8, false],
      ["Boeing 777", 1, 4, false],
      ["No airline named", 0, 3, false],
    ]);
    expect(klm[3].title).toBe("KLM › Boeing 747 › PH-BFA");
    expect(klm[3].query).toEqual({
      tags: ["Aircraft|Airline|KLM", "Aircraft|Type|Boeing 747", "Aircraft|Registration|PH-BFA"],
    });
  });

  it("by aircraft: an unnamed line finds the photos naming none at that step", () => {
    const n = aircraftTreeNodes(fleet, new Set(["\u001f", "\u001f\u001f"]));
    const reg = n.find((x) => x.label === "C-GZQX");
    expect(reg?.title).toBe("No airline named › No type named › C-GZQX");
    expect(reg?.query).toEqual({
      tags: ["Aircraft|Registration|C-GZQX"],
      withoutTags: ["Aircraft|Airline", "Aircraft|Type"],
    });
    expect(n.find((x) => x.label === "No airline named")?.query).toEqual({
      tags: ["Aircraft"],
      withoutTags: ["Aircraft|Airline"],
    });
  });

  it("by aircraft: a registration opens to its years, a year to its months", () => {
    const reg = "KLMBoeing 747PH-BFA";
    const open = new Set(["KLM", "KLMBoeing 747", reg]);
    // Not loaded yet: the open registration asks for its years.
    const before = aircraftTreeNodes(fleet, open);
    const r = before.find((n) => n.label === "PH-BFA");
    expect(r?.expanded).toBe(true);
    expect(r?.load).toEqual({
      key: reg,
      query: {
        tags: ["Aircraft|Airline|KLM", "Aircraft|Type|Boeing 747", "Aircraft|Registration|PH-BFA"],
      },
      level: { level: "years" },
    });
    const b = (key: string, count: number) => ({ key, count, coverId: null, cover: null });
    const dates = new Map([
      [reg, [b("2019", 6), b("2024", 1), b("undated", 1)]],
      [`${reg}2019`, [b("2019-01", 2), b("2019-06", 4)]],
    ]);
    const n = aircraftTreeNodes(fleet, new Set([...open, `${reg}2019`]), dates);
    const i = n.findIndex((x) => x.label === "PH-BFA");
    expect(n[i].load).toBeUndefined();
    expect(n.slice(i + 1, i + 6).map((x) => [x.label, x.depth, x.count, x.expanded])).toEqual([
      // Newest first, No date last.
      ["2024", 3, 1, false],
      ["2019", 3, 6, true],
      ["January", 4, 2, undefined],
      ["June", 4, 4, undefined],
      ["No date", 3, 1, undefined],
    ]);
    expect(n[i + 4].title).toBe("KLM › Boeing 747 › PH-BFA › 2019 › June");
    expect(n[i + 4].query).toEqual({
      tags: ["Aircraft|Airline|KLM", "Aircraft|Type|Boeing 747", "Aircraft|Registration|PH-BFA"],
      from: "2019-06-01",
      to: "2019-06-30",
    });
    expect(n[i + 5].query).toMatchObject({ undated: true });
    // An open year not loaded yet asks for its months.
    const y = aircraftTreeNodes(fleet, new Set([...open, `${reg}2024`]), dates).find(
      (x) => x.label === "2024",
    );
    expect(y?.load?.level).toEqual({ level: "months", year: 2024 });
  });

  it("by face: people A to Z, the unnamed faces last, then years (newest first) and months", () => {
    const n = faceTreeNodes(people, new Set(["Sara", "Sara\u001f2019"]));
    expect(n.map((x) => [x.label, x.depth, x.count, x.expanded])).toEqual([
      ["Sara", 0, 2, true],
      ["2022", 1, 1, false],
      ["2019", 1, 1, true],
      ["January", 2, 1, undefined],
      ["No date", 1, 1, undefined],
      ["tom", 0, 9, false],
      ["Unnamed faces", 0, 4, false],
    ]);
    expect(n[3].title).toBe("Sara › 2019 › January");
    expect(n[3].query).toEqual({ tags: ["People|Sara"], from: "2019-01-01", to: "2019-01-31" });
  });

  it("by face: each node's search", () => {
    expect(faceQuery("Sara")).toEqual({ tags: ["People|Sara"] });
    expect(faceQuery(null)).toEqual({ unnamedFaces: true });
    // A month ends on its own last day (a leap year here).
    expect(faceDateQuery(null, 2024, 2)).toEqual({
      unnamedFaces: true,
      from: "2024-02-01",
      to: "2024-02-29",
    });
    expect(faceDateQuery("Sara", null)).toEqual({ tags: ["People|Sara"], undated: true });
  });

  it("compares queries whatever their order", () => {
    expect(sameQuery({ tags: ["a"], from: "x" }, { from: "x", tags: ["a"], undated: false })).toBe(
      true,
    );
    expect(sameQuery({ tags: ["a"] }, { tags: ["b"] })).toBe(false);
  });
});
