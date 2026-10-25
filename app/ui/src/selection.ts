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
