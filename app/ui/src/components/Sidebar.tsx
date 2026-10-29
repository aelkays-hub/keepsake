// Sidebar: Library (with the calendar as a tree of years, months and days),
// Events, Volumes, Tools, Imports. Arrow keys move between entries (a
// single tab stop), Enter or click opens one; Right and Left open and close
// a calendar node, or go to its first child or its parent.

import { useEffect, useRef, useState } from "react";

import { api, events, type Sidebar as SidebarData } from "../api";
import * as fmt from "../format";
import { useUi, type Section } from "../state";
import type { Screen } from "../uiModel";
import { useCalendarTree, type CalNode } from "../useCalendarTree";

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
}

function same(a: Section, b: Section): boolean {
  return a.kind === b.kind && ("id" in a ? a.id : "") === ("id" in b ? b.id : "");
}

export function Sidebar() {
  const { s, dispatch, refreshes } = useUi();
  const [data, setData] = useState<SidebarData | null>(null);
  const [tick, setTick] = useState(0);
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
  const tree = useCalendarTree(s.filters, [refreshes, s.revision]);
  // The tree follows the calendar view down to where it is.
  const { reveal } = tree;
  useEffect(() => {
    if (s.section.kind === "calendar") reveal(s.cal.year, s.cal.month);
  }, [reveal, s.section.kind, s.cal.year, s.cal.month]);
  const groups: { title: string; entries: Entry[] }[] = [
    {
      title: "Library",
      entries: [
        { key: "all", label: "All items", section: { kind: "all" } },
        ...tree.nodes.map((c) => ({
          key: `c${c.key}`,
          label: c.label,
          count: c.count,
          section: { kind: "calendar" } as Section,
          cal: c,
        })),
      ],
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
      title: "Volumes",
      entries: (data?.volumes ?? []).map((v) => ({
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
  const openEntry = (e: Entry) => {
    const c = e.cal;
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
    if (c.expanded === false) tree.setExpanded(c.key, true);
  };
  const onKey = (e: React.KeyboardEvent) => {
    const c = flat[focus]?.cal;
    if (e.key === "ArrowDown") move(focus + 1);
    else if (e.key === "ArrowUp") move(focus - 1);
    else if (e.key === "Home") move(0);
    else if (e.key === "End") move(flat.length - 1);
    else if (e.key === "ArrowRight" && c) {
      if (c.expanded === false) tree.setExpanded(c.key, true);
      else if (c.expanded) move(focus + 1);
    } else if (e.key === "ArrowLeft" && c) {
      if (c.expanded) tree.setExpanded(c.key, false);
      else if (c.depth > 0) {
        // Up to the parent node.
        for (let i = focus - 1; i >= 0; i--) {
          const p = flat[i].cal;
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
            <ul>
              {g.entries.map((e) => {
                n += 1;
                const i = n;
                const c = e.cal;
                return (
                  <li key={e.key}>
                    <button
                      data-entry={i}
                      data-cal={c?.key}
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
                            tree.setExpanded(c.key, !c.expanded);
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
