// The sidebar's trees by name (by aircraft: airline › type › registration;
// by face: person › year › month): the whole tree loads at once, again
// when the library changes; the old one stays on screen until then. The
// open nodes are remembered for each tree (`treeMemory`).

import { useCallback, useEffect, useState } from "react";

import { api } from "./api";
import type { AircraftReportGroup } from "./bindings/AircraftReportGroup";
import type { NameTreeNode } from "./bindings/NameTreeNode";
import { aircraftTreeNodes, faceTreeNodes, type NameNode, type NameTreeKind } from "./nameTree";
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
        ? aircraftTreeNodes(tree.groups, st.open)
        : faceTreeNodes(tree.nodes, st.open);
  return { nodes, setExpanded, loaded: kind !== null && tree?.kind === kind };
}
