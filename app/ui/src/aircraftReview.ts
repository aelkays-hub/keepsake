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
 * the type guessed from the look. */
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

/** The keywords left ticked: those offered, less those the user unticked. */
export function ticked(item: AircraftReviewItemDto, unticked: Set<string> | undefined): string[] {
  return item.offered.filter((k) => !unticked?.has(k));
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
