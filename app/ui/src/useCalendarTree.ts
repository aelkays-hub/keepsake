// The sidebar's calendar tree: years, then a year's months, then a month's
// days, each with its count. Levels load when they are first expanded, and
// again when the library or the filters change: the old ones stay on screen
// until the new ones arrive, so the tree never shrinks and keeps its place.
// The open levels are remembered (`treeMemory`).

import { useCallback, useEffect, useRef, useState } from "react";

import { api, type Bucket, type Query } from "./api";
import * as fmt from "./format";
import { keepOpenLevels, openLevels } from "./treeMemory";

export interface CalNode {
  /** `cal`, `2025`, `2025-06`, `2025-06-15` or `undated`. */
  key: string;
  label: string;
  depth: number;
  count?: number;
  /** `undefined`: a leaf (a day, or "No date"). */
  expanded?: boolean;
  year: number | null;
  month: number | null;
  day: number | null;
}

const two = (n: number) => String(n).padStart(2, "0");

/** "Sun 15", in the user's language. */
export function dayLabel(year: number, month: number, day: number): string {
  return new Date(Date.UTC(year, month - 1, day)).toLocaleDateString(undefined, {
    weekday: "short",
    day: "numeric",
    timeZone: "UTC",
  });
}

export function useCalendarTree(query: Query, refresh: unknown) {
  const [open, setOpen] = useState<Set<string>>(() => openLevels("date", ["cal"]));
  const [levels, setLevels] = useState<Map<string, Bucket[]>>(() => new Map());
  const loading = useRef(new Set<string>());
  /** The levels loaded for the current library and filters. */
  const fresh = useRef(new Set<string>());
  const qkey = JSON.stringify([query, refresh]);
  const current = useRef(qkey);

  useEffect(() => keepOpenLevels("date", open), [open]);

  // The library or filters changed: every level loads again, the old ones
  // shown until then.
  useEffect(() => {
    current.current = qkey;
    loading.current = new Set();
    fresh.current = new Set();
  }, [qkey]);

  // Load each open level that is not loaded (for these filters) yet.
  useEffect(() => {
    for (const k of open) {
      if (fresh.current.has(k) || loading.current.has(k) || k === "undated") continue;
      const parts = k === "cal" ? [] : k.split("-").map(Number);
      if (parts.length > 2) continue; // days are leaves
      const pending = loading.current;
      pending.add(k);
      const level =
        parts.length === 0
          ? ({ level: "years" } as const)
          : parts.length === 1
            ? ({ level: "months", year: parts[0] } as const)
            : ({ level: "days", year: parts[0], month: parts[1] } as const);
      const asked = qkey;
      api
        .calendar(query, level)
        .then((b) => {
          // An answer for an older library or filter is dropped.
          if (asked !== current.current) return;
          fresh.current.add(k);
          setLevels((m) => new Map(m).set(k, b));
        })
        .catch((e) => console.error("calendar tree", e))
        .finally(() => pending.delete(k));
    }
    // `query` is covered by `qkey`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, levels, qkey]);

  const setExpanded = useCallback((key: string, on: boolean) => {
    setOpen((o) => {
      if (o.has(key) === on) return o;
      const n = new Set(o);
      if (on) n.add(key);
      else n.delete(key);
      return n;
    });
  }, []);

  /** Open the path to a calendar position (when the view moves there). */
  const reveal = useCallback((year: number | null, month: number | null) => {
    setOpen((o) => {
      const want = ["cal"];
      if (year !== null) want.push(String(year));
      if (year !== null && month !== null) want.push(`${year}-${two(month)}`);
      if (want.every((k) => o.has(k))) return o;
      return new Set([...o, ...want]);
    });
  }, []);

  // The visible nodes, in order.
  const nodes: CalNode[] = [];
  nodes.push({
    key: "cal",
    label: "Calendar",
    depth: 0,
    expanded: open.has("cal"),
    year: null,
    month: null,
    day: null,
  });
  if (open.has("cal")) {
    for (const y of levels.get("cal") ?? []) {
      if (y.key === "undated") {
        nodes.push({
          key: "undated",
          label: "No date",
          depth: 1,
          count: y.count,
          year: null,
          month: null,
          day: null,
        });
        continue;
      }
      const year = Number(y.key);
      nodes.push({
        key: y.key,
        label: y.key,
        depth: 1,
        count: y.count,
        expanded: open.has(y.key),
        year,
        month: null,
        day: null,
      });
      if (!open.has(y.key)) continue;
      for (const m of levels.get(y.key) ?? []) {
        const month = Number(m.key.split("-")[1]);
        nodes.push({
          key: m.key,
          label: fmt.monthName(month),
          depth: 2,
          count: m.count,
          expanded: open.has(m.key),
          year,
          month,
          day: null,
        });
        if (!open.has(m.key)) continue;
        for (const d of levels.get(m.key) ?? []) {
          const day = Number(d.key.split("-")[2]);
          nodes.push({
            key: d.key,
            label: dayLabel(year, month, day),
            depth: 3,
            count: d.count,
            year,
            month,
            day,
          });
        }
      }
    }
  }
  return { nodes, setExpanded, reveal };
}
