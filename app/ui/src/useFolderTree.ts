// The sidebar's folder tree: each volume (a folder added in place, or an
// import destination), then its folders as they are on disk, each with the
// number of items in it and below that match the filters. A folder's
// sub-folders load when it is first expanded, and again when the library or
// the filters change.

import { useCallback, useEffect, useRef, useState } from "react";

import { api, type FolderNode, type Query, type Sidebar } from "./api";

export interface FolderTreeNode {
  /** `v:<volume>` or `v:<volume>:<path>`. */
  key: string;
  label: string;
  depth: number;
  count?: number;
  /** `undefined`: no sub-folders. */
  expanded?: boolean;
  volume: string;
  /** `null`: the volume itself. */
  path: string | null;
  /** Volumes only. */
  online?: boolean;
}

const keyOf = (volume: string, path: string | null) =>
  path === null ? `v:${volume}` : `v:${volume}:${path}`;

export function useFolderTree(
  volumes: Sidebar["volumes"],
  query: Query,
  refresh: unknown,
  enabled: boolean,
) {
  const [open, setOpen] = useState<Set<string>>(() => new Set());
  const [levels, setLevels] = useState<Map<string, FolderNode[]>>(() => new Map());
  const loading = useRef(new Set<string>());
  const qkey = JSON.stringify([query, refresh]);
  const current = useRef(qkey);

  useEffect(() => {
    current.current = qkey;
    loading.current = new Set();
    setLevels(new Map());
  }, [qkey]);

  // Load each open node's sub-folders once.
  useEffect(() => {
    if (!enabled) return;
    for (const k of open) {
      if (levels.has(k) || loading.current.has(k)) continue;
      const [, volume, ...rest] = k.split(":");
      const path = rest.length ? rest.join(":") : null;
      const pending = loading.current;
      pending.add(k);
      const asked = qkey;
      api
        .folderTree(volume, path, query)
        .then((f) => {
          if (asked !== current.current) return;
          setLevels((m) => new Map(m).set(k, f));
        })
        .catch((e) => console.error("folder tree", e))
        .finally(() => pending.delete(k));
    }
    // `query` is covered by `qkey`.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, levels, qkey, enabled]);

  const setExpanded = useCallback((key: string, on: boolean) => {
    setOpen((o) => {
      if (o.has(key) === on) return o;
      const n = new Set(o);
      if (on) n.add(key);
      else n.delete(key);
      return n;
    });
  }, []);

  const nodes: FolderTreeNode[] = [];
  const add = (volume: string, parent: string, depth: number) => {
    for (const f of levels.get(parent) ?? []) {
      const key = keyOf(volume, f.path);
      nodes.push({
        key,
        label: f.name,
        depth,
        count: f.count,
        expanded: f.hasChildren ? open.has(key) : undefined,
        volume,
        path: f.path,
      });
      if (f.hasChildren && open.has(key)) add(volume, key, depth + 1);
    }
  };
  for (const v of volumes) {
    const key = keyOf(v.id, null);
    nodes.push({
      key,
      label: v.label,
      depth: 0,
      count: v.count,
      expanded: open.has(key),
      volume: v.id,
      path: null,
      online: v.online,
    });
    if (open.has(key)) add(v.id, key, 1);
  }
  return { nodes, setExpanded };
}
