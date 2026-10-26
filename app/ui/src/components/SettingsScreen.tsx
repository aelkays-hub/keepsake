// Settings (Ctrl+,): library, rendition cache, workers, "Open in"
// applications per file type, and import presets. Saved to config.toml;
// rendering restarts with the new values.

import { useEffect, useState } from "react";

import { api, pickFile, pickFolder, type OpenApp, type Preset, type SettingsInfo } from "../api";
import * as fmt from "../format";

const GB = 1024 ** 3;

function Library({ info }: { info: SettingsInfo }) {
  const [problem, setProblem] = useState<string | null>(null);
  const use = (f: (p: string) => Promise<unknown>, title: string) =>
    pickFolder(title)
      .then((p) => (p ? f(p).then(() => window.location.reload()) : undefined))
      .catch((e) => setProblem(String(e)));
  return (
    <div className="card">
      <h2>Library</h2>
      <p>
        <span className="muted">Open library: </span>
        <code data-testid="settings-library">{info.library ?? "none"}</code>
      </p>
      <div className="row">
        <button onClick={() => void use(api.libraryOpen, "Open a Keepsake library")}>
          Open another library…
        </button>
        <button onClick={() => void use(api.libraryCreate, "Folder for a new library")}>
          New library…
        </button>
      </div>
      {problem && <p className="error">{problem}</p>}
    </div>
  );
}

function Presets() {
  const [list, setList] = useState<Preset[]>([]);
  const [edit, setEdit] = useState<Preset | null>(null);
  const [original, setOriginal] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const load = () =>
    api
      .presetsList()
      .then(setList)
      .catch((e) => setProblem(String(e)));
  useEffect(() => {
    void load();
  }, []);
  const save = async () => {
    if (!edit) return;
    try {
      await api.presetSave({
        ...edit,
        destinations: edit.destinations.map((d) => d.trim()).filter(Boolean),
        event: edit.event?.trim() || null,
      });
      if (original && original !== edit.name) await api.presetDelete(original);
      setEdit(null);
      setProblem(null);
      await load();
    } catch (e) {
      setProblem(String(e));
    }
  };
  return (
    <div className="card">
      <h2>Import presets</h2>
      <ul className="preset-list" data-testid="presets">
        {list.map((p) => (
          <li key={p.name}>
            <div className="grow">
              <strong>{p.name}</strong>
              <div className="muted">
                {p.destinations.join(" + ")} · {p.template}
                {p.event ? ` · ${p.event}` : ""}
              </div>
            </div>
            <button
              onClick={() => {
                setEdit({ ...p, destinations: [...p.destinations] });
                setOriginal(p.name);
              }}
            >
              Edit
            </button>
            <button
              aria-label={`Delete ${p.name}`}
              onClick={() =>
                api
                  .presetDelete(p.name)
                  .then(load)
                  .catch((e) => setProblem(String(e)))
              }
            >
              Delete
            </button>
          </li>
        ))}
      </ul>
      {list.length === 0 && (
        <p className="muted">No presets. The import window can save one too.</p>
      )}
      {edit ? (
        <div className="preset-edit" data-testid="preset-edit">
          <label>
            Name
            <input value={edit.name} onChange={(e) => setEdit({ ...edit, name: e.target.value })} />
          </label>
          {[0, 1].map((i) => (
            <label key={i}>
              {i === 0 ? "First destination" : "Second destination (another disk)"}
              <input
                value={edit.destinations[i] ?? ""}
                onChange={(e) => {
                  const d = [...edit.destinations];
                  d[i] = e.target.value;
                  setEdit({ ...edit, destinations: d });
                }}
              />
            </label>
          ))}
          <label>
            File names
            <input
              value={edit.template}
              onChange={(e) => setEdit({ ...edit, template: e.target.value })}
            />
          </label>
          <label>
            Event (optional)
            <input
              value={edit.event ?? ""}
              onChange={(e) => setEdit({ ...edit, event: e.target.value })}
            />
          </label>
          <div className="row">
            <span className="spacer" />
            <button onClick={() => setEdit(null)}>Cancel</button>
            <button className="primary" onClick={() => void save()} disabled={!edit.name.trim()}>
              Save preset
            </button>
          </div>
        </div>
      ) : (
        <button
          onClick={() => {
            setOriginal(null);
            setEdit({
              name: "",
              destinations: ["", ""],
              template: "{yyyy}/{yyyy}-{mm}-{dd}/{orig}",
              event: null,
            });
          }}
        >
          New preset…
        </button>
      )}
      {problem && <p className="error">{problem}</p>}
    </div>
  );
}

export function SettingsScreen() {
  const [info, setInfo] = useState<SettingsInfo | null>(null);
  const [cacheDir, setCacheDir] = useState<string | null>(null);
  const [limitGb, setLimitGb] = useState("20");
  const [workers, setWorkers] = useState<number | null>(null);
  const [proxy, setProxy] = useState(1);
  const [battery, setBattery] = useState(true);
  const [apps, setApps] = useState<OpenApp[]>([]);
  const [newExt, setNewExt] = useState("");
  const [newApp, setNewApp] = useState("");
  const [saved, setSaved] = useState<string | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [oldCache, setOldCache] = useState<string | null>(null);
  const take = (s: SettingsInfo) => {
    setInfo(s);
    setCacheDir(s.cacheDirCustom ? s.cacheDir : null);
    setLimitGb(String(Math.round((s.cacheMaxBytes / GB) * 10) / 10));
    setWorkers(s.workers);
    setProxy(s.proxyWorkers);
    setBattery(s.pauseOnBattery);
    setApps(s.openApps);
  };
  useEffect(() => {
    api
      .settingsGet()
      .then(take)
      .catch((e) => setProblem(String(e)));
  }, []);
  if (!info)
    return <section className="screen">{problem && <p className="error">{problem}</p>}</section>;
  const withNew =
    newExt.trim() && newApp.trim() ? [...apps, { ext: newExt.trim(), app: newApp.trim() }] : apps;
  const save = () => {
    setSaved(null);
    setProblem(null);
    api
      .settingsSet({
        cacheDir,
        cacheMaxBytes: Math.round(Number(limitGb) * GB),
        workers,
        proxyWorkers: proxy,
        pauseOnBattery: battery,
        openApps: withNew,
      })
      .then((r) => {
        take(r.settings);
        setNewExt("");
        setNewApp("");
        setOldCache(r.oldCache);
        setSaved("Saved.");
      })
      .catch((e) => setProblem(String(e)));
  };
  return (
    <section className="screen" aria-label="Settings" data-testid="settings-screen">
      <h1 className="heading">Settings</h1>
      <Library info={info} />

      <div className="card">
        <h2>Previews and thumbnails</h2>
        <p className="muted">
          Thumbnails, previews and playable copies of videos are kept in a cache, ideally on a fast
          drive. They can always be made again; your originals are never stored there.
        </p>
        <label className="block">
          Cache folder
          <div className="row">
            <input
              className="grow"
              value={cacheDir ?? ""}
              placeholder={`${info.cacheDirCustom ? "" : info.cacheDir} (default)`}
              onChange={(e) => setCacheDir(e.target.value || null)}
              data-testid="cache-dir"
            />
            <button
              onClick={() =>
                pickFolder("Folder for the cache")
                  .then((p) => p && setCacheDir(p))
                  .catch(() => undefined)
              }
            >
              Browse…
            </button>
            <button onClick={() => setCacheDir(null)} disabled={cacheDir === null}>
              Use default
            </button>
          </div>
        </label>
        <p className="muted" data-testid="cache-used">
          In use: {fmt.bytes(info.cacheBytes)} in {info.cacheDir}
        </p>
        {oldCache && (
          <p className="warn" data-testid="old-cache">
            The previous cache is still at {oldCache}.{" "}
            <button
              onClick={() =>
                api
                  .cacheRemoveOld(oldCache)
                  .then((n) => {
                    setOldCache(null);
                    setSaved(`Removed ${fmt.bytes(n)} from the old cache.`);
                  })
                  .catch((e) => setProblem(String(e)))
              }
            >
              Remove old files
            </button>
          </p>
        )}
        <div className="row">
          <label>
            Size limit (GB)
            <input
              type="number"
              min={0.25}
              step={1}
              value={limitGb}
              onChange={(e) => setLimitGb(e.target.value)}
              data-testid="cache-limit"
            />
          </label>
          <label>
            Image workers
            <select
              value={workers ?? 0}
              onChange={(e) => setWorkers(Number(e.target.value) || null)}
              data-testid="workers"
            >
              <option value={0}>Automatic ({info.workersDefault})</option>
              {Array.from({ length: 16 }, (_, i) => i + 1).map((n) => (
                <option key={n} value={n}>
                  {n}
                </option>
              ))}
            </select>
          </label>
          <label>
            Video workers
            <select value={proxy} onChange={(e) => setProxy(Number(e.target.value))}>
              {[1, 2, 3, 4].map((n) => (
                <option key={n} value={n}>
                  {n}
                </option>
              ))}
            </select>
          </label>
          <label className="check">
            <input
              type="checkbox"
              checked={battery}
              onChange={(e) => setBattery(e.target.checked)}
            />
            Pause on battery
          </label>
        </div>
      </div>

      <div className="card">
        <h2>Open in</h2>
        <p className="muted">
          Ctrl+O opens an item in another application. Types not listed open in the system default.
        </p>
        <table className="apps" data-testid="open-apps">
          <tbody>
            {apps.map((a, i) => (
              <tr key={a.ext}>
                <td>
                  <code>.{a.ext}</code>
                </td>
                <td className="grow">{a.app}</td>
                <td>
                  <button
                    aria-label={`Remove .${a.ext}`}
                    onClick={() => setApps(apps.filter((_, j) => j !== i))}
                  >
                    Remove
                  </button>
                </td>
              </tr>
            ))}
            <tr>
              <td>
                <input
                  placeholder="arw"
                  aria-label="File type"
                  size={6}
                  value={newExt}
                  onChange={(e) => setNewExt(e.target.value)}
                  data-testid="app-ext"
                />
              </td>
              <td className="grow">
                <input
                  className="grow"
                  placeholder="Application"
                  aria-label="Application"
                  value={newApp}
                  onChange={(e) => setNewApp(e.target.value)}
                  data-testid="app-path"
                />
              </td>
              <td>
                <button
                  onClick={() =>
                    pickFile("Choose an application")
                      .then((p) => p && setNewApp(p))
                      .catch(() => undefined)
                  }
                >
                  Browse…
                </button>
              </td>
            </tr>
          </tbody>
        </table>
      </div>

      <div className="save-bar">
        <span className="muted">Settings file: {info.configFile}</span>
        <span className="spacer" />
        {saved && (
          <span className="ok" role="status" data-testid="settings-saved">
            {saved}
          </span>
        )}
        {problem && (
          <span className="error" role="alert">
            {problem}
          </span>
        )}
        <button className="primary" onClick={save} data-testid="settings-save">
          Save settings
        </button>
      </div>

      <Presets />
    </section>
  );
}
