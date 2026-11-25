// The sidebar's trees by name (an airline, type or registration, or a
// person; then year and month): the whole tree loads at once, again when the library
// changes; the old one stays on screen until then. The open nodes are
// remembered for each kind (`treeMemory`).

import { useCallback, useEffect, useState } from "react";

import { api } from "./api";
import type { NameTreeNode } from "./bindings/NameTreeNode";
import { nameTreeNodes, type NameTreeKind, type NameNode } from "./nameTree";
import { keepOpenLevels, openLevels } from "./treeMemory";

export function useNameTree(kind: NameTreeKind | null, refresh: unknown) {
  const [st, setSt] = useState(() => ({
    kind,
    open: kind ? openLevels(kind, []) : new Set<string>(),
  }));
  // Another kind: its own open nodes (set while rendering, before they are
  // kept, so one kind's never land in another's).
  if (st.kind !== kind) setSt({ kind, open: kind ? openLevels(kind, []) : new Set() });
  const [tree, setTree] = useState<{ kind: NameTreeKind; nodes: NameTreeNode[] } | null>(null);
  const rkey = JSON.stringify(refresh);

  useEffect(() => {
    if (st.kind) keepOpenLevels(st.kind, st.open);
  }, [st]);

  useEffect(() => {
    if (!kind) return;
    let live = true;
    (kind === "face" ? api.facesTree() : api.aircraftTree(kind))
      .then((nodes) => {
        if (live) setTree({ kind, nodes });
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

  const nodes: NameNode[] =
    kind && tree?.kind === kind && st.kind === kind ? nameTreeNodes(kind, tree.nodes, st.open) : [];
  return { nodes, setExpanded, loaded: kind !== null && tree?.kind === kind };
}
