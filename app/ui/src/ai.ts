// Helpers for the on-device AI screens (ADR-035).

import type { AiStatus } from "./api";
import * as fmt from "./format";

/** One line on what the AI is doing. */
export function aiProgress(st: AiStatus): string {
  if (!st.enabled) return "Off.";
  if (st.problem) return `Not running: ${st.problem}`;
  if (!st.accel) return "Starting…";
  const left = st.pending;
  const done = `${fmt.count(st.analysed)} of ${fmt.count(st.assets)} items analysed`;
  return left > 0 ? `${done}, ${fmt.count(left)} to go (on the ${st.accel}).` : `${done}.`;
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
