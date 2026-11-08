// Tags in the search bar: which tags to suggest for the words typed.

/** Most tags suggested at once. */
const MAX_SUGGESTIONS = 8;

/** `Places|Lake` as people read it: `Places › Lake`. */
export const tagName = (t: string) => t.split("|").join(" › ");

/** The tags whose name contains `text` (any case), not already `chosen`,
 * those starting with it first, then the most used. */
export function suggestTags(
  all: { value: string; count: number }[],
  text: string,
  chosen: string[],
): { value: string; count: number }[] {
  const want = text.trim().toLowerCase();
  if (!want) return [];
  const starts = (v: string) =>
    v
      .toLowerCase()
      .split("|")
      .some((part) => part.startsWith(want));
  return all
    .filter((t) => !chosen.includes(t.value) && tagName(t.value).toLowerCase().includes(want))
    .sort((a, b) => Number(starts(b.value)) - Number(starts(a.value)) || b.count - a.count)
    .slice(0, MAX_SUGGESTIONS);
}
