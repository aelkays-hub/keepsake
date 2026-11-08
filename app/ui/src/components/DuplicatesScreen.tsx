// Duplicates: near-identical photos and videos taken on the same day
// (bursts, re-shoots, the same shot saved twice), found by the on-device
// AI for the dates the user asked about (right-click a year, month or day
// in the sidebar → Find duplicates), or every analysed day. Pick the one to keep; the others are marked "not kept" and can be
// hidden from the library with the toolbar's "Hide not kept". Nothing is
// moved or deleted.
//
// Keyboard: ←/→ move between items of a group, ↑/↓ between groups,
// K keeps the focused item, N marks the group "not duplicates",
// U undoes the group's decision.

import { useCallback, useEffect, useMemo, useState } from "react";

import {
  api,
  fileUrl,
  type AiRangeStatus,
  type AiStatus,
  type DuplicateGroupDto,
  type DuplicateSummaryDto,
} from "../api";
import { aiProgress, dayTitle, memberBadge, rangeProgress } from "../ai";
import * as fmt from "../format";
import { useUi } from "../state";

function timeOf(ms: number | null): string {
  if (ms === null) return "";
  return new Date(ms).toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export function DuplicatesScreen() {
  const { s, dispatch } = useUi();
  const range = s.dupRange;
  const [status, setStatus] = useState<AiStatus | null>(null);
  const [rangeSt, setRangeSt] = useState<AiRangeStatus | null>(null);
  const [sum, setSum] = useState<DuplicateSummaryDto | null>(null);
  const [day, setDay] = useState<string | null>(null);
  const [groups, setGroups] = useState<DuplicateGroupDto[] | null>(null);
  const [focus, setFocus] = useState<{ g: number; m: number }>({ g: 0, m: 0 });
  const [problem, setProblem] = useState<string | null>(null);

  const loadSummary = useCallback(() => {
    api.aiStatus().then(setStatus).catch(console.error);
    if (range)
      api.aiRangeStatus(range.from, range.to, "duplicates").then(setRangeSt).catch(console.error);
    else setRangeSt(null);
    api
      .aiDuplicates(range?.from ?? null, range?.to ?? null)
      .then((s) => {
        setSum(s);
        setDay((d) => (d && s.days.some((x) => x.date === d) ? d : (s.days[0]?.date ?? null)));
      })
      .catch((e) => setProblem(String(e)));
  }, [range]);
  useEffect(loadSummary, [loadSummary]);

  const loadDay = useCallback(() => {
    if (!day) {
      setGroups(null);
      return;
    }
    api
      .aiDuplicateDay(day)
      .then(setGroups)
      .catch((e) => setProblem(String(e)));
  }, [day]);
  useEffect(() => {
    setFocus({ g: 0, m: 0 });
    loadDay();
  }, [loadDay]);
  // While the dates are being analysed, look again every few seconds.
  const busy = (rangeSt?.pending ?? 0) > 0;
  useEffect(() => {
    if (!busy) return;
    const t = window.setInterval(() => {
      loadSummary();
      loadDay();
    }, 3000);
    return () => window.clearInterval(t);
  }, [busy, loadSummary, loadDay]);

  const after = (p: Promise<unknown>) => {
    setProblem(null);
    p.then(() => {
      loadDay();
      loadSummary();
      dispatch({ t: "edited" });
    }).catch((e) => setProblem(String(e)));
  };
  const keep = (g: DuplicateGroupDto, id: string) =>
    after(
      api.aiPickKeeper(
        id,
        g.members.map((m) => m.id),
      ),
    );
  const notDuplicates = (g: DuplicateGroupDto) =>
    after(
      api.aiSetChoice(
        g.members.map((m) => m.id),
        "not_duplicate",
      ),
    );
  const undo = (g: DuplicateGroupDto) =>
    after(
      api.aiSetChoice(
        g.members.map((m) => m.id),
        null,
      ),
    );

  const onKey = (e: React.KeyboardEvent) => {
    if (!groups?.length) return;
    const g = groups[Math.min(focus.g, groups.length - 1)];
    const k = e.key.toLowerCase();
    if (e.key === "ArrowRight")
      setFocus({ g: focus.g, m: Math.min(focus.m + 1, g.members.length - 1) });
    else if (e.key === "ArrowLeft") setFocus({ g: focus.g, m: Math.max(focus.m - 1, 0) });
    else if (e.key === "ArrowDown") setFocus({ g: Math.min(focus.g + 1, groups.length - 1), m: 0 });
    else if (e.key === "ArrowUp") setFocus({ g: Math.max(focus.g - 1, 0), m: 0 });
    else if (k === "k" && g.members[focus.m]) keep(g, g.members[focus.m].id);
    else if (k === "n") notDuplicates(g);
    else if (k === "u") undo(g);
    else return;
    e.preventDefault();
  };

  const dayInfo = useMemo(() => sum?.days.find((d) => d.date === day) ?? null, [sum, day]);

  if (status && !status.enabled)
    return (
      <section className="screen" aria-label="Duplicates" data-testid="duplicates-screen">
        <h1 className="heading">Duplicates</h1>
        <div className="card">
          <p>
            Duplicates are found by the on-device AI, which is off. Turn it on in Settings, then
            right-click a year, month or day in the sidebar and choose Find duplicates. It runs on
            this computer.
          </p>
          <button
            className="primary"
            onClick={() => dispatch({ t: "screen", screen: "settings" })}
            data-testid="dup-open-settings"
          >
            Open Settings
          </button>
        </div>
      </section>
    );

  return (
    <section className="screen" aria-label="Duplicates" data-testid="duplicates-screen">
      <h1 className="heading" data-testid="dup-title">
        Duplicates{range ? ` · ${range.title}` : ""}
      </h1>
      <p className="muted">
        Near-identical RAW photos taken on the same day (a JPEG with the same name as a RAW is an
        edited copy, so JPEG-only photos and videos are not compared). Pick the one to keep: the
        others are marked “not kept” (the toolbar’s Hide not kept leaves them out of the library).
        No file is moved or deleted.
      </p>
      <div className="row">
        <p className="muted" role="status" data-testid="dup-ai-status">
          {range && rangeSt
            ? rangeProgress(rangeSt)
            : status
              ? `${aiProgress(status)} Right-click a year, month or day in the sidebar → Find duplicates.`
              : ""}
        </p>
        {range && (
          <button onClick={() => dispatch({ t: "duplicates", range: null })} data-testid="dup-all">
            All analysed dates
          </button>
        )}
      </div>
      {problem && (
        <p className="error" role="alert">
          {problem}
        </p>
      )}
      {sum && sum.days.length === 0 && (
        <p data-testid="dup-none">
          No duplicates found{busy ? " yet" : range ? " on these dates" : ""}.
        </p>
      )}
      {sum && sum.days.length > 0 && (
        <div className="dup-layout">
          <nav className="dup-days" aria-label="Days with duplicates">
            <p className="muted" data-testid="dup-totals">
              {fmt.plural(sum.groups, "group")}, {fmt.count(sum.undecided)} to decide
            </p>
            <ul>
              {sum.days.map((d) => (
                <li key={d.date}>
                  <button
                    aria-current={d.date === day ? "true" : undefined}
                    onClick={() => setDay(d.date)}
                    data-testid={`dup-day-${d.date}`}
                  >
                    <span>{dayTitle(d.date)}</span>
                    <span className="muted">
                      {d.undecided > 0 ? `${d.undecided} of ${d.groups}` : "done"}
                    </span>
                  </button>
                </li>
              ))}
            </ul>
          </nav>
          <div
            className="dup-groups"
            tabIndex={0}
            onKeyDown={onKey}
            aria-label={
              day
                ? `Groups on ${dayTitle(day)}. Arrows move, K keeps, N not duplicates, U undoes.`
                : undefined
            }
            data-testid="dup-groups"
          >
            {dayInfo && <h2>{dayTitle(dayInfo.date)}</h2>}
            {groups?.map((g, gi) => (
              <div
                key={g.members.map((m) => m.id).join()}
                className={`dup-group${g.decided ? " decided" : ""}`}
                data-testid="dup-group"
              >
                <div className="dup-tiles" role="list">
                  {g.members.map((m, mi) => {
                    const badge = memberBadge(m, g.keeper);
                    const focused = gi === focus.g && mi === focus.m;
                    return (
                      <button
                        key={m.id}
                        role="listitem"
                        className={`card-tile${m.choice === "not_kept" ? " known" : ""}${
                          badge === "Kept" || badge === "Suggested" ? " selected" : ""
                        }${focused ? " focused" : ""}`}
                        onClick={() => {
                          setFocus({ g: gi, m: mi });
                          keep(g, m.id);
                        }}
                        title={`Keep ${m.name}`}
                        data-testid="dup-member"
                      >
                        {m.thumb ? (
                          <img src={fileUrl(m.thumb) ?? undefined} alt={m.name} loading="lazy" />
                        ) : (
                          <span className="ph" />
                        )}
                        <span className="name">
                          {m.name} {timeOf(m.captureMs)}
                          {m.rating > 0 && ` ${"★".repeat(m.rating)}`}
                        </span>
                        <span className="name muted">
                          {m.id === g.keeper
                            ? ""
                            : m.exact
                              ? "Same picture"
                              : `${m.likeness}% alike`}
                          {badge && <strong className="dup-badge"> {badge}</strong>}
                        </span>
                      </button>
                    );
                  })}
                </div>
                <div className="row">
                  <button onClick={() => keep(g, g.keeper)} data-testid="dup-keep-suggested">
                    Keep the {g.decided ? "kept" : "suggested"} one
                  </button>
                  <button onClick={() => notDuplicates(g)} data-testid="dup-not-duplicates">
                    Not duplicates
                  </button>
                  {g.members.some((m) => m.choice !== null) && (
                    <button onClick={() => undo(g)} data-testid="dup-undo">
                      Undo
                    </button>
                  )}
                </div>
              </div>
            ))}
          </div>
        </div>
      )}
    </section>
  );
}
