// The sidebar's aircraft trees (by airline, type or registration, then year
// and month): the whole tree loads at once, again when the library
// changes; the old one stays on screen until then. The open nodes are
// remembered for each kind (`treeMemory`).

import { useCallback, useEffect, useState } from "react";

import { api } from "./api";
import type { NameTreeNode } from "./bindings/NameTreeNode";
import { nameTreeNodes, type AircraftTreeKind, type NameNode } from "./aircraftTree";
import { keepOpenLevels, openLevels } from "./treeMemory";

export function useNameTree(kind: AircraftTreeKind | null, refresh: unknown) {
  const [st, setSt] = useState(() => ({
    kind,
    open: kind ? openLevels(kind, []) : new Set<string>(),
  }));
  // Another kind: its own open nodes (set while rendering, before they are
  // kept, so one kind's never land in another's).
  if (st.kind !== kind) setSt({ kind, open: kind ? openLevels(kind, []) : new Set() });
  const [tree, setTree] = useState<{ kind: AircraftTreeKind; nodes: NameTreeNode[] } | null>(null);
  const rkey = JSON.stringify(refresh);

  useEffect(() => {
    if (st.kind) keepOpenLevels(st.kind, st.open);
  }, [st]);

  useEffect(() => {
    if (!kind) return;
    let live = true;
    api
      .aircraftTree(kind)
      .then((nodes) => {
        if (live) setTree({ kind, nodes });
      })
      .catch((e) => console.error("aircraft tree", e));
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

  const nodes: NameNode[] =
    kind && tree?.kind === kind && st.kind === kind ? nameTreeNodes(kind, tree.nodes, st.open) : [];
  return { nodes, setExpanded, loaded: kind !== null && tree?.kind === kind };
}
