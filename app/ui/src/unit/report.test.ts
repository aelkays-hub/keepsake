import { describe, expect, it } from "vitest";

import type { LibraryReport } from "../bindings/LibraryReport";
import { csvName, dateLabel, lines, sum, totals } from "../report";

const g = (group: string | null, photos: number, videos: number, files: number, bytes: number) => ({
  group,
  items: photos + videos,
  photos,
  videos,
  files,
  bytes,
});

/** File types and drives: items only. */
const n = (group: string | null, items: number, files: number, bytes: number) => ({
  group,
  items,
  photos: null,
  videos: null,
  files,
  bytes,
});

const report: LibraryReport = {
  totals: g(null, 3, 1, 6, 600),
  years: [g("2024", 2, 1, 5, 500), g(null, 1, 0, 1, 100)],
  months: [g("2024-06", 2, 1, 5, 500), g(null, 1, 0, 1, 100)],
  days: [g("2024-06-01", 2, 1, 5, 500), g(null, 1, 0, 1, 100)],
  cameras: [g("ILCE-7M4", 3, 1, 6, 600)],
  lenses: [g(null, 3, 1, 6, 600)],
  fileTypes: [n("JPG", 3, 3, 100), n("ARW", 2, 2, 400), n("MP4", 1, 1, 100)],
  drives: [
    { volume: "a", online: true, row: n("Photos", 4, 6, 600) },
    { volume: "b", online: false, row: n("Backup", 4, 6, 600) },
  ],
};

describe("report", () => {
  it("labels dates and unknown groups for people", () => {
    expect(dateLabel("2024-06")).toBe("June 2024");
    expect(dateLabel("2024-06-01")).toBe("1 June 2024");
    expect(dateLabel("2024")).toBe("2024");
    expect(lines(report, "month").map((l) => l.label)).toEqual(["June 2024", "Unknown"]);
    expect(lines(report, "day")[0].label).toBe("1 June 2024");
    expect(lines(report, "lens")[0].label).toBe("Unknown");
  });

  it("counts items as photos plus videos", () => {
    const [y] = lines(report, "year");
    expect(y).toMatchObject({ label: "2024", items: 3, files: 5, bytes: 500 });
    expect(totals(report)).toMatchObject({ label: "All", items: 4, files: 6, bytes: 600 });
  });

  it("marks unplugged drives and sums every copy", () => {
    const d = lines(report, "drive");
    expect(d.map((l) => [l.key, l.label, l.offline])).toEqual([
      ["a", "Photos", false],
      ["b", "Backup", true],
    ]);
    expect(sum(d)).toEqual({ files: 12, bytes: 1200 });
    expect(lines(report, "type")[0]).toMatchObject({ items: 3, photos: null, videos: null });
  });

  it("names the CSV after the day", () => {
    expect(csvName(new Date(2026, 10, 1))).toBe("Keepsake report 2026-11-01.csv");
  });
});
