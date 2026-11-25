// The sidebar's trees by name, then year, then month (ADR-053): by airline,
// type or registration, from the aircraft keywords, or by face, from the
// people's names. Pure functions (unit-tested); the counts come from the
// `aircraft_tree` and `faces_tree` commands.
//
// Each node opens exactly its photos, as the aircraft report's lines do: a
// report section with the name's keyword in `tags` (`Aircraft|Airline|KLM`,
// `People|Sara`), and `from`/`to` for a year or month, or `undated` for the
// photos with no date. The unnamed node: for an aircraft kind, the photos
// with `Aircraft` in `tags` and the kind in `withoutTags`; by face, the
// photos with a face that has no name (`unnamedFaces`).

import type { NameTreeNode } from "./bindings/NameTreeNode";
import type { Query } from "./api";
import { calRange } from "./ai";
import * as fmt from "./format";

/** The kinds of aircraft tree. */
export type AircraftTreeKind = "airline" | "type" | "registration";
export const AIRCRAFT_TREE_KINDS: AircraftTreeKind[] = ["airline", "type", "registration"];
/** The trees by name: an aircraft kind, or by face. */
export type NameTreeKind = AircraftTreeKind | "face";

export function isAircraftTree(m: string): m is AircraftTreeKind {
  return (AIRCRAFT_TREE_KINDS as string[]).includes(m);
}

export function isNameTree(m: string): m is NameTreeKind {
  return m === "face" || isAircraftTree(m);
}

/** As in the keyword: `Aircraft|Airline|KLM`. */
export function keywordKind(kind: AircraftTreeKind): string {
  return kind[0].toUpperCase() + kind.slice(1);
}

/** The button's label: `Airline`. */
export const treeKindLabel = keywordKind;

/** A visible node of the tree. */
export interface NameNode {
  /** Its place: `KLM`, `KLM\u001f2019`, `KLM\u001f2019\u001f6`. */
  key: string;
  label: string;
  /** 0 a name, 1 a year, 2 a month. */
  depth: number;
  count: number;
  /** `undefined`: a leaf (a month, or the photos with no date). */
  expanded?: boolean;
  /** The library's heading for its photos: `KLM › 2019 › June`. */
  title: string;
  /** What finds exactly its photos. */
  query: Partial<Query>;
}

const SEP = "\u001f";

/** `No airline named`; by face, `Unnamed faces`. */
export function unnamedLabel(kind: NameTreeKind): string {
  return kind === "face" ? "Unnamed faces" : `No ${kind} named`;
}

/** The search for a name's photos (every date). */
export function nameQuery(kind: NameTreeKind, name: string | null): Partial<Query> {
  if (kind === "face") return name === null ? { unnamedFaces: true } : { tags: [`People|${name}`] };
  const k = keywordKind(kind);
  return name === null
    ? { tags: ["Aircraft"], withoutTags: [`Aircraft|${k}`] }
    : { tags: [`Aircraft|${k}|${name}`] };
}

/** The search for a name's photos of a year (or of a month of it); `year`
 * null: those with no date. */
export function dateQuery(
  kind: NameTreeKind,
  name: string | null,
  year: number | null,
  month: number | null = null,
): Partial<Query> {
  const base = nameQuery(kind, name);
  if (year === null) return { ...base, undated: true };
  const r = calRange(year, month, null);
  return r ? { ...base, from: r.from, to: r.to } : base;
}

/** The visible nodes, in order: the names, under each open one its years,
 * under each open year its months. */
export function nameTreeNodes(
  kind: NameTreeKind,
  tree: NameTreeNode[],
  open: ReadonlySet<string>,
): NameNode[] {
  const out: NameNode[] = [];
  for (const n of tree) {
    const nameKey = n.name ?? "";
    const label = n.name ?? unnamedLabel(kind);
    const isOpen = open.has(nameKey);
    out.push({
      key: nameKey,
      label,
      depth: 0,
      count: n.photos,
      expanded: n.years.length > 0 ? isOpen : undefined,
      title: label,
      query: nameQuery(kind, n.name),
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
        query: dateQuery(kind, n.name, y.year),
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
          query: dateQuery(kind, n.name, y.year, m.month),
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
