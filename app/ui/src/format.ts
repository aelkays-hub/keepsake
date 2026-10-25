// Formatting for display. Pure functions (unit-tested).

export function bytes(n: number | null | undefined): string {
  if (n === null || n === undefined) return "—";
  if (n >= 1e12) return `${(n / 1e12).toFixed(2)} TB`;
  if (n >= 1e9) return `${(n / 1e9).toFixed(1)} GB`;
  if (n >= 1e6) return `${(n / 1e6).toFixed(1)} MB`;
  if (n >= 1e3) return `${Math.round(n / 1e3)} KB`;
  return `${n} B`;
}

export function duration(ms: number | null | undefined): string {
  if (ms === null || ms === undefined) return "—";
  const s = Math.round(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  const two = (v: number) => String(v).padStart(2, "0");
  return h > 0 ? `${h}:${two(m)}:${two(sec)}` : `${m}:${two(sec)}`;
}

/** `2024-06-15T14:02:11` → `2024-06-15 14:02`. */
export function dateTime(s: string | null | undefined): string {
  if (!s) return "—";
  return s.slice(0, 16).replace("T", " ");
}

const MONTHS = [
  "January",
  "February",
  "March",
  "April",
  "May",
  "June",
  "July",
  "August",
  "September",
  "October",
  "November",
  "December",
];

export function monthName(m: number): string {
  return MONTHS[m - 1] ?? String(m);
}

export function count(n: number): string {
  return n.toLocaleString("en-US");
}

export function plural(n: number, one: string, many = `${one}s`): string {
  return `${count(n)} ${n === 1 ? one : many}`;
}

/** Days in a month (month 1–12). */
export function daysIn(year: number, month: number): number {
  return new Date(year, month, 0).getDate();
}

/** Weekday of the first day, Monday = 0. */
export function firstWeekday(year: number, month: number): number {
  return (new Date(year, month - 1, 1).getDay() + 6) % 7;
}
