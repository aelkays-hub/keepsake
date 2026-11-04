import { describe, expect, it } from "vitest";

import * as fmt from "../format";
import * as sel from "../selection";

const ntsc = { num: 30000, den: 1001 };

describe("timecode", () => {
  it("counts frames from the start timecode", () => {
    const r = { num: 25, den: 1 };
    expect(fmt.timecode("10:00:00:00", r, 0)).toBe("10:00:00:00");
    expect(fmt.timecode("10:00:00:00", r, 1 / 25)).toBe("10:00:00:01");
    expect(fmt.timecode("10:00:00:00", r, 61.5)).toBe("10:01:01:12");
    expect(fmt.timecode(null, r, 3)).toBe("00:00:03:00");
    expect(fmt.timecode("23:59:59:24", r, 1 / 25)).toBe("00:00:00:00");
  });

  it("skips frame numbers 0 and 1 each minute in drop-frame, except every tenth", () => {
    // The frame after 00:00:59;29 is 00:01:00;02.
    const at = (n: number) => fmt.timecode("00:00:00;00", ntsc, (n * 1001) / 30000);
    expect(at(1799)).toBe("00:00:59;29");
    expect(at(1800)).toBe("00:01:00;02");
    // Minute 10 keeps its frames 0 and 1.
    expect(at(17982)).toBe("00:10:00;00");
    // One hour of 29.97 is 107,892 frames: exactly 01:00:00;00.
    expect(at(107892)).toBe("01:00:00;00");
    expect(fmt.timecode("01:00:00;00", ntsc, 0)).toBe("01:00:00;00");
    // Without a start timecode, 29.97 is counted drop-frame too.
    expect(fmt.timecode(null, ntsc, (1800 * 1001) / 30000)).toBe("00:01:00;02");
    // A non-drop start at 29.97 stays non-drop.
    expect(fmt.timecode("00:00:00:00", ntsc, (1800 * 1001) / 30000)).toBe("00:01:00:00");
  });

  it("finds the frame on screen without rounding errors", () => {
    for (let n = 0; n < 500; n++) expect(fmt.frameAt(ntsc, (n * 1001) / 30000)).toBe(n);
    expect(fmt.frameAt({ num: 25, den: 1 }, 0.039)).toBe(0);
  });
});

describe("offset text", () => {
  it("reads like a person wrote it", () => {
    expect(fmt.offset(0)).toBe("no change");
    expect(fmt.offset(3600)).toBe("+1 h");
    expect(fmt.offset(-1800)).toBe("−30 min");
    expect(fmt.offset(90061)).toBe("+1 d 1 h 1 min 1 s");
  });
});

describe("viewer navigation", () => {
  const many: sel.Sel = {
    ranges: [
      [2, 4],
      [10, 12],
    ],
    anchor: 2,
    focus: 3,
  };
  it("moves through the selection when several items are selected", () => {
    expect(sel.viewerStep(many, 100, 2, "next")).toBe(3);
    expect(sel.viewerStep(many, 100, 3, "next")).toBe(10);
    expect(sel.viewerStep(many, 100, 11, "next")).toBeNull();
    expect(sel.viewerStep(many, 100, 10, "prev")).toBe(3);
    expect(sel.viewerStep(many, 100, 2, "prev")).toBeNull();
    expect(sel.viewerStep(many, 100, 3, "last")).toBe(11);
    expect(sel.viewerStep(many, 100, 11, "first")).toBe(2);
  });
  it("moves through the whole view otherwise", () => {
    const one = sel.only(5);
    expect(sel.viewerStep(one, 100, 5, "next")).toBe(6);
    expect(sel.viewerStep(one, 100, 5, "prev")).toBe(4);
    expect(sel.viewerStep(one, 100, 99, "next")).toBeNull();
    expect(sel.viewerStep(sel.emptySel, 100, 0, "last")).toBe(99);
  });
});
