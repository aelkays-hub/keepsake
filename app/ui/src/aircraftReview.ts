// Read aircraft's review window (ADR-041): what each photo offers at the
// window's confidence, which of it is ticked, and what the window says.

import type { AircraftReviewDto, AircraftReviewItemDto } from "./api";
import * as fmt from "./format";

/** The three kinds of mark, as the sliders and the keywords name them. */
export const KINDS = [
  { key: "registrationPct", kind: "Registration", label: "Registration" },
  { key: "airlinePct", kind: "Airline", label: "Airline" },
  { key: "typePct", kind: "Type", label: "Type" },
] as const;

/** The lowest a slider goes: lines read less surely are not kept. */
export const MIN_PCT = 50;

/** A keyword's parts: `Aircraft|Type|Boeing 777` → `["Type", "Boeing 777"]`. */
export function parts(keyword: string): [string, string] {
  const [, kind = "", ...name] = keyword.split("|");
  return [kind, name.join("|")];
}

/** How surely the mark a keyword names was read (percent), or `null` for
 * the type from the look or from the online lookup. */
export function pctOf(item: AircraftReviewItemDto, keyword: string): number | null {
  const [kind, name] = parts(keyword);
  const list =
    kind === "Registration"
      ? item.registrations
      : kind === "Airline"
        ? item.airlines
        : kind === "Type"
          ? item.types
          : [];
  return list.find((m) => m.name.replace(/[/|]/g, "-") === name)?.pct ?? null;
}

/** Where an offered keyword comes from, for its label. */
export function sourceOf(item: AircraftReviewItemDto, keyword: string): string {
  const pct = pctOf(item, keyword);
  if (item.fromLookup.includes(keyword)) return pct === null ? "(adsbdb)" : `${pct}%, adsbdb`;
  return pct === null ? "(its look)" : `${pct}%`;
}

/** Is `keyword` ticked? Ticked at first unless offered unticked; each
 * click the user made (`toggled`) turns it over. */
export function isTicked(
  item: AircraftReviewItemDto,
  keyword: string,
  toggled: Set<string> | undefined,
): boolean {
  return !item.unticked.includes(keyword) !== Boolean(toggled?.has(keyword));
}

/** The keywords left ticked. */
export function ticked(item: AircraftReviewItemDto, toggled: Set<string> | undefined): string[] {
  return item.offered.filter((k) => isTicked(item, k, toggled));
}

/** The neighbours left ticked: alike ones at first; clicks turn over.
 * `null` when they were not looked for (the photo was not on screen): the
 * app then gives the keywords to the alike ones. */
export function tickedNeighbours(
  item: AircraftReviewItemDto,
  toggled: Set<string> | undefined,
): string[] | null {
  if (!item.neighbours) return null;
  return item.neighbours.filter((n) => n.alike !== Boolean(toggled?.has(n.id))).map((n) => n.id);
}

/** The photos on screen: the first `shown` of those listed. */
export function shownIds(review: AircraftReviewDto | null, all: boolean, shown: number): string[] {
  return (review?.items ?? [])
    .filter((i) => all || i.review === "waiting")
    .slice(0, shown)
    .map((i) => i.id);
}

/** What the lookup said, in a line ("" when it said nothing). */
export function lookupLine(item: AircraftReviewItemDto): string {
  return item.lookups
    .map((l) =>
      l.known
        ? `${l.registration}: ${[l.aircraftType, l.airline ?? l.owner, l.country]
            .filter(Boolean)
            .join(", ")}`
        : `adsbdb does not know ${l.registration}`,
    )
    .join("; ");
}

/** Possible marks below the window's confidence, for "also read". */
export function alsoRead(item: AircraftReviewItemDto): { label: string; pct: number }[] {
  const offered = new Set(item.offered.map((k) => parts(k)[1]));
  const out: { label: string; pct: number }[] = [];
  for (const [kind, list] of [
    ["Registration", item.registrations],
    ["Airline", item.airlines],
    ["Type", item.types],
  ] as const) {
    for (const m of list)
      if (!offered.has(m.name)) out.push({ label: `${kind} ${m.name}`, pct: m.pct });
  }
  return out;
}

/** "+3 s", "−6 s". */
export function secondsLabel(s: number): string {
  const r = Math.round(s);
  return r < 0 ? `−${-r} s` : `+${r} s`;
}

/** The window's line on how reading is getting on. */
export function progressLine(r: AircraftReviewDto, stopped: boolean): string {
  if (r.total === 0) return "Nothing to read.";
  const of = `${fmt.count(r.read)} of ${fmt.count(r.total)}`;
  if (r.pending > 0 && !stopped) return `Reading… ${of} read.`;
  const failed = r.failed
    ? ` ${fmt.plural(r.failed, "photo")} could not be read${r.problem ? `: ${r.problem}` : ""}.`
    : "";
  return (stopped && r.pending > 0 ? `Stopped: ${of} read.` : `${of} photos read.`) + failed;
}

/** How many are waiting for review. */
export function waitingCount(r: AircraftReviewDto): number {
  return r.items.filter((i) => i.review === "waiting").length;
}
