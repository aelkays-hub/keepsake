import { describe, expect, it } from "vitest";

import {
  aircraftCsvName,
  aircraftRows,
  aircraftRowTitle,
  dayLabel,
  seen,
  share,
  drillKeys,
  drillRows,
} from "../aircraftReport";
import type { AircraftReport } from "../bindings/AircraftReport";
import type { AircraftReportRow } from "../bindings/AircraftReportRow";

const row = (
  name: string,
  airlines: string[],
  types: string[],
  first: string | null = null,
  last: string | null = null,
): AircraftReportRow => ({
  name,
  path: `Aircraft|Registration|${name}`,
  photos: 1,
  registrations: 0,
  airlines,
  types,
  first,
  last,
});

const report: AircraftReport = {
  totals: {
    photos: 3,
    shots: 3,
    withMarks: 2,
    withAirline: 2,
    withType: 2,
    withRegistration: 2,
    airlines: 2,
    types: 2,
    registrations: 2,
    waitingReview: 0,
  },
  airlines: [],
  types: [],
  registrations: [row("PH-BFA", ["KLM"], ["Boeing 747"]), row("XA-MBA", ["Interjet"], [])],
  years: [],
  byAirline: [
    {
      name: "KLM",
      photos: 3,
      children: [
        {
          name: "Boeing 747",
          photos: 2,
          children: [{ name: "PH-BFA", photos: 2, children: [] }],
        },
        { name: null, photos: 1, children: [{ name: "PH-BVA", photos: 1, children: [] }] },
      ],
    },
    {
      name: "Interjet",
      photos: 1,
      children: [{ name: null, photos: 1, children: [{ name: null, photos: 1, children: [] }] }],
    },
  ],
};

describe("aircraft report", () => {
  it("finds rows by name, airline or type, in any case", () => {
    expect(aircraftRows(report, "registrations", "").map((r) => r.name)).toEqual([
      "PH-BFA",
      "XA-MBA",
    ]);
    expect(aircraftRows(report, "registrations", " xa- ").map((r) => r.name)).toEqual(["XA-MBA"]);
    expect(aircraftRows(report, "registrations", "klm").map((r) => r.name)).toEqual(["PH-BFA"]);
    expect(aircraftRows(report, "registrations", "747").map((r) => r.name)).toEqual(["PH-BFA"]);
    expect(aircraftRows(report, "registrations", "Concorde")).toEqual([]);
  });

  it("says when a row was seen", () => {
    expect(dayLabel("2019-01-06")).toBe("6 January 2019");
    expect(dayLabel(null)).toBe("—");
    expect(seen(row("A", [], [], "2019-01-06", "2019-01-06"))).toBe("6 January 2019");
    expect(seen(row("A", [], [], "2018-12-31", "2019-03-01"))).toBe(
      "31 December 2018 – 1 March 2019",
    );
    expect(seen(row("A", [], []))).toBe("—");
  });

  it("titles a row's photos and names the export", () => {
    expect(aircraftRowTitle("airlines", row("KLM", [], []))).toBe("Airline KLM");
    expect(aircraftRowTitle("registrations", row("PH-BFA", [], []))).toBe("Registration PH-BFA");
    expect(aircraftCsvName(new Date(2026, 10, 4))).toBe("Keepsake aircraft 2026-11-04.csv");
  });

  it("drills down from airline to type to registration", () => {
    const g = report.byAirline;
    const lines = (open: string[], find = "") =>
      drillRows(g, new Set(open), find).map((r) => `${r.depth} ${r.label} ${r.photos}`);
    // Closed: the airlines, with how many types each has.
    const closed = drillRows(g, new Set(), "");
    expect(lines([])).toEqual(["0 KLM 3", "0 Interjet 1"]);
    expect(closed.map((r) => r.under)).toEqual(["2 types", "1 type"]);
    expect(closed[0].open).toBe(false);
    // Open KLM, then its 747: types, then the registration.
    const klm = closed[0].key;
    const opened = drillRows(g, new Set([klm]), "");
    expect(opened.map((r) => r.label)).toEqual(["KLM", "Boeing 747", "No type named", "Interjet"]);
    expect(opened[1].under).toBe("1 registration");
    expect(lines([klm, opened[1].key])).toEqual([
      "0 KLM 3",
      "1 Boeing 747 2",
      "2 PH-BFA 2",
      "1 No type named 1",
      "0 Interjet 1",
    ]);
    // A line named all the way up opens exactly its photos by keywords;
    // under an unnamed step no keywords say so.
    const reg = drillRows(g, new Set([klm, opened[1].key]), "")[2];
    expect(reg.tags).toEqual([
      "Aircraft|Airline|KLM",
      "Aircraft|Type|Boeing 747",
      "Aircraft|Registration|PH-BFA",
    ]);
    expect(reg.title).toBe("KLM › Boeing 747 › PH-BFA");
    expect(opened[2].tags).toBeNull();
    expect(closed[0].tags).toEqual(["Aircraft|Airline|KLM"]);
    // Find opens down to what it finds and leaves the rest out.
    expect(lines([], "ph-bva")).toEqual(["0 KLM 3", "1 No type named 1", "2 PH-BVA 1"]);
    expect(lines([], "concorde")).toEqual([]);
    // Expand all: every airline and type.
    expect(drillRows(g, new Set(drillKeys(g)), "")).toHaveLength(8);
  });

  it("sizes bars as a share, never past the end", () => {
    expect(share(5, 20)).toBe(25);
    expect(share(3, 0)).toBe(0);
    expect(share(30, 20)).toBe(100);
  });
});
