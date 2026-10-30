// Sidebar: Library (with a tree: the calendar's years, months and days, or
// the folders on disk), Events, Volumes, Tools, Imports. Arrow keys move
// between entries (a single tab stop), Enter or click opens one; Right and
// Left open and close a tree node, or go to its first child or its parent.

import { useEffect, useRef, useState } from "react";

import { api, events, type Sidebar as SidebarData } from "../api";
import * as fmt from "../format";
import { useUi, type Section } from "../state";
import type { Screen } from "../uiModel";
import { useCalendarTree, type CalNode } from "../useCalendarTree";
import { useFolderTree, type FolderTreeNode } from "../useFolderTree";

/** How the Library tree is shown. */
export type TreeMode = "date" | "folder";
const MODE_KEY = "keepsake.sidebarTree";

function rememberedMode(): TreeMode {
  try {
    return localStorage.getItem(MODE_KEY) === "folder" ? "folder" : "date";
  } catch {
    return "date";
  }
}

/** A node of either tree, for keyboard moves and the twisty. */
interface TreeNode {
  key: string;
  depth: number;
  /** `undefined`: a leaf. */
  expanded?: boolean;
}

interface Entry {
  key: string;
  label: string;
  section: Section;
  /** A full-window screen instead of a library section. */
  screen?: Screen;
  count?: number;
  status?: "online" | "offline";
  /** A calendar tree node. */
  cal?: CalNode;
  /** A folder tree node. */
  fold?: FolderTreeNode;
}

const nodeOf = (e: Entry | undefined): TreeNode | undefined => e?.cal ?? e?.fold;

function same(a: Section, b: Section): boolean {
  if (a.kind === "folder" || b.kind === "folder")
    return a.kind === "folder" && b.kind === "folder" && a.id === b.id && a.path === b.path;
  return a.kind === b.kind && ("id" in a ? a.id : "") === ("id" in b ? b.id : "");
}

export function Sidebar() {
  const { s, dispatch, refreshes } = useUi();
  const [data, setData] = useState<SidebarData | null>(null);
  const [tick, setTick] = useState(0);
  const [mode, setModeState] = useState<TreeMode>(rememberedMode);
  const setMode = (m: TreeMode) => {
    setModeState(m);
    try {
      localStorage.setItem(MODE_KEY, m);
    } catch {
      // Not kept: fine.
    }
  };
  useEffect(() => {
    api.sidebar().then(setData).catch(console.error);
  }, [refreshes, tick, s.revision]);
  useEffect(() => {
    const un = events.card(() => setTick((t) => t + 1));
    const t = setInterval(() => setTick((x) => x + 1), 15000);
    return () => {
      clearInterval(t);
      un.then((f) => f());
    };
  }, []);
  const cal = useCalendarTree(s.filters, [refreshes, s.revision]);
  const folders = useFolderTree(
    data?.volumes ?? [],
    s.filters,
    [refreshes, s.revision],
    mode === "folder",
  );
  // The calendar tree follows the calendar view down to where it is.
  const { reveal } = cal;
  useEffect(() => {
    if (s.section.kind === "calendar") reveal(s.cal.year, s.cal.month);
  }, [reveal, s.section.kind, s.cal.year, s.cal.month]);
  const treeEntries: Entry[] =
    mode === "date"
      ? cal.nodes.map((c) => ({
          key: `c${c.key}`,
          label: c.label,
          count: c.count,
          section: { kind: "calendar" } as Section,
          cal: c,
        }))
      : folders.nodes.map((f) => ({
          key: f.key,
          label: f.label,
          count: f.count,
          status: f.online === undefined ? undefined : f.online ? "online" : "offline",
          section:
            f.path === null
              ? ({ kind: "volume", id: f.volume, title: f.label } as Section)
              : ({
                  kind: "folder",
                  id: f.volume,
                  path: f.path,
                  title: folderTitle(data?.volumes ?? [], f.volume, f.path),
                } as Section),
          fold: f,
        }));
  const groups: { title: string; entries: Entry[]; switcher?: boolean }[] = [
    {
      title: "Library",
      switcher: true,
      entries: [{ key: "all", label: "All items", section: { kind: "all" } }, ...treeEntries],
    },
    {
      title: "Events",
      entries: (data?.events ?? []).map((e) => ({
        key: `e${e.id}`,
        label: e.title,
        count: e.count,
        section: { kind: "event", id: e.id, title: e.title },
      })),
    },
    {
      // In the folder tree the volumes are its roots already.
      title: "Volumes",
      entries:
        mode === "folder"
          ? []
          : (data?.volumes ?? []).map((v) => ({
              key: `v${v.id}`,
              label: v.label,
              count: v.count,
              status: v.online ? "online" : "offline",
              section: { kind: "volume", id: v.id, title: v.label },
            })),
    },
    {
      title: "Tools",
      entries: [
        { key: "folders", label: "Folders", section: { kind: "all" }, screen: "folders" },
        { key: "backup", label: "Backup", section: { kind: "all" }, screen: "backup" },
        { key: "settings", label: "Settings", section: { kind: "all" }, screen: "settings" },
      ],
    },
    {
      title: "Imports",
      entries: (data?.imports ?? []).map((i) => {
        const when = new Date(i.startedMs).toLocaleString(undefined, {
          dateStyle: "medium",
          timeStyle: "short",
        });
        return {
          key: `i${i.id}`,
          label: `${when} · ${i.source}`,
          count: i.count,
          section: { kind: "import", id: i.id, title: `Import ${when}` },
        };
      }),
    },
  ];
  const flat = groups.flatMap((g) => g.entries);
  const calActive = (c: CalNode) => {
    if (s.screen !== "library") return false;
    if (c.key === "undated") return s.filters.undated && s.section.kind !== "calendar";
    return (
      s.section.kind === "calendar" &&
      s.cal.year === c.year &&
      s.cal.month === c.month &&
      s.cal.day === c.day
    );
  };
  const isActive = (e: Entry) =>
    e.cal
      ? calActive(e.cal)
      : e.screen
        ? s.screen === e.screen
        : s.screen === "library" && same(e.section, s.section);
  const active = Math.max(0, flat.findIndex(isActive));
  const [focus, setFocus] = useState(active);
  const nav = useRef<HTMLElement>(null);
  const move = (to: number) => {
    const i = Math.max(0, Math.min(flat.length - 1, to));
    setFocus(i);
    nav.current?.querySelector<HTMLButtonElement>(`[data-entry="${i}"]`)?.focus();
  };
  const setExpanded = (e: Entry, on: boolean) => {
    if (e.cal) cal.setExpanded(e.cal.key, on);
    else if (e.fold) folders.setExpanded(e.fold.key, on);
  };
  const openEntry = (e: Entry) => {
    const c = e.cal;
    if (e.fold) {
      dispatch({ t: "section", section: e.section });
      if (e.fold.expanded === false) folders.setExpanded(e.fold.key, true);
      return;
    }
    if (!c) {
      if (e.screen) dispatch({ t: "screen", screen: e.screen });
      else dispatch({ t: "section", section: e.section });
      return;
    }
    if (c.key === "undated") {
      // As in the calendar: items without a date, in the grid.
      dispatch({ t: "section", section: { kind: "all" } });
      dispatch({ t: "filters", filters: { ...s.filters, undated: true } });
      return;
    }
    if (c.key === "cal") dispatch({ t: "section", section: { kind: "calendar" } });
    dispatch({ t: "cal", cal: { year: c.year, month: c.month, day: c.day } });
    if (c.expanded === false) cal.setExpanded(c.key, true);
  };
  const onKey = (e: React.KeyboardEvent) => {
    const entry = flat[focus];
    const c = nodeOf(entry);
    if (e.key === "ArrowDown") move(focus + 1);
    else if (e.key === "ArrowUp") move(focus - 1);
    else if (e.key === "Home") move(0);
    else if (e.key === "End") move(flat.length - 1);
    else if (e.key === "ArrowRight" && c) {
      if (c.expanded === false) setExpanded(entry, true);
      else if (c.expanded) move(focus + 1);
    } else if (e.key === "ArrowLeft" && c) {
      if (c.expanded) setExpanded(entry, false);
      else if (c.depth > 0) {
        // Up to the parent node.
        for (let i = focus - 1; i >= 0; i--) {
          const p = nodeOf(flat[i]);
          if (p && p.depth < c.depth) {
            move(i);
            break;
          }
        }
      }
    } else return;
    e.preventDefault();
  };
  let n = -1;
  return (
    <nav ref={nav} className="sidebar" aria-label="Library" onKeyDown={onKey} data-testid="sidebar">
      {groups.map((g) =>
        g.entries.length === 0 ? null : (
          <section key={g.title}>
            <h3>{g.title}</h3>
            {g.switcher && (
              <div
                className="segmented tree-mode"
                role="group"
                aria-label="Show the library"
                data-testid="tree-mode"
              >
                {(
                  [
                    ["date", "By date"],
                    ["folder", "By folder"],
                  ] as const
                ).map(([m, label]) => (
                  <button
                    key={m}
                    aria-pressed={mode === m}
                    onClick={() => setMode(m)}
                    onKeyDown={(ev) => ev.stopPropagation()}
                  >
                    {label}
                  </button>
                ))}
              </div>
            )}
            <ul>
              {g.entries.map((e) => {
                n += 1;
                const i = n;
                const c = nodeOf(e);
                return (
                  <li key={e.key}>
                    <button
                      data-entry={i}
                      data-cal={e.cal?.key}
                      data-folder={e.fold?.path ?? undefined}
                      tabIndex={i === focus ? 0 : -1}
                      aria-current={isActive(e) ? "page" : undefined}
                      aria-expanded={c?.expanded}
                      className={`${isActive(e) ? "active" : ""}${c ? ` cal-node depth-${c.depth}` : ""}`}
                      onClick={() => {
                        setFocus(i);
                        openEntry(e);
                      }}
                      onFocus={() => setFocus(i)}
                    >
                      {c && (
                        <span
                          className="twisty"
                          aria-hidden
                          title={
                            c.expanded === undefined ? undefined : c.expanded ? "Close" : "Open"
                          }
                          onClick={(ev) => {
                            if (c.expanded === undefined) return;
                            // The arrow only opens or closes; the label opens the level.
                            ev.stopPropagation();
                            setFocus(i);
                            setExpanded(e, !c.expanded);
                          }}
                        >
                          {c.expanded === undefined ? "" : c.expanded ? "▾" : "▸"}
                        </span>
                      )}
                      {e.status && (
                        <span
                          className={`vol ${e.status}`}
                          aria-label={e.status}
                          title={e.status === "online" ? "Connected" : "Not connected"}
                        />
                      )}
                      <span className="label">{e.label}</span>
                      {e.count !== undefined && <span className="count">{fmt.count(e.count)}</span>}
                    </button>
                  </li>
                );
              })}
            </ul>
          </section>
        ),
      )}
    </nav>
  );
}

/** "Pictures › 2025 › May" for the heading. */
function folderTitle(volumes: SidebarData["volumes"], volume: string, path: string): string {
  const label = volumes.find((v) => v.id === volume)?.label ?? "";
  return [label, ...path.split("/")].filter(Boolean).join(" › ");
}
