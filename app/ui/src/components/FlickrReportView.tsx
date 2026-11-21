// Report → Flickr (ADR-046): thumbnails of the photos uploaded to Flickr,
// newest first, each opening its Flickr page; and above them the photos
// under way (waiting, uploading, trying again or not uploaded). Refreshed
// while uploads are under way.

import { useEffect, useState } from "react";

import { api, fileUrl } from "../api";
import type { FlickrReportDto } from "../bindings/FlickrReportDto";
import type { FlickrReportItemDto } from "../bindings/FlickrReportItemDto";
import { privacyLabel } from "../flickr";
import * as fmt from "../format";

const POLL_MS = 2000;

const STATE: Record<string, string> = {
  waiting: "Waiting",
  uploading: "Uploading…",
  retrying: "Trying again",
  failed: "Not uploaded",
};

export function FlickrReportView() {
  const [r, setR] = useState<FlickrReportDto | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [tick, setTick] = useState(0);

  useEffect(() => {
    let live = true;
    api
      .flickrReport()
      .then((x) => live && setR(x))
      .catch((e) => live && setProblem(String(e)));
    return () => {
      live = false;
    };
  }, [tick]);
  const busy = (r?.current ?? []).some((c) => c.state !== "failed");
  useEffect(() => {
    if (!busy) return;
    const t = window.setTimeout(() => setTick((n) => n + 1), POLL_MS);
    return () => window.clearTimeout(t);
  }, [busy, r]);

  const open = (url: string) => api.flickrOpen(url).catch((e) => setProblem(String(e)));
  if (!r)
    return problem ? (
      <p className="error" role="alert">
        {problem}
      </p>
    ) : (
      <p className="muted">Reading…</p>
    );
  return (
    <div className="flickr-report" data-testid="flickr-report">
      {problem && (
        <p className="error" role="alert">
          {problem}
        </p>
      )}
      {r.current.length > 0 && (
        <>
          <h2 data-testid="flickr-report-current-title">Under way ({r.current.length})</h2>
          <ul className="fr-grid" data-testid="flickr-report-current">
            {r.current.map((it) => (
              <li key={it.id} data-testid="fr-current">
                <Thumb it={it} />
                <span
                  className={it.state === "failed" ? "fr-cap error" : "fr-cap"}
                  title={it.error ?? undefined}
                >
                  {STATE[it.state] ?? it.state}
                </span>
              </li>
            ))}
          </ul>
        </>
      )}
      <h2 data-testid="flickr-report-uploaded-title">
        On Flickr ({fmt.plural(r.uploaded.length, "photo")})
      </h2>
      {r.uploaded.length === 0 ? (
        <p className="muted" data-testid="flickr-report-none">
          Nothing uploaded yet: right-click photos and choose Upload to Flickr….
        </p>
      ) : (
        <ul className="fr-grid" data-testid="flickr-report-uploaded">
          {r.uploaded.map((it, i) => (
            <li key={`${it.id}-${it.url ?? i}`} data-testid="fr-uploaded">
              <button
                className="fr-link"
                title={`${it.name}: ${privacyLabel(it.privacy ?? "")}, ${it.date ?? ""}. Open on Flickr`}
                aria-label={`Open ${it.name} on Flickr`}
                onClick={() => it.url && open(it.url)}
              >
                <Thumb it={it} />
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function Thumb({ it }: { it: FlickrReportItemDto }) {
  const src = it.thumb ? fileUrl(it.thumb) : null;
  return src ? (
    <img src={src} alt="" className="fr-thumb" loading="lazy" />
  ) : (
    <span className="fr-thumb" aria-hidden={true} />
  );
}
