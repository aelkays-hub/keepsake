// The aircraft report's tables (Tools → Report → Aircraft). Pure functions
// (unit-tested); the numbers come from the `aircraft_report` command.

import type { AircraftReport } from "./bindings/AircraftReport";
import type { AircraftReportRow } from "./bindings/AircraftReportRow";
import { dateLabel } from "./report";

export type AircraftTable = "airlines" | "types" | "registrations";

export const AIRCRAFT_TABLES: {
  key: AircraftTable;
  label: string;
  column: string;
  /** The library's heading for a row's photos: `Airline KLM`. */
  title: string;
}[] = [
  { key: "airlines", label: "Airlines", column: "Airline", title: "Airline" },
  { key: "types", label: "Types", column: "Type", title: "Type" },
  { key: "registrations", label: "Registrations", column: "Registration", title: "Registration" },
];

/** The rows of one table whose name, airlines or types hold `find` (any
 * case); all of them when `find` is blank. */
export function aircraftRows(
  r: AircraftReport,
  table: AircraftTable,
  find: string,
): AircraftReportRow[] {
  const q = find.trim().toLowerCase();
  const rows = r[table];
  if (!q) return rows;
  return rows.filter((row) =>
    [row.name, ...row.airlines, ...row.types].some((s) => s.toLowerCase().includes(q)),
  );
}

/** `2019-01-06` → `6 January 2019`; a dash when unknown. */
export function dayLabel(d: string | null): string {
  return d ? dateLabel(d) : "—";
}

/** First and last day: one day when both are the same. */
export function seen(row: AircraftReportRow): string {
  if (!row.first || !row.last) return "—";
  return row.first === row.last
    ? dayLabel(row.first)
    : `${dayLabel(row.first)} – ${dayLabel(row.last)}`;
}

/** The library's heading for a row's photos: `Airline KLM`. */
export function aircraftRowTitle(table: AircraftTable, row: AircraftReportRow): string {
  const t = AIRCRAFT_TABLES.find((x) => x.key === table);
  return `${t?.title ?? ""} ${row.name}`.trim();
}

/** The share of `n` in `of`, 0–100, for a bar. */
export function share(n: number, of: number): number {
  return of > 0 ? Math.min(100, (100 * n) / of) : 0;
}

/** A default file name for the CSV export: `Keepsake aircraft 2026-11-14.csv`. */
export function aircraftCsvName(now: Date): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `Keepsake aircraft ${now.getFullYear()}-${p(now.getMonth() + 1)}-${p(now.getDate())}.csv`;
}
