// Folders: add photos and videos that are already on this computer or a
// drive, where they are. Nothing is copied, and nothing is written into the
// folder: ratings and other edits are kept in the library.

import { useCallback, useEffect, useState } from "react";

import {
  api,
  events,
  pickFolder,
  type FolderDone,
  type MaintProgress,
  type Sidebar as SidebarData,
} from "../api";
import * as fmt from "../format";
import { useUi } from "../state";

/** How often the list is read again while fingerprints are being made. */
const FINGERPRINT_REFRESH_MS = 3000;

export function FoldersScreen() {
  const { dispatch } = useUi();
  const [path, setPath] = useState("");
  const [volumes, setVolumes] = useState<SidebarData["volumes"] | null>(null);
  const [progress, setProgress] = useState<MaintProgress | null>(null);
  const [done, setDone] = useState<FolderDone | null>(null);
  /** Items of the folder being added that are in the library so far. */
  const [saved, setSaved] = useState<number | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const load = useCallback(() => {
    api
      .sidebar()
      .then((s) => setVolumes(s.volumes))
      .catch((e) => setProblem(String(e)));
    // A scan started before this screen opened is still running.
    api
      .backupOverview()
      .then((o) => setRunning((r) => r || o.busy))
      .catch(() => undefined);
  }, []);
  // Fingerprints are made in the background: look again while any are left.
  const fingerprinting = volumes?.some((v) => v.online && v.unfingerprinted > 0) ?? false;
  useEffect(() => {
    if (!fingerprinting) return;
    const t = window.setInterval(load, FINGERPRINT_REFRESH_MS);
    return () => window.clearInterval(t);
  }, [fingerprinting, load]);
  useEffect(() => {
    load();
    const a = events.maintProgress((p) => {
      if (p.kind !== "folder") return;
      setRunning(true);
      setProgress(p);
    });
    const b = events.folderDone((d) => {
      setRunning(false);
      setProgress(null);
      setSaved(null);
      setDone(d);
      load();
    });
    // Saved as it is read: the list shows the folder and its count growing.
    const c = events.folderSaved((e) => {
      setSaved(e.items);
      load();
    });
    return () => {
      a.then((f) => f());
      b.then((f) => f());
      c.then((f) => f());
    };
  }, [load]);
  const add = (folder: string) => {
    setProblem(null);
    setDone(null);
    setProgress(null);
    setSaved(null);
    api
      .folderAdd(folder)
      .then(() => setRunning(true))
      .catch((e) => setProblem(String(e)));
  };
  const pct =
    progress && progress.bytesTotal > 0
      ? Math.min(100, Math.round((progress.bytesDone / progress.bytesTotal) * 100))
      : 0;
  return (
    <section className="screen" aria-label="Folders" data-testid="folders-screen">
      <h1 className="heading">Folders</h1>
      <p className="muted">
        Add a folder of photos and videos that is already on this computer or a drive. Keepsake
        catalogues them where they are: nothing is copied, moved or changed, and nothing is written
        into the folder. Ratings, labels and other edits are kept in the library. Ratings and
        keywords from Lightroom (.xmp files) are read.
      </p>

      <div className="card">
        <h2>Add a folder</h2>
        <div className="row">
          <input
            className="grow"
            placeholder="Folder, e.g. E:\Pictures\2025"
            aria-label="Folder to add"
            value={path}
            onChange={(e) => setPath(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && path.trim() && !running) add(path);
            }}
            data-testid="folder-path"
          />
          <button
            onClick={() =>
              pickFolder("Choose a folder of photos and videos")
                .then((p) => p && setPath(p))
                .catch(() => undefined)
            }
          >
            Browse…
          </button>
          <button
            className="primary"
            disabled={running || !path.trim()}
            onClick={() => add(path)}
            data-testid="folder-add"
          >
            Add folder
          </button>
        </div>
        <p className="muted">
          Every file is read once to check it, so a large folder takes a while. Its photos appear in
          the library as they are read, and you can keep using Keepsake meanwhile.
        </p>
      </div>

      {(running || done || problem) && (
        <div className="card" role="status" data-testid="folder-status">
          {running && (
            <>
              <h2>Adding a folder…</h2>
              <progress max={100} value={pct} aria-label="Progress" />
              <p className="muted">
                {fmt.count(progress?.filesDone ?? 0)} files · {fmt.bytes(progress?.bytesDone ?? 0)}{" "}
                of {fmt.bytes(progress?.bytesTotal ?? 0)}
                {progress?.current ? ` · ${progress.current}` : ""}
              </p>
              {saved !== null && (
                <p className="muted" data-testid="folder-saved">
                  {fmt.plural(saved, "item")} in the library so far. They are saved as they are
                  read: if this stops, adding the folder again carries on from here.
                </p>
              )}
              <button onClick={() => void api.maintCancel()} data-testid="folder-stop">
                Stop
              </button>
            </>
          )}
          {!running && done && (
            <Outcome
              d={done}
              onShow={(id, title) =>
                dispatch({ t: "section", section: { kind: "volume", id, title } })
              }
            />
          )}
          {problem && <p className="error">{problem}</p>}
        </div>
      )}

      <div className="card">
        <h2>In the library</h2>
        {volumes && volumes.length === 0 && <p className="muted">No folders or drives yet.</p>}
        <ul className="dest-list" data-testid="folder-list">
          {volumes?.map((v) => (
            <li key={v.id}>
              <span
                className={`vol ${v.online ? "online" : "offline"}`}
                role="img"
                aria-label={v.online ? "connected" : "not connected"}
              />
              <div className="grow">
                <strong>{v.label}</strong>
                <div className="muted">
                  {v.mount ?? "Not connected"} · {fmt.plural(v.count, "item")}
                </div>
                {v.unfingerprinted > 0 && (
                  <div className="muted" data-testid="folder-fingerprinting">
                    {v.online
                      ? `Fingerprinting ${fmt.plural(v.unfingerprinted, "file")} in the background`
                      : `${fmt.plural(v.unfingerprinted, "file")} to fingerprint when connected`}
                  </div>
                )}
              </div>
              <button
                onClick={() =>
                  dispatch({ t: "section", section: { kind: "volume", id: v.id, title: v.label } })
                }
              >
                Show
              </button>
              <button
                disabled={running || !v.online || !v.mount}
                onClick={() => v.mount && add(v.mount)}
                title={
                  v.online
                    ? "Find new, changed and removed files in this folder"
                    : "Connect the drive first"
                }
                data-testid="folder-rescan"
              >
                Scan again
              </button>
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}

function Outcome({ d, onShow }: { d: FolderDone; onShow: (id: string, title: string) => void }) {
  if (d.error) return <p className="error">{d.error}</p>;
  if (d.cancelled)
    return (
      <p data-testid="folder-outcome">
        Stopped. What was read so far is in the library; add the folder again to carry on.
      </p>
    );
  return (
    <div data-testid="folder-outcome">
      <h2 className="ok">
        Added {d.label}: {fmt.plural(d.items, "item")}
      </h2>
      <p>
        {fmt.plural(d.files, "file")} in {d.folder}. Nothing in the folder was changed.
        {d.withSidecars > 0 &&
          ` ${fmt.count(d.withSidecars)} took ratings or keywords from .xmp files.`}
        {d.otherFiles > 0 &&
          ` ${fmt.plural(d.otherFiles, "other file")} (not photos or videos) left out.`}{" "}
        Thumbnails and full fingerprints are being made in the background.
      </p>
      {d.problems.length > 0 && (
        <ul className="problems">
          {d.problems.map((p) => (
            <li key={p}>{p}</li>
          ))}
        </ul>
      )}
      {d.volume && (
        <button className="primary" onClick={() => onShow(d.volume ?? "", d.label ?? "")}>
          Show them
        </button>
      )}
    </div>
  );
}
