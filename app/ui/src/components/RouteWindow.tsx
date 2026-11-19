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
  takenLine,
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
  // The camera's clock on Eastern time, changed for daylight saving.
  const [eastern, setEastern] = useState<boolean | null>(null);
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
      .aircraftRouteStatus()
      .then((r) => live && setEastern(r.eastern))
      .catch(() => live && setEastern(false));
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
  const refresh = () => {
    setStopped(false);
    setTick((n) => n + 1);
  };
  // Ask OpenSky once more about these photos (answers kept are forgotten).
  const again = (which: string[]) => {
    setProblem(null);
    setAdded((m) => {
      const n = new Map(m);
      for (const id of which) n.delete(id);
      return n;
    });
    api
      .aircraftRouteAgain(which)
      .then(refresh)
      .catch((e) => setProblem(String(e)));
  };
  // Read the camera's clock the other way; photos whose days are not
  // asked about yet are asked.
  const setClock = (on: boolean) => {
    setProblem(null);
    setEastern(on);
    api
      .aircraftSetRouteEastern(on)
      .then(() => api.aircraftRoute(job.selection))
      .then(refresh)
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
        className="modal route-window"
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
        <div className="rt-bar">
          <span role="status" data-testid="rt-progress" className="grow">
            {review
              ? routeProgress(review.pending, review.items.length, stopped)
              : problem
                ? ""
                : "Starting…"}
          </span>
          {asking && ids && (
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
          )}
          {ids && ids.length > 1 && !asking && (
            <button onClick={() => again(ids)} data-testid="rt-again-all">
              Ask again for all
            </button>
          )}
        </div>
        <label
          className="check rt-clock"
          title="For a camera whose clock shows Eastern time and is changed for daylight saving, but which marks every photo UTC−05:00: from the second Sunday of March to the first Sunday of November its photos are read as UTC−04:00. Photos without a time zone are read as Eastern time too; other time zones are kept."
        >
          <input
            type="checkbox"
            checked={eastern ?? false}
            disabled={eastern === null}
            onChange={(e) => setClock(e.target.checked)}
            data-testid="rt-eastern"
          />
          Camera clock on Eastern time, changed for daylight saving (photos marked UTC−05:00)
        </label>
        {problem && (
          <p className="error" role="alert" data-testid="rt-problem">
            {problem}
          </p>
        )}
        <ul className="rt-list">
          {(review?.items ?? []).map((item) => {
            const line = stateLine(item);
            const taken = takenLine(item);
            const done = added.get(item.id);
            const pickIdx = chosen.get(item.id) ?? (item.state === "several" ? -1 : 0);
            const c = item.choices[pickIdx];
            const thumb = item.thumb ? fileUrl(item.thumb) : null;
            const canAsk = item.registration !== null && item.state !== "waiting";
            return (
              <li key={item.id} className="rt-photo" data-testid="rt-photo">
                {thumb ? (
                  <img src={thumb} alt="" className="rt-thumb" loading="lazy" />
                ) : (
                  <span className="rt-thumb" aria-hidden={true} />
                )}
                <div className="rt-body">
                  <div className="rt-head">
                    <strong className="rt-name">{item.name ?? item.id}</strong>
                    {item.registration && (
                      <span className="rt-reg" data-testid="rt-reg">
                        {item.registration}
                      </span>
                    )}
                  </div>
                  {taken && (
                    <p className="muted rt-taken" data-testid="rt-taken">
                      {taken}
                    </p>
                  )}
                  {line && (
                    <p className="rt-state" data-testid="rt-state">
                      {line}
                    </p>
                  )}
                  {item.state === "several" && (
                    <fieldset className="rt-several" data-testid="rt-several">
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
                    <div className="rt-flight">
                      <p className="rt-route" data-testid="rt-route">
                        {routeLine(c)}
                      </p>
                      <p data-testid="rt-number">{flightLine(c)}</p>
                      <p className="muted" data-testid="rt-when">
                        {whenLine(c)}
                      </p>
                      <div className="rt-marks">
                        {c.keywords.map((k) => (
                          <label key={k} className="check rt-mark">
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
                    </div>
                  )}
                  {item.has.length > 0 && !done && (
                    <p className="muted" data-testid="rt-has">
                      Has already: {item.has.map(keywordLabel).join(", ")}
                    </p>
                  )}
                  {done && (
                    <p className="rt-added" data-testid="rt-added">
                      Added: {done.map(keywordLabel).join(", ")}
                    </p>
                  )}
                  <div className="rt-actions">
                    {!done && c && (
                      <button
                        className="primary"
                        onClick={() => accept(item)}
                        data-testid="rt-accept"
                      >
                        Accept
                      </button>
                    )}
                    {canAsk && (
                      <button
                        onClick={() => again([item.id])}
                        title="Ask OpenSky again for this photo's flights (the answer kept is forgotten)"
                        data-testid="rt-again"
                      >
                        Ask again
                      </button>
                    )}
                  </div>
                </div>
              </li>
            );
          })}
        </ul>
        <p className="muted rt-note">
          OpenSky's times are in UTC. A flight is offered when it took off or landed within three
          hours of the photo; an airport OpenSky did not see is "Not known".
        </p>
        <div className="modal-foot">
          <span className="grow" />
          <button onClick={close} data-testid="rt-close">
            Close
          </button>
        </div>
      </div>
    </div>
  );
}
