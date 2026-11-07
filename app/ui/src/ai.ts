// Helpers for the on-device AI screens (ADR-035).

import type { AiStatus } from "./api";
import type { DateRange } from "./uiModel";
import * as fmt from "./format";

/** One line on what the AI is doing. */
export function aiProgress(st: AiStatus): string {
  if (!st.enabled) return "Off.";
  if (st.problem) return `Not running: ${st.problem}`;
  if (!st.accel) return "Starting…";
  const done = `Ready, on the ${st.accel}. ${fmt.count(st.analysed)} ${st.analysed === 1 ? "item" : "items"} analysed so far`;
  return st.pending > 0 ? `${done}; ${fmt.count(st.pending)} waiting.` : `${done}.`;
}

/** "Sat 4 July 2026" for `2026-07-04`. */
export function dayTitle(date: string): string {
  const [y, m, d] = date.split("-").map(Number);
  const wd = new Date(Date.UTC(y, m - 1, d)).getUTCDay();
  const names = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
  return `${names[wd]} ${d} ${fmt.monthName(m)} ${y}`;
}

/** What a member's badge says. */
export function memberBadge(m: { choice: string | null; id: string }, keeper: string): string {
  if (m.choice === "not_kept") return "Not kept";
  if (m.choice === "keeper" || (m.choice === null && m.id === keeper))
    return m.choice === "keeper" ? "Kept" : "Suggested";
  return "";
}

const two = (n: number) => String(n).padStart(2, "0");

/** The dates of a calendar year, month or day (`null` for the whole calendar). */
export function calRange(
  year: number | null,
  month: number | null,
  day: number | null,
): DateRange | null {
  if (year === null) return null;
  if (month === null) return { from: `${year}-01-01`, to: `${year}-12-31`, title: String(year) };
  const ym = `${year}-${two(month)}`;
  if (day === null)
    return { from: `${ym}-01`, to: `${ym}-31`, title: `${fmt.monthName(month)} ${year}` };
  const d = `${ym}-${two(day)}`;
  return { from: d, to: d, title: dayTitle(d) };
}

/** How far Find duplicates (`duplicates`) or Tag photos (`tags`) has got. */
export function rangeProgress(
  st: { items: number; analysed: number; pending: number },
  what: "duplicates" | "tags" = "duplicates",
): string {
  const verb = what === "tags" ? "Tagging" : "Comparing";
  if (st.items === 0)
    return what === "tags" ? "No photos on these dates." : "No RAW photos on these dates.";
  if (st.pending > 0) return `${verb}: ${fmt.count(st.analysed)} of ${fmt.count(st.items)} done…`;
  const n = `${fmt.count(st.analysed)} of ${fmt.count(st.items)}`;
  return what === "tags" ? `${n} tagged.` : `${n} RAW photos compared.`;
}
