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
  const [state, setState] = useState<RouteFillDto | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [tick, setTick] = useState(0);
  const box = useRef<HTMLDivElement>(null);
  const close = useCallback(() => dispatch({ t: "routeFill", job: null }), [dispatch]);

  useEffect(() => {
    setIds(null);
    setState(null);
    setProblem(null);
    if (!job) return;
    box.current?.focus();
    let live = true;
    api
      .aircraftRouteFill(job.selection)
      .then((r) => live && setIds(r))
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
          From the route keywords a photo has — Aircraft|Flight (AT210 or RAM210), Aircraft|From,
          Aircraft|To — the missing ones are added: a flight number&apos;s airports (from adsbdb),
          the flight number from the aircraft&apos;s flights Add route found that day, and the
          countries. What a photo has is never changed.
        </p>
        <p role="status" data-testid="rf-progress">
          {!state
            ? problem
              ? ""
              : "Starting…"
            : state.pending > 0
              ? `Filling in… ${state.items.length - state.pending} of ${state.items.length} done.`
              : `Done: ${state.items.filter((i) => i.added.length > 0).length} of ${state.items.length} photos got keywords.`}
        </p>
        {problem && (
          <p className="error" role="alert" data-testid="rf-problem">
            {problem}
          </p>
        )}
        <ul className="rt-list">
          {(state?.items ?? []).map((item) => (
            <li key={item.id} data-testid="rf-item">
              <strong>{item.name}</strong>
              {item.state === "waiting" && <p className="muted">Waiting…</p>}
              {item.added.length > 0 && (
                <p data-testid="rf-added">
                  Added:{" "}
                  {item.added.map((k) => (
                    <span key={k} className="chip">
                      {keywordLabel(k)}
                    </span>
                  ))}
                </p>
              )}
              {item.message && (
                <p className="muted" data-testid="rf-message">
                  {item.message}
                </p>
              )}
            </li>
          ))}
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
