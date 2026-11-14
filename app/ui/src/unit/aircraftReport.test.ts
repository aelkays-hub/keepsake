import { describe, expect, it } from "vitest";

import {
  aircraftCsvName,
  aircraftRows,
  aircraftRowTitle,
  dayLabel,
  seen,
  share,
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

  it("sizes bars as a share, never past the end", () => {
    expect(share(5, 20)).toBe(25);
    expect(share(3, 0)).toBe(0);
    expect(share(30, 20)).toBe(100);
  });
});
