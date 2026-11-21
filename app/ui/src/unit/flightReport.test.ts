import { describe, expect, it } from "vitest";

import { flightCsvName, flightMatches, flightPlace } from "../flightReport";
import type { FlightReportRowDto } from "../bindings/FlightReportRowDto";

const row: FlightReportRowDto = {
  date: "2026-06-07",
  time: "20:00",
  flight: "AT210",
  from: "CMN",
  fromCountry: "Morocco",
  to: "YYZ",
  toCountry: "Canada",
  airline: "Royal Air Maroc",
  registration: "CN-RGY",
  aircraftType: "Boeing 787-9",
  photos: 6,
  firstPhoto: "A7402578.ARW",
  tags: ["Aircraft|Registration|CN-RGY"],
};

describe("the flight report", () => {
  it("names the CSV file by the date", () => {
    expect(flightCsvName(new Date(2026, 10, 20))).toBe("Keepsake flights 2026-11-20.csv");
  });
  it("says an airport with its country", () => {
    expect(flightPlace("YYZ", "Canada")).toBe("YYZ (Canada)");
    expect(flightPlace("ZZZZ", null)).toBe("ZZZZ");
    expect(flightPlace(null, null)).toBe("");
  });
  it("finds a flight by any of its words", () => {
    expect(flightMatches(row, "")).toBe(true);
    expect(flightMatches(row, "maroc")).toBe(true);
    expect(flightMatches(row, "at210")).toBe(true);
    expect(flightMatches(row, "morocco")).toBe(true);
    expect(flightMatches(row, "LHR")).toBe(false);
  });
});
