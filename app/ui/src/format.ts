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

/** A frame rate as a fraction (30000/1001 = 29.97). */
export interface Rate {
  num: number;
  den: number;
}

/** Frames dropped per minute in drop-frame timecode (2 at 29.97, 4 at 59.94). */
function dropFrames(nominal: number, drop: boolean): number {
  return drop ? Math.round(nominal / 15) : 0;
}

function tcToFrames(tc: string, nominal: number, drop: number): number {
  const p = tc.split(/[:;.]/).map(Number);
  if (p.length !== 4 || p.some((v) => !Number.isFinite(v))) return 0;
  const [h, m, s, f] = p;
  const minutes = h * 60 + m;
  return (h * 3600 + m * 60 + s) * nominal + f - drop * (minutes - Math.floor(minutes / 10));
}

function framesToTc(frames: number, nominal: number, drop: number): string {
  const day = (nominal * 86400 - drop * (1440 - 144)) | 0;
  let n = ((frames % day) + day) % day;
  if (drop) {
    const per10 = nominal * 600 - drop * 9;
    const perMin = nominal * 60 - drop;
    const d = Math.floor(n / per10);
    const r = n % per10;
    n += drop * 9 * d + (r > drop ? drop * Math.floor((r - drop) / perMin) : 0);
  }
  const f = n % nominal;
  const s = Math.floor(n / nominal) % 60;
  const m = Math.floor(n / (nominal * 60)) % 60;
  const h = Math.floor(n / (nominal * 3600)) % 24;
  const two = (v: number) => String(v).padStart(2, "0");
  return `${two(h)}:${two(m)}:${two(s)}${drop ? ";" : ":"}${two(f)}`;
}

/**
 * Timecode `seconds` into a clip whose first frame is `start`
 * (`HH:MM:SS:FF`; `;` before the frames means drop-frame). Without a start
 * timecode, counts from 00:00:00:00 (drop-frame at 29.97 and 59.94).
 */
export function timecode(start: string | null, rate: Rate, seconds: number): string {
  const fps = rate.num / Math.max(1, rate.den);
  const nominal = Math.max(1, Math.round(fps));
  const ntsc = rate.den === 1001 && (nominal === 30 || nominal === 60);
  const drop = dropFrames(nominal, start ? start.includes(";") && ntsc : ntsc);
  const first = start ? tcToFrames(start, nominal, drop) : 0;
  return framesToTc(first + frameAt(rate, seconds), nominal, drop);
}

/** The frame shown at `seconds` (frame n covers [n/fps, (n+1)/fps)). */
export function frameAt(rate: Rate, seconds: number): number {
  return Math.max(0, Math.floor((seconds * rate.num) / Math.max(1, rate.den) + 1e-3));
}

/** A time shift for people: `+1 h 30 min`, `−2 d 3 h 4 s`, `no change`. */
export function offset(sec: number): string {
  if (sec === 0) return "no change";
  let s = Math.abs(sec);
  const d = Math.floor(s / 86400);
  s %= 86400;
  const h = Math.floor(s / 3600);
  s %= 3600;
  const m = Math.floor(s / 60);
  s %= 60;
  const parts: string[] = [];
  if (d) parts.push(`${d} d`);
  if (h) parts.push(`${h} h`);
  if (m) parts.push(`${m} min`);
  if (s) parts.push(`${s} s`);
  return `${sec < 0 ? "−" : "+"}${parts.join(" ")}`;
}

/** Milliseconds since 1970 as a local date and time, `—` if none. */
export function when(ms: number | null | undefined): string {
  if (ms === null || ms === undefined) return "—";
  return new Date(ms).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}
