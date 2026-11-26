// The sidebar's trees by name (by aircraft: airline › type › registration
// › year › month; by face: person › year › month): the whole tree loads at
// once, again when the library changes; the old one stays on screen until
// then. A registration's years, and a year's months, load when it is
// opened (the calendar's counts for its search). The
// open nodes are remembered for each tree (`treeMemory`).

import { useCallback, useEffect, useRef, useState } from "react";

import { api, emptyQuery } from "./api";
import type { Bucket } from "./bindings/Bucket";
import type { AircraftReportGroup } from "./bindings/AircraftReportGroup";
import type { NameTreeNode } from "./bindings/NameTreeNode";
import {
  aircraftTreeNodes,
  faceTreeNodes,
  type DateLoad,
  type NameNode,
  type NameTreeKind,
} from "./nameTree";
import { keepOpenLevels, openLevels } from "./treeMemory";

type Loaded =
  { kind: "aircraft"; groups: AircraftReportGroup[] } | { kind: "face"; nodes: NameTreeNode[] };

export function useNameTree(kind: NameTreeKind | null, refresh: unknown) {
  const [st, setSt] = useState(() => ({
    kind,
    open: kind ? openLevels(kind, []) : new Set<string>(),
  }));
  // Another tree: its own open nodes (set while rendering, before they are
  // kept, so one tree's never land in another's).
  if (st.kind !== kind) setSt({ kind, open: kind ? openLevels(kind, []) : new Set() });
  const [tree, setTree] = useState<Loaded | null>(null);
  const rkey = JSON.stringify(refresh);
  // The years and months loaded under the aircraft lines; the old ones
  // stay shown until loaded again for a changed library.
  const [dates, setDates] = useState<Map<string, Bucket[]>>(() => new Map());
  const fresh = useRef({ rkey, keys: new Set<string>() });
  if (fresh.current.rkey !== rkey) fresh.current = { rkey, keys: new Set() };

  useEffect(() => {
    if (st.kind) keepOpenLevels(st.kind, st.open);
  }, [st]);

  useEffect(() => {
    if (!kind) return;
    let live = true;
    const load: Promise<Loaded> =
      kind === "aircraft"
        ? api.aircraftReport().then((r) => ({ kind: "aircraft", groups: r.byAirline }))
        : api.facesTree().then((nodes) => ({ kind: "face", nodes }));
    load
      .then((t) => {
        if (live) setTree(t);
      })
      .catch((e) => console.error("name tree", e));
    return () => {
      live = false;
    };
  }, [kind, rkey]);

  const setExpanded = useCallback((key: string, on: boolean) => {
    setSt((s) => {
      if (s.open.has(key) === on) return s;
      const open = new Set(s.open);
      if (on) open.add(key);
      else open.delete(key);
      return { ...s, open };
    });
  }, []);

  let nodes: NameNode[] = [];
  if (kind !== null && st.kind === kind && tree?.kind === kind)
    nodes =
      tree.kind === "aircraft"
        ? aircraftTreeNodes(tree.groups, st.open, dates)
        : faceTreeNodes(tree.nodes, st.open);

  // Load what an open line shows and has not loaded (for this library).
  const shown = new Set(dates.keys());
  const wanted: DateLoad[] = [];
  for (const n of nodes) {
    if (n.load) wanted.push(n.load);
    else if (n.expanded && !fresh.current.keys.has(n.key) && shown.has(n.key))
      wanted.push(staleLoad(n));
  }
  const wkey = JSON.stringify(wanted.map((w) => w.key));
  useEffect(() => {
    const asked = fresh.current;
    for (const w of wanted) {
      if (asked.keys.has(w.key)) continue;
      asked.keys.add(w.key);
      api
        .calendar({ ...emptyQuery, ...w.query }, w.level)
        .then((b) => {
          if (fresh.current === asked) setDates((m) => new Map(m).set(w.key, b));
        })
        .catch((e) => {
          asked.keys.delete(w.key);
          console.error("name tree dates", e);
        });
    }
    // `wanted` is covered by `wkey`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [wkey, rkey]);
  return { nodes, setExpanded, loaded: kind !== null && tree?.kind === kind };
}

/** An open line whose level is shown from before the library changed:
 * load it again. A year line's key ends with its year. */
function staleLoad(n: NameNode): DateLoad {
  const base = { ...n.query };
  delete base.from;
  delete base.to;
  delete base.undated;
  if (n.depth === 3) {
    const year = Number(n.label);
    return { key: n.key, query: base, level: { level: "months", year } };
  }
  return { key: n.key, query: base, level: { level: "years" } };
}
