// The aircraft's age (ADR-044): worked out when it is shown, from when it
// was built (kept per registration, or the photo's `Aircraft|Built|…`
// keyword), so it goes up by itself each year. Never stored. Pure
// functions (unit-tested).

/** A whole number of years; `about` when only the build year is known, so
 * the age may be one less. */
export interface Age {
  years: number;
  about: boolean;
}

interface Day {
  y: number;
  m: number;
  d: number;
}

/** `2014`, `2014-03-05` or `2014-03-05T…` → its parts (`m`, `d` 0 when
 * only the year is given). */
function day(s: string): (Day & { yearOnly: boolean }) | null {
  const m = /^(\d{4})(?:-(\d{2})-(\d{2}))?/.exec(s.trim());
  if (!m) return null;
  return {
    y: Number(m[1]),
    m: m[2] ? Number(m[2]) : 0,
    d: m[3] ? Number(m[3]) : 0,
    yearOnly: !m[2],
  };
}

/** Today, on this computer. */
export function today(now: Date = new Date()): string {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${now.getFullYear()}-${p(now.getMonth() + 1)}-${p(now.getDate())}`;
}

/** How old an aircraft `built` (`2014` or `2014-03-05`) was `on` a day
 * (`YYYY-MM-DD…`). With the date known: whole years, a birthday not yet
 * reached not counted. With only the year: the years between, "about".
 * `null` when either is unknown or `on` is before it was built. */
export function ageOn(built: string | null, on: string | null): Age | null {
  if (!built || !on) return null;
  const b = day(built);
  const o = day(on);
  if (!b || !o || o.yearOnly) return null;
  let years = o.y - b.y;
  if (!b.yearOnly && (o.m < b.m || (o.m === b.m && o.d < b.d))) years -= 1;
  if (years < 0) return null;
  return { years, about: b.yearOnly };
}

/** `about 12 years`, `1 year`, `under a year`. */
export function ageLabel(a: Age): string {
  const n = a.years === 0 ? "under a year" : a.years === 1 ? "1 year" : `${a.years} years`;
  return a.about && a.years > 0 ? `about ${n}` : n;
}

/** `2014` or `5 March 2014`. */
export function builtLabel(built: string): string {
  const b = day(built);
  if (!b) return built;
  if (b.yearOnly) return String(b.y);
  const month = new Date(Date.UTC(b.y, b.m - 1, 1)).toLocaleString("en-GB", {
    month: "long",
    timeZone: "UTC",
  });
  return `${b.d} ${month} ${b.y}`;
}

/** The registrations a photo's keywords name (`Aircraft|Registration|…`). */
export function registrationsOf(keywords: string[]): string[] {
  return keywords
    .filter((k) => k.startsWith("Aircraft|Registration|"))
    .map((k) => k.slice("Aircraft|Registration|".length));
}

/** The build year a photo's keywords say (`Aircraft|Built|2014`). */
export function builtKeyword(keywords: string[]): string | null {
  const k = keywords.find((x) => /^Aircraft\|Built\|\d{4}$/.test(x));
  return k ? k.slice("Aircraft|Built|".length) : null;
}

/** Where the build date came from, for people. */
export function builtSource(source: string | null): string {
  if (source === "opensky") return "from OpenSky";
  if (source === "typed") return "typed by you";
  return "from the photo's keyword";
}

/** What the Inspector and the viewer show for a photo with a registration:
 * the age today, and in the photo (when it is dated). */
export interface AgeFacts {
  /** `12 years (built 2014)`. */
  today: string | null;
  /** `about 9 years (built 2014, photographed 2023)`. */
  inPhoto: string | null;
}

export function ageFacts(built: string | null, captured: string | null, now: string): AgeFacts {
  if (!built) return { today: null, inPhoto: null };
  const b = builtLabel(built);
  const t = ageOn(built, now);
  const p = ageOn(built, captured);
  const shot = captured ? day(captured) : null;
  return {
    today: t ? `${ageLabel(t)} (built ${b})` : null,
    inPhoto: p && shot ? `${ageLabel(p)} (built ${b}, photographed ${shot.y})` : null,
  };
}
