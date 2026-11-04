// Rows of the open view, fetched in pages around what is on screen and kept
// in a bounded cache. Tiles whose thumbnail is missing are reported to the
// render queue as visible; `render://thumbs` refreshes them when ready.

import { useCallback, useEffect, useRef, useState } from "react";

import { api, events, type Row, type ViewInfo } from "./api";

const PAGE = 200;
const KEEP_PAGES = 30;

export interface RowSource {
  get: (i: number) => Row | undefined;
  /** Make sure rows [from, to) are (being) loaded; report them as visible. */
  want: (from: number, to: number) => void;
  count: number;
}

export function useRows(view: ViewInfo | null, revision: number): RowSource {
  const pages = useRef(new Map<number, Row[]>());
  const loading = useRef(new Set<number>());
  const order = useRef<number[]>([]);
  const byId = useRef(new Map<string, number>());
  const [, setTick] = useState(0);
  const visibleTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const viewId = view?.view ?? null;

  // A new view or an edit: start over.
  useEffect(() => {
    pages.current.clear();
    loading.current.clear();
    order.current = [];
    byId.current.clear();
    setTick((t) => t + 1);
  }, [viewId, revision]);

  const load = useCallback(
    (p: number, force = false) => {
      if (viewId === null || loading.current.has(p)) return;
      if (!force && pages.current.has(p)) return;
      loading.current.add(p);
      const v = viewId;
      api
        .viewRows(v, p * PAGE, PAGE)
        .then((rows) => {
          if (v !== viewId) return;
          pages.current.set(p, rows);
          rows.forEach((r, k) => byId.current.set(r.id, p * PAGE + k));
          order.current = [...order.current.filter((x) => x !== p), p];
          while (order.current.length > KEEP_PAGES) {
            const old = order.current.shift();
            if (old !== undefined) pages.current.delete(old);
          }
          setTick((t) => t + 1);
        })
        .catch((e) => console.error("view_rows", e))
        .finally(() => loading.current.delete(p));
    },
    [viewId],
  );

  const get = useCallback((i: number) => pages.current.get(Math.floor(i / PAGE))?.[i % PAGE], []);

  const want = useCallback(
    (from: number, to: number) => {
      for (let p = Math.floor(from / PAGE); p <= Math.floor(Math.max(from, to - 1) / PAGE); p++)
        load(p);
      if (visibleTimer.current) clearTimeout(visibleTimer.current);
      visibleTimer.current = setTimeout(() => {
        const ids: string[] = [];
        for (let i = from; i < to; i++) {
          const r = get(i);
          if (r && !r.thumb && r.online) ids.push(r.id);
        }
        if (ids.length) api.renderSetVisible(ids.slice(0, 500)).catch(() => undefined);
      }, 250);
    },
    [load, get],
  );

  // Thumbnails were made: reload the pages holding them (batched; the old
  // rows stay on screen until the new ones arrive).
  const stale = useRef(new Set<number>());
  useEffect(() => {
    const flush = setInterval(() => {
      for (const p of stale.current) load(p, true);
      stale.current.clear();
    }, 300);
    const un = events.thumbs((e) => {
      for (const id of e.ids) {
        const i = byId.current.get(id);
        if (i !== undefined) stale.current.add(Math.floor(i / PAGE));
      }
    });
    return () => {
      clearInterval(flush);
      un.then((f) => f());
    };
  }, [load]);

  return { get, want, count: view?.count ?? 0 };
}
