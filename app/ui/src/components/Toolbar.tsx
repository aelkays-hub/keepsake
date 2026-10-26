// Search bar and filters (date range, camera, lens, type, rating, words),
// view switch and sort.

import { useEffect, useState } from "react";

import { api, emptyQuery, type Facets, type Query } from "../api";
import type { KindFilter } from "../bindings/KindFilter";
import type { SortKey } from "../bindings/SortKey";
import * as fmt from "../format";
import { useUi, type Mode } from "../state";

const SORTS: { key: SortKey; label: string }[] = [
  { key: "date", label: "Date" },
  { key: "name", label: "Name" },
  { key: "camera", label: "Camera" },
  { key: "lens", label: "Lens" },
  { key: "type", label: "Type" },
  { key: "size", label: "Size" },
  { key: "duration", label: "Duration" },
  { key: "rating", label: "Rating" },
];

export function Toolbar() {
  const { s, dispatch, refreshes } = useUi();
  const [text, setText] = useState(s.filters.text ?? "");
  const [facets, setFacets] = useState<Facets>({ cameras: [], lenses: [] });
  // Cameras and lenses of the whole library: they change only with imports
  // and edits, not with each new view (fetching them per view held up the
  // next search behind a 200 ms query at 300,000 items).
  useEffect(() => {
    api
      .facets()
      .then(setFacets)
      .catch(() => undefined);
  }, [refreshes, s.revision]);
  // Words are applied after a short pause in typing.
  useEffect(() => {
    const t = setTimeout(() => {
      if ((s.filters.text ?? "") !== text)
        dispatch({ t: "filters", filters: { ...s.filters, text: text || null } });
    }, 100);
    return () => clearTimeout(t);
  }, [text, s.filters, dispatch]);
  const set = (patch: Partial<Query>) =>
    dispatch({ t: "filters", filters: { ...s.filters, ...patch } });
  const active =
    JSON.stringify({ ...s.filters, text: null }) !== JSON.stringify({ ...emptyQuery }) ||
    (s.filters.text ?? "") !== "";
  const modes: { mode: Mode; label: string; key: string }[] = [
    { mode: "calendar", label: "Calendar", key: "1" },
    { mode: "grid", label: "Grid", key: "2" },
    { mode: "details", label: "Details", key: "3" },
  ];
  return (
    <header className="toolbar" role="search">
      <input
        id="search"
        type="search"
        placeholder="Search names, comments, keywords…"
        aria-label="Search"
        value={text}
        onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Escape") setText("");
          e.stopPropagation();
        }}
        data-testid="search"
      />
      <label className="filter">
        From
        <input
          type="date"
          value={s.filters.from ?? ""}
          onChange={(e) => set({ from: e.target.value || null })}
          data-testid="from"
        />
      </label>
      <label className="filter">
        To
        <input
          type="date"
          value={s.filters.to ?? ""}
          onChange={(e) => set({ to: e.target.value || null })}
          data-testid="to"
        />
      </label>
      <select
        aria-label="Camera"
        value={s.filters.camera ?? ""}
        onChange={(e) => set({ camera: e.target.value || null })}
        data-testid="camera"
      >
        <option value="">Any camera</option>
        {facets.cameras.map((c) => (
          <option key={c.value} value={c.value}>
            {c.value} ({fmt.count(c.count)})
          </option>
        ))}
      </select>
      <select
        aria-label="Lens"
        value={s.filters.lens ?? ""}
        onChange={(e) => set({ lens: e.target.value || null })}
        data-testid="lens"
      >
        <option value="">Any lens</option>
        {facets.lenses.map((c) => (
          <option key={c.value} value={c.value}>
            {c.value} ({fmt.count(c.count)})
          </option>
        ))}
      </select>
      <select
        aria-label="Type"
        value={s.filters.kind ?? ""}
        onChange={(e) => set({ kind: (e.target.value || null) as KindFilter | null })}
        data-testid="type"
      >
        <option value="">Any type</option>
        <option value="photo">Photos</option>
        <option value="raw">RAW</option>
        <option value="video">Videos</option>
      </select>
      <select
        aria-label="Minimum rating"
        value={s.filters.minRating ?? 0}
        onChange={(e) => set({ minRating: Number(e.target.value) || null })}
        data-testid="rating"
      >
        <option value={0}>Any rating</option>
        {[1, 2, 3, 4, 5].map((r) => (
          <option key={r} value={r}>
            {"★".repeat(r)} or more
          </option>
        ))}
      </select>
      {active && (
        <button
          className="clear"
          onClick={() => {
            setText("");
            dispatch({ t: "filters", filters: emptyQuery });
          }}
          aria-label="Clear filters"
        >
          ✕ Clear
        </button>
      )}
      <span className="spacer" />
      <div className="segmented" role="tablist" aria-label="View">
        {modes.map((m) => (
          <button
            key={m.mode}
            role="tab"
            aria-selected={s.mode === m.mode}
            onClick={() => dispatch({ t: "mode", mode: m.mode })}
            title={`${m.label} (Ctrl+${m.key})`}
            data-testid={`mode-${m.mode}`}
          >
            {m.label}
          </button>
        ))}
      </div>
      {s.mode !== "calendar" && (
        <label className="filter">
          Sort
          <select
            value={s.sort.field}
            onChange={(e) =>
              dispatch({ t: "sort", sort: { ...s.sort, field: e.target.value as SortKey } })
            }
            data-testid="sort"
          >
            {SORTS.map((o) => (
              <option key={o.key} value={o.key}>
                {o.label}
              </option>
            ))}
          </select>
          <button
            aria-label={s.sort.descending ? "Descending" : "Ascending"}
            onClick={() =>
              dispatch({ t: "sort", sort: { ...s.sort, descending: !s.sort.descending } })
            }
            data-testid="sort-dir"
          >
            {s.sort.descending ? "↓" : "↑"}
          </button>
        </label>
      )}
    </header>
  );
}
