import { describe, expect, it } from "vitest";

import type { AircraftReviewDto, AircraftReviewItemDto } from "../api";
import { alsoRead, parts, pctOf, progressLine, ticked, waitingCount } from "../aircraftReview";

const item = (p: Partial<AircraftReviewItemDto> = {}): AircraftReviewItemDto => ({
  id: "a1",
  name: "DSC05972.ARW",
  thumb: null,
  review: "waiting",
  fullSize: true,
  registrations: [{ name: "VT-JEX", pct: 100 }],
  airlines: [{ name: "Jet Airways", pct: 82 }],
  types: [{ name: "Boeing 777-300ER", pct: 81 }],
  looksLike: "Boeing 777",
  offered: ["Aircraft|Registration|VT-JEX", "Aircraft|Type|Boeing 777-300ER"],
  ...p,
});

const review = (p: Partial<AircraftReviewDto> = {}): AircraftReviewDto => ({
  items: [item(), item({ id: "a2", review: "accepted" })],
  confidence: { registrationPct: 80, airlinePct: 85, typePct: 70 },
  total: 5,
  read: 2,
  pending: 3,
  failed: 0,
  problem: null,
  ...p,
});

describe("Read aircraft review", () => {
  it("splits a keyword and finds how surely its mark was read", () => {
    expect(parts("Aircraft|Type|Boeing 777-300ER")).toEqual(["Type", "Boeing 777-300ER"]);
    expect(pctOf(item(), "Aircraft|Registration|VT-JEX")).toBe(100);
    expect(pctOf(item(), "Aircraft|Type|Boeing 777-300ER")).toBe(81);
    // The type from its look has no confidence of its own.
    expect(pctOf(item(), "Aircraft|Type|Boeing 777")).toBeNull();
  });

  it("keeps what is offered less what was unticked", () => {
    expect(ticked(item(), undefined)).toEqual([
      "Aircraft|Registration|VT-JEX",
      "Aircraft|Type|Boeing 777-300ER",
    ]);
    expect(ticked(item(), new Set(["Aircraft|Type|Boeing 777-300ER"]))).toEqual([
      "Aircraft|Registration|VT-JEX",
    ]);
  });

  it("lists what was read below the confidence", () => {
    expect(alsoRead(item())).toEqual([{ label: "Airline Jet Airways", pct: 82 }]);
    expect(alsoRead(item({ airlines: [] }))).toEqual([]);
  });

  it("says how reading is getting on", () => {
    expect(progressLine(review(), false)).toBe("Reading… 2 of 5 read.");
    expect(progressLine(review(), true)).toBe("Stopped: 2 of 5 read.");
    expect(progressLine(review({ pending: 0, read: 5 }), false)).toBe("5 of 5 photos read.");
    expect(
      progressLine(review({ pending: 0, read: 4, failed: 1, problem: "no file" }), false),
    ).toBe("4 of 5 photos read. 1 photo could not be read: no file.");
    expect(progressLine(review({ total: 0, read: 0, pending: 0 }), false)).toBe("Nothing to read.");
    expect(waitingCount(review())).toBe(1);
  });
});
