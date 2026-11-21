// Report → Flickr (ADR-046 addition): what is on the Flickr account, read
// from Flickr itself (photos uploaded with Keepsake or any other way), as a
// tree by upload date: year › month › day with counts, the newest open
// first. A day shows Flickr's thumbnails, each opening its Flickr page;
// those uploaded with Keepsake are marked. Read on the first open after
// start and on Refresh; kept in memory only.

import { useCallback, useEffect, useState } from "react";

import { api } from "../api";
import type { FlickrAccountDto } from "../bindings/FlickrAccountDto";
import type { FlickrAccountPhotoDto } from "../bindings/FlickrAccountPhotoDto";
import { dayName, monthName, newestOpen, privacyLabel } from "../flickr";
import * as fmt from "../format";
import { useUi } from "../state";

const POLL_MS = 1000;

export function FlickrReportView() {
  const { dispatch } = useUi();
  const [acc, setAcc] = useState<FlickrAccountDto | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [openKeys, setOpenKeys] = useState<Set<string> | null>(null);
  const [days, setDays] = useState<Map<string, FlickrAccountPhotoDto[]>>(new Map());
  const [tick, setTick] = useState(0);

  const read = useCallback((again: boolean) => {
    setProblem(null);
    if (again) setDays(new Map());
    api
      .flickrAccountRead(again)
      .then(setAcc)
      .catch((e) => setProblem(String(e)));
  }, []);

  // First open after start: read the account (kept for the session).
  useEffect(() => {
    api
      .flickrStatus()
      .then((st) => {
        if (st.signedIn) read(false);
        else
          setAcc({
            signedIn: false,
            username: null,
            reading: false,
            read: 0,
            total: 0,
            error: null,
            readAt: null,
            years: [],
            photos: 0,
          });
      })
      .catch((e) => setProblem(String(e)));
  }, [read]);

  // While reading: how far it has got.
  const reading = acc?.reading ?? false;
  useEffect(() => {
    if (!reading) return;
    const t = window.setTimeout(() => {
      api
        .flickrAccount()
        .then((a) => {
          setAcc(a);
          setTick((n) => n + 1);
        })
        .catch((e) => setProblem(String(e)));
    }, POLL_MS);
    return () => window.clearTimeout(t);
  }, [reading, tick]);

  // Once read: the newest year, month and day open.
  const years = acc?.years;
  useEffect(() => {
    if (years && years.length > 0 && openKeys === null) setOpenKeys(new Set(newestOpen(years)));
  }, [years, openKeys]);

  // An open day's photos.
  useEffect(() => {
    if (!openKeys || reading) return;
    for (const k of openKeys) {
      if (k.length !== 10 || days.has(k)) continue;
      api
        .flickrAccountDay(k)
        .then((p) => setDays((m) => new Map(m).set(k, p)))
        .catch((e) => setProblem(String(e)));
    }
  }, [openKeys, days, reading]);

  const toggle = (k: string) =>
    setOpenKeys((s) => {
      const n = new Set(s ?? []);
      if (n.has(k)) n.delete(k);
      else n.add(k);
      return n;
    });
  const isOpen = (k: string) => openKeys?.has(k) ?? false;
  const openPage = (url: string) => api.flickrOpen(url).catch((e) => setProblem(String(e)));

  if (!acc)
    return problem ? (
      <p className="error" role="alert">
        {problem}
      </p>
    ) : (
      <p className="muted">Reading…</p>
    );
  if (!acc.signedIn)
    return (
      <div className="flickr-report" data-testid="flickr-report">
        <p className="muted" data-testid="flickr-report-signed-out">
          Sign in to Flickr in Settings → Flickr to see what is on your account.{" "}
          <button onClick={() => dispatch({ t: "screen", screen: "settings" })}>
            Open Settings
          </button>
        </p>
      </div>
    );
  return (
    <div className="flickr-report" data-testid="flickr-report">
      <div className="fr-bar">
        <p role="status" data-testid="flickr-report-status">
          {acc.reading
            ? `Reading ${acc.username ?? "your account"} from Flickr… ${fmt.count(acc.read)}${acc.total ? ` of ${fmt.count(acc.total)}` : ""} photos.`
            : acc.readAt
              ? `${fmt.plural(acc.photos, "photo")} on the Flickr account of ${acc.username ?? "?"}, by upload date (read from Flickr at ${acc.readAt}).`
              : ""}
        </p>
        <button
          onClick={() => {
            setOpenKeys(null);
            read(true);
          }}
          disabled={acc.reading}
          data-testid="flickr-report-refresh"
        >
          Refresh from Flickr
        </button>
      </div>
      {(problem ?? acc.error) && (
        <p className="error" role="alert" data-testid="flickr-report-problem">
          {problem ?? acc.error}
        </p>
      )}
      {!acc.reading && acc.readAt && acc.years.length === 0 && (
        <p className="muted" data-testid="flickr-report-none">
          Nothing on the account yet.
        </p>
      )}
      <ul className="fr-tree" data-testid="flickr-tree">
        {acc.years.map((y) => {
          const yk = String(y.year);
          return (
            <li key={yk}>
              <button
                className="fr-node"
                aria-expanded={isOpen(yk)}
                onClick={() => toggle(yk)}
                data-testid="fr-year"
              >
                {y.year} <span className="muted">({fmt.count(y.count)})</span>
              </button>
              {isOpen(yk) && (
                <ul>
                  {y.months.map((m) => (
                    <li key={m.month}>
                      <button
                        className="fr-node"
                        aria-expanded={isOpen(m.month)}
                        onClick={() => toggle(m.month)}
                        data-testid="fr-month"
                      >
                        {monthName(m.month)} <span className="muted">({fmt.count(m.count)})</span>
                      </button>
                      {isOpen(m.month) && (
                        <ul>
                          {m.days.map((d) => (
                            <li key={d.date}>
                              <button
                                className="fr-node"
                                aria-expanded={isOpen(d.date)}
                                onClick={() => toggle(d.date)}
                                data-testid="fr-day"
                              >
                                {dayName(d.date)}{" "}
                                <span className="muted">({fmt.count(d.count)})</span>
                              </button>
                              {isOpen(d.date) && (
                                <Photos photos={days.get(d.date)} onOpen={openPage} />
                              )}
                            </li>
                          ))}
                        </ul>
                      )}
                    </li>
                  ))}
                </ul>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}

function Photos({
  photos,
  onOpen,
}: {
  photos: FlickrAccountPhotoDto[] | undefined;
  onOpen: (url: string) => void;
}) {
  if (!photos) return <p className="muted">Reading…</p>;
  return (
    <ul className="fr-grid" data-testid="fr-photos">
      {photos.map((p) => {
        const name = p.title || p.id;
        return (
          <li key={p.id} data-testid="fr-photo">
            <button
              className="fr-link"
              title={`${name}: ${privacyLabel(p.privacy)}, uploaded ${p.uploaded}${p.keepsake ? ", with Keepsake" : ""}. Open on Flickr`}
              aria-label={`Open ${name} on Flickr`}
              onClick={() => onOpen(p.url)}
            >
              {p.thumb ? (
                <img src={p.thumb} alt="" className="fr-thumb" loading="lazy" />
              ) : (
                <span className="fr-thumb" aria-hidden={true} />
              )}
              {p.keepsake && (
                <span className="fr-mark" title="Uploaded with Keepsake" data-testid="fr-keepsake">
                  K
                </span>
              )}
            </button>
          </li>
        );
      })}
    </ul>
  );
}
