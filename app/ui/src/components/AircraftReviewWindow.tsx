// Read aircraft's review window (ADR-041). Every Read aircraft (the photos
// picked, a folder, dates) opens it: reading starts in the background and
// each photo appears as it is read. Three sliders set how sure a
// registration, an airline and a painted type must be to be offered; moving
// them re-sorts at once (what was read is kept; nothing is read again) and
// is remembered. The user unticks mistakes and accepts or dismisses each
// photo, or accepts all shown. Nothing becomes a keyword until accepted.

import { useCallback, useEffect, useRef, useState } from "react";

import {
  api,
  fileUrl,
  type AircraftConfidenceDto,
  type AircraftReviewDto,
  type AircraftReviewItemDto,
  type SamePlaneDto,
} from "../api";
import {
  alsoRead,
  isTicked,
  KINDS,
  lookupLine,
  MIN_PCT,
  parts,
  progressLine,
  secondsLabel,
  sourceOf,
  ticked,
  tickedNeighbours,
  waitingCount,
} from "../aircraftReview";
import * as fmt from "../format";
import { useUi } from "../state";
import type { AircraftJob } from "../uiModel";

/** How often to look again while photos are being read. */
const POLL_MS = 2000;
/** Photos shown at once (Show more adds as many again). */
const PAGE = 100;

/** Start reading `job`; the photos it covers. */
async function start(job: AircraftJob): Promise<string[]> {
  if ("selection" in job) return (await api.aircraftRead(job.selection)).ids;
  if ("volume" in job) {
    await api.aiAnalyzeFolder(job.volume, job.folder, "aircraft");
    return api.aircraftTargetIds({ volume: job.volume, folder: job.folder });
  }
  await api.aiAnalyzeRange(job.from, job.to, "aircraft");
  return api.aircraftTargetIds({ from: job.from, to: job.to });
}

function stop(job: AircraftJob, ids: string[]): Promise<void> {
  if ("selection" in job) return api.aircraftReadStop(ids);
  if ("volume" in job) return api.aiFolderStop(job.volume, job.folder, "aircraft");
  return api.aiRangeStop(job.from, job.to, "aircraft");
}

function Photo({
  item,
  toggled,
  toggledNear,
  onToggle,
  onToggleNear,
  onAccept,
  onDismiss,
  busy,
  sameShotS,
}: {
  item: AircraftReviewItemDto;
  toggled: Set<string> | undefined;
  toggledNear: Set<string> | undefined;
  onToggle: (keyword: string) => void;
  onToggleNear: (id: string) => void;
  onAccept: () => void;
  onDismiss: () => void;
  busy: boolean;
  sameShotS: number;
}) {
  const thumb = fileUrl(item.thumb);
  const also = alsoRead(item);
  const done = item.review !== "waiting";
  const looked = lookupLine(item);
  const near = tickedNeighbours(item, toggledNear);
  return (
    <li className={`ar-photo ${item.review}`} data-testid="ar-photo">
      {thumb ? (
        <img src={thumb} alt="" className="ar-thumb" loading="lazy" />
      ) : (
        <span className="ar-thumb" aria-hidden={true} />
      )}
      <div className="ar-body">
        <div className="ar-name">
          <strong>{item.name ?? "Photo"}</strong>
          {!item.fullSize && <span className="muted"> (read from the thumbnail)</span>}
          {done && (
            <span className="ar-state" data-testid="ar-state">
              {item.review === "accepted" ? "Accepted" : "Dismissed"}
            </span>
          )}
        </div>
        {item.offered.length === 0 ? (
          <p className="muted" data-testid="ar-nothing">
            Nothing at this confidence.
          </p>
        ) : (
          <ul className="ar-marks">
            {item.offered.map((k) => {
              const [kind, name] = parts(k);
              return (
                <li key={k}>
                  <label>
                    <input
                      type="checkbox"
                      checked={isTicked(item, k, toggled)}
                      disabled={done || busy}
                      onChange={() => onToggle(k)}
                      data-testid="ar-mark"
                    />{" "}
                    <span className="muted">{kind}</span> {name}{" "}
                    <span className="muted">{sourceOf(item, k)}</span>
                  </label>
                </li>
              );
            })}
          </ul>
        )}
        {looked && (
          <p className="muted ar-lookup" data-testid="ar-lookup">
            Registry (adsbdb): {looked}
          </p>
        )}
        {also.length > 0 && (
          <p className="muted ar-also" data-testid="ar-also">
            Also read, below your confidence: {also.map((a) => `${a.label} ${a.pct}%`).join(", ")}
          </p>
        )}
        {!done && item.neighbours.length > 0 && (
          <div className="ar-near" data-testid="ar-near">
            <span className="muted">
              Taken within ±{sameShotS} s by the same camera and lens (ticked ones get the same
              keywords):
            </span>
            <ul>
              {item.neighbours.map((n) => {
                const nt = fileUrl(n.thumb);
                return (
                  <li key={n.id}>
                    <label>
                      <input
                        type="checkbox"
                        checked={near.includes(n.id)}
                        disabled={busy}
                        onChange={() => onToggleNear(n.id)}
                        data-testid="ar-near-photo"
                      />{" "}
                      {nt && <img src={nt} alt="" className="ar-near-thumb" loading="lazy" />}{" "}
                      {n.name ?? "Photo"}{" "}
                      <span className="muted">
                        {secondsLabel(n.seconds)},{" "}
                        {n.similarityPct === null
                          ? "not compared yet"
                          : `${n.similarityPct}% alike`}
                      </span>
                    </label>
                  </li>
                );
              })}
            </ul>
          </div>
        )}
      </div>
      {!done && (
        <div className="ar-actions">
          <button onClick={onAccept} disabled={busy} data-testid="ar-accept">
            {ticked(item, toggled).length
              ? near.length
                ? `Accept (and ${near.length} more)`
                : "Accept"
              : "Nothing to keep"}
          </button>
          <button onClick={onDismiss} disabled={busy} data-testid="ar-dismiss">
            Dismiss
          </button>
        </div>
      )}
    </li>
  );
}

/** Other photos of the same registration, offered what they lack. */
interface SamePlane {
  registration: string;
  photos: SamePlaneDto[];
}

export function AircraftReviewWindow() {
  const { s, dispatch } = useUi();
  const job = s.aircraftReview;
  const [ids, setIds] = useState<string[] | null>(null);
  const [review, setReview] = useState<AircraftReviewDto | null>(null);
  const [conf, setConf] = useState<AircraftConfidenceDto | null>(null);
  const [toggled, setToggled] = useState<Map<string, Set<string>>>(new Map());
  const [toggledNear, setToggledNear] = useState<Map<string, Set<string>>>(new Map());
  const [same, setSame] = useState<SamePlane[]>([]);
  const [all, setAll] = useState(false);
  const [shown, setShown] = useState(PAGE);
  const [stopped, setStopped] = useState(false);
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const box = useRef<HTMLDivElement>(null);
  const close = useCallback(() => dispatch({ t: "aircraftReview", job: null }), [dispatch]);

  // A new job: start reading.
  useEffect(() => {
    setIds(null);
    setReview(null);
    setToggled(new Map());
    setToggledNear(new Map());
    setSame([]);
    setStopped(false);
    setShown(PAGE);
    setProblem(null);
    if (!job) return;
    box.current?.focus();
    let live = true;
    start(job)
      .then((ids) => live && setIds(ids))
      .catch((e) => live && setProblem(String(e)));
    return () => {
      live = false;
    };
  }, [job]);

  // What has been read, at the window's confidence; again while reading.
  useEffect(() => {
    if (!ids) return;
    let live = true;
    const t = window.setTimeout(
      () => {
        api
          .aircraftReview(ids, conf)
          .then((r) => {
            if (!live) return;
            setReview(r);
            if (!conf) setConf(r.confidence);
          })
          .catch((e) => live && setProblem(String(e)));
      },
      review ? (review.pending > 0 && !stopped ? POLL_MS : 150) : 0,
    );
    return () => {
      live = false;
      window.clearTimeout(t);
    };
    // `review` restarts the poll after each answer.
  }, [ids, conf, review, stopped]);

  // Remember the sliders (a moment after they stop moving).
  const saved = useRef<string | null>(null);
  useEffect(() => {
    if (!conf) return;
    const key = JSON.stringify(conf);
    if (saved.current === null) {
      saved.current = key;
      return;
    }
    if (saved.current === key) return;
    const t = window.setTimeout(() => {
      saved.current = key;
      api.aircraftSetConfidence(conf).catch(() => undefined);
    }, 600);
    return () => window.clearTimeout(t);
  }, [conf]);

  if (!job) return null;

  const items = (review?.items ?? []).filter((i) => all || i.review === "waiting");
  const setState = (id: string, state: string) =>
    setReview((r) =>
      r ? { ...r, items: r.items.map((i) => (i.id === id ? { ...i, review: state } : i)) } : r,
    );
  const flip = (set: typeof setToggled) => (id: string, k: string) =>
    set((m) => {
      const n = new Map(m);
      const one = new Set(n.get(id));
      if (one.has(k)) one.delete(k);
      else one.add(k);
      n.set(id, one);
      return n;
    });
  const toggle = flip(setToggled);
  const toggleNear = flip(setToggledNear);
  const accept = async (item: AircraftReviewItemDto) => {
    const keep = ticked(item, toggled.get(item.id));
    const near = keep.length ? tickedNeighbours(item, toggledNear.get(item.id)) : [];
    await api.aircraftAccept(item.id, keep, near);
    setState(item.id, keep.length ? "accepted" : "dismissed");
    // The same aircraft's other photos that lack its airline or type.
    const reg = keep.find((k) => parts(k)[0] === "Registration");
    if (reg && keep.some((k) => ["Airline", "Type"].includes(parts(k)[0]))) {
      const photos = (await api.aircraftSamePlane(item.id, keep)).filter(
        (p) => !near.includes(p.id),
      );
      if (photos.length)
        setSame((s) => [
          ...s.filter((x) => x.registration !== parts(reg)[1]),
          { registration: parts(reg)[1], photos },
        ]);
    }
  };
  const run = (f: () => Promise<void>) => {
    setBusy(true);
    setProblem(null);
    f()
      .then(() => dispatch({ t: "edited" }))
      .catch((e) => setProblem(String(e)))
      .finally(() => setBusy(false));
  };
  const acceptAll = () =>
    run(async () => {
      for (const item of items) if (item.review === "waiting") await accept(item);
    });
  const waiting = review ? waitingCount(review) : 0;

  return (
    <div className="modal-back" onMouseDown={(e) => e.target === e.currentTarget && close()}>
      <div
        className="modal aircraft-review"
        role="dialog"
        aria-modal="true"
        aria-labelledby="ar-title"
        tabIndex={-1}
        ref={box}
        data-testid="aircraft-review"
        onKeyDown={(e) => {
          if (e.key === "Escape") close();
          e.stopPropagation();
        }}
      >
        <h2 id="ar-title">Read aircraft: {job.title}</h2>

        <p role="status" data-testid="ar-progress">
          {review && review.pending > 0 && !stopped && (
            <>
              <span className="spinner" aria-hidden />{" "}
            </>
          )}
          {review ? progressLine(review, stopped) : problem ? "" : "Starting…"}
          {review && review.pending > 0 && !stopped && ids && (
            <>
              {" "}
              <button
                onClick={() =>
                  stop(job, ids)
                    .then(() => setStopped(true))
                    .catch((e) => setProblem(String(e)))
                }
                data-testid="ar-stop"
              >
                Stop
              </button>
            </>
          )}
        </p>

        {conf && (
          <fieldset className="ar-sliders">
            <legend>How sure must a mark be to be offered?</legend>
            {KINDS.map(({ key, label }) => (
              <label key={key} className="ar-slider">
                <span>{label}</span>
                <input
                  type="range"
                  min={MIN_PCT}
                  max={100}
                  step={1}
                  value={conf[key]}
                  onChange={(e) => setConf({ ...conf, [key]: Number(e.target.value) })}
                  data-testid={`ar-${key}`}
                  aria-valuetext={`${conf[key]}%`}
                />
                <output>{conf[key]}%</output>
              </label>
            ))}
            <label className="ar-slider">
              <span>Same shot</span>
              <input
                type="range"
                min={1}
                max={60}
                step={1}
                value={conf.sameShotS}
                onChange={(e) => setConf({ ...conf, sameShotS: Number(e.target.value) })}
                data-testid="ar-sameShotS"
                aria-valuetext={`${conf.sameShotS} seconds`}
              />
              <output>±{conf.sameShotS} s</output>
            </label>
            <p className="muted">
              Lower finds more, with more mistakes; higher finds less, more surely. What was read is
              kept, so moving these re-sorts at once and nothing is read again. Same shot: photos
              this many seconds either side, by the same camera and lens, that look alike (Tag with
              AI uses it too). Remembered for next time.
            </p>
          </fieldset>
        )}

        {review && (
          <div className="row ar-filter">
            <label>
              <input
                type="checkbox"
                checked={all}
                onChange={(e) => setAll(e.target.checked)}
                data-testid="ar-all"
              />{" "}
              Show accepted and dismissed too
            </label>
            <span className="muted" data-testid="ar-waiting">
              {fmt.plural(waiting, "photo")} waiting for review
            </span>
          </div>
        )}

        {same.map((sp) => {
          const lacks = [...new Set(sp.photos.flatMap((p) => p.missing.map((k) => parts(k)[1])))];
          return (
            <div className="ar-same" key={sp.registration} data-testid="ar-same">
              <p>
                <strong>
                  {fmt.plural(sp.photos.length, "other photo")} of {sp.registration}
                </strong>{" "}
                lack {lacks.join(", ")}.
              </p>
              <button
                disabled={busy}
                onClick={() =>
                  run(async () => {
                    for (const p of sp.photos) await api.aircraftAddKeywords([p.id], p.missing);
                    setSame((s) => s.filter((x) => x.registration !== sp.registration));
                  })
                }
                data-testid="ar-same-add"
              >
                Add to {fmt.plural(sp.photos.length, "photo")}
              </button>{" "}
              <button
                onClick={() => setSame((s) => s.filter((x) => x.registration !== sp.registration))}
                data-testid="ar-same-skip"
              >
                Not now
              </button>
            </div>
          );
        })}

        <ul className="ar-list">
          {items.slice(0, shown).map((item) => (
            <Photo
              key={item.id}
              item={item}
              toggled={toggled.get(item.id)}
              toggledNear={toggledNear.get(item.id)}
              onToggle={(k) => toggle(item.id, k)}
              onToggleNear={(n) => toggleNear(item.id, n)}
              onAccept={() => run(() => accept(item))}
              onDismiss={() =>
                run(() => api.aircraftDismiss(item.id).then(() => setState(item.id, "dismissed")))
              }
              busy={busy}
              sameShotS={conf?.sameShotS ?? 10}
            />
          ))}
        </ul>
        {items.length > shown && (
          <button onClick={() => setShown((n) => n + PAGE)} data-testid="ar-more">
            Show more ({fmt.count(items.length - shown)})
          </button>
        )}
        {review && items.length === 0 && review.read > 0 && (
          <p className="muted" data-testid="ar-none-waiting">
            Every photo read so far has been reviewed.
          </p>
        )}

        {problem && (
          <p className="error" role="alert" data-testid="ar-problem">
            {problem}
          </p>
        )}
        {problem?.includes("Settings") && (
          <button
            onClick={() => {
              dispatch({ t: "screen", screen: "settings" });
              close();
            }}
            data-testid="ar-settings"
          >
            Open Settings
          </button>
        )}

        <div className="modal-foot">
          <button
            onClick={acceptAll}
            disabled={busy || !items.some((i) => i.review === "waiting")}
            title="Accept what is ticked for every photo shown that is waiting for review (a photo with nothing ticked is dismissed)"
            data-testid="ar-accept-all"
          >
            Accept all shown
          </button>
          <span className="grow" />
          <button onClick={close} data-testid="ar-close">
            Close
          </button>
        </div>
        <p className="muted">
          Reading goes on when this is closed; open Read aircraft again on the same photos to review
          them.
        </p>
      </div>
    </div>
  );
}
