// Backup: copy the library's originals and sidecars to another drive,
// incrementally (only what is not there yet), checking every copy against
// the catalog's hash; verify a backup later; see the history.

import { useCallback, useEffect, useState } from "react";

import {
  api,
  events,
  pickFolder,
  type BackupOverview,
  type MaintDone,
  type MaintProgress,
} from "../api";
import * as fmt from "../format";

export function BackupScreen() {
  const [o, setO] = useState<BackupOverview | null>(null);
  const [dest, setDest] = useState("");
  const [label, setLabel] = useState("");
  const [progress, setProgress] = useState<MaintProgress | null>(null);
  const [done, setDone] = useState<MaintDone | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const load = useCallback(() => {
    api
      .backupOverview()
      .then((x) => {
        setO(x);
        setRunning((r) => r || x.busy);
      })
      .catch((e) => setProblem(String(e)));
  }, []);
  useEffect(() => {
    load();
    const a = events.maintProgress((p) => {
      setRunning(true);
      setProgress(p);
    });
    const b = events.maintDone((d) => {
      setRunning(false);
      setProgress(null);
      setDone(d);
      load();
    });
    return () => {
      a.then((f) => f());
      b.then((f) => f());
    };
  }, [load]);
  const start = (path: string, name: string | null) => {
    setProblem(null);
    setDone(null);
    api
      .backupRun(path, name)
      .then(() => setRunning(true))
      .catch((e) => setProblem(String(e)));
  };
  const verify = (volume: string) => {
    setProblem(null);
    setDone(null);
    api
      .verifyRun(volume)
      .then(() => setRunning(true))
      .catch((e) => setProblem(String(e)));
  };
  const pct =
    progress && progress.bytesTotal > 0
      ? Math.min(100, Math.round((progress.bytesDone / progress.bytesTotal) * 100))
      : 0;
  return (
    <section className="screen" aria-label="Backup" data-testid="backup-screen">
      <h1 className="heading">Backup</h1>
      <p className="muted">
        Copies every original and its sidecars to another drive, laid out by date, with a copy of
        the library. Only new items are copied on later runs, and every copy is checked against the
        catalog. A damaged original is never copied over a good backup.
      </p>

      <div className="card">
        <h2>Back up to a new place</h2>
        <div className="row">
          <input
            className="grow"
            placeholder="Backup folder or drive, e.g. E:\Keepsake backup"
            aria-label="Backup folder"
            value={dest}
            onChange={(e) => setDest(e.target.value)}
            data-testid="backup-dest"
          />
          <button
            onClick={() =>
              pickFolder("Choose a backup folder")
                .then((p) => p && setDest(p))
                .catch(() => undefined)
            }
          >
            Browse…
          </button>
        </div>
        <div className="row">
          <input
            placeholder="Name (optional)"
            aria-label="Backup name"
            value={label}
            onChange={(e) => setLabel(e.target.value)}
          />
          <span className="spacer" />
          <button
            className="primary"
            disabled={running || !dest.trim()}
            onClick={() => start(dest, label || null)}
            data-testid="backup-start"
          >
            Start backup
          </button>
        </div>
      </div>

      {(running || done || problem) && (
        <div className="card" role="status" data-testid="backup-status">
          {running && (
            <>
              <h2>{progress?.kind === "verify" ? "Verifying…" : "Backing up…"}</h2>
              <progress max={100} value={pct} aria-label="Progress" />
              <p className="muted">
                {fmt.count(progress?.filesDone ?? 0)} files · {fmt.bytes(progress?.bytesDone ?? 0)}{" "}
                of {fmt.bytes(progress?.bytesTotal ?? 0)}
                {progress?.current ? ` · ${progress.current}` : ""}
              </p>
              <button onClick={() => void api.maintCancel()} data-testid="backup-stop">
                Stop after this file
              </button>
            </>
          )}
          {!running && done && <Outcome d={done} />}
          {problem && <p className="error">{problem}</p>}
        </div>
      )}

      <div className="card">
        <h2>Backups</h2>
        {o && o.destinations.length === 0 && <p className="muted">No backups yet.</p>}
        <ul className="dest-list" data-testid="backup-destinations">
          {o?.destinations.map((d) => (
            <li key={d.volume}>
              <span className={`vol ${d.online ? "online" : "offline"}`} />
              <div className="grow">
                <strong>{d.label}</strong>
                <div className="muted">{d.path ?? "—"}</div>
              </div>
              <div className="dest-dates">
                <div>
                  Last backup: <span data-testid="last-backup">{fmt.when(d.lastBackupMs)}</span>
                </div>
                <div>
                  Last verified:{" "}
                  <span data-testid="last-verified">
                    {d.lastVerifiedMs ? fmt.when(d.lastVerifiedMs) : "never"}
                  </span>
                </div>
              </div>
              <button
                disabled={running || !d.online || !d.path}
                onClick={() => d.path && start(d.path, d.label)}
                title={d.online ? "" : "Connect the drive first"}
              >
                Back up now
              </button>
              <button
                disabled={running || !d.online}
                onClick={() => verify(d.volume)}
                data-testid="verify-now"
                title={
                  d.online ? "Check every file against the catalog" : "Connect the drive first"
                }
              >
                Verify now
              </button>
            </li>
          ))}
        </ul>
      </div>

      <div className="card">
        <h2>History</h2>
        {o && o.runs.length === 0 ? (
          <p className="muted">Nothing yet.</p>
        ) : (
          <table className="history" data-testid="history">
            <thead>
              <tr>
                <th>When</th>
                <th>What</th>
                <th>Where</th>
                <th className="num">Files</th>
                <th className="num">Copied</th>
                <th>Result</th>
              </tr>
            </thead>
            <tbody>
              {o?.runs.map((r) => (
                <tr key={`${r.kind}${r.startedMs}${r.volume}`}>
                  <td>{fmt.when(r.finishedMs)}</td>
                  <td>{r.kind === "backup" ? "Backup" : "Verify"}</td>
                  <td title={r.destination ?? ""}>{r.label}</td>
                  <td className="num">{fmt.count(r.files)}</td>
                  <td className="num">{r.kind === "backup" ? fmt.bytes(r.bytes) : "—"}</td>
                  <td className={r.ok ? "ok" : "bad"}>
                    {r.cancelled ? "Stopped" : r.ok ? "OK" : fmt.plural(r.problems, "problem")}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </section>
  );
}

function Outcome({ d }: { d: MaintDone }) {
  if (d.error) return <p className="error">{d.error}</p>;
  const r = d.run;
  if (!r) return null;
  const what = d.kind === "backup" ? "Backup" : "Verification";
  return (
    <div data-testid="backup-outcome">
      <h2 className={r.ok ? "ok" : "bad"}>
        {r.cancelled ? `${what} stopped` : r.ok ? `${what} finished` : `${what} found problems`}
      </h2>
      <p>
        {d.kind === "backup"
          ? `${fmt.count(r.files)} files on ${r.label} (${fmt.bytes(r.bytes)} copied this time).`
          : `${fmt.count(r.files)} files checked on ${r.label}.`}
      </p>
      {d.problems.length > 0 && (
        <ul className="problems">
          {d.problems.map((p) => (
            <li key={p}>{p}</li>
          ))}
        </ul>
      )}
    </div>
  );
}
