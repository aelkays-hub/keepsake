// Sidebar: Library, Calendar, Events, Volumes, Imports. Arrow keys move
// between entries (a single tab stop), Enter or click opens one.

import { useEffect, useRef, useState } from "react";

import { api, events, type Sidebar as SidebarData } from "../api";
import * as fmt from "../format";
import { useUi, type Section } from "../state";

interface Entry {
  key: string;
  label: string;
  section: Section;
  count?: number;
  status?: "online" | "offline";
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
  const groups: { title: string; entries: Entry[] }[] = [
    {
      title: "Library",
      entries: [
        { key: "all", label: "All items", section: { kind: "all" } },
        { key: "cal", label: "Calendar", section: { kind: "calendar" } },
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
  const active = Math.max(
    0,
    flat.findIndex((e) => same(e.section, s.section)),
  );
  const [focus, setFocus] = useState(active);
  const nav = useRef<HTMLElement>(null);
  const move = (to: number) => {
    const i = Math.max(0, Math.min(flat.length - 1, to));
    setFocus(i);
    nav.current?.querySelector<HTMLButtonElement>(`[data-entry="${i}"]`)?.focus();
  };
  const onKey = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") move(focus + 1);
    else if (e.key === "ArrowUp") move(focus - 1);
    else if (e.key === "Home") move(0);
    else if (e.key === "End") move(flat.length - 1);
    else return;
    e.preventDefault();
  };
  let n = -1;
  return (
    <nav ref={nav} className="sidebar" aria-label="Library" onKeyDown={onKey} data-testid="sidebar">
      {groups.map((g) =>
        g.entries.length === 0 && g.title !== "Library" ? null : (
          <section key={g.title}>
            <h3>{g.title}</h3>
            <ul>
              {g.entries.map((e) => {
                n += 1;
                const i = n;
                return (
                  <li key={e.key}>
                    <button
                      data-entry={i}
                      tabIndex={i === focus ? 0 : -1}
                      aria-current={same(e.section, s.section) ? "page" : undefined}
                      className={same(e.section, s.section) ? "active" : ""}
                      onClick={() => {
                        setFocus(i);
                        dispatch({ t: "section", section: e.section });
                      }}
                    >
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
