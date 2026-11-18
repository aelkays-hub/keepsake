// Add route (ADR-044): right-click photos → Add route. Asks OpenSky (in
// the background) for the flights of each photo's aircraft that day, and
// offers the one nearest the photo: from and to (IATA codes and countries)
// and the flight number. Nothing becomes a keyword until accepted.

import { useCallback, useEffect, useRef, useState } from "react";

import { api, fileUrl } from "../api";
import type { RouteItemDto } from "../bindings/RouteItemDto";
import type { RouteReviewDto } from "../bindings/RouteReviewDto";
import {
  flightLine,
  keywordLabel,
  routeLine,
  routeProgress,
  stateLine,
  tickedKeywords,
  whenLine,
} from "../aircraftRoute";
import { useUi } from "../state";

const POLL_MS = 1000;

export function RouteWindow() {
  const { s, dispatch } = useUi();
  const job = s.route;
  const [ids, setIds] = useState<string[] | null>(null);
  const [review, setReview] = useState<RouteReviewDto | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [stopped, setStopped] = useState(false);
  const [tick, setTick] = useState(0);
  // Per photo: the flight chosen (index) and the keywords unticked.
  const [chosen, setChosen] = useState<Map<string, number>>(new Map());
  const [unticked, setUnticked] = useState<Map<string, Set<string>>>(new Map());
  const [added, setAdded] = useState<Map<string, string[]>>(new Map());
  const box = useRef<HTMLDivElement>(null);
  const close = useCallback(() => dispatch({ t: "route", job: null }), [dispatch]);

  useEffect(() => {
    setIds(null);
    setReview(null);
    setProblem(null);
    setStopped(false);
    setChosen(new Map());
    setUnticked(new Map());
    setAdded(new Map());
    if (!job) return;
    box.current?.focus();
    let live = true;
    api
      .aircraftRoute(job.selection)
      .then((r) => live && setIds(r.ids))
      .catch((e) => live && setProblem(String(e)));
    return () => {
      live = false;
    };
  }, [job]);

  useEffect(() => {
    if (!ids) return;
    let live = true;
    api
      .aircraftRouteReview(ids)
      .then((r) => live && setReview(r))
      .catch((e) => live && setProblem(String(e)));
    return () => {
      live = false;
    };
  }, [ids, tick]);
  const asking = Boolean(review && review.pending > 0 && !stopped);
  useEffect(() => {
    if (!asking) return;
    const t = window.setTimeout(() => setTick((n) => n + 1), POLL_MS);
    return () => window.clearTimeout(t);
  }, [asking, review]);

  if (!job) return null;

  const accept = (item: RouteItemDto) => {
    const c = item.choices[chosen.get(item.id) ?? (item.state === "several" ? -1 : 0)];
    if (!c) return;
    const keep = tickedKeywords(c, unticked.get(item.id));
    setProblem(null);
    api
      .aircraftRouteAccept(item.id, keep)
      .then(() => {
        setAdded((m) => new Map(m).set(item.id, keep));
        dispatch({ t: "edited" });
      })
      .catch((e) => setProblem(String(e)));
  };
  const flip = (id: string, k: string) =>
    setUnticked((m) => {
      const n = new Map(m);
      const one = new Set(n.get(id));
      if (one.has(k)) one.delete(k);
      else one.add(k);
      n.set(id, one);
      return n;
    });

  return (
    <div className="modal-back" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div
        className="modal aircraft-review"
        role="dialog"
        aria-modal="true"
        aria-labelledby="rt-title"
        tabIndex={-1}
        ref={box}
        data-testid="route-window"
        onKeyDown={(e) => {
          if (e.key === "Escape") close();
          e.stopPropagation();
        }}
      >
        <h2 id="rt-title">Add route: {job.title}</h2>
        <p role="status" data-testid="rt-progress">
          {review
            ? routeProgress(review.pending, review.items.length, stopped)
            : problem
              ? ""
              : "Starting…"}
          {asking && ids && (
            <>
              {" "}
              <button
                onClick={() =>
                  api
                    .aircraftRouteStop(ids)
                    .then(() => setStopped(true))
                    .catch((e) => setProblem(String(e)))
                }
                data-testid="rt-stop"
              >
                Stop
              </button>
            </>
          )}
        </p>
        {problem && (
          <p className="error" role="alert" data-testid="rt-problem">
            {problem}
          </p>
        )}
        <ul className="ar-photos">
          {(review?.items ?? []).map((item) => {
            const line = stateLine(item);
            const done = added.get(item.id);
            const pickIdx = chosen.get(item.id) ?? (item.state === "several" ? -1 : 0);
            const c = item.choices[pickIdx];
            return (
              <li key={item.id} className="ar-photo" data-testid="rt-photo">
                {item.thumb && <img src={fileUrl(item.thumb) ?? undefined} alt="" width={160} />}
                <div className="grow">
                  <strong>{item.name ?? item.id}</strong>
                  {item.registration && <span className="muted"> · {item.registration}</span>}
                  {item.taken && (
                    <span className="muted">
                      {" "}
                      · {item.taken.slice(0, 16).replace("T", " ")} UTC
                    </span>
                  )}
                  {line && (
                    <p className="muted" data-testid="rt-state">
                      {line}
                    </p>
                  )}
                  {item.state === "several" && (
                    <fieldset data-testid="rt-several">
                      <legend>Two flights are as near the photo's time: which is it?</legend>
                      {item.choices.map((ch, i) => (
                        <label key={i} className="check">
                          <input
                            type="radio"
                            name={`rt-${item.id}`}
                            checked={pickIdx === i}
                            onChange={() => setChosen((m) => new Map(m).set(item.id, i))}
                            data-testid="rt-choice"
                          />
                          {flightLine(ch)}: {routeLine(ch)}. {whenLine(ch)}.
                        </label>
                      ))}
                    </fieldset>
                  )}
                  {c && (
                    <>
                      <p data-testid="rt-route">
                        {routeLine(c)} · {flightLine(c)}
                      </p>
                      <p className="muted" data-testid="rt-when">
                        {whenLine(c)}
                      </p>
                      <div className="ar-marks">
                        {c.keywords.map((k) => (
                          <label key={k} className="check">
                            <input
                              type="checkbox"
                              checked={!unticked.get(item.id)?.has(k)}
                              disabled={Boolean(done)}
                              onChange={() => flip(item.id, k)}
                              data-testid="rt-keyword"
                            />
                            {keywordLabel(k)}
                          </label>
                        ))}
                      </div>
                    </>
                  )}
                  {item.has.length > 0 && !done && (
                    <p className="muted" data-testid="rt-has">
                      Has already: {item.has.map(keywordLabel).join(", ")}
                    </p>
                  )}
                  {done ? (
                    <p data-testid="rt-added">Added: {done.map(keywordLabel).join(", ")}</p>
                  ) : (
                    c && (
                      <button onClick={() => accept(item)} data-testid="rt-accept">
                        Accept
                      </button>
                    )
                  )}
                </div>
              </li>
            );
          })}
        </ul>
        <div className="modal-foot">
          <span className="grow" />
          <button onClick={close} data-testid="rt-close">
            Close
          </button>
        </div>
        <p className="muted">
          Times are OpenSky's, in UTC. A flight is offered when it took off or landed within three
          hours of the photo; an airport OpenSky did not see is "Not known".
        </p>
      </div>
    </div>
  );
}
