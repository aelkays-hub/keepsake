// Tag photos (a year, month, day or folder), Find duplicates (a folder) and
// Tag with AI (the photos picked): a window in the corner shows how far it
// has got, with Stop and Hide. Hidden, a small button in the corner brings
// it back. The work runs in the background either way. Tag with AI first
// asks which AI (Claude, ChatGPT or Gemini, those with a key, or All of
// them, ADR-039), and says what it will send and roughly what it costs.

import { useEffect, useMemo, useState } from "react";

import { api, type AiFolderStatus, type CloudInfo } from "../api";
import { ALL, chosen, costOf, firstChoice, help, names, usable } from "../cloud";
import * as fmt from "../format";
import { useUi } from "../state";
import type { AiJob } from "../uiModel";

/** How far it has got (Tag with AI adds what failed and why, and with
 * All, how each AI is getting on). */
type St = AiFolderStatus & {
  failed?: number;
  problem?: string | null;
  each?: { name: string; st: St }[];
};

/** Tag with AI: the providers' statuses as one (with All, a problem says
 * whose it is). */
function merge(list: { name: string; st: St }[]): St {
  const sum = (f: (s: St) => number | undefined) => list.reduce((n, e) => n + (f(e.st) ?? 0), 0);
  const bad = list.find((e) => e.st.problem);
  return {
    items: sum((s) => s.items),
    analysed: sum((s) => s.analysed),
    pending: sum((s) => s.pending),
    failed: sum((s) => s.failed),
    problem: bad ? (list.length > 1 ? `${bad.name}: ${bad.st.problem}` : bad.st.problem) : null,
    each: list,
  };
}

/** The job's three calls, for a folder, dates or picked photos (sent to
 * each of `providers`). */
function calls(job: AiJob, providers: { id: string; name: string }[]) {
  if ("cloud" in job) {
    // The photos are known once queued: Stop and the status use them.
    let ids: string[] = [];
    const sel = job.cloud;
    const named = (sts: St[]) => merge(sts.map((st, i) => ({ name: providers[i].name, st })));
    return {
      start: async (): Promise<St> => {
        // One after the other: each queues the same photos.
        const sts: St[] = [];
        for (const p of providers) {
          const r = await api.aiCloudTag(sel, p.id);
          ids = r.ids;
          sts.push(r.status);
        }
        return named(sts);
      },
      status: (): Promise<St> =>
        Promise.all(providers.map((p) => api.aiCloudStatus(ids, p.id))).then(named),
      stop: () =>
        Promise.all(providers.map((p) => api.aiCloudStop(ids, p.id))).then(() => undefined),
    };
  }
  if ("undated" in job) {
    return {
      start: (): Promise<St> => api.aiAnalyzeUndated(job.what),
      status: (): Promise<St> => api.aiUndatedStatus(job.what),
      stop: () => api.aiUndatedStop(job.what),
    };
  }
  if ("range" in job) {
    const { from, to } = job.range;
    return {
      start: (): Promise<St> => api.aiAnalyzeRange(from, to, job.what),
      status: (): Promise<St> => api.aiRangeStatus(from, to, job.what),
      stop: () => api.aiRangeStop(from, to, job.what),
    };
  }
  return {
    start: (): Promise<St> => api.aiAnalyzeFolder(job.volume, job.folder, job.what),
    status: (): Promise<St> => api.aiFolderStatus(job.volume, job.folder, job.what),
    stop: () => api.aiFolderStop(job.volume, job.folder, job.what),
  };
}

export function AiJobWindow() {
  const { s, dispatch } = useUi();
  const job = s.aiJob;
  const cloud = job !== null && "cloud" in job;
  /** Tag with AI: the providers, and the one picked to send to. */
  const [info, setInfo] = useState<CloudInfo | null>(null);
  const [picked, setPicked] = useState<string | null>(null);
  /** Who the photos go to: the AI picked, or with All every AI with a key. */
  const sendTo = useMemo(() => (info ? chosen(info, picked) : []), [info, picked]);
  const c = useMemo(
    () =>
      job &&
      calls(
        job,
        sendTo.map((p) => ({ id: p.id, name: p.name })),
      ),
    [job, sendTo],
  );
  const [st, setSt] = useState<St | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [stopped, setStopped] = useState(false);
  const [hidden, setHidden] = useState(false);
  /** Tag with AI: the job the user said Send to (other jobs start at once). */
  const [sent, setSent] = useState<AiJob | null>(null);
  const go = job !== null && (!cloud || sent === job);

  useEffect(() => {
    setSt(null);
    setProblem(null);
    setStopped(false);
    setHidden(false);
    setInfo(null);
    setPicked(null);
    if (cloud)
      api
        .aiCloudInfo()
        .then((x) => {
          setInfo(x);
          setPicked(firstChoice(x));
        })
        .catch((e) => setProblem(String(e)));
  }, [job, cloud]);

  useEffect(() => {
    if (!c || !go) return;
    let live = true;
    let timer = 0;
    const poll = () =>
      c
        .status()
        .then((x) => {
          if (!live) return;
          setSt(x);
          if (x.pending > 0) timer = window.setTimeout(poll, 2000);
          else dispatch({ t: "edited" });
        })
        .catch((e) => live && setProblem(String(e)));
    // Queueing can take a few seconds on a busy drive: the window says so.
    c.start()
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
  }, [c, go, dispatch]);

  if (!job || !c) return null;
  const tags = job.what === "tags";
  const asking = cloud && !go;
  const running = !asking && !problem && (!st || st.pending > 0) && !stopped;
  const pct = st && st.items > 0 ? Math.round((100 * st.analysed) / st.items) : 0;
  const all = picked === ALL;
  const ready = info ? usable(info) : [];
  const models = sendTo.map((p) => p.models.find((m) => m.id === p.model));
  const cents = models.reduce((n, m) => n + (m?.centsPerPhoto ?? 0), 0);
  const aiName = sendTo.length > 0 ? names(sendTo.map((p) => p.name)) : "AI";
  const notes = sendTo.map((p) => help(p.id).note).filter((n): n is string => n !== null);
  const what = cloud ? `Tagging with ${aiName}:` : tags ? "Tagging" : "Finding duplicates in";
  const close = () => dispatch({ t: "aiJob", job: null });
  const send = (j: AiJob) => {
    setSent(j);
    // The provider used is offered first next time.
    if (picked) api.aiCloudSetDefault(picked).catch(() => undefined);
  };

  if (hidden)
    return (
      <button
        className="ai-job-pill"
        onClick={() => setHidden(false)}
        data-testid="ai-job-pill"
        title="Show progress"
      >
        {running && <span className="spinner" aria-hidden={true} />}
        {cloud ? (all ? "All AIs" : aiName) : tags ? "Tagging" : "Duplicates"} ·{" "}
        {running ? `${pct}%` : "done"}
      </button>
    );

  let line: string;
  if (problem) line = `Could not start: ${problem}`;
  else if (asking && !info) line = "Getting ready…";
  else if (asking && ready.length === 0)
    line =
      "Add an API key for Claude, ChatGPT or Gemini in Settings first (Settings → Tag with AI).";
  else if (asking && "cloud" in job)
    line =
      `Send ${fmt.plural(job.count, "photo")} to ` +
      names(sendTo.map((p, i) => models[i]?.name ?? p.name)) +
      ` to be tagged?${all ? " Each one tags them." : ""}` +
      (sendTo.length > 0 && models.every((m) => m)
        ? ` It costs ${costOf(job.count, cents)}${all ? " in all" : ""}, charged by ` +
          `${names(sendTo.map((p) => p.company))}.`
        : "");
  else if (!st) line = "Getting ready…";
  else if (st.items === 0)
    line = `No ${tags ? "" : "RAW "}photos ${
      "undated" in job ? "without a date" : "range" in job ? "on these dates" : "in this folder"
    }.`;
  else if (stopped && st.pending === 0)
    line = `Stopped: ${fmt.count(st.analysed)} of ${fmt.count(st.items)} done. What was done stays.`;
  else if (st.pending > 0) line = `${fmt.count(st.analysed)} of ${fmt.count(st.items)} done…`;
  else if (cloud && st.each && st.each.length > 1)
    line =
      "Photos tagged: " +
      st.each
        .map((e) => `${e.name} ${fmt.count(e.st.analysed)} of ${fmt.count(e.st.items)}`)
        .join(", ") +
      "." +
      (st.failed ? ` ${fmt.plural(st.failed, "photo")} could not be: ${st.problem ?? "?"}` : "");
  else if (cloud)
    line =
      `${fmt.count(st.analysed)} of ${fmt.count(st.items)} photos tagged.` +
      (st.failed ? ` ${fmt.plural(st.failed, "photo")} could not be: ${st.problem ?? "?"}` : "");
  else
    line = tags
      ? `${fmt.count(st.analysed)} of ${fmt.count(st.items)} photos tagged.`
      : `${fmt.count(st.analysed)} of ${fmt.count(st.items)} RAW photos compared.`;

  return (
    <div className="ai-job" role="dialog" aria-label={`${what} ${job.title}`} data-testid="ai-job">
      <h2>
        {what} {job.title}
      </h2>
      <p className="muted">
        {cloud
          ? "A 1024-pixel copy of each photo is sent; the tags become keywords."
          : "undated" in job
            ? "Photos and videos with no date."
            : "range" in job
              ? "Photos taken on these dates."
              : "This folder and the folders in it."}
      </p>
      {asking && ready.length > 1 && (
        <label className="row">
          AI{" "}
          <select
            value={picked ?? ""}
            onChange={(e) => setPicked(e.target.value)}
            data-testid="ai-job-provider"
          >
            {ready.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name} ({p.models.find((m) => m.id === p.model)?.name ?? p.model})
              </option>
            ))}
            <option value={ALL}>All ({names(ready.map((p) => p.name))})</option>
          </select>
        </label>
      )}
      {!asking && (
        <progress
          max={100}
          value={st && st.items > 0 ? pct : undefined}
          aria-label="Progress"
          data-testid="ai-job-bar"
        />
      )}
      <p role={problem ? "alert" : "status"} data-testid="ai-job-status">
        {running && <span className="spinner" aria-hidden={true} />}
        {line}
      </p>
      {asking && notes.length > 0 && (
        <p className="muted" data-testid="ai-job-privacy">
          {notes.join(" ")}
        </p>
      )}
      {running && st?.problem && (
        <p className="muted" data-testid="ai-job-note">
          {st.problem}
        </p>
      )}
      <footer className="modal-foot">
        {asking && sendTo.length > 0 && (
          <button className="primary" onClick={() => send(job)} data-testid="ai-job-send">
            Send
          </button>
        )}
        {asking && info && ready.length === 0 && (
          <button
            onClick={() => {
              dispatch({ t: "screen", screen: "settings" });
              close();
            }}
            data-testid="ai-job-settings"
          >
            Open Settings
          </button>
        )}
        {asking && (
          <button onClick={close} data-testid="ai-job-cancel">
            Cancel
          </button>
        )}
        {running && st && (
          <button
            onClick={() =>
              c
                .stop()
                .then(() => setStopped(true))
                .then(() => c.status())
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
        {!running && !asking && !tags && !problem && st && st.items > 0 && (
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
        {!running && !asking && (
          <button onClick={close} data-testid="ai-job-close">
            Close
          </button>
        )}
      </footer>
    </div>
  );
}
