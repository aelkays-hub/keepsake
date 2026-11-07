// Find duplicates or Tag photos for a folder and the folders in it, from
// the folder tree's right-click menu: a window shows how far it has got,
// with Stop and Hide. Hidden, a small button in the corner brings it back.
// The work runs in the AI worker either way.

import { useEffect, useState } from "react";

import { api, type AiFolderStatus } from "../api";
import * as fmt from "../format";
import { useUi } from "../state";

export function AiJobWindow() {
  const { s, dispatch } = useUi();
  const job = s.aiJob;
  const [st, setSt] = useState<AiFolderStatus | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [stopped, setStopped] = useState(false);
  const [hidden, setHidden] = useState(false);

  useEffect(() => {
    setSt(null);
    setProblem(null);
    setStopped(false);
    setHidden(false);
    if (!job) return;
    let live = true;
    let timer = 0;
    const poll = () =>
      api
        .aiFolderStatus(job.volume, job.folder, job.what)
        .then((x) => {
          if (!live) return;
          setSt(x);
          if (x.pending > 0) timer = window.setTimeout(poll, 2000);
          else dispatch({ t: "edited" });
        })
        .catch((e) => live && setProblem(String(e)));
    // Queueing can take a few seconds on a busy drive: the window says so.
    api
      .aiAnalyzeFolder(job.volume, job.folder, job.what)
      .then((x) => {
        if (!live) return;
        setSt(x);
        poll();
      })
      .catch((e) => live && setProblem(String(e)));
    return () => {
      live = false;
      window.clearTimeout(timer);
    };
  }, [job, dispatch]);

  if (!job) return null;
  const tags = job.what === "tags";
  const running = !problem && (!st || st.pending > 0) && !stopped;
  const pct = st && st.items > 0 ? Math.round((100 * st.analysed) / st.items) : 0;
  const what = tags ? "Tagging" : "Finding duplicates in";
  const close = () => dispatch({ t: "aiJob", job: null });

  if (hidden)
    return (
      <button
        className="ai-job-pill"
        onClick={() => setHidden(false)}
        data-testid="ai-job-pill"
        title="Show progress"
      >
        {running && <span className="spinner" aria-hidden={true} />}
        {tags ? "Tagging" : "Duplicates"} · {running ? `${pct}%` : "done"}
      </button>
    );

  let line: string;
  if (problem) line = `Could not start: ${problem}`;
  else if (!st) line = "Getting ready…";
  else if (st.items === 0)
    line = tags ? "No photos in this folder." : "No RAW photos in this folder.";
  else if (stopped && st.pending === 0)
    line = `Stopped: ${fmt.count(st.analysed)} of ${fmt.count(st.items)} done. What was done stays.`;
  else if (st.pending > 0) line = `${fmt.count(st.analysed)} of ${fmt.count(st.items)} done…`;
  else
    line = tags
      ? `${fmt.count(st.analysed)} of ${fmt.count(st.items)} photos tagged.`
      : `${fmt.count(st.analysed)} of ${fmt.count(st.items)} RAW photos compared.`;

  return (
    <div className="ai-job" role="dialog" aria-label={`${what} ${job.title}`} data-testid="ai-job">
      <h2>
        {what} {job.title}
      </h2>
      <p className="muted">This folder and the folders in it.</p>
      <progress
        max={100}
        value={st && st.items > 0 ? pct : undefined}
        aria-label="Progress"
        data-testid="ai-job-bar"
      />
      <p role={problem ? "alert" : "status"} data-testid="ai-job-status">
        {running && <span className="spinner" aria-hidden={true} />} {line}
      </p>
      <footer className="modal-foot">
        {running && st && (
          <button
            onClick={() =>
              api
                .aiFolderStop(job.volume, job.folder, job.what)
                .then(() => setStopped(true))
                .then(() => api.aiFolderStatus(job.volume, job.folder, job.what))
                .then(setSt)
                .catch((e) => setProblem(String(e)))
            }
            data-testid="ai-job-stop"
          >
            Stop
          </button>
        )}
        {running && (
          <button onClick={() => setHidden(true)} data-testid="ai-job-hide">
            Hide
          </button>
        )}
        {!running && !tags && !problem && st && st.items > 0 && (
          <button
            onClick={() => {
              dispatch({ t: "duplicates", range: null });
              close();
            }}
            data-testid="ai-job-show-dups"
          >
            Show duplicates
          </button>
        )}
        {!running && (
          <button onClick={close} data-testid="ai-job-close">
            Close
          </button>
        )}
      </footer>
    </div>
  );
}
