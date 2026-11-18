// The aircraft report's tables (Tools → Report → Aircraft). Pure functions
// (unit-tested); the numbers come from the `aircraft_report` command.

import type { AircraftReport } from "./bindings/AircraftReport";
import type { AircraftReportGroup } from "./bindings/AircraftReportGroup";
import type { AircraftReportRow } from "./bindings/AircraftReportRow";
import { ageOn } from "./aircraftAge";
import { dateLabel } from "./report";
import type { SortValue } from "./tableSort";

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

/** A table cell's value for sorting. `age` is worked out from `now`
 * (`YYYY-MM-DD`, when the report opened), from when it was built. */
export function aircraftValue(row: AircraftReportRow, col: string, now: string): SortValue {
  switch (col) {
    case "name":
      return row.name;
    case "photos":
      return row.photos;
    case "registrations":
      return row.registrations;
    case "airline":
      return row.airlines[0] ?? null;
    case "type":
      return row.types[0] ?? null;
    case "seen":
      return row.last ?? null;
    case "built":
      return row.built;
    case "age":
      return ageOn(row.built, now)?.years ?? null;
    default:
      return null;
  }
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

// ---- The drill-down: airline › type › registration ----

/** What the report shows: the drill-down or one of the tables. */
export type AircraftView = "drill" | AircraftTable;

/** The drill-down's steps, top first. */
export const DRILL_KINDS = ["Airline", "Type", "Registration"] as const;
const DRILL_PLURAL = ["airlines", "types", "registrations"] as const;
const SEP = "\u001f";

/** One line of the drill-down as shown. */
export interface DrillRow {
  /** Its place in the tree: the names above it and its own. */
  key: string;
  /** 0 an airline, 1 a type, 2 a registration. */
  depth: number;
  /** `KLM`, or `No type named` for the photos that name none. */
  label: string;
  named: boolean;
  photos: number;
  /** How many lines are under it. */
  children: number;
  /** `3 types`, `1 registration`; empty for a registration. */
  under: string;
  open: boolean;
  /** The keywords that find exactly its photos; `null` when a step above
   * it (or its own) is unnamed, where no keywords say so. */
  tags: string[] | null;
  /** The library's heading for its photos: `KLM › Boeing 747 › PH-BFA`. */
  title: string;
}

function drillLabel(name: string | null, depth: number): string {
  return name ?? `No ${DRILL_KINDS[depth].toLowerCase()} named`;
}

/** Does `g`, or a step under it, hold `q` (lower case)? */
function drillHolds(g: AircraftReportGroup, depth: number, q: string): boolean {
  return (
    drillLabel(g.name, depth).toLowerCase().includes(q) ||
    g.children.some((c) => drillHolds(c, depth + 1, q))
  );
}

/** Every key that can be opened (airlines and types), for Expand all. */
export function drillKeys(groups: AircraftReportGroup[], above = "", depth = 0): string[] {
  if (depth > 1) return [];
  return groups.flatMap((g) => {
    const key = above + (g.name ?? "") + SEP;
    return [key, ...drillKeys(g.children, key, depth + 1)];
  });
}

/** The drill-down's lines as shown: the airlines, and under each open one
 * its types, and under each open type its registrations. With `find`,
 * only the lines that hold it (any case), those above them and those
 * under them, opened down to what was found. */
export function drillRows(
  groups: AircraftReportGroup[],
  open: ReadonlySet<string>,
  find: string,
): DrillRow[] {
  const q = find.trim().toLowerCase();
  const out: DrillRow[] = [];
  const walk = (
    list: AircraftReportGroup[],
    depth: number,
    above: string,
    names: (string | null)[],
    matched: boolean,
  ) => {
    for (const g of list) {
      const own = Boolean(q) && drillLabel(g.name, depth).toLowerCase().includes(q);
      const below = Boolean(q) && g.children.some((c) => drillHolds(c, depth + 1, q));
      if (q && !matched && !own && !below) continue;
      const key = above + (g.name ?? "") + SEP;
      const path = [...names, g.name];
      const isOpen = depth < 2 && g.children.length > 0 && (below || open.has(key));
      out.push({
        key,
        depth,
        label: drillLabel(g.name, depth),
        named: g.name !== null,
        photos: g.photos,
        children: g.children.length,
        under:
          depth < 2
            ? `${g.children.length} ${DRILL_PLURAL[depth + 1].slice(0, g.children.length === 1 ? -1 : undefined)}`
            : "",
        open: isOpen,
        tags: path.every((n) => n !== null)
          ? path.map((n, i) => `Aircraft|${DRILL_KINDS[i]}|${n}`)
          : null,
        title: path.map((n, i) => drillLabel(n, i)).join(" › "),
      });
      if (isOpen) walk(g.children, depth + 1, key, path, matched || own);
    }
  };
  walk(groups, 0, "", [], false);
  return out;
}
