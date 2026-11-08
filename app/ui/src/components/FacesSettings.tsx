// Settings → Recognise faces (ADR-040): turn it on (downloads two small
// face models once), search every photo now or stop, and delete all face
// data. Everything stays on this computer; originals are never changed.

import { useCallback, useEffect, useState } from "react";

import { api, events, type FacesStatus } from "../api";
import { facesProgress } from "../faces";
import * as fmt from "../format";

export function FacesSettings() {
  const [st, setSt] = useState<FacesStatus | null>(null);
  const [download, setDownload] = useState<{ done: number; total: number } | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [note, setNote] = useState<string | null>(null);
  const [asking, setAsking] = useState(false);
  const refresh = useCallback(() => {
    api
      .facesStatus()
      .then((x) => {
        setSt(x);
        if (x.installing !== null)
          setDownload((d) => d ?? { done: x.installing ?? 0, total: x.modelsBytes });
      })
      .catch((e) => setProblem(String(e)));
  }, []);
  useEffect(() => {
    refresh();
    const t = window.setInterval(refresh, 4000);
    const un = [
      events.facesStatus(refresh),
      events.aiStatus(refresh),
      events.facesInstall((p) => {
        if (p.finished) {
          setDownload(null);
          if (p.error && p.error !== "cancelled") setProblem(`The download failed: ${p.error}`);
          refresh();
        } else setDownload({ done: p.done, total: p.total });
      }),
    ];
    return () => {
      window.clearInterval(t);
      un.forEach((p) => void p.then((f) => f()).catch(() => undefined));
    };
  }, [refresh]);
  const run = (p: Promise<unknown>, done?: string) => {
    setProblem(null);
    setNote(null);
    p.then(() => {
      if (done) setNote(done);
      refresh();
    }).catch((e) => setProblem(String(e)));
  };
  if (!st) return null;
  const on = st.enabled && st.aiEnabled;
  const left = Math.max(st.photos - st.scanned, 0);
  return (
    <div className="card" data-testid="faces-settings">
      <h2>Recognise faces</h2>
      <p className="muted">
        Finds the faces in your photos so you can name a person and find the other photos they are
        in. It runs on this computer from the thumbnails: no face is ever sent anywhere, and your
        originals are never changed.
      </p>
      <label className="check">
        <input
          type="checkbox"
          checked={st.enabled || download !== null}
          disabled={download !== null || (!st.aiEnabled && !st.enabled)}
          onChange={(e) => run(api.facesEnable(e.target.checked))}
          data-testid="faces-enabled"
        />
        Recognise faces
      </label>
      {!st.modelsInstalled && !st.enabled && download === null && st.aiEnabled && (
        <p className="muted">
          Turning it on downloads two face models once ({fmt.bytes(st.modelsBytes)}).
        </p>
      )}
      {download && (
        <div className="row" role="status" data-testid="faces-download">
          <progress max={download.total} value={download.done} />
          <span>
            Downloading the face models: {fmt.bytes(download.done)} of {fmt.bytes(download.total)}
          </span>
          <button onClick={() => run(api.facesInstallCancel())}>Cancel</button>
        </div>
      )}
      <p role="status" data-testid="faces-progress">
        {facesProgress(st)}
        {st.failed > 0 && ` ${fmt.plural(st.failed, "photo")} could not be searched.`}
      </p>
      {on && (
        <div className="row">
          {st.pending > 0 ? (
            <button onClick={() => run(api.facesStop(), "Stopped.")} data-testid="faces-stop">
              Stop
            </button>
          ) : (
            <button
              onClick={() =>
                run(
                  api.facesScanAll(),
                  left > 0 ? `Searching ${fmt.plural(left, "photo")}…` : undefined,
                )
              }
              disabled={left === 0}
              title="Search every photo not searched yet, in the background"
              data-testid="faces-scan-all"
            >
              {left === 0 ? "Every photo is searched" : `Search all ${fmt.plural(left, "photo")}`}
            </button>
          )}
        </div>
      )}
      {(st.faces > 0 || st.scanned > 0 || st.people > 0) &&
        (asking ? (
          <div
            className="row"
            role="group"
            aria-label="Delete all face data?"
            data-testid="faces-delete-ask"
          >
            <span>
              Forget {fmt.plural(st.faces, "face")} and {fmt.plural(st.people, "person", "people")}?
              Names you gave are lost too. This cannot be undone.
            </span>
            <button
              className="danger"
              onClick={() => {
                setAsking(false);
                run(api.facesDeleteAll(), "All face data was deleted.");
              }}
              data-testid="faces-delete-yes"
            >
              Delete all face data
            </button>
            <button onClick={() => setAsking(false)} data-testid="faces-delete-no">
              Cancel
            </button>
          </div>
        ) : (
          <button onClick={() => setAsking(true)} data-testid="faces-delete">
            Delete all face data…
          </button>
        ))}
      {st.modelsInstalled && !st.enabled && (
        <button onClick={() => run(api.facesRemoveModels())} data-testid="faces-remove">
          Delete the downloaded face models
        </button>
      )}
      {note && (
        <p className="muted" role="status" data-testid="faces-note">
          {note}
        </p>
      )}
      {problem && (
        <p className="error" role="alert">
          {problem}
        </p>
      )}
    </div>
  );
}
