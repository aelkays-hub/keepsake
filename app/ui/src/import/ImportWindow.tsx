// The import window, opened by the device watcher when a Sony card is
// inserted: new items grouped by day (one tile per asset), select all or
// none, destinations and template (or a preset), a second destination,
// progress, and the safe-to-format result.

import { useEffect, useMemo, useRef, useState } from "react";

import {
  api,
  events,
  fileUrl,
  pickFolder,
  type CardScan,
  type ImportCheck,
  type ImportProgress,
  type ImportRequest,
  type ImportPrefs,
  type ImportResult,
  type Preset,
} from "../api";
import type { CardItem } from "../bindings/CardItem";
import * as fmt from "../format";

const DEFAULT_TEMPLATE = "{yyyy}/{yyyy}-{mm}-{dd}/{orig}";

function hashParam(name: string): string | null {
  const q = window.location.hash.split("?")[1] ?? "";
  return new URLSearchParams(q).get(name);
}

const mountFromHash = () => hashParam("mount");

/** Seconds a good automatic import's window stays before closing. */
const AUTO_CLOSE_S = 5;

function CardTile({
  mount,
  item,
  checked,
  onToggle,
}: {
  mount: string;
  item: CardItem;
  checked: boolean;
  onToggle: () => void;
}) {
  const ref = useRef<HTMLLabelElement>(null);
  const [src, setSrc] = useState<string | null>(null);
  // Thumbnails are made when the tile scrolls into view.
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    let done = false;
    const io = new IntersectionObserver((entries) => {
      if (done || !entries.some((e) => e.isIntersecting)) return;
      done = true;
      api
        .cardThumb(mount, item.files)
        .then((p) => setSrc(fileUrl(p)))
        .catch(() => undefined);
    });
    io.observe(el);
    return () => io.disconnect();
  }, [mount, item.files]);
  const name = item.files[0]?.split("/").pop() ?? item.key;
  return (
    <label
      ref={ref}
      className={`card-tile${checked ? " selected" : ""}${item.new ? "" : " known"}`}
      data-testid="card-tile"
    >
      <input type="checkbox" checked={checked} onChange={onToggle} aria-label={name} />
      {src ? <img src={src} alt="" /> : <div className="ph" />}
      <span className="name">{name}</span>
      <span className="badges">
        {item.kind === "video" && <span className="badge">▶</span>}
        {item.kind === "raw" && <span className="badge">RAW</span>}
        {!item.new && <span className="badge">in library</span>}
      </span>
    </label>
  );
}

export function ImportWindow() {
  const [mount, setMount] = useState<string | null>(mountFromHash());
  const [scan, setScan] = useState<CardScan | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [presets, setPresets] = useState<Preset[]>([]);
  const [preset, setPreset] = useState("");
  const [destA, setDestA] = useState("");
  const [destB, setDestB] = useState("");
  const [useB, setUseB] = useState(true);
  const [template, setTemplate] = useState(DEFAULT_TEMPLATE);
  const [eventName, setEventName] = useState("");
  const [check, setCheck] = useState<ImportCheck | null>(null);
  const [progress, setProgress] = useState<ImportProgress | null>(null);
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<ImportResult | null>(null);
  const [presetName, setPresetName] = useState("");
  // Opened because a card was inserted and Settings say to import by itself.
  const [auto, setAuto] = useState(hashParam("auto") === "1");
  const [prefs, setPrefs] = useState<ImportPrefs | null>(null);
  const [autoNote, setAutoNote] = useState<string | null>(null);
  // After a good automatic import (or nothing new), the window closes by
  // itself after a few seconds, unless asked to stay.
  const [closeIn, setCloseIn] = useState<number | null>(null);
  const keepOpen = useRef(false);
  const autoStarted = useRef(false);

  useEffect(() => {
    const un = (async () => {
      const { listen } = await import("@tauri-apps/api/event");
      const a = await listen<string>("import://open", (e) => {
        setMount(e.payload);
        setAuto(false);
        setResult(null);
      });
      const b = await listen<string>("import://auto", (e) => {
        setMount(e.payload);
        setAuto(true);
        setResult(null);
      });
      return () => {
        a();
        b();
      };
    })();
    return () => {
      un.then((f) => f());
    };
  }, []);
  useEffect(() => {
    autoStarted.current = false;
    keepOpen.current = false;
    setCloseIn(null);
    setAutoNote(null);
    if (!auto) return;
    api
      .settingsGet()
      .then((s) => setPrefs(s.import))
      .catch((e) => setAutoNote(`Not imported automatically: ${String(e)}`));
  }, [auto, mount]);
  useEffect(() => {
    if (!mount) return;
    setScan(null);
    setError(null);
    api
      .importScan(mount)
      .then((s) => {
        setScan(s);
        setSelected(new Set(s.items.filter((i) => i.new).map((i) => i.key)));
        if (s.preset && !auto) setPreset(s.preset);
      })
      .catch((e) => setError(String(e)));
  }, [mount, auto]);
  useEffect(() => {
    api.presetsList().then((p) => {
      setPresets(p);
    });
  }, []);
  // Applying a preset fills destinations and template.
  useEffect(() => {
    const p = presets.find((x) => x.name === preset);
    if (!p) return;
    setDestA(p.destinations[0] ?? "");
    setDestB(p.destinations[1] ?? "");
    setUseB(p.destinations.length > 1);
    setTemplate(p.template);
    setEventName(p.event ?? "");
  }, [preset, presets]);
  useEffect(() => {
    const a = events.importProgress(setProgress);
    const b = events.importDone((r) => {
      setResult(r);
      setRunning(false);
    });
    return () => {
      a.then((f) => f());
      b.then((f) => f());
    };
  }, []);

  const items = useMemo(() => scan?.items ?? [], [scan]);
  const groups = useMemo(() => {
    const m = new Map<string, CardItem[]>();
    for (const i of items) {
      const k = i.day ?? "Undated";
      m.set(k, [...(m.get(k) ?? []), i]);
    }
    return [...m.entries()];
  }, [items]);
  const destinations = [destA, useB ? destB : ""].filter((d) => d.trim());
  const request: ImportRequest | null =
    scan && mount
      ? {
          mount: scan.mount,
          label: scan.label,
          destinations,
          template,
          event: eventName || null,
          keys: [...selected],
        }
      : null;
  const chosen = items.filter((i) => selected.has(i.key));
  const reqKey = JSON.stringify(request);
  // Re-check (space, same-disk rule, example name) as settings change.
  useEffect(() => {
    if (!request) return;
    const t = setTimeout(() => {
      const first = chosen[0];
      api
        .importCheck(
          request,
          chosen.map((i) => Number(i.size)),
          first?.files[0] ?? null,
          null,
        )
        .then(setCheck)
        .catch((e) => setCheck({ ok: false, problem: String(e), space: [], example: null }));
    }, 250);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [reqKey]);

  // Automatic import: the destination and layout from Settings, every new
  // item, and start as soon as the check passes. A failed check stops it.
  const autoDest = auto && prefs?.auto ? prefs.destination : null;
  useEffect(() => {
    if (!autoDest || !prefs || !scan) return;
    setPreset("");
    setDestA(autoDest);
    setUseB(false);
    setTemplate(prefs.template);
    setEventName("");
    setSelected(new Set(scan.items.filter((i) => i.new).map((i) => i.key)));
  }, [autoDest, prefs, scan]);
  useEffect(() => {
    if (!autoDest || !prefs || !scan || !check || autoStarted.current || running || result) return;
    if (destinations.length !== 1 || destinations[0] !== autoDest) return;
    if (template !== prefs.template) return;
    if (selected.size === 0) {
      setAutoNote("Nothing new on this card: everything is already in your library.");
      autoStarted.current = true;
      if (!keepOpen.current) setCloseIn(AUTO_CLOSE_S);
      return;
    }
    if (!check.ok) {
      setAutoNote(`Not imported automatically: ${check.problem ?? "check the destination"}.`);
      autoStarted.current = true;
      return;
    }
    autoStarted.current = true;
    start();
    // `start` uses the current request; `check` follows it.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [autoDest, prefs, scan, check, selected, template, reqKey, running, result]);

  // A good automatic import: no error, nothing failed, not stopped.
  const autoDone =
    !!autoDest && !!result && !result.error && !result.cancelled && result.failed.length === 0;
  useEffect(() => {
    if (autoDone && !keepOpen.current) setCloseIn(AUTO_CLOSE_S);
  }, [autoDone]);
  useEffect(() => {
    if (closeIn === null) return;
    if (closeIn <= 0) {
      window.close();
      return;
    }
    const t = setTimeout(() => setCloseIn((n) => (n === null ? null : n - 1)), 1000);
    return () => clearTimeout(t);
  }, [closeIn]);
  const stayOpen = () => {
    keepOpen.current = true;
    setCloseIn(null);
  };

  const start = () => {
    if (!request) return;
    setResult(null);
    setProgress(null);
    setRunning(true);
    api.importStart(request).catch((e) => {
      setRunning(false);
      setResult({
        session: null,
        safeToFormat: false,
        notSafe: [],
        catalogued: 0,
        failed: [],
        cancelled: false,
        error: String(e),
      });
    });
  };
  const browse = (set: (v: string) => void) =>
    pickFolder("Choose a destination folder").then((p) => p && set(p));

  if (!mount)
    return (
      <main className="import">
        <p>Insert a card to import from it.</p>
      </main>
    );
  if (error)
    return (
      <main className="import">
        <p className="error" role="alert">
          {error}
        </p>
      </main>
    );
  if (!scan)
    return (
      <main className="import">
        <p>Reading the card…</p>
      </main>
    );
  const newCount = items.filter((i) => i.new).length;
  const pct =
    progress && Number(progress.bytesTotal) > 0
      ? (100 * Number(progress.bytesDone)) / Number(progress.bytesTotal)
      : 0;
  return (
    <main className="import" data-testid="import">
      <header>
        <h1>
          Import from “{scan.label}” <span className="muted">({scan.mount})</span>
        </h1>
        {autoDest && (
          <p className="auto-banner" role="status" data-testid="auto-banner">
            Importing new items automatically to <strong>{autoDest}</strong>, in folders by{" "}
            {prefs?.template === "{yyyy}/{month}/{yyyy}-{mm}-{dd}/{orig}"
              ? "year, month and day"
              : `“${prefs?.template}”`}
            . Change this in Settings → When a card is inserted.
          </p>
        )}
        {autoNote && (
          <p className="warn" role="alert" data-testid="auto-note">
            {autoNote}
          </p>
        )}
        <p data-testid="import-summary">
          {fmt.plural(newCount, "new item")}
          {scan.known ? " since last import" : ""} ({fmt.count(items.length)} on the card) ·{" "}
          {fmt.count(selected.size)} selected
        </p>
        <div className="row">
          <button
            onClick={() => setSelected(new Set(items.map((i) => i.key)))}
            data-testid="select-all"
          >
            Select all
          </button>
          <button onClick={() => setSelected(new Set())} data-testid="select-none">
            None
          </button>
          <button
            onClick={() => setSelected(new Set(items.filter((i) => i.new).map((i) => i.key)))}
          >
            New only
          </button>
        </div>
      </header>
      <section className="card-items" aria-label="Items on the card">
        {groups.map(([day, list]) => (
          <div key={day} className="day-group">
            <h2>
              {day} <span className="muted">({list.length})</span>
            </h2>
            <div className="card-grid">
              {list.map((i) => (
                <CardTile
                  key={i.key}
                  mount={scan.mount}
                  item={i}
                  checked={selected.has(i.key)}
                  onToggle={() => {
                    const s = new Set(selected);
                    if (s.has(i.key)) s.delete(i.key);
                    else s.add(i.key);
                    setSelected(s);
                  }}
                />
              ))}
            </div>
          </div>
        ))}
      </section>
      <section className="settings" aria-label="Import settings">
        <label>
          Preset
          <select value={preset} onChange={(e) => setPreset(e.target.value)} data-testid="preset">
            <option value="">(none)</option>
            {presets.map((p) => (
              <option key={p.name} value={p.name}>
                {p.name}
              </option>
            ))}
          </select>
        </label>
        <label className="grow">
          Template
          <input
            value={template}
            onChange={(e) => setTemplate(e.target.value)}
            data-testid="template"
          />
        </label>
        {check?.example && <span className="muted example">→ {check.example}</span>}
        <label>
          Event
          <input
            value={eventName}
            onChange={(e) => setEventName(e.target.value)}
            placeholder="optional"
          />
        </label>
        <div className="dest">
          <label className="grow">
            Destination
            <input value={destA} onChange={(e) => setDestA(e.target.value)} data-testid="dest-a" />
          </label>
          <button onClick={() => browse(setDestA)}>Browse…</button>
        </div>
        <div className="dest">
          <label>
            <input
              type="checkbox"
              checked={useB}
              onChange={(e) => setUseB(e.target.checked)}
              data-testid="use-b"
            />{" "}
            Second destination
          </label>
          <input
            value={destB}
            disabled={!useB}
            onChange={(e) => setDestB(e.target.value)}
            aria-label="Second destination"
            data-testid="dest-b"
          />
          <button disabled={!useB} onClick={() => browse(setDestB)}>
            Browse…
          </button>
        </div>
        {!useB && <p className="warn">With one destination the card will not be safe to format.</p>}
        {check && (
          <p className={check.ok ? "muted" : "warn"} data-testid="check">
            {check.problem ??
              check.space
                .map(
                  (s) =>
                    `${s.destination}: needs ${fmt.bytes(Number(s.needed))}, ${fmt.bytes(s.free === null ? null : Number(s.free))} free`,
                )
                .join(" · ")}
          </p>
        )}
        <div className="row">
          <input
            value={presetName}
            onChange={(e) => setPresetName(e.target.value)}
            placeholder="Save as preset…"
            aria-label="Preset name"
          />
          <button
            disabled={!presetName.trim() || destinations.length === 0}
            onClick={() =>
              api
                .presetSave({
                  name: presetName.trim(),
                  destinations,
                  template,
                  event: eventName || null,
                })
                .then(() => api.presetsList().then(setPresets))
                .then(() => setPreset(presetName.trim()))
                .catch((e) => alert(String(e)))
            }
          >
            Save preset
          </button>
          <span className="spacer" />
          <button onClick={() => window.close()}>Close</button>
          <button
            className="primary"
            disabled={running || !check?.ok}
            onClick={start}
            data-testid="start-import"
          >
            Import {fmt.count(selected.size)}
          </button>
        </div>
      </section>
      {(running || progress) && !result && (
        <section className="progress" aria-label="Progress" data-testid="progress">
          <progress max={100} value={pct} />
          <span>
            {progress?.phase ??
              `${fmt.bytes(Number(progress?.bytesDone ?? 0))} / ${fmt.bytes(Number(progress?.bytesTotal ?? 0))}`}{" "}
            {progress?.current && <span className="muted">· {progress.current}</span>}
          </span>
          <button onClick={() => api.importCancel()} data-testid="stop-import">
            Stop
          </button>
        </section>
      )}
      {result && (
        <section
          className={`result ${result.safeToFormat ? "ok" : "bad"}`}
          role="status"
          data-testid="result"
        >
          {result.error ? (
            <p>Import failed: {result.error}</p>
          ) : autoDone && !result.safeToFormat ? (
            <p>
              ✅ Copied {fmt.plural(Number(result.catalogued), "item")} to{" "}
              <strong>{autoDest}</strong>, each checked. One copy only: the card is{" "}
              <strong>NOT safe to format</strong>.
            </p>
          ) : result.safeToFormat ? (
            <p>
              ✅ <strong>SAFE TO FORMAT</strong>: all {fmt.count(Number(result.catalogued))} assets
              are verified on two drives.
            </p>
          ) : (
            <>
              <p>
                ⚠ <strong>NOT safe to format</strong>
                {result.cancelled
                  ? " (stopped after the current file; run the import again to continue)"
                  : ""}
                .
              </p>
              <ul>
                {result.notSafe.slice(0, 8).map((r) => (
                  <li key={r}>{r}</li>
                ))}
                {result.failed.slice(0, 8).map((r) => (
                  <li key={r}>{r}</li>
                ))}
              </ul>
            </>
          )}
        </section>
      )}
      {closeIn !== null && (
        <p className="row auto-close" role="status" data-testid="auto-close">
          Closing in {closeIn} s…
          <button onClick={stayOpen} data-testid="keep-open">
            Keep open
          </button>
        </p>
      )}
    </main>
  );
}
