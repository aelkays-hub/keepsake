import { describe, expect, it } from "vitest";

import { byAirline, finDates, finSource, importLine, learnLine } from "../aircraftFins";
import type { FinDto } from "../bindings/FinDto";

const fin = (over: Partial<FinDto> = {}): FinDto => ({
  airline: "Air Canada",
  fin: "147",
  registration: "C-GJYI",
  from: "2019-05-05",
  to: "2024-05-05",
  source: "learned",
  seen: 3,
  conflict: false,
  ...over,
});

describe("fin numbers", () => {
  it("says when a fin stood for its registration", () => {
    expect(finDates(fin())).toBe("2019-05-05 – 2024-05-05");
    expect(finDates(fin({ to: "2019-05-05" }))).toBe("2019-05-05");
    expect(finDates(fin({ to: null }))).toBe("from 2019-05-05");
    expect(finDates(fin({ from: null }))).toBe("until 2024-05-05");
    expect(finDates(fin({ from: null, to: null }))).toBe("any time");
  });

  it("says where a fin came from", () => {
    expect(finSource(fin())).toBe("Learned from 3 photos");
    expect(finSource(fin({ seen: 1 }))).toBe("Learned from 1 photo");
    expect(finSource(fin({ source: "imported", seen: 0 }))).toBe("Imported");
  });

  it("says what an import and going over the library did", () => {
    expect(importLine({ added: 12, updated: 3, problems: [] })).toBe("12 added, 3 updated.");
    expect(importLine({ added: 0, updated: 0, problems: ["line 2: x"] })).toBe(
      "Nothing imported. 1 line not kept: line 2: x",
    );
    const many = ["line 2: a", "line 3: b", "line 4: c", "line 5: d"];
    expect(importLine({ added: 1, updated: 0, problems: many })).toBe(
      "1 added, 0 updated. 4 lines not kept: line 2: a; line 3: b; line 4: c; …",
    );
    expect(learnLine(2)).toBe("2 photos gave a fin.");
    expect(learnLine(0)).toMatch(/Read again/);
  });

  it("groups fins by airline in order", () => {
    const g = byAirline([fin(), fin({ fin: "401" }), fin({ airline: "WestJet", fin: "501" })]);
    expect(g.map((x) => [x.airline, x.fins.length])).toEqual([
      ["Air Canada", 2],
      ["WestJet", 1],
    ]);
  });
});
