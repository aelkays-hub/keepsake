import { describe, expect, it } from "vitest";

import {
  ageFacts,
  ageLabel,
  ageOn,
  builtKeyword,
  builtLabel,
  builtSource,
  registrationsOf,
  today,
} from "../aircraftAge";
import { aircraftValue } from "../aircraftReport";
import type { AircraftReportRow } from "../bindings/AircraftReportRow";
import { sortRows } from "../tableSort";

describe("the aircraft's age", () => {
  it("counts whole years from a date, a birthday not yet reached not counted", () => {
    expect(ageOn("2014-03-05", "2026-03-05")).toEqual({ years: 12, about: false });
    expect(ageOn("2014-03-05", "2026-03-04")).toEqual({ years: 11, about: false });
    expect(ageOn("2014-03-05", "2026-02-28T23:59:00")).toEqual({ years: 11, about: false });
    expect(ageOn("2014-03-05", "2014-12-31")).toEqual({ years: 0, about: false });
  });

  it("with only the year, counts the years between and says about", () => {
    expect(ageOn("2014", "2026-11-18")).toEqual({ years: 12, about: true });
    expect(ageOn("2014", "2023-01-02T10:00:00+01:00")).toEqual({ years: 9, about: true });
    expect(ageLabel({ years: 12, about: true })).toBe("about 12 years");
    expect(ageLabel({ years: 1, about: false })).toBe("1 year");
    expect(ageLabel({ years: 0, about: true })).toBe("under a year");
  });

  it("has no age when undated, unknown, or photographed before it was built", () => {
    expect(ageOn("2014", null)).toBeNull();
    expect(ageOn(null, "2026-01-01")).toBeNull();
    expect(ageOn("2014", "2012-06-01")).toBeNull();
    expect(ageOn("not a year", "2026-01-01")).toBeNull();
  });

  it("goes up by itself: worked out from the day it is shown", () => {
    expect(ageOn("2014", "2026-11-18")?.years).toBe(12);
    expect(ageOn("2014", "2027-11-18")?.years).toBe(13);
    expect(today(new Date(2026, 10, 18, 9, 30))).toBe("2026-11-18");
  });

  it("says both ages, and none in the photo when it is undated", () => {
    expect(ageFacts("2014", "2023-06-01T12:00:00", "2026-11-18")).toEqual({
      today: "about 12 years (built 2014)",
      inPhoto: "about 9 years (built 2014, photographed 2023)",
    });
    expect(ageFacts("2014-03-05", null, "2026-11-18")).toEqual({
      today: "12 years (built 5 March 2014)",
      inPhoto: null,
    });
    expect(ageFacts(null, "2023-06-01", "2026-11-18")).toEqual({ today: null, inPhoto: null });
  });

  it("reads the photo's keywords and says where the date came from", () => {
    const kws = ["Aircraft|Registration|C-FIUJ", "Aircraft|Built|2008", "Beach"];
    expect(registrationsOf(kws)).toEqual(["C-FIUJ"]);
    expect(builtKeyword(kws)).toBe("2008");
    expect(builtKeyword(["Aircraft|Built|soon"])).toBeNull();
    expect(builtLabel("2008")).toBe("2008");
    expect(builtSource("opensky")).toBe("from OpenSky");
    expect(builtSource("typed")).toBe("typed by you");
    expect(builtSource(null)).toBe("from the photo's keyword");
  });

  it("sorts the report's registrations by when built and by age, unknown last", () => {
    const row = (name: string, built: string | null): AircraftReportRow => ({
      name,
      path: `Aircraft|Registration|${name}`,
      photos: 1,
      registrations: 0,
      airlines: [],
      types: [],
      first: null,
      last: null,
      built,
    });
    const rows = [row("A", "2016-03-10"), row("B", null), row("C", "1989"), row("D", "2008")];
    const now = "2026-11-18";
    const by = (col: string, desc: boolean) =>
      sortRows(rows, { col, desc }, (r, c) => aircraftValue(r, c, now)).map((r) => r.name);
    expect(by("age", true)).toEqual(["C", "D", "A", "B"]);
    expect(by("age", false)).toEqual(["A", "D", "C", "B"]);
    expect(by("built", false)).toEqual(["C", "D", "A", "B"]);
    expect(aircraftValue(rows[0], "age", now)).toBe(10);
  });
});
