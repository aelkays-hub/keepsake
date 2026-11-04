// Multi-selection as sorted, non-overlapping index ranges [from, to) of a
// view. Selecting 300,000 items is one range, not 300,000 ids. Pure
// functions (unit-tested).

export type Range = [number, number];

export interface Sel {
  ranges: Range[];
  /** Where Shift-click and Shift+arrows extend from. */
  anchor: number | null;
  /** The focused item (keyboard cursor), also shown in the inspector. */
  focus: number | null;
}

export const emptySel: Sel = { ranges: [], anchor: null, focus: null };

export function normalize(ranges: Range[]): Range[] {
  const r = ranges
    .filter(([a, b]) => b > a)
    .map(([a, b]) => [a, b] as Range)
    .sort((x, y) => x[0] - y[0]);
  const out: Range[] = [];
  for (const cur of r) {
    const last = out[out.length - 1];
    if (last && cur[0] <= last[1]) last[1] = Math.max(last[1], cur[1]);
    else out.push(cur);
  }
  return out;
}

export function size(s: Sel): number {
  return s.ranges.reduce((n, [a, b]) => n + (b - a), 0);
}

export function has(s: Sel, i: number): boolean {
  return s.ranges.some(([a, b]) => i >= a && i < b);
}

/** Plain click / arrow: just this item. */
export function only(i: number): Sel {
  return { ranges: [[i, i + 1]], anchor: i, focus: i };
}

/** Ctrl/⌘-click: toggle this item, keep the rest. */
export function toggle(s: Sel, i: number): Sel {
  if (has(s, i)) {
    const out: Range[] = [];
    for (const [a, b] of s.ranges) {
      if (i >= a && i < b) {
        if (a < i) out.push([a, i]);
        if (i + 1 < b) out.push([i + 1, b]);
      } else out.push([a, b]);
    }
    return { ranges: out, anchor: i, focus: i };
  }
  return { ranges: normalize([...s.ranges, [i, i + 1]]), anchor: i, focus: i };
}

/** Shift-click / Shift+arrow: from the anchor to here. */
export function extend(s: Sel, i: number): Sel {
  const anchor = s.anchor ?? i;
  const lo = Math.min(anchor, i);
  const hi = Math.max(anchor, i) + 1;
  return { ranges: [[lo, hi]], anchor, focus: i };
}

export function all(count: number): Sel {
  return count > 0 ? { ranges: [[0, count]], anchor: 0, focus: 0 } : emptySel;
}

/** Keep only what is inside a view of `count` items. */
export function clamp(s: Sel, count: number): Sel {
  const ranges = normalize(s.ranges.map(([a, b]) => [Math.min(a, count), Math.min(b, count)]));
  const f = s.focus !== null && s.focus < count ? s.focus : null;
  return { ranges, anchor: s.anchor !== null && s.anchor < count ? s.anchor : f, focus: f };
}

/**
 * Where the viewer goes next from `current`: through the selection when
 * more than one item is selected, else through the whole view. `null` when
 * there is nowhere to go.
 */
export function viewerStep(
  s: Sel,
  count: number,
  current: number,
  dir: "next" | "prev" | "first" | "last",
): number | null {
  if (count === 0) return null;
  let to: number | null;
  if (size(s) > 1) {
    const r = s.ranges;
    switch (dir) {
      case "first":
        to = r[0][0];
        break;
      case "last":
        to = r[r.length - 1][1] - 1;
        break;
      case "next": {
        const hit = r.find(([, b]) => b - 1 > current);
        to = hit ? Math.max(hit[0], current + 1) : null;
        break;
      }
      case "prev": {
        const hit = [...r].reverse().find(([a]) => a < current);
        to = hit ? Math.min(hit[1] - 1, current - 1) : null;
        break;
      }
    }
  } else {
    to =
      dir === "first" ? 0 : dir === "last" ? count - 1 : dir === "next" ? current + 1 : current - 1;
  }
  if (to === null || to < 0 || to >= count || to === current) return null;
  return to;
}

/** Add `ranges` to a selection (Ctrl/Shift-drag), focusing `focus`. */
export function union(s: Sel, ranges: Range[], focus: number | null): Sel {
  return { ranges: normalize([...s.ranges, ...ranges]), anchor: s.anchor ?? focus, focus };
}

/** How the grid lays out its tiles: `cols` per row, each `tile` px square. */
export interface GridGeom {
  cols: number;
  count: number;
  tile: number;
  gap: number;
}

/**
 * The tiles a drag rectangle touches, as index ranges (one per row). The
 * rectangle is in the grid's content coordinates (scroll included), corners
 * in any order. Row `r` starts at y = r·(tile+gap) with its tiles `gap` px
 * down; column `c` starts at x = gap + c·(tile+gap).
 */
export function inRect(g: GridGeom, x0: number, y0: number, x1: number, y1: number): Range[] {
  if (g.count <= 0 || g.cols <= 0) return [];
  const step = g.tile + g.gap;
  const [left, right] = [Math.min(x0, x1), Math.max(x0, x1)];
  const [top, bottom] = [Math.min(y0, y1), Math.max(y0, y1)];
  // First/last column and row whose tile overlaps the rectangle.
  const c0 = Math.max(0, Math.ceil((left - g.gap - g.tile) / step));
  const c1 = Math.min(g.cols - 1, Math.floor((right - g.gap) / step));
  const rows = Math.ceil(g.count / g.cols);
  const r0 = Math.max(0, Math.ceil((top - g.gap - g.tile) / step));
  const r1 = Math.min(rows - 1, Math.floor((bottom - g.gap) / step));
  if (c0 > c1 || r0 > r1) return [];
  const out: Range[] = [];
  for (let r = r0; r <= r1; r++) {
    const a = r * g.cols + c0;
    const b = Math.min(g.count, r * g.cols + c1 + 1);
    if (a < b) out.push([a, b]);
  }
  return normalize(out);
}
