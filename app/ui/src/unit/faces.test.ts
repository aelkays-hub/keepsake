import { describe, expect, it } from "vitest";

import type { FacesStatus } from "../api";
import { byYear, cropStyle, facesProgress, searchLine, widen } from "../faces";

const status = (p: Partial<FacesStatus>): FacesStatus => ({
  enabled: true,
  aiEnabled: true,
  modelsInstalled: true,
  modelsBytes: 38928942,
  runtimeFound: true,
  accel: "graphics card (DirectML)",
  problem: null,
  installing: null,
  photos: 2000,
  scanned: 1200,
  withFaces: 700,
  faces: 1050,
  people: 0,
  pending: 800,
  failed: 0,
  ...p,
});

describe("Recognise faces status", () => {
  it("says what it needs and whether it is on", () => {
    expect(facesProgress(status({ aiEnabled: false }))).toBe("Needs On-device AI (just above).");
    expect(facesProgress(status({ enabled: false }))).toBe("Off.");
    expect(facesProgress(status({ problem: "no model" }))).toBe("Not running: no model");
    expect(facesProgress(status({ accel: null }))).toBe("Starting…");
  });

  it("counts faces, photos searched and what is waiting", () => {
    expect(facesProgress(status({}))).toBe(
      "1,050 faces in 700 of 1,200 photos searched (2,000 photos in all); 800 waiting. " +
        "On the graphics card (DirectML).",
    );
    expect(facesProgress(status({ faces: 0, withFaces: 0, scanned: 0, pending: 0 }))).toBe(
      "0 of 2,000 photos searched; no faces yet. On the graphics card (DirectML).",
    );
    expect(facesProgress(status({ faces: 1, withFaces: 1, pending: 0, accel: null }))).toBe(
      "1 face in 1 of 1,200 photos searched (2,000 photos in all).",
    );
  });
});

describe("Name and find", () => {
  it("widens a face's box, inside the picture", () => {
    const b = widen({ x: 0.5, y: 0.5, w: 0.2, h: 0.2 });
    expect(b.x).toBeCloseTo(0.46);
    expect(b.w).toBeCloseTo(0.28);
    const edge = widen({ x: 0, y: 0.9, w: 0.2, h: 0.1 });
    expect(edge.x).toBe(0);
    expect(edge.y + edge.h).toBeCloseTo(1);
  });

  it("cuts the face out at its own shape", () => {
    // A square face in a 1000 × 500 picture: 0.1 wide is 100 px, 0.2 high
    // is 100 px, so the crop is square too.
    const c = cropStyle({ x: 0.45, y: 0.4, w: 0.1, h: 0.2, pw: 1000, ph: 500 }, 80);
    expect(c.width).toBe("80px");
    expect(c.height).toBe("80px");
    // The widened box (0.14 × 0.28) fills the crop.
    expect(parseFloat(c.backgroundSize)).toBeCloseTo(100 / 0.14, 1);
    // A box at the very left starts the picture at the left edge.
    expect(cropStyle({ x: 0, y: 0, w: 0.5, h: 0.5, pw: 100, ph: 100 }, 50).backgroundPosition).toBe(
      "0.000% 0.000%",
    );
  });

  it("groups faces by year, newest first, undated last", () => {
    const g = byYear([
      { id: 1, date: "2014-05-01" },
      { id: 2, date: null },
      { id: 3, date: "2020-01-02" },
      { id: 4, date: "2014-12-31" },
    ]);
    expect(g.map(([y, l]) => [y, l.map((f) => f.id)])).toEqual([
      ["2020", [3]],
      ["2014", [1, 4]],
      ["Undated", [2]],
    ]);
  });

  it("says how far the search for faces has got", () => {
    expect(searchLine(null)).toBe("Getting ready…");
    expect(searchLine(status({ pending: 0 }))).toBe("Every photo is searched.");
    expect(searchLine(status({}))).toContain("1,200 of 2,000 done (800 to go)");
    expect(searchLine(status({ problem: "no model" }))).toBe("Not running: no model");
  });
});
