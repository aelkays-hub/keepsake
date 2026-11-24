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

/** How surely a photo's offered marks were read: the mean of their
 * percentages (marks from the look or the lookup alone have none). */
export function confidenceOf(item: AircraftReviewItemDto): number {
  const pcts = item.offered.map((k) => pctOf(item, k)).filter((p): p is number => p !== null);
  return pcts.length ? pcts.reduce((a, b) => a + b, 0) / pcts.length : 0;
}

const REVIEW_ORDER: Record<string, number> = { waiting: 0, accepted: 1, dismissed: 2 };

/** The order the window lists photos in: waiting for review first, the
 * ones offering more marks, then read more surely, first; then those
 * accepted; dismissed ones last. Otherwise as read (newest first). */
export function ordered(items: AircraftReviewItemDto[]): AircraftReviewItemDto[] {
  return items
    .map((item, i) => ({ item, i, conf: confidenceOf(item) }))
    .sort(
      (a, b) =>
        (REVIEW_ORDER[a.item.review] ?? 1) - (REVIEW_ORDER[b.item.review] ?? 1) ||
        b.item.offered.length - a.item.offered.length ||
        b.conf - a.conf ||
        a.i - b.i,
    )
    .map((x) => x.item);
}

/** Who knows an aircraft a registration was read one letter off. */
function knownBy(by: string): string {
  if (by === "register") return "the register";
  return by === "flightradar24" ? "Flightradar24" : "adsbdb";
}

/** Where an offered keyword comes from, for its label. */
export function sourceOf(item: AircraftReviewItemDto, keyword: string): string {
  const [kind, name] = parts(keyword);
  if (kind === "Registration") {
    // Read one look-alike letter off a known aircraft (ADR-051).
    const fixed = item.near.find((n) => n.real === name);
    if (fixed) {
      const read = item.registrations.find((m) => m.name === fixed.read);
      return `(read ${fixed.read}${read ? ` ${read.pct}%` : ""}; ${knownBy(fixed.by)} knows ${fixed.real})`;
    }
    if (item.near.some((n) => n.read === name)) {
      const read = item.registrations.find((m) => m.name === name);
      return `(${read ? `${read.pct}%, ` : ""}as read)`;
    }
  }
  const pct = pctOf(item, keyword);
  // The build year comes from OpenSky's database on this computer (ADR-044).
  if (item.fromLookup.includes(keyword) && parts(keyword)[0] === "Built") return "(OpenSky)";
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
    .filter((i) => all || toReview(i))
    .slice(0, shown)
    .map((i) => i.id);
}

/** What the lookup said, in a line ("" when it said nothing). */
export function lookupLine(item: AircraftReviewItemDto): string {
  return item.lookups
    .map((l) =>
      l.known
        ? `${l.registration}: ${[
            l.aircraftType,
            l.airline ?? l.owner,
            l.country,
            l.built && `built ${l.built.slice(0, 4)}`,
          ]
            .filter(Boolean)
            .join(", ")}`
        : `adsbdb does not know ${l.registration}`,
    )
    .join("; ");
}

/** What the photos of its shot read, in a line ("" with no shot, or when
 * they read no registration): "Registration read on 3 of 21 photos of
 * this shot: JA02NJ". */
export function shotLine(item: AircraftReviewItemDto): string {
  if (item.shotPhotos < 2 || item.sameShot.length === 0) return "";
  return `Registration read on ${item.sameShot
    .map((m) => `${m.photos} of ${item.shotPhotos} photos (${m.name})`)
    .join(", ")} of this shot.`;
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
  // A fin means something only with its airline (read on it or its shot).
  if (item.airlines.length > 0)
    for (const m of item.fins) out.push({ label: `Fin ${m.name}`, pct: m.pct });
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
  // Stopped with photos left unread (once stopped, none is pending).
  const unread = r.pending > 0 || r.read + r.failed < r.total;
  return (stopped && unread ? `Stopped: ${of} read.` : `${of} photos read.`) + failed;
}

/** Waiting for review with something to accept at the window's levels:
 * the photos the window lists. */
export function toReview(i: AircraftReviewItemDto): boolean {
  return i.review === "waiting" && i.offered.length > 0;
}

/** How many are waiting for review with something to accept. */
export function waitingCount(r: AircraftReviewDto): number {
  return r.items.filter(toReview).length;
}

/** Read, with no registration, airline or type found at any confidence. */
export function nothingFound(i: AircraftReviewItemDto): boolean {
  return i.registrations.length + i.airlines.length + i.types.length + i.offered.length === 0;
}

/** The window's line when it lists no photo although some were read: why. */
export function noneWaitingLine(r: AircraftReviewDto): string {
  if (belowLevels(r) > 0) return "Nothing more to accept at these levels.";
  const empty = r.items.filter(nothingFound).length;
  const reviewed = r.items.length - empty;
  const none = `No registration, airline or type was found on ${
    empty === 1 && r.items.length === 1 ? "this photo" : fmt.plural(empty, "photo")
  }.`;
  if (empty === 0) return "Every photo read so far has been reviewed.";
  if (reviewed === 0) return none;
  return `${none} The ${reviewed === 1 ? "other has" : "others have"} been reviewed (tick "Show accepted and dismissed too" to see them).`;
}

/** How many wait with nothing at the window's levels (shown when a level
 * is lowered enough). */
export function belowLevels(r: AircraftReviewDto): number {
  return r.items.filter((i) => i.review === "waiting" && i.offered.length === 0).length;
}
