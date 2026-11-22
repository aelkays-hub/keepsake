import { describe, expect, it } from "vitest";

import {
  flightLine,
  keywordLabel,
  placeLabel,
  routeLine,
  routeProgress,
  takenLine,
  offsetLabel,
  cleanCode,
  zoneChoices,
  ZONES,
  stateLine,
  tickedKeywords,
  whenLine,
  cleanFlight,
  suggestionLine,
  flightOnlyKeywords,
} from "../aircraftRoute";
import type { RouteSuggestionDto } from "../bindings/RouteSuggestionDto";
import type { RouteChoiceDto } from "../bindings/RouteChoiceDto";
import type { RouteItemDto } from "../bindings/RouteItemDto";

const choice = (p: Partial<RouteChoiceDto>): RouteChoiceDto => ({
  callsign: "ACA856",
  flight: "AC856",
  from: "YYZ",
  fromIcao: "CYYZ",
  fromCountry: "Canada",
  to: "LHR",
  toIcao: "EGLL",
  toCountry: "United Kingdom",
  departed: "2026-05-31T12:16:00Z",
  arrived: "2026-05-31T19:25:00Z",
  at: "departure",
  gapMin: 15,
  keywords: ["Aircraft|From|YYZ", "Aircraft|To|LHR", "Aircraft|Flight|AC856"],
  inferred: false,
  byCallsign: false,
  ...p,
});

const item = (state: string, p: Partial<RouteItemDto> = {}): RouteItemDto => ({
  id: "a1",
  name: "A.jpg",
  thumb: null,
  registration: "C-GBIJ",
  taken: "2026-05-31T12:31:11Z",
  local: "2026-05-31T08:31:11",
  offsetMin: -240,
  zoneSet: false,
  code: "C04826",
  codeTyped: false,
  state,
  error: null,
  choices: [],
  has: [],
  ...p,
});

describe("Add route", () => {
  it("says the route, the flight and where the photo is in it", () => {
    const c = choice({});
    expect(routeLine(c)).toBe("YYZ (Canada) → LHR (United Kingdom)");
    expect(flightLine(c)).toBe("Flight AC856 (callsign ACA856)");
    expect(whenLine(c)).toBe("Taken 15 min before take-off (12:16 UTC)");
    expect(whenLine(choice({ at: "arrival", gapMin: 20 }))).toBe(
      "Taken 20 min after landing (19:25 UTC)",
    );
    expect(whenLine(choice({ at: "arrival", gapMin: 0 }))).toBe(
      "Taken in the air, before landing (19:25 UTC)",
    );
  });

  it("says a flight the source did not see, worked out from those before and after", () => {
    const c = choice({
      inferred: true,
      at: "between",
      from: "YYZ",
      to: "WAW",
      flight: null,
      callsign: null,
      departed: "2026-05-18T21:22:00Z",
      arrived: "2026-05-19T10:52:00Z",
    });
    expect(flightLine(c)).toBe("Flight not seen (worked out from the flights before and after)");
    expect(whenLine(c)).toBe(
      "Between landing at YYZ (21:22 UTC) and next taking off from WAW (10:52 UTC, next day)",
    );
  });

  it("says the flight in or out the source did not see, and an end from the callsign", () => {
    // A7402578.ARW: CN-RGY landed at YYZ from Casablanca; OpenSky saw only
    // its take-off for CMN at 02:30 UTC.
    const inbound = choice({
      inferred: true,
      at: "inbound",
      from: "CMN",
      to: "YYZ",
      flight: null,
      callsign: null,
      departed: "2026-06-08T02:30:06Z",
      arrived: "2026-06-08T02:30:06Z",
      gapMin: 0,
    });
    expect(flightLine(inbound)).toBe("Flight in not seen (worked out from the next take-off)");
    expect(whenLine(inbound)).toBe(
      "Landed at YYZ before the photo; next took off from there at 02:30 UTC for CMN, so most likely came from there",
    );
    expect(whenLine({ ...inbound, from: null })).toBe(
      "Landed at YYZ before the photo; next took off from there at 02:30 UTC",
    );
    const outbound = choice({
      inferred: true,
      at: "outbound",
      from: "YYZ",
      to: "CMN",
      flight: null,
      callsign: null,
      departed: "2026-06-07T22:10:00Z",
      arrived: "2026-06-07T22:10:00Z",
    });
    expect(flightLine(outbound)).toBe("Flight out not seen (worked out from the landing before)");
    expect(whenLine(outbound)).toBe(
      "Took off from YYZ after the photo, having landed there at 22:10 UTC; it came from CMN, so most likely went back there",
    );
    expect(flightLine(choice({ byCallsign: true }))).toBe(
      "Flight AC856 (callsign ACA856), where the source lost it from the callsign's usual route",
    );
  });

  it("says Not known for an airport OpenSky did not see, and keeps an ICAO code", () => {
    expect(routeLine(choice({ to: null, toCountry: null }))).toBe("YYZ (Canada) → Not known");
    expect(placeLabel("ZZZZ", null)).toBe("ZZZZ");
    expect(flightLine(choice({ flight: "BAW12AB", callsign: "BAW12AB" }))).toBe("Flight BAW12AB");
    expect(flightLine(choice({ flight: null, callsign: null }))).toBe("Flight not known");
  });

  it("explains every photo without a flight", () => {
    expect(stateLine(item("no-registration"))).toContain("Read aircraft");
    expect(stateLine(item("no-transponder"))).toContain("C-GBIJ is not known");
    expect(stateLine(item("nothing"))).toContain("back to about 2023");
    expect(stateLine(item("none-near"))).toContain("Shift time");
    expect(stateLine(item("none-near"))).toContain("Camera clock on Eastern time");
    expect(stateLine(item("failed", { error: "OpenSky's daily limit is reached" }))).toBe(
      "OpenSky's daily limit is reached",
    );
    expect(stateLine(item("found"))).toBeNull();
    const limit =
      "OpenSky's limit is reached (it allows more at 2026-11-21 11:10 UTC); Add route goes on by itself then";
    expect(stateLine(item("later", { error: limit }))).toBe(limit);
    expect(stateLine(item("later"))).toContain("goes on by itself");
  });

  it("says when the photo was taken, in the camera's time and UTC", () => {
    expect(offsetLabel(-240)).toBe("UTC−04:00");
    expect(offsetLabel(330)).toBe("UTC+05:30");
    expect(takenLine(item("found"))).toBe("Taken 2026-05-31 08:31 (UTC−04:00) · 12:31 UTC");
    expect(takenLine(item("found", { local: null, offsetMin: null }))).toBe(
      "Taken 2026-05-31 12:31, time zone not known (read as UTC)",
    );
    expect(takenLine(item("no-time", { taken: null }))).toBeNull();
  });

  it("offers time zones and reads a typed transponder code", () => {
    expect(ZONES[0]).toBe(-720);
    expect(ZONES[ZONES.length - 1]).toBe(840);
    expect(ZONES).toContain(-300);
    expect(ZONES).toContain(330);
    expect(zoneChoices(-300)).toBe(ZONES);
    expect(zoneChoices(-17)).toContain(-17);
    expect(cleanCode(" 484f6e ")).toBe("484F6E");
    expect(cleanCode("0x484F6E")).toBe("484F6E");
    expect(cleanCode("")).toBeNull();
    expect(cleanCode("PH-BKO")).toBeUndefined();
    expect(stateLine(item("no-transponder"))).toContain("Type it below");
  });

  it("offers the keywords left ticked", () => {
    const c = choice({});
    expect(tickedKeywords(c, new Set(["Aircraft|To|LHR"]))).toEqual([
      "Aircraft|From|YYZ",
      "Aircraft|Flight|AC856",
    ]);
    expect(tickedKeywords(c, undefined)).toHaveLength(3);
    expect(keywordLabel("Aircraft|From country|Canada")).toBe("From country Canada");
    expect(routeProgress(2, 5, false)).toBe("Asking OpenSky about 2 of 5 photos…");
    expect(routeProgress(0, 1, false)).toBe("1 photo.");
    expect(routeProgress(1, 5, true)).toContain("Stopped");
    expect(routeProgress(2, 5, false, "flightradar")).toBe(
      "Asking Flightradar24 about 2 of 5 photos…",
    );
    expect(stateLine(item("waiting"), "flightradar")).toBe("Asking Flightradar24…");
    expect(stateLine(item("nothing"), "flightradar")).toContain("depends on your API plan");
  });
});

describe("a route typed by hand", () => {
  const sg = (p: Partial<RouteSuggestionDto>): RouteSuggestionDto => ({
    from: "YYZ",
    fromCountry: "Canada",
    to: "LHR",
    toCountry: "United Kingdom",
    flight: "AC856",
    photos: 3,
    latest: "2026-05-31",
    keywords: [],
    ...p,
  });

  it("cleans the flight number typed", () => {
    expect(cleanFlight(" ac 856 ")).toBe("AC856");
    expect(cleanFlight("AC/85|6")).toBe("AC-85-6");
    expect(cleanFlight("   ")).toBeNull();
  });

  it("says each route offered, with its photos and the latest day", () => {
    expect(suggestionLine(sg({}))).toBe(
      "YYZ (Canada) → LHR (United Kingdom): 3 photos, latest 2026-05-31",
    );
    expect(suggestionLine(sg({ photos: 1, latest: null, to: null, toCountry: null }))).toBe(
      "YYZ (Canada) → Not known: 1 photo",
    );
  });

  it("adds only the flight number, keeping the airports the photo has", () => {
    expect(flightOnlyKeywords(["Aircraft|From|YYZ", "Aircraft|Flight|AC855"], "AC856")).toEqual([
      "Aircraft|From|YYZ",
      "Aircraft|Flight|AC856",
    ]);
    expect(flightOnlyKeywords([], "XX1")).toEqual(["Aircraft|Flight|XX1"]);
  });
});
