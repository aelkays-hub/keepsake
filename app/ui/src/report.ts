// The library report's breakdowns as tables (Tools → Report). Pure
// functions (unit-tested); the numbers come from the `library_report`
// command.

import type { LibraryReport } from "./bindings/LibraryReport";
import type { Query } from "./bindings/Query";
import type { ReportGroup } from "./bindings/ReportGroup";

export type Breakdown = "year" | "month" | "day" | "camera" | "lens" | "type" | "drive";

export const BREAKDOWNS: { key: Breakdown; label: string; column: string }[] = [
  { key: "year", label: "Year", column: "Year" },
  { key: "month", label: "Month", column: "Month" },
  { key: "day", label: "Day", column: "Day" },
  { key: "camera", label: "Camera", column: "Camera" },
  { key: "lens", label: "Lens", column: "Lens" },
  { key: "type", label: "File type", column: "Type" },
  { key: "drive", label: "Drive", column: "Drive" },
];

export interface ReportLine {
  key: string;
  /** The group as the report gives it (`2024-06`, `ILCE-7M4`); null when
   * unknown. For drives, the drive's name (`key` is its volume). */
  group: string | null;
  /** As shown: `Unknown` for a missing group, `June 2024` for a month. */
  label: string;
  /** The drive is unplugged (drives only). */
  offline?: boolean;
  /** Null where not split (file types and drives). */
  photos: number | null;
  videos: number | null;
  items: number;
  files: number;
  bytes: number;
}

const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

/** `2024-06` → `June 2024`; `2024-06-01` → `1 June 2024`. */
export function dateLabel(g: string): string {
  const [y, m, d] = g.split("-");
  const month = MONTHS[Number(m) - 1];
  if (!month) return g;
  return d ? `${Number(d)} ${month} ${y}` : `${month} ${y}`;
}

function line(r: ReportGroup, label?: string): ReportLine {
  return {
    key: r.group ?? "\u0000unknown",
    group: r.group,
    label: label ?? r.group ?? "Unknown",
    photos: r.photos,
    videos: r.videos,
    items: r.items,
    files: r.files,
    bytes: r.bytes,
  };
}

/** The rows of one breakdown. */
export function lines(r: LibraryReport, b: Breakdown): ReportLine[] {
  const dated = (rows: ReportGroup[]) =>
    rows.map((g) => line(g, g.group === null ? "Unknown" : dateLabel(g.group)));
  switch (b) {
    case "year":
      return r.years.map((g) => line(g));
    case "month":
      return dated(r.months);
    case "day":
      return dated(r.days);
    case "camera":
      return r.cameras.map((g) => line(g));
    case "lens":
      return r.lenses.map((g) => line(g));
    case "type":
      return r.fileTypes.map((g) => line(g));
    case "drive":
      return r.drives.map((d) => ({ ...line(d.row), key: d.volume, offline: !d.online }));
  }
}

/** `2024-02` → 29: the last day of a `YYYY-MM` month. */
function lastDay(month: string): number {
  const [y, m] = month.split("-").map(Number);
  return new Date(Date.UTC(y, m, 0)).getUTCDate();
}

/** The filter that shows a line's items in the library, so that the grid
 * holds as many items as the line counts. */
export function lineQuery(b: Breakdown, l: ReportLine): Partial<Query> {
  const g = l.group;
  switch (b) {
    case "year":
    case "month":
    case "day": {
      if (g === null) return { undated: true };
      if (b === "year") return { from: `${g}-01-01`, to: `${g}-12-31` };
      if (b === "month") return { from: `${g}-01`, to: `${g}-${String(lastDay(g))}` };
      return { from: g, to: g };
    }
    case "camera":
      return { cameraIs: g ?? "" };
    case "lens":
      return { lensIs: g ?? "" };
    case "type":
      return { fileType: g ?? "" };
    case "drive":
      return { volume: l.key };
  }
}

/** The library's heading for a line's items: `Year 2024`, `Camera ILCE-7M4`. */
export function lineTitle(b: Breakdown, l: ReportLine): string {
  if (b === "type" && l.group === "") return "Files without an extension";
  const meta = BREAKDOWNS.find((x) => x.key === b);
  return `${meta?.column ?? ""} ${l.label}`.trim();
}

/** The grand totals as a line. */
export function totals(r: LibraryReport): ReportLine {
  return line(r.totals, "All");
}

/** Files and bytes of some lines (the drives table: every copy on every
 * drive). Items are not added up: one item can be on several drives. */
export function sum(rows: ReportLine[]): { files: number; bytes: number } {
  let files = 0;
  let bytes = 0;
  for (const r of rows) {
    files += r.files;
    bytes += r.bytes;
  }
  return { files, bytes };
}

/** A default file name for the CSV export: `Keepsake report 2026-11-01.csv`. */
export function csvName(now: Date): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `Keepsake report ${now.getFullYear()}-${p(now.getMonth() + 1)}-${p(now.getDate())}.csv`;
}
