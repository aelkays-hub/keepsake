// Settings → Read aircraft (ADR-041): turn it on (downloads two text
// models once) or off. Reading itself is started from the photos:
// right-click them and choose Read aircraft. Everything stays on this
// computer; originals are only read, never changed.

import { useCallback, useEffect, useState } from "react";

import { aircraftProgress } from "../aircraft";
import { api, events, type AircraftStatus } from "../api";
import * as fmt from "../format";

export function AircraftSettings() {
  const [st, setSt] = useState<AircraftStatus | null>(null);
  const [download, setDownload] = useState<{ done: number; total: number } | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const refresh = useCallback(() => {
    api
      .aircraftStatus()
      .then((x) => {
        setSt(x);
        if (x.installing !== null)
          setDownload((d) => d ?? { done: x.installing ?? 0, total: x.modelsBytes });
      })
      .catch((e) => setProblem(String(e)));
  }, []);
  useEffect(() => {
    refresh();
    const un = [
      events.aircraftStatus(refresh),
      events.aiStatus(refresh),
      events.aircraftInstall((p) => {
        if (p.finished) {
          setDownload(null);
          if (p.error && p.error !== "cancelled") setProblem(`The download failed: ${p.error}`);
          refresh();
        } else setDownload({ done: p.done, total: p.total });
      }),
    ];
    return () => {
      un.forEach((p) => void p.then((f) => f()).catch(() => undefined));
    };
  }, [refresh]);
  const run = (p: Promise<unknown>) => {
    setProblem(null);
    p.then(refresh).catch((e) => setProblem(String(e)));
  };
  if (!st) return null;
  return (
    <div className="card" data-testid="aircraft-settings">
      <h2>Read aircraft</h2>
      <p className="muted">
        For the photos you pick (right-click them and choose Read aircraft), Keepsake reads the
        registration and the airline painted on the aircraft and works out its type. A window then
        shows what it found: you choose how sure each kind must be, untick mistakes and accept, and
        only what you accept becomes keywords under Aircraft. It runs on this computer: nothing is
        sent anywhere, and your originals are only read, never changed.
      </p>
      <label className="check">
        <input
          type="checkbox"
          checked={st.enabled || download !== null}
          disabled={download !== null}
          onChange={(e) => run(api.aircraftEnable(e.target.checked))}
          data-testid="aircraft-enabled"
        />
        Read aircraft
      </label>
      {!st.modelsInstalled && !st.enabled && download === null && (
        <p className="muted">
          Turning it on downloads two text models once ({fmt.bytes(st.modelsBytes)}).
        </p>
      )}
      {download && (
        <div className="row" role="status" data-testid="aircraft-download">
          <progress max={download.total} value={download.done} />
          <span>
            Downloading the text models: {fmt.bytes(download.done)} of {fmt.bytes(download.total)}
          </span>
          <button onClick={() => run(api.aircraftInstallCancel())}>Cancel</button>
        </div>
      )}
      <p role="status" data-testid="aircraft-progress">
        {aircraftProgress(st)}
      </p>
      <p className="muted" data-testid="aircraft-confidence">
        Offered from: registration {st.confidence.registrationPct}%, airline{" "}
        {st.confidence.airlinePct}%, type {st.confidence.typePct}% sure (set with the sliders in the
        Read aircraft window).
      </p>
      {st.modelsInstalled && !st.enabled && (
        <button onClick={() => run(api.aircraftRemoveModels())} data-testid="aircraft-remove">
          Delete the downloaded text models
        </button>
      )}
      {problem && (
        <p className="error" role="alert">
          {problem}
        </p>
      )}
    </div>
  );
}
