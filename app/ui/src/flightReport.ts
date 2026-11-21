// The flight report (Report → Flights): pure helpers, unit-tested.

import type { FlightReportRowDto } from "./bindings/FlightReportRowDto";

/** `Keepsake flights 2026-11-20.csv`. */
export function flightCsvName(now: Date): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `Keepsake flights ${now.getFullYear()}-${p(now.getMonth() + 1)}-${p(now.getDate())}.csv`;
}

/** `YYZ (Canada)`, `YYZ`, or nothing. */
export function flightPlace(code: string | null, country: string | null): string {
  if (!code) return "";
  return country ? `${code} (${country})` : code;
}

/** Whether a flight matches what is typed in Find (any of its words). */
export function flightMatches(f: FlightReportRowDto, find: string): boolean {
  const q = find.trim().toLowerCase();
  if (!q) return true;
  return [
    f.date,
    f.flight,
    f.from,
    f.fromCountry,
    f.to,
    f.toCountry,
    f.airline,
    f.registration,
    f.aircraftType,
  ].some((v) => v?.toLowerCase().includes(q));
}
