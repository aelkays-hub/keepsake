// A report table's column heading that sorts by it (see tableSort.ts).

import { ariaSort, nextSort, type TableSort } from "../tableSort";

export function SortHeader(p: {
  label: string;
  col: string;
  /** Numbers sort most first on the first click; text A–Z. */
  numeric?: boolean;
  sort: TableSort;
  onSort: (s: TableSort) => void;
  testId: string;
}) {
  const active = p.sort?.col === p.col;
  return (
    <th scope="col" className={p.numeric ? "num" : undefined} aria-sort={ariaSort(p.sort, p.col)}>
      <button
        className={`sort-head${active ? " active" : ""}`}
        onClick={() => p.onSort(nextSort(p.sort, p.col, Boolean(p.numeric)))}
        title={`Sort by ${p.label.toLowerCase()}`}
        data-testid={p.testId}
      >
        {p.label}
        <span className="sort-arrow" aria-hidden>
          {active ? (p.sort?.desc ? "▼" : "▲") : ""}
        </span>
      </button>
    </th>
  );
}
