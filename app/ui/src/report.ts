// The library report's breakdowns as tables (Tools → Report). Pure
// functions (unit-tested); the numbers come from the `library_report`
// command.

import type { LibraryReport } from "./bindings/LibraryReport";
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
