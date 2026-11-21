// Fill in route keywords (ADR-044 addition): right-click photos → Fill in
// route keywords. From the route keywords each photo has (the flight
// number, From or To), the missing ones are added: a flight number's
// airports (adsbdb), the flight number from the aircraft's flights kept
// that day, and the countries. In the background; this window shows what
// each photo got.

import { useCallback, useEffect, useRef, useState } from "react";

import { api } from "../api";
import type { RouteFillDto } from "../bindings/RouteFillDto";
import { keywordLabel } from "../aircraftRoute";
import { useUi } from "../state";

const POLL_MS = 1000;

export function RouteFillWindow() {
  const { s, dispatch } = useUi();
  const job = s.routeFill;
  const [ids, setIds] = useState<string[] | null>(null);
  // Each photo's route keywords before, and the flight numbers chosen.
  const [had, setHad] = useState<Map<string, string[]>>(new Map());
  const [chosen, setChosen] = useState<Map<string, string>>(new Map());
  const [state, setState] = useState<RouteFillDto | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [tick, setTick] = useState(0);
  const box = useRef<HTMLDivElement>(null);
  const close = useCallback(() => dispatch({ t: "routeFill", job: null }), [dispatch]);

  useEffect(() => {
    setIds(null);
    setState(null);
    setProblem(null);
    setHad(new Map());
    setChosen(new Map());
    if (!job) return;
    box.current?.focus();
    let live = true;
    api
      .aircraftRouteFill(job.selection)
      .then((r) => {
        if (!live) return;
        setHad(new Map(r.items.map((i) => [i.id, i.had])));
        setIds(r.items.map((i) => i.id));
      })
      .catch((e) => live && setProblem(String(e)));
    return () => {
      live = false;
    };
  }, [job]);

  useEffect(() => {
    if (!ids) return;
    let live = true;
    api
      .aircraftRouteFillStatus(ids)
      .then((r) => {
        if (!live) return;
        setState(r);
        if (r.pending === 0) dispatch({ t: "edited" });
      })
      .catch((e) => live && setProblem(String(e)));
    return () => {
      live = false;
    };
  }, [ids, tick, dispatch]);
  const pending = state?.pending ?? 1;
  useEffect(() => {
    if (!ids || pending === 0) return;
    const t = window.setTimeout(() => setTick((n) => n + 1), POLL_MS);
    return () => window.clearTimeout(t);
  }, [ids, pending, state]);

  if (!job) return null;
  // What each photo got: its route keywords now that it did not have.
  const newOf = (id: string, now: string[]) => {
    const before = had.get(id) ?? [];
    const pick = chosen.get(id);
    return [...now.filter((k) => !before.includes(k)), ...(pick ? [pick] : [])];
  };
  const items = state?.items ?? [];
  const got = items.filter((i) => newOf(i.id, i.added).length > 0).length;
  const choose = (id: string, flight: string) => {
    setProblem(null);
    api
      .aircraftRouteFillChoose(id, flight)
      .then((k) => {
        setChosen((m) => new Map(m).set(id, k));
        dispatch({ t: "edited" });
      })
      .catch((e) => setProblem(String(e)));
  };
  return (
    <div className="modal-back" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div
        className="modal route-window"
        role="dialog"
        aria-modal="true"
        aria-labelledby="rf-title"
        tabIndex={-1}
        ref={box}
        data-testid="route-fill-window"
        onKeyDown={(e) => {
          if (e.key === "Escape") close();
          e.stopPropagation();
        }}
      >
        <h2 id="rf-title">Fill in route keywords: {job.title}</h2>
        <p className="muted">
          Each photo gets the route keywords it is missing, worked out from those it has: a flight
          number gives its airports; airports give the flight number of a flight Add route found
          that day; every airport gets its country. What a photo has is never changed.
        </p>
        <p role="status" data-testid="rf-progress">
          {!state
            ? problem
              ? ""
              : "Starting…"
            : state.pending > 0
              ? `Filling in… ${items.length - state.pending} of ${items.length} done.`
              : `Done: ${got} of ${items.length} photos got keywords.`}
        </p>
        {problem && (
          <p className="error" role="alert" data-testid="rf-problem">
            {problem}
          </p>
        )}
        <ul className="rt-list">
          {items.map((item) => {
            const added = newOf(item.id, item.added);
            const before = had.get(item.id) ?? [];
            return (
              <li key={item.id} data-testid="rf-item">
                <strong>{item.name}</strong>
                {before.length > 0 && (
                  <p className="muted" data-testid="rf-had">
                    Had: {before.map(keywordLabel).join(", ")}
                  </p>
                )}
                {item.state === "waiting" && <p className="muted">Waiting…</p>}
                {added.length > 0 && (
                  <p data-testid="rf-added">
                    Added:{" "}
                    {added.map((k) => (
                      <span key={k} className="chip">
                        {keywordLabel(k)}
                      </span>
                    ))}
                  </p>
                )}
                {item.state === "done" && added.length === 0 && (
                  <p className="muted">Nothing was missing.</p>
                )}
                {item.message && !chosen.has(item.id) && (
                  <p className="muted" data-testid="rf-message">
                    {item.message}
                  </p>
                )}
                {item.choices.length > 0 && !chosen.has(item.id) && (
                  <div className="rt-actions" data-testid="rf-choices">
                    {item.choices.map((c) => (
                      <button key={c} onClick={() => choose(item.id, c)} data-testid="rf-choose">
                        Tag {c}
                      </button>
                    ))}
                  </div>
                )}
              </li>
            );
          })}
        </ul>
        <div className="modal-foot">
          <span className="grow" />
          <button onClick={close} data-testid="rf-close">
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
