import { describe, expect, it } from "vitest";

import { aircraftJobLine, aircraftProgress } from "../aircraft";
import type { AircraftStatus } from "../api";

const status = (p: Partial<AircraftStatus>): AircraftStatus => ({
  enabled: true,
  modelsInstalled: true,
  modelsBytes: 95861692,
  runtimeFound: true,
  looks: true,
  accel: "graphics card (DirectML)",
  problem: null,
  installing: null,
  ...p,
});

describe("Read aircraft status", () => {
  it("says whether it is on, downloading or not running", () => {
    expect(aircraftProgress(status({ enabled: false }))).toBe("Off.");
    expect(aircraftProgress(status({ enabled: false, installing: 1000 }))).toBe(
      "Downloading the text models…",
    );
    expect(aircraftProgress(status({ problem: "det.onnx is missing" }))).toBe(
      "Not running: det.onnx is missing",
    );
    expect(aircraftProgress(status({ runtimeFound: false }))).toBe(
      "Not running: ONNX Runtime is missing.",
    );
  });

  it("says where it runs and how the type is found", () => {
    expect(aircraftProgress(status({}))).toBe(
      "Ready, on the graphics card (DirectML). The type is read when it is painted on the " +
        "aircraft, else guessed from its look when that is clear.",
    );
    // Before the first photo is read the models are not loaded yet.
    expect(aircraftProgress(status({ accel: null, looks: false }))).toBe(
      "Ready. The type is read when it is painted on the aircraft. To have it guessed from " +
        "the look otherwise, turn on On-device AI.",
    );
  });
});

describe("Read aircraft progress window", () => {
  const st = (analysed: number, pending: number, failed = 0, problem: string | null = null) => ({
    items: 1200,
    analysed,
    pending,
    failed,
    problem,
  });

  it("counts what was read", () => {
    expect(aircraftJobLine(st(0, 1200), false)).toBe("0 of 1,200 read…");
    expect(aircraftJobLine(st(1200, 0), false)).toBe("1,200 of 1,200 photos read.");
    expect(aircraftJobLine({ items: 0, analysed: 0, pending: 0 }, false)).toBe("Nothing to read.");
  });

  it("says what stopping keeps and what failed", () => {
    expect(aircraftJobLine(st(40, 0), true)).toBe(
      "Stopped: 40 of 1,200 read. Keywords already added stay.",
    );
    // Stop was asked for but one is still being read.
    expect(aircraftJobLine(st(40, 1), true)).toBe("40 of 1,200 read…");
    expect(aircraftJobLine(st(1198, 0, 2, "LibRaw could not decode it"), false)).toBe(
      "1,198 of 1,200 photos read. 2 photos could not be: LibRaw could not decode it",
    );
  });
});
