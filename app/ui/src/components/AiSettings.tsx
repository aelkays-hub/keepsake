// Settings → On-device AI: turn tagging and duplicate finding on or off.
// Turning it on downloads the model once (372 MB) into this computer's
// cache folder; nothing is sent anywhere. Changes apply at once.

import { useCallback, useEffect, useState } from "react";

import { api, events, type AiStatus } from "../api";
import { aiProgress } from "../ai";
import * as fmt from "../format";

export function AiSettings() {
  const [st, setSt] = useState<AiStatus | null>(null);
  const [download, setDownload] = useState<{ done: number; total: number } | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const refresh = useCallback(() => {
    api
      .aiStatus()
      .then((x) => {
        setSt(x);
        if (x.installing !== null)
          setDownload((d) => d ?? { done: x.installing ?? 0, total: x.modelBytes });
      })
      .catch((e) => setProblem(String(e)));
  }, []);
  useEffect(() => {
    refresh();
    const t = window.setInterval(refresh, 4000);
    const un = [
      events.aiStatus(refresh),
      events.aiInstall((p) => {
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
  const run = (p: Promise<unknown>) => {
    setProblem(null);
    p.then(refresh).catch((e) => setProblem(String(e)));
  };
  if (!st) return null;
  return (
    <div className="card" data-testid="ai-settings">
      <h2>On-device AI</h2>
      <p className="muted">
        Tags photos by what they show (people, beach, dog, food…) and finds near-identical shots
        taken on the same day, so you can pick the one to keep. It looks only at the dates you ask
        about: right-click a year, month or day in the sidebar and choose Find duplicates and tags.
        It runs on this computer from the thumbnails; no photo leaves it, and your originals are
        never changed.
      </p>
      <label className="check">
        <input
          type="checkbox"
          checked={st.enabled || download !== null}
          disabled={download !== null}
          onChange={(e) => run(api.aiEnable(e.target.checked))}
          data-testid="ai-enabled"
        />
        Tag photos and find duplicates
      </label>
      {!st.modelInstalled && !st.enabled && download === null && (
        <p className="muted">
          Turning it on downloads the model once ({fmt.bytes(st.modelBytes)}).
        </p>
      )}
      {download && (
        <div className="row" role="status" data-testid="ai-download">
          <progress max={download.total} value={download.done} />
          <span>
            Downloading the model: {fmt.bytes(download.done)} of {fmt.bytes(download.total)}
          </span>
          <button onClick={() => run(api.aiInstallCancel())}>Cancel</button>
        </div>
      )}
      <label className="check">
        <input
          type="checkbox"
          checked={st.gpu}
          disabled={download !== null}
          onChange={(e) => run(api.aiEnable(st.enabled, e.target.checked))}
          data-testid="ai-gpu"
        />
        Use the graphics card when possible
      </label>
      <p role="status" data-testid="ai-progress">
        {aiProgress(st)}
        {st.failed > 0 && ` ${fmt.plural(st.failed, "item")} could not be analysed.`}
      </p>
      {!st.runtimeFound && (
        <p className="error">ONNX Runtime is missing from this installation of Keepsake.</p>
      )}
      {st.modelInstalled && !st.enabled && (
        <button onClick={() => run(api.aiRemoveModel())} data-testid="ai-remove">
          Delete the downloaded model
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
