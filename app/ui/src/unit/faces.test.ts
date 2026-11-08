import { describe, expect, it } from "vitest";

import type { FacesStatus } from "../api";
import { facesProgress } from "../faces";

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
