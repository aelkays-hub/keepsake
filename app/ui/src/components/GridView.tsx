// The thumbnail grid: virtualized rows of tiles, multi-select with mouse
// and keyboard, offline badges, ratings, labels and video durations.

import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { fileUrl, type Row } from "../api";
import { useListKeys } from "../edits";
import * as fmt from "../format";
import * as sel from "../selection";
import { useUi } from "../state";
import { useRows } from "../useRows";
import { labelColor } from "../labels";
import { Stars } from "./Stars";
import { markReady } from "../diagnostics";
import { describeRow, useRovingFocus } from "../a11y";

const TILE = 176;
const GAP = 8;

function Tile({
  row,
  index,
  selected,
  focused,
  onPick,
  onOpen,
}: {
  row: Row | undefined;
  index: number;
  selected: boolean;
  focused: boolean;
  onPick: (i: number, e: React.MouseEvent) => void;
  onOpen: (i: number) => void;
}) {
  const src = fileUrl(row?.thumb);
  return (
    <div
      role="gridcell"
      aria-selected={selected}
      data-index={index}
      data-testid="tile"
      id={`tile-${index}`}
      tabIndex={focused ? 0 : -1}
      aria-label={describeRow(row, index)}
      className={`tile${selected ? " selected" : ""}${focused ? " focused" : ""}${row && !row.online ? " offline" : ""}`}
      style={{ width: TILE, height: TILE }}
      onMouseDown={(e) => onPick(index, e)}
      onDoubleClick={() => onOpen(index)}
      title={row?.name ?? ""}
    >
      {src ? (
        <img src={src} alt={row?.name ?? ""} draggable={false} loading="lazy" />
      ) : (
        <div className="ph" />
      )}
      {row && (
        <div className="badges">
          {!row.online && (
            <span className="badge off" title="The drive with this item is not connected">
              offline
            </span>
          )}
          {row.kind === "video" && <span className="badge">▶ {fmt.duration(row.durationMs)}</span>}
          {row.kind === "raw" && <span className="badge">RAW</span>}
        </div>
      )}
      {row && (
        <div className="foot">
          <Stars value={row.rating} size="sm" />
          {row.label && (
            <span
              className="dot"
              style={{ background: labelColor(row.label) }}
              aria-label={row.label}
            />
          )}
        </div>
      )}
    </div>
  );
}

export function GridView({ onOpen }: { onOpen: (i: number) => void }) {
  const { s, dispatch, view } = useUi();
  const rows = useRows(view, s.revision);
  const parent = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(800);
  useLayoutEffect(() => {
    const el = parent.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    setWidth(el.clientWidth);
    return () => ro.disconnect();
  }, []);
  const cols = Math.max(1, Math.floor((width - GAP) / (TILE + GAP)));
  const rowCount = Math.ceil(rows.count / cols);
  const v = useVirtualizer({
    count: rowCount,
    getScrollElement: () => parent.current,
    estimateSize: () => TILE + GAP,
    overscan: 3,
  });
  const items = v.getVirtualItems();
  const first = items[0]?.index ?? 0;
  const last = items[items.length - 1]?.index ?? 0;
  useEffect(() => {
    rows.want(first * cols, Math.min(rows.count, (last + 1) * cols));
  });
  // Keep the focused tile in view when moving with the keyboard.
  useEffect(() => {
    if (s.sel.focus !== null) v.scrollToIndex(Math.floor(s.sel.focus / cols), { align: "auto" });
  }, [s.sel.focus, cols, v]);
  useRovingFocus(parent, s.sel.focus, [first, last, cols]);
  const pageRows = Math.max(1, Math.floor((parent.current?.clientHeight ?? 600) / (TILE + GAP)));
  const onKey = useListKeys(rows.count, cols, pageRows, onOpen);
  const pick = (i: number, e: React.MouseEvent) => {
    parent.current?.focus();
    if (e.shiftKey) dispatch({ t: "sel", sel: sel.extend(s.sel, i) });
    else if (e.ctrlKey || e.metaKey) dispatch({ t: "sel", sel: sel.toggle(s.sel, i) });
    else dispatch({ t: "sel", sel: sel.only(i) });
  };
  const loaded = view !== null && (view.count === 0 || rows.get(0) !== undefined);
  useEffect(() => {
    if (loaded) markReady("grid");
  }, [loaded]);
  if (view && view.count === 0) {
    return <div className="empty">Nothing here. Change the filters, or import a card.</div>;
  }
  return (
    <div
      ref={parent}
      className="grid-scroll"
      tabIndex={0}
      role="grid"
      aria-label="Items"
      aria-rowcount={rowCount}
      aria-multiselectable
      data-testid="grid"
      data-count={rows.count}
      onKeyDown={onKey}
    >
      <div style={{ height: v.getTotalSize(), position: "relative" }}>
        {items.map((vr) => (
          <div
            key={vr.key}
            role="row"
            className="grid-row"
            style={{ transform: `translateY(${vr.start}px)`, gap: GAP, paddingLeft: GAP }}
          >
            {Array.from({ length: cols }, (_, c) => {
              const i = vr.index * cols + c;
              if (i >= rows.count) return null;
              return (
                <Tile
                  key={i}
                  index={i}
                  row={rows.get(i)}
                  selected={sel.has(s.sel, i)}
                  focused={s.sel.focus === i}
                  onPick={pick}
                  onOpen={onOpen}
                />
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}
