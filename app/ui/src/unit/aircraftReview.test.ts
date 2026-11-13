import { describe, expect, it } from "vitest";

import type { AircraftReviewDto, AircraftReviewItemDto } from "../api";
import {
  alsoRead,
  isTicked,
  lookupLine,
  parts,
  pctOf,
  progressLine,
  secondsLabel,
  sourceOf,
  ticked,
  tickedNeighbours,
  waitingCount,
} from "../aircraftReview";

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
  unticked: [],
  fromLookup: [],
  lookups: [],
  neighbours: [],
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

  it("starts a disagreeing painted type unticked; a click turns any over", () => {
    const it2 = item({
      offered: [
        "Aircraft|Registration|C-FIUJ",
        "Aircraft|Type|Boeing 777-300ER",
        "Aircraft|Type|Boeing 777-200LR",
      ],
      unticked: ["Aircraft|Type|Boeing 777-300ER"],
      fromLookup: ["Aircraft|Type|Boeing 777-200LR"],
    });
    expect(ticked(it2, undefined)).toEqual([
      "Aircraft|Registration|C-FIUJ",
      "Aircraft|Type|Boeing 777-200LR",
    ]);
    const clicks = new Set(["Aircraft|Type|Boeing 777-300ER", "Aircraft|Type|Boeing 777-200LR"]);
    expect(isTicked(it2, "Aircraft|Type|Boeing 777-300ER", clicks)).toBe(true);
    expect(isTicked(it2, "Aircraft|Type|Boeing 777-200LR", clicks)).toBe(false);
    expect(sourceOf(it2, "Aircraft|Type|Boeing 777-200LR")).toBe("(adsbdb)");
    expect(sourceOf(it2, "Aircraft|Registration|C-FIUJ")).toBe("(its look)");
  });

  it("ticks alike neighbours, says what the registry knows", () => {
    const it3 = item({
      neighbours: [
        { id: "b", name: "B", thumb: null, seconds: 2, similarityPct: 96, alike: true },
        { id: "c", name: "C", thumb: null, seconds: -5, similarityPct: 61, alike: false },
      ],
      lookups: [
        {
          registration: "C-FIUJ",
          known: true,
          aircraftType: "Boeing 777-200LR",
          airline: "Air Canada",
          owner: "Air Canada",
          country: "Canada",
        },
        {
          registration: "VT-JEX",
          known: false,
          aircraftType: null,
          airline: null,
          owner: null,
          country: null,
        },
      ],
    });
    expect(tickedNeighbours(it3, undefined)).toEqual(["b"]);
    expect(tickedNeighbours(it3, new Set(["b", "c"]))).toEqual(["c"]);
    expect(lookupLine(it3)).toBe(
      "C-FIUJ: Boeing 777-200LR, Air Canada, Canada; adsbdb does not know VT-JEX",
    );
    expect(secondsLabel(2.4)).toBe("+2 s");
    expect(secondsLabel(-5)).toBe("−5 s");
  });
});
