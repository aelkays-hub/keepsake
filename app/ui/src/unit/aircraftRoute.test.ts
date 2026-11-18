import { describe, expect, it } from "vitest";

import {
  flightLine,
  keywordLabel,
  placeLabel,
  routeLine,
  routeProgress,
  stateLine,
  tickedKeywords,
  whenLine,
} from "../aircraftRoute";
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
  ...p,
});

const item = (state: string, p: Partial<RouteItemDto> = {}): RouteItemDto => ({
  id: "a1",
  name: "A.jpg",
  thumb: null,
  registration: "C-GBIJ",
  taken: "2026-05-31T12:31:11Z",
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
    expect(stateLine(item("failed", { error: "OpenSky's daily limit is reached" }))).toBe(
      "OpenSky's daily limit is reached",
    );
    expect(stateLine(item("found"))).toBeNull();
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
  });
});
