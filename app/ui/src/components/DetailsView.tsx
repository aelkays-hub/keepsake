// The details table: virtualized rows, sortable columns.

import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useRef } from "react";

import type { SortSpec } from "../api";
import type { SortKey } from "../bindings/SortKey";
import { useListKeys } from "../edits";
import * as fmt from "../format";
import * as sel from "../selection";
import { useUi } from "../state";
import { useRows } from "../useRows";
import { labelColor } from "../labels";
import { Stars } from "./Stars";

const ROW = 30;

const COLUMNS: { key: SortKey; title: string; width: string }[] = [
  { key: "name", title: "Name", width: "minmax(160px, 2fr)" },
  { key: "date", title: "Date", width: "150px" },
  { key: "camera", title: "Camera", width: "minmax(90px, 1fr)" },
  { key: "lens", title: "Lens", width: "minmax(110px, 1.4fr)" },
  { key: "type", title: "Type", width: "70px" },
  { key: "size", title: "Size", width: "86px" },
  { key: "duration", title: "Duration", width: "80px" },
  { key: "rating", title: "Rating", width: "86px" },
];
const TEMPLATE = COLUMNS.map((c) => c.width).join(" ");

export function DetailsView({ onOpen }: { onOpen: (i: number) => void }) {
  const { s, dispatch, view } = useUi();
  const rows = useRows(view, s.revision);
  const parent = useRef<HTMLDivElement>(null);
  const v = useVirtualizer({
    count: rows.count,
    getScrollElement: () => parent.current,
    estimateSize: () => ROW,
    overscan: 10,
  });
  const items = v.getVirtualItems();
  const first = items[0]?.index ?? 0;
  const last = items[items.length - 1]?.index ?? 0;
  useEffect(() => {
    rows.want(first, Math.min(rows.count, last + 1));
  });
  useEffect(() => {
    if (s.sel.focus !== null) v.scrollToIndex(s.sel.focus, { align: "auto" });
  }, [s.sel.focus, v]);
  const pageRows = Math.max(1, Math.floor((parent.current?.clientHeight ?? 600) / ROW) - 1);
  const onKey = useListKeys(rows.count, 1, pageRows, onOpen);
  const sortBy = (key: SortKey) => {
    const next: SortSpec =
      s.sort.field === key
        ? { field: key, descending: !s.sort.descending }
        : { field: key, descending: key === "date" || key === "rating" || key === "size" };
    dispatch({ t: "sort", sort: next });
  };
  const pick = (i: number, e: React.MouseEvent) => {
    parent.current?.focus();
    if (e.shiftKey) dispatch({ t: "sel", sel: sel.extend(s.sel, i) });
    else if (e.ctrlKey || e.metaKey) dispatch({ t: "sel", sel: sel.toggle(s.sel, i) });
    else dispatch({ t: "sel", sel: sel.only(i) });
  };
  return (
    <div className="details" role="table" aria-label="Items" aria-rowcount={rows.count}>
      <div className="details-head" role="row" style={{ gridTemplateColumns: TEMPLATE }}>
        {COLUMNS.map((c) => {
          const active = s.sort.field === c.key;
          return (
            <button
              key={c.key}
              role="columnheader"
              aria-sort={active ? (s.sort.descending ? "descending" : "ascending") : "none"}
              className={active ? "active" : ""}
              onClick={() => sortBy(c.key)}
              data-testid={`sort-${c.key}`}
            >
              {c.title} {active ? (s.sort.descending ? "▼" : "▲") : ""}
            </button>
          );
        })}
      </div>
      <div
        ref={parent}
        className="details-scroll"
        tabIndex={0}
        data-testid="details"
        data-count={rows.count}
        onKeyDown={onKey}
      >
        <div style={{ height: v.getTotalSize(), position: "relative" }}>
          {items.map((vr) => {
            const r = rows.get(vr.index);
            const i = vr.index;
            return (
              <div
                key={vr.key}
                role="row"
                aria-selected={sel.has(s.sel, i)}
                data-index={i}
                data-testid="drow"
                className={`drow${sel.has(s.sel, i) ? " selected" : ""}${s.sel.focus === i ? " focused" : ""}${r && !r.online ? " offline" : ""}`}
                style={{
                  transform: `translateY(${vr.start}px)`,
                  height: ROW,
                  gridTemplateColumns: TEMPLATE,
                }}
                onMouseDown={(e) => pick(i, e)}
                onDoubleClick={() => onOpen(i)}
              >
                <span role="cell" className="name">
                  {r?.label && <span className="dot" style={{ background: labelColor(r.label) }} />}
                  {r?.name ?? "…"}
                  {r && !r.online && <span className="badge off">offline</span>}
                </span>
                <span role="cell">{fmt.dateTime(r?.captured)}</span>
                <span role="cell">{r?.camera ?? "—"}</span>
                <span role="cell">{r?.lens ?? "—"}</span>
                <span role="cell">
                  {r ? { photo: "Photo", video: "Video", raw: "RAW" }[r.kind] : ""}
                </span>
                <span role="cell" className="num">
                  {fmt.bytes(r?.size)}
                </span>
                <span role="cell" className="num">
                  {r?.kind === "video" ? fmt.duration(r.durationMs) : "—"}
                </span>
                <span role="cell">
                  <Stars value={r?.rating ?? 0} size="sm" />
                </span>
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
