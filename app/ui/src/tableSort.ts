// Sortable report tables (Tools → Report): click a column's heading to sort
// by it, again to turn the order over. Pure functions (unit-tested).

/** A table's sort: which column, which way; `null` keeps the report's own
 * order (newest first for dates, most first for the others). */
export type TableSort = { col: string; desc: boolean } | null;

/** A cell's value for sorting: a number, a text, or nothing (sorted last). */
export type SortValue = number | string | null;

/** The sort after clicking `col`: a new column starts most first for
 * numbers (`numeric`) and A–Z for text; the same column turns over. */
export function nextSort(cur: TableSort, col: string, numeric: boolean): TableSort {
  if (cur?.col === col) return { col, desc: !cur.desc };
  return { col, desc: numeric };
}

const text = new Intl.Collator(undefined, { numeric: true, sensitivity: "base" });

/** `rows` sorted by `value` of the sorted column; empty values last either
 * way, rows that tie keep their order. `pinned` rows (an Unknown group)
 * stay at the end. Unsorted (`null`): as given. */
export function sortRows<T>(
  rows: T[],
  sort: TableSort,
  value: (row: T, col: string) => SortValue,
  pinned: (row: T) => boolean = () => false,
): T[] {
  if (!sort) return rows;
  const dir = sort.desc ? -1 : 1;
  const keyed = rows.map((row, i) => ({ row, i, v: value(row, sort.col), pin: pinned(row) }));
  keyed.sort((a, b) => {
    if (a.pin !== b.pin) return a.pin ? 1 : -1;
    const ae = a.v === null || a.v === "";
    const be = b.v === null || b.v === "";
    if (ae || be) return ae === be ? a.i - b.i : ae ? 1 : -1;
    const c =
      typeof a.v === "number" && typeof b.v === "number"
        ? a.v - b.v
        : text.compare(String(a.v), String(b.v));
    return c * dir || a.i - b.i;
  });
  return keyed.map((k) => k.row);
}

/** `aria-sort` for a column's heading. */
export function ariaSort(sort: TableSort, col: string): "ascending" | "descending" | "none" {
  if (sort?.col !== col) return "none";
  return sort.desc ? "descending" : "ascending";
}
