// The sidebar's trees by name (ADR-053). Pure functions (unit-tested).
//
// - By aircraft: airline › type › registration, the aircraft report's
//   drill-down (`aircraft_report`'s `byAirline`, lines from `drillRows`):
//   each line opens exactly its photos, as in the report: the keywords of
//   the steps named down to it in `tags`, the kinds unnamed in
//   `withoutTags` (`No type named`).
// - By face: person › year › month (`faces_tree`): a person's photos have
//   `People|<name>` in `tags`; the unnamed node is the photos with a face
//   that has no name (`unnamedFaces`); `from`/`to` for a year or month, or
//   `undated` for the photos with no date.
//
// Both: names A to Z (any case), the unnamed last.

import type { AircraftReportGroup } from "./bindings/AircraftReportGroup";
import type { NameTreeNode } from "./bindings/NameTreeNode";
import type { Query } from "./api";
import { calRange } from "./ai";
import { drillRows } from "./aircraftReport";
import * as fmt from "./format";

/** The trees by name. */
export type NameTreeKind = "aircraft" | "face";

export function isNameTree(m: string): m is NameTreeKind {
  return m === "aircraft" || m === "face";
}

/** A visible node of the tree. */
export interface NameNode {
  /** Its place: the names above it and its own (each kind its own form). */
  key: string;
  label: string;
  /** 0 to 2: an airline, a type, a registration; a person, a year, a month. */
  depth: number;
  count: number;
  /** `undefined`: a leaf. */
  expanded?: boolean;
  /** The library's heading for its photos: `KLM › Boeing 747`. */
  title: string;
  /** What finds exactly its photos. */
  query: Partial<Query>;
}

/** A to Z ignoring case; `null` (the unnamed) last. */
function byName(a: string | null, b: string | null): number {
  if (a === null || b === null) return a === b ? 0 : a === null ? 1 : -1;
  return a.localeCompare(b, undefined, { sensitivity: "base" }) || (a < b ? -1 : a > b ? 1 : 0);
}

/** The drill-down's steps, each sorted A to Z, the unnamed last. */
export function sortGroups(groups: AircraftReportGroup[]): AircraftReportGroup[] {
  return [...groups]
    .sort((a, b) => byName(a.name, b.name))
    .map((g) => ({ ...g, children: sortGroups(g.children) }));
}

/** By aircraft: the visible lines, airlines, under each open one its
 * types, under each open type its registrations. */
export function aircraftTreeNodes(
  groups: AircraftReportGroup[],
  open: ReadonlySet<string>,
): NameNode[] {
  return drillRows(sortGroups(groups), open, "").map((r) => ({
    key: r.key,
    label: r.label,
    depth: r.depth,
    count: r.photos,
    expanded: r.depth < 2 && r.children > 0 ? r.open : undefined,
    title: r.title,
    query: r.without.length > 0 ? { tags: r.tags, withoutTags: r.without } : { tags: r.tags },
  }));
}

const SEP = "\u001f";

export const UNNAMED_FACES = "Unnamed faces";

/** By face: the search for a person's photos (every date); `null`: the
 * photos with a face that has no name. */
export function faceQuery(name: string | null): Partial<Query> {
  return name === null ? { unnamedFaces: true } : { tags: [`People|${name}`] };
}

/** A person's photos of a year (or of a month of it); `year` null: those
 * with no date. */
export function faceDateQuery(
  name: string | null,
  year: number | null,
  month: number | null = null,
): Partial<Query> {
  const base = faceQuery(name);
  if (year === null) return { ...base, undated: true };
  const r = calRange(year, month, null);
  return r ? { ...base, from: r.from, to: r.to } : base;
}

/** By face: the visible nodes, the people A to Z (then the unnamed), under
 * each open one its years, under each open year its months. */
export function faceTreeNodes(tree: NameTreeNode[], open: ReadonlySet<string>): NameNode[] {
  const out: NameNode[] = [];
  for (const n of [...tree].sort((a, b) => byName(a.name, b.name))) {
    const nameKey = n.name ?? "";
    const label = n.name ?? UNNAMED_FACES;
    const isOpen = open.has(nameKey);
    out.push({
      key: nameKey,
      label,
      depth: 0,
      count: n.photos,
      expanded: n.years.length > 0 ? isOpen : undefined,
      title: label,
      query: faceQuery(n.name),
    });
    if (!isOpen) continue;
    for (const y of n.years) {
      const yearKey = `${nameKey}${SEP}${y.year ?? "undated"}`;
      const yearLabel = y.year === null ? "No date" : String(y.year);
      const yearOpen = open.has(yearKey);
      out.push({
        key: yearKey,
        label: yearLabel,
        depth: 1,
        count: y.photos,
        expanded: y.months.length > 0 ? yearOpen : undefined,
        title: `${label} › ${yearLabel}`,
        query: faceDateQuery(n.name, y.year),
      });
      if (!yearOpen || y.year === null) continue;
      for (const m of y.months) {
        const monthLabel = fmt.monthName(m.month);
        out.push({
          key: `${yearKey}${SEP}${m.month}`,
          label: monthLabel,
          depth: 2,
          count: m.photos,
          title: `${label} › ${yearLabel} › ${monthLabel}`,
          query: faceDateQuery(n.name, y.year, m.month),
        });
      }
    }
  }
  return out;
}

/** Is `q` (a report section's query) the one of `node`? */
export function sameQuery(a: Partial<Query>, b: Partial<Query>): boolean {
  const norm = (q: Partial<Query>) =>
    JSON.stringify(
      Object.entries(q)
        .filter(([, v]) => v !== undefined && v !== false)
        .sort(([x], [y]) => x.localeCompare(y)),
    );
  return norm(a) === norm(b);
}
