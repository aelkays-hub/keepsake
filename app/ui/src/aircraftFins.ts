// Fin numbers (ADR-052): the lines the Fin numbers block shows.

import type { FinDto } from "./bindings/FinDto";
import type { FinImportDto } from "./bindings/FinImportDto";
import * as fmt from "./format";

/** "2019-05-05 – 2024-05-05", "from 2019-01-01", "until …", or "any time". */
export function finDates(f: FinDto): string {
  if (f.from && f.to) return f.from === f.to ? f.from : `${f.from} – ${f.to}`;
  if (f.from) return `from ${f.from}`;
  if (f.to) return `until ${f.to}`;
  return "any time";
}

/** "Learned from 3 photos", "Imported". */
export function finSource(f: FinDto): string {
  return f.source === "learned" ? `Learned from ${fmt.plural(f.seen, "photo")}` : "Imported";
}

/** What an import did: "12 added, 3 updated. 2 lines not kept." */
export function importLine(r: FinImportDto): string {
  const kept =
    r.added + r.updated === 0 ? "Nothing imported." : `${r.added} added, ${r.updated} updated.`;
  const bad = r.problems.length
    ? ` ${fmt.plural(r.problems.length, "line")} not kept: ${r.problems.slice(0, 3).join("; ")}${r.problems.length > 3 ? "; …" : ""}`
    : "";
  return kept + bad;
}

/** What going over the library did. */
export function learnLine(n: number): string {
  return n === 0
    ? "No tagged photo has a fin read yet. Use Read again on photos read before fins were kept."
    : `${fmt.plural(n, "photo")} gave a fin.`;
}

/** The airlines in the table, each with how many fins. */
export function byAirline(fins: FinDto[]): { airline: string; fins: FinDto[] }[] {
  const out: { airline: string; fins: FinDto[] }[] = [];
  for (const f of fins) {
    const last = out[out.length - 1];
    if (last && last.airline === f.airline) last.fins.push(f);
    else out.push({ airline: f.airline, fins: [f] });
  }
  return out;
}
