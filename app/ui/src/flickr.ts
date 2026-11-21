// Upload to Flickr (ADR-046): who may see the photos, and the tags each
// goes with as the user adjusts them in the window. Pure helpers, tested
// in src/unit.

/** Who may see the uploaded photos, as Flickr offers it. */
export const PRIVACY = [
  { id: "public", label: "Public", hint: "Anyone can see them" },
  { id: "friends_family", label: "Friends and family", hint: "Your Flickr friends and family" },
  { id: "friends", label: "Friends", hint: "Your Flickr friends only" },
  { id: "family", label: "Family", hint: "Your Flickr family only" },
  { id: "private", label: "Private", hint: "Only you" },
] as const;

export type PrivacyId = (typeof PRIVACY)[number]["id"];

/** Private until the user chooses otherwise. */
export const DEFAULT_PRIVACY: PrivacyId = "private";

/** Flickr takes at most 75 tags per photo. */
export const MAX_TAGS = 75;

export function privacyLabel(id: string): string {
  return PRIVACY.find((p) => p.id === id)?.label ?? id;
}

/** A tag as Flickr can take it: trimmed, spaces collapsed, no `"`. */
export function cleanTag(t: string): string {
  return t.replace(/"/g, "").trim().split(/\s+/).filter(Boolean).join(" ");
}

/** Tags typed in a box: separated by commas (a tag may have spaces). */
export function parseTags(input: string): string[] {
  return input.split(",").map(cleanTag).filter(Boolean);
}

/** Each tag once (ignoring case), at most {@link MAX_TAGS}. */
export function dedupe(tags: string[]): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const raw of tags) {
    const t = cleanTag(raw);
    const k = t.toLowerCase();
    if (!t || seen.has(k)) continue;
    seen.add(k);
    out.push(t);
    if (out.length === MAX_TAGS) break;
  }
  return out;
}

/** What the user changed about one photo's tags. */
export interface TagEdits {
  /** Typed in, for this photo or for all. */
  added: string[];
  /** Removed, lower case. */
  removed: string[];
}

export const noEdits: TagEdits = { added: [], removed: [] };

/** A photo's tags: those suggested, less the removed, plus the added. */
export function finalTags(suggested: string[], e: TagEdits): string[] {
  const gone = new Set(e.removed);
  return dedupe([...suggested, ...e.added].filter((t) => !gone.has(cleanTag(t).toLowerCase())));
}

export function addTags(e: TagEdits, tags: string[]): TagEdits {
  const keys = new Set(tags.map((t) => cleanTag(t).toLowerCase()));
  return {
    added: dedupe([...e.added, ...tags]),
    removed: e.removed.filter((r) => !keys.has(r)),
  };
}

export function removeTag(e: TagEdits, tag: string): TagEdits {
  const k = cleanTag(tag).toLowerCase();
  return {
    added: e.added.filter((t) => cleanTag(t).toLowerCase() !== k),
    removed: e.removed.includes(k) ? e.removed : [...e.removed, k],
  };
}

/** A Flickr machine tag (`aircraft:registration=C-FEJC`). */
export function isMachineTag(t: string): boolean {
  return /^[a-z][a-z0-9_]*:[a-z][a-z0-9_]*=.+/.test(t);
}

/** The tree's keys open at first: the newest year, its newest month and
 * that month's newest day (`2024`, `2024-06`, `2024-06-15`). */
export function newestOpen(
  years: { year: number; months: { month: string; days: { date: string }[] }[] }[],
): string[] {
  const y = years[0];
  if (!y) return [];
  const m = y.months[0];
  const d = m?.days[0];
  return [String(y.year), ...(m ? [m.month] : []), ...(d ? [d.date] : [])];
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

/** `2024-06` → `June`. */
export function monthName(month: string): string {
  return MONTHS[Number(month.slice(5, 7)) - 1] ?? month;
}

/** `2024-06-15` → `15 June`. */
export function dayName(date: string): string {
  return `${Number(date.slice(8, 10))} ${monthName(date.slice(0, 7))}`;
}
