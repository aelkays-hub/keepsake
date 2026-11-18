import { describe, expect, it } from "vitest";

import type { AircraftReviewDto, AircraftReviewItemDto } from "../api";
import {
  alsoRead,
  confidenceOf,
  isTicked,
  lookupLine,
  ordered,
  parts,
  pctOf,
  progressLine,
  secondsLabel,
  shownIds,
  sourceOf,
  ticked,
  tickedNeighbours,
  waitingCount,
  toReview,
  belowLevels,
  nothingFound,
  noneWaitingLine,
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
  confidence: { registrationPct: 80, airlinePct: 85, typePct: 70, sameShotS: 10 },
  total: 5,
  read: 2,
  pending: 3,
  failed: 0,
  problem: null,
  ...p,
});

describe("Read aircraft review", () => {
  it("lists more marks and surer ones first, accepted next, dismissed last", () => {
    const reg = "Aircraft|Registration|VT-JEX";
    const airline = "Aircraft|Airline|Jet Airways";
    const type = "Aircraft|Type|Boeing 777-300ER";
    // Registration 100, airline 82, type 81.
    expect(confidenceOf(item({ offered: [reg, type] }))).toBe(90.5);
    expect(confidenceOf(item({ offered: [] }))).toBe(0);
    const list = [
      item({ id: "dismissed", review: "dismissed", offered: [reg, airline, type] }),
      item({ id: "one", offered: [type] }),
      item({ id: "accepted", review: "accepted", offered: [reg, airline, type] }),
      item({ id: "two-less-sure", offered: [airline, type] }),
      item({ id: "three", offered: [reg, airline, type] }),
      item({ id: "two-surer", offered: [reg, type] }),
      item({ id: "none", offered: [] }),
      item({ id: "one-later", offered: [type] }),
    ];
    expect(ordered(list).map((i) => i.id)).toEqual([
      "three",
      "two-surer",
      "two-less-sure",
      "one",
      "one-later",
      "none",
      "accepted",
      "dismissed",
    ]);
  });

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
    // After Stop nothing is pending, but the photos not read still say so.
    expect(progressLine(review({ pending: 0 }), true)).toBe("Stopped: 2 of 5 read.");
    expect(progressLine(review({ pending: 0, read: 5 }), true)).toBe("5 of 5 photos read.");
    expect(progressLine(review({ pending: 0, read: 5 }), false)).toBe("5 of 5 photos read.");
    expect(
      progressLine(review({ pending: 0, read: 4, failed: 1, problem: "no file" }), false),
    ).toBe("4 of 5 photos read. 1 photo could not be read: no file.");
    expect(progressLine(review({ total: 0, read: 0, pending: 0 }), false)).toBe("Nothing to read.");
    expect(waitingCount(review())).toBe(1);
    // Waiting with nothing at these levels: not to review, counted apart.
    const below = review({ items: [item(), item({ id: "a3", offered: [] })] });
    expect(toReview(below.items[1])).toBe(false);
    expect(toReview(below.items[0])).toBe(true);
    expect(toReview(item({ review: "dismissed" }))).toBe(false);
    expect(waitingCount(below)).toBe(1);
    expect(belowLevels(below)).toBe(1);
  });

  it("says why no photo is listed", () => {
    const empty = { registrations: [], airlines: [], types: [], offered: [] };
    const none = item({ ...empty, review: "dismissed" });
    expect(nothingFound(none)).toBe(true);
    expect(nothingFound(item({ offered: [] }))).toBe(false);
    // The one photo picked had nothing painted that could be read.
    expect(noneWaitingLine(review({ items: [none] }))).toBe(
      "No registration, airline or type was found on this photo.",
    );
    expect(noneWaitingLine(review({ items: [none, { ...none, id: "a2" }] }))).toBe(
      "No registration, airline or type was found on 2 photos.",
    );
    expect(noneWaitingLine(review({ items: [none, item({ review: "accepted" })] }))).toContain(
      "The other has been reviewed",
    );
    expect(noneWaitingLine(review({ items: [item({ review: "accepted" })] }))).toBe(
      "Every photo read so far has been reviewed.",
    );
    expect(noneWaitingLine(review({ items: [item({ offered: [] })] }))).toBe(
      "Nothing more to accept at these levels.",
    );
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
    // The build year comes from OpenSky's database (ADR-044).
    const built = item({
      offered: ["Aircraft|Built|2008"],
      fromLookup: ["Aircraft|Built|2008"],
    });
    expect(sourceOf(built, "Aircraft|Built|2008")).toBe("(OpenSky)");
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
          built: "2008-03-05",
        },
        {
          registration: "VT-JEX",
          known: false,
          aircraftType: null,
          airline: null,
          owner: null,
          country: null,
          built: null,
        },
      ],
    });
    expect(tickedNeighbours(it3, undefined)).toEqual(["b"]);
    expect(tickedNeighbours(it3, new Set(["b", "c"]))).toEqual(["c"]);
    expect(lookupLine(it3)).toBe(
      "C-FIUJ: Boeing 777-200LR, Air Canada, Canada, built 2008; adsbdb does not know VT-JEX",
    );
    expect(secondsLabel(2.4)).toBe("+2 s");
    expect(secondsLabel(-5)).toBe("−5 s");
  });

  it("asks for neighbours only for the photos on screen", () => {
    const r = review({
      items: [item({ id: "a" }), item({ id: "b", review: "accepted" }), item({ id: "c" })],
    });
    expect(shownIds(r, false, 100)).toEqual(["a", "c"]);
    expect(shownIds(r, true, 2)).toEqual(["a", "b"]);
    expect(shownIds(null, false, 100)).toEqual([]);
    // A photo offering nothing at these levels is not listed (unless all).
    const empty = review({ items: [item({ id: "x", offered: [] }), item({ id: "y" })] });
    expect(shownIds(empty, false, 100)).toEqual(["y"]);
    expect(shownIds(empty, true, 100)).toEqual(["x", "y"]);
    // Not looked for: the app gives the keywords to the alike ones.
    expect(tickedNeighbours(item({ neighbours: null }), undefined)).toBeNull();
  });
});
