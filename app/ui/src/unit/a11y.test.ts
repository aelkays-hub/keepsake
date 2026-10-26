import { describe, expect, it } from "vitest";

import { describeRow } from "../a11y";
import type { Row } from "../api";

const row: Row = {
  id: "a1",
  name: "DSC01234.ARW",
  captured: "2024-06-15T14:02:11",
  camera: "ILCE-7M4",
  lens: null,
  kind: "raw",
  size: 24e6,
  durationMs: null,
  rating: 4,
  label: "Red",
  thumb: null,
  online: false,
};

describe("spoken tile descriptions", () => {
  it("say what the item is, when, its rating, label and whether it is available", () => {
    expect(describeRow(row, 0)).toBe(
      "DSC01234.ARW, RAW photo, 2024-06-15 14:02, 4 stars, Red label, drive not connected",
    );
    expect(
      describeRow(
        { ...row, kind: "video", durationMs: 75_000, rating: 1, label: null, online: true },
        0,
      ),
    ).toBe("DSC01234.ARW, video, 2024-06-15 14:02, 1:15, 1 star");
    expect(describeRow(undefined, 41)).toBe("Item 42, loading");
  });
});
