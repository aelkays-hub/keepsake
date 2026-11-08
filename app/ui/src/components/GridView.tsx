// The thumbnail grid: virtualized rows of tiles, multi-select with mouse
// (click, Ctrl/Shift-click, the tick-box on each tile, Select mode, and
// dragging a rectangle) and keyboard, offline badges, ratings, labels and
// video durations. Right-click (or the Menu key) opens a menu for the
// selection: Open, Tag with Claude.

import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useLayoutEffect, useRef, useState } from "react";

import { fileUrl, type Row } from "../api";
import { selectionPayload, useListKeys } from "../edits";
import * as fmt from "../format";
import * as sel from "../selection";
import { useUi } from "../state";
import { useRows } from "../useRows";
import { labelColor } from "../labels";
import { Stars } from "./Stars";
import { markReady } from "../diagnostics";
import { describeRow, useRovingFocus } from "../a11y";
import { stepThumbSize, tilePx, useThumbSize, wantsLarge } from "../thumbSize";
import { ContextMenu } from "./ContextMenu";

const MAC =
  typeof navigator !== "undefined" && /Mac/.test(navigator.platform || navigator.userAgent);
const GAP = 8;
/** How far the mouse moves before a press becomes a drag. */
const DRAG_PX = 4;
/** How close to the top or bottom a drag scrolls the grid. */
const EDGE_PX = 48;

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

interface Drag {
  stop: () => void;
}

function Tile({
  row,
  index,
  px,
  large,
  selected,
  focused,
  onOpen,
}: {
  row: Row | undefined;
  index: number;
  px: number;
  /** Use the 1024 px thumbnail when it is made. */
  large: boolean;
  selected: boolean;
  focused: boolean;
  onOpen: (i: number) => void;
}) {
  const src = fileUrl((large ? row?.thumbLarge : null) ?? row?.thumb);
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
      style={{ width: px, height: px }}
      onDoubleClick={(e) => {
        if (!(e.target as HTMLElement).closest(".check")) onOpen(index);
      }}
      title={row?.name ?? ""}
    >
      {src ? (
        <img src={src} alt={row?.name ?? ""} draggable={false} loading="lazy" />
      ) : (
        <div className="ph" />
      )}
      <span
        className="check"
        role="checkbox"
        aria-checked={selected}
        aria-label={selected ? "Remove from selection" : "Add to selection"}
        title={selected ? "Remove from selection" : "Add to selection"}
        data-testid="tile-check"
      >
        {selected ? "✓" : ""}
      </span>
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
  const size = useThumbSize();
  const TILE = tilePx(size);
  const large = wantsLarge(size, window.devicePixelRatio || 1);
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
  // A new size: measure the rows again, and keep the focused item in view.
  useEffect(() => {
    v.measure();
  }, [TILE, v]);
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
  const listKeys = useListKeys(rows.count, cols, pageRows, onOpen);
  /** The right-click menu, at the pointer, for this selection. */
  const [menu, setMenu] = useState<{ x: number; y: number; sel: sel.Sel } | null>(null);
  const openMenu = (x: number, y: number, i: number | null) => {
    const cur = i !== null && !sel.has(s.sel, i) ? sel.only(i) : s.sel;
    if (!cur.ranges.length && cur.focus === null) return;
    setMenu({ x, y, sel: cur });
  };
  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) {
      const el = parent.current?.querySelector<HTMLElement>(`#tile-${s.sel.focus ?? -1}`);
      const r = el?.getBoundingClientRect();
      if (r) openMenu(r.left + 8, r.bottom - 8, s.sel.focus);
      e.preventDefault();
      return;
    }
    // + and − change the thumbnail size.
    if (
      !e.ctrlKey &&
      !e.metaKey &&
      !e.altKey &&
      (e.key === "+" || e.key === "=" || e.key === "-")
    ) {
      stepThumbSize(e.key === "-" ? -1 : 1);
      e.preventDefault();
      return;
    }
    listKeys(e);
  };
  const content = useRef<HTMLDivElement>(null);
  const [box, setBox] = useState<Box | null>(null);
  const drag = useRef<Drag | null>(null);
  const geom = { cols, count: rows.count, tile: TILE, gap: GAP };
  const geomRef = useRef(geom);
  geomRef.current = geom;
  // Stop a drag left running when the grid goes away.
  useEffect(() => () => drag.current?.stop(), []);
  const onMouseDown = (e: React.MouseEvent) => {
    const t = e.target as HTMLElement;
    const hit = t.closest<HTMLElement>("[data-index]");
    const i = hit ? Number(hit.dataset.index) : null;
    parent.current?.focus({ preventScroll: true });
    if (e.button !== 0) {
      // Right-click keeps a selection it lands in.
      if (i !== null && !sel.has(s.sel, i)) dispatch({ t: "sel", sel: sel.only(i) });
      return;
    }
    if (t === parent.current && e.nativeEvent.offsetX >= t.clientWidth) return; // scroll bar
    const additive = e.ctrlKey || e.metaKey || s.selecting;
    if (i !== null) {
      if (e.shiftKey) dispatch({ t: "sel", sel: sel.extend(s.sel, i) });
      else if (additive || t.closest(".check")) dispatch({ t: "sel", sel: sel.toggle(s.sel, i) });
      else dispatch({ t: "sel", sel: sel.only(i) });
    }
    // A drag from here selects a rectangle; extra with Ctrl/Shift or in
    // Select mode, else in place of the selection.
    const base = additive || e.shiftKey ? s.sel : sel.emptySel;
    startDrag(e, i, base);
  };
  const startDrag = (e: React.MouseEvent, start: number | null, base: sel.Sel) => {
    const c = content.current;
    const p = parent.current;
    if (!c || !p) return;
    const at = (x: number, y: number) => {
      const r = c.getBoundingClientRect();
      return { x: x - r.left, y: y - r.top };
    };
    const origin = at(e.clientX, e.clientY);
    let last = { x: e.clientX, y: e.clientY };
    let moved = false;
    let speed = 0;
    let frame = 0;
    const update = () => {
      const cur = at(last.x, last.y);
      setBox({
        x: Math.min(origin.x, cur.x),
        y: Math.min(origin.y, cur.y),
        w: Math.abs(cur.x - origin.x),
        h: Math.abs(cur.y - origin.y),
      });
      const ranges = sel.inRect(geomRef.current, origin.x, origin.y, cur.x, cur.y);
      const focus = start ?? ranges[0]?.[0] ?? null;
      dispatch({ t: "sel", sel: sel.union(base, ranges, focus) });
    };
    // Near the top or bottom edge the grid scrolls, faster nearer the edge.
    const tick = () => {
      if (speed !== 0) {
        p.scrollTop += speed;
        update();
      }
      frame = requestAnimationFrame(tick);
    };
    const move = (ev: MouseEvent) => {
      last = { x: ev.clientX, y: ev.clientY };
      if (!moved && Math.hypot(ev.clientX - e.clientX, ev.clientY - e.clientY) < DRAG_PX) return;
      if (!moved) {
        moved = true;
        frame = requestAnimationFrame(tick);
      }
      const r = p.getBoundingClientRect();
      const edge = Math.min(EDGE_PX, r.height / 4);
      speed =
        ev.clientY < r.top + edge
          ? -Math.ceil((r.top + edge - ev.clientY) / 3)
          : ev.clientY > r.bottom - edge
            ? Math.ceil((ev.clientY - (r.bottom - edge)) / 3)
            : 0;
      update();
    };
    const stop = () => {
      window.removeEventListener("mousemove", move);
      window.removeEventListener("mouseup", up);
      cancelAnimationFrame(frame);
      drag.current = null;
      setBox(null);
    };
    const up = () => {
      stop();
      // A click on the space between tiles clears the selection.
      if (!moved && start === null && base === sel.emptySel) {
        dispatch({ t: "sel", sel: sel.emptySel });
      }
    };
    drag.current?.stop();
    drag.current = { stop };
    window.addEventListener("mousemove", move);
    window.addEventListener("mouseup", up);
  };
  const loaded = view !== null && (view.count === 0 || rows.get(0) !== undefined);
  useEffect(() => {
    if (loaded) markReady("grid");
  }, [loaded]);
  if (view && view.count === 0) {
    return (
      <div className="empty" data-testid="empty">
        <p>Nothing here. Change the filters, or import a card.</p>
        <p>
          Photos already on this computer or a drive?{" "}
          <button
            className="primary"
            onClick={() => dispatch({ t: "screen", screen: "folders" })}
            data-testid="empty-add-folder"
          >
            Add a folder…
          </button>
        </p>
      </div>
    );
  }
  return (
    <>
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
        data-size={size}
        data-selecting={s.selecting || undefined}
        onKeyDown={onKey}
        onMouseDown={onMouseDown}
        onContextMenu={(e) => {
          e.preventDefault();
          // On a Mac, Ctrl+click also fires contextmenu: it toggles the
          // selection there, so it opens no menu.
          if (e.ctrlKey && (e.button === 0 || MAC)) return;
          const hit = (e.target as HTMLElement).closest<HTMLElement>("[data-index]");
          if (hit) openMenu(e.clientX, e.clientY, Number(hit.dataset.index));
        }}
      >
        <div ref={content} style={{ height: v.getTotalSize(), position: "relative" }}>
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
                    px={TILE}
                    large={large}
                    selected={sel.has(s.sel, i)}
                    focused={s.sel.focus === i}
                    onOpen={onOpen}
                  />
                );
              })}
            </div>
          ))}
          {box && (
            <div
              className="marquee"
              data-testid="marquee"
              style={{ left: box.x, top: box.y, width: box.w, height: box.h }}
            />
          )}
        </div>
      </div>
      {menu && (
        <ContextMenu
          x={menu.x}
          y={menu.y}
          label="Selected items"
          testId="tile-menu"
          onClose={() => setMenu(null)}
          items={[
            {
              label: "Open",
              disabled: menu.sel.focus === null,
              run: () => {
                if (menu.sel.focus !== null) onOpen(menu.sel.focus);
              },
            },
            {
              label: "Tag with Claude…",
              title: "Send these photos to Claude to be tagged (asks first)",
              testId: "tile-menu-claude",
              run: () => {
                const n = menu.sel.ranges.length ? sel.size(menu.sel) : 1;
                const one = n === 1 && menu.sel.focus !== null ? rows.get(menu.sel.focus) : null;
                dispatch({
                  t: "aiJob",
                  job: {
                    claude: selectionPayload(view?.view ?? null, menu.sel),
                    count: n,
                    title: one?.name ?? fmt.plural(n, "photo"),
                    what: "tags",
                  },
                });
              },
            },
          ]}
        />
      )}
    </>
  );
}
