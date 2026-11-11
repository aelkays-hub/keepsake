// Search bar and filters (date range, camera, lens, type, rating, words),
// Select mode, view switch and sort. Typing in the search bar also
// suggests tags: picking one adds it as a chip, and the items shown have
// every chip's tag (or one below it). Backspace in an empty box removes
// the last chip.

import { useEffect, useMemo, useState } from "react";

import { api, emptyQuery, type Facets, type Query } from "../api";
import type { KindFilter } from "../bindings/KindFilter";
import type { SortKey } from "../bindings/SortKey";
import * as fmt from "../format";
import * as sel from "../selection";
import { useUi, type Mode } from "../state";
import { suggestTags, tagName } from "../tagSearch";
import { setThumbSize, THUMB_SIZES, useThumbSize } from "../thumbSize";

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
  const { s, dispatch, refreshes, view } = useUi();
  const thumbSize = useThumbSize();
  const [text, setText] = useState(s.filters.text ?? "");
  const [facets, setFacets] = useState<Facets>({ cameras: [], lenses: [], tags: [] });
  // Cameras and lenses of the whole library: they change only with imports
  // and edits, not with each new view (fetching them per view held up the
  // next search behind a 200 ms query at 300,000 items).
  useEffect(() => {
    api
      .facets()
      .then(setFacets)
      .catch(() => undefined);
  }, [refreshes, s.revision]);
  // Words cleared from elsewhere (a report row shows its items unfiltered).
  useEffect(() => {
    if (!s.filters.text) setText((t) => (t ? "" : t));
  }, [s.filters.text]);
  // Words are applied after a short pause in typing.
  useEffect(() => {
    const t = setTimeout(() => {
      if ((s.filters.text ?? "") !== text)
        dispatch({ t: "filters", filters: { ...s.filters, text: text || null } });
    }, 80);
    return () => clearTimeout(t);
  }, [text, s.filters, dispatch]);
  const set = (patch: Partial<Query>) =>
    dispatch({ t: "filters", filters: { ...s.filters, ...patch } });
  // Tag chips in the search bar.
  const chips = useMemo(() => s.filters.tags ?? [], [s.filters.tags]);
  const [open, setOpen] = useState(false);
  const [hi, setHi] = useState(-1);
  const suggestions = useMemo(
    () => suggestTags(facets.tags, text, chips),
    [facets.tags, text, chips],
  );
  const showing = open && suggestions.length > 0;
  useEffect(() => setHi(-1), [text]);
  const setChips = (tags: string[]) => set({ tags: tags.length ? tags : undefined });
  const addChip = (tag: string) => {
    // The words typed were to find the tag: they go.
    setText("");
    dispatch({ t: "filters", filters: { ...s.filters, text: null, tags: [...chips, tag] } });
    setHi(-1);
  };
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
      <div className="search-box" data-testid="search-box">
        {chips.map((t) => (
          <span key={t} className="search-chip" data-testid="search-chip">
            {tagName(t)}
            <button
              aria-label={`Remove tag ${tagName(t)} from the search`}
              onClick={() => setChips(chips.filter((c) => c !== t))}
            >
              ✕
            </button>
          </span>
        ))}
        <input
          id="search"
          type="search"
          role="combobox"
          aria-expanded={showing}
          aria-controls="search-tags"
          aria-autocomplete="list"
          aria-activedescendant={showing && hi >= 0 ? `search-tag-${hi}` : undefined}
          placeholder={
            chips.length ? "More words or tags…" : "Search names, comments, keywords, tags…"
          }
          aria-label="Search"
          value={text}
          onChange={(e) => {
            setText(e.target.value);
            setOpen(true);
          }}
          onFocus={() => setOpen(true)}
          onBlur={() => setOpen(false)}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === "Escape") {
              if (showing) setOpen(false);
              else setText("");
            } else if (e.key === "ArrowDown" && suggestions.length > 0) {
              e.preventDefault();
              setOpen(true);
              setHi((h) => (h + 1) % suggestions.length);
            } else if (e.key === "ArrowUp" && suggestions.length > 0) {
              e.preventDefault();
              setHi((h) => (h <= 0 ? suggestions.length - 1 : h - 1));
            } else if (e.key === "Enter" && showing && hi >= 0) {
              e.preventDefault();
              addChip(suggestions[hi].value);
            } else if (e.key === "Backspace" && text === "" && chips.length > 0) {
              setChips(chips.slice(0, -1));
            }
          }}
          data-testid="search"
        />
        {showing && (
          <ul id="search-tags" role="listbox" aria-label="Tags" className="search-tags">
            {suggestions.map((t, i) => (
              <li
                key={t.value}
                id={`search-tag-${i}`}
                role="option"
                aria-selected={i === hi}
                // Keep the focus in the box (a click would blur it first).
                onMouseDown={(e) => e.preventDefault()}
                onClick={() => addChip(t.value)}
                data-testid="search-tag"
              >
                <span className="tag-icon" aria-hidden={true}>
                  #
                </span>
                {tagName(t.value)} <span className="muted">({fmt.count(t.count)})</span>
              </li>
            ))}
          </ul>
        )}
      </div>
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
        aria-label="Tag"
        title="Show only items with this keyword or AI tag"
        value={s.filters.tag ?? ""}
        onChange={(e) => set({ tag: e.target.value || undefined })}
        data-testid="tag"
      >
        <option value="">Any tag</option>
        {s.filters.tag && !facets.tags.some((t) => t.value === s.filters.tag) && (
          <option value={s.filters.tag}>{s.filters.tag.split("|").join(" › ")}</option>
        )}
        {facets.tags.map((t) => (
          <option key={t.value} value={t.value}>
            {t.value.split("|").join(" › ")} ({fmt.count(t.count)})
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
      <label
        className="filter"
        title="Leave out items you marked “not kept” on the Duplicates screen"
      >
        <input
          type="checkbox"
          checked={!!s.filters.hideNotKept}
          onChange={(e) => set({ hideNotKept: e.target.checked || undefined })}
          data-testid="hide-not-kept"
        />
        Hide not kept
      </label>
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
      {s.mode === "grid" && (
        <div className="filter thumb-size-box">
          <span id="thumb-size-label">Thumbnails</span>
          <div
            className="segmented thumb-size"
            role="group"
            aria-labelledby="thumb-size-label"
            data-testid="thumb-size"
          >
            {THUMB_SIZES.map((t) => (
              <button
                key={t.size}
                aria-pressed={thumbSize === t.size}
                onClick={() => setThumbSize(t.size)}
                title={`${t.label} thumbnails (+ and − in the grid)`}
                aria-label={`${t.label} thumbnails`}
                data-testid={`thumb-${t.size}`}
              >
                <span aria-hidden="true" className={`thumb-icon ${t.size}`} />
                {t.label}
              </button>
            ))}
          </div>
        </div>
      )}
      {s.mode !== "calendar" && (
        <div className="filter select-box" role="group" aria-label="Selection">
          <button
            aria-pressed={s.selecting}
            onClick={() => dispatch({ t: "selecting", on: !s.selecting })}
            title="Select mode: a click adds or removes an item, without Ctrl (Esc to leave)"
            data-testid="select-mode"
          >
            {s.selecting ? "✓ Selecting" : "Select"}
          </button>
          {s.selecting && (
            <>
              <button
                onClick={() => dispatch({ t: "sel", sel: sel.all(Number(view?.count ?? 0)) })}
                title="Select everything here (Ctrl+A)"
                data-testid="select-all"
              >
                All
              </button>
              <button
                onClick={() => dispatch({ t: "sel", sel: sel.emptySel })}
                title="Select nothing"
                data-testid="select-none"
              >
                None
              </button>
            </>
          )}
        </div>
      )}
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
