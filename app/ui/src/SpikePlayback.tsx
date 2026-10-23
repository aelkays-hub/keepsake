/**
 * Spike playback test page — Gate 1 validation only.
 *
 * Tests:
 *   1. Camera proxy (or app proxy) plays in Tauri webview with seeking.
 *   2. Thumbnail grid of 2,000+ items scrolls smoothly.
 *
 * Run: pnpm tauri dev → click "spike →" on the placeholder screen.
 */

import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { convertFileSrc } from "@tauri-apps/api/core";
import { useVirtualizer } from "@tanstack/react-virtual";

const CORPUS = "C:/Users/Ahmad El-Kays/keepsake-corpus";
const LIBRARY_SAMPLE = `${CORPUS}/library-sample`;
const CLIP_PATH = `${CORPUS}/cards/ILCE-7M4/PRIVATE/M4ROOT/CLIP/C0006.MP4`;

// Platform temp dir where spike-playback.ps1 writes proxies
const PROXY_DIR = "C:/Users/Ahmad El-Kays/AppData/Local/Temp/keepsake-spike-proxies";

// ── Source-type inference from filename ─────────────────────────────────────

function proxyLabel(filename: string): string {
  const f = filename.toLowerCase();
  if (/^c\d{4}-proxy/.test(filename.split("/").pop() ?? "")) return "XAVC S (corpus)";
  if (f.includes("synthetic_4k")) return "Synthetic 4K (HEVC stand-in)";
  if (f.includes("synthetic_intra") || f.includes("intra")) return "Synthetic intra (XAVC S-I)";
  if (f.includes("proxy")) return "Proxy";
  return "Source clip";
}

// ── Shared video player ──────────────────────────────────────────────────────

interface VideoPlayerProps {
  activePath: string;
  onPathChange: (p: string) => void;
}

function VideoPlayer({ activePath, onPathChange }: VideoPlayerProps) {
  const [src, setSrc] = useState(() => convertFileSrc(activePath));
  const [inputPath, setInputPath] = useState(activePath);
  const [log, setLog] = useState<string[]>([]);
  const videoRef = useRef<HTMLVideoElement>(null);

  // Sync when parent changes active path (e.g. clicking a proxy row)
  useEffect(() => {
    setSrc(convertFileSrc(activePath));
    setInputPath(activePath);
    addLog(`Loading: ${activePath.split("/").pop()}`);
  }, [activePath]);

  const addLog = (msg: string) =>
    setLog((prev) => [`${new Date().toISOString().slice(11, 23)}  ${msg}`, ...prev.slice(0, 29)]);

  const loadFromInput = () => {
    onPathChange(inputPath);
  };

  const testSeek = () => {
    const v = videoRef.current;
    if (!v || !v.duration) {
      addLog("Seek test: video not ready");
      return;
    }
    const targets = [v.duration * 0.5, v.duration * 0.9, 0.1, v.duration * 0.3];
    let i = 0;
    const step = () => {
      if (i >= targets.length) {
        addLog("Seek test: done");
        return;
      }
      const t0 = performance.now();
      v.currentTime = targets[i];
      const done = () => {
        addLog(`Seek to ${targets[i].toFixed(1)}s → ${(performance.now() - t0).toFixed(0)} ms`);
        i++;
        v.removeEventListener("seeked", done);
        step();
      };
      v.addEventListener("seeked", done);
      i++;
    };
    step();
  };

  return (
    <section>
      <h2>Player</h2>

      <div style={{ display: "flex", gap: 8, marginBottom: 8 }}>
        <input
          value={inputPath}
          onChange={(e) => setInputPath(e.target.value)}
          style={{
            flex: 1,
            fontFamily: "monospace",
            fontSize: 12,
            background: "#222",
            color: "#eee",
            border: "1px solid #444",
            padding: "2px 6px",
          }}
        />
        <button onClick={loadFromInput}>Load</button>
      </div>

      <video
        ref={videoRef}
        src={src}
        controls
        style={{ width: "100%", maxHeight: 400, background: "#000" }}
        onLoadStart={() => addLog("loadstart")}
        onCanPlay={() => addLog("canplay ✓")}
        onError={(e) => {
          const v = e.currentTarget;
          addLog(`error: code=${v.error?.code} "${v.error?.message}"`);
        }}
        onSeeking={() => addLog("seeking…")}
        onSeeked={() => addLog("seeked ✓")}
      />

      <div style={{ marginTop: 8 }}>
        <button onClick={testSeek}>Run seek test</button>
      </div>

      <pre
        style={{
          marginTop: 8,
          fontSize: 11,
          background: "#111",
          color: "#0f0",
          padding: 8,
          height: 120,
          overflow: "auto",
        }}
      >
        {log.join("\n")}
      </pre>
    </section>
  );
}

// ── Proxy list ───────────────────────────────────────────────────────────────

interface ProxyListProps {
  activePath: string;
  onSelect: (p: string) => void;
}

function ProxyList({ activePath, onSelect }: ProxyListProps) {
  const [proxies, setProxies] = useState<string[]>([]);
  const [status, setStatus] = useState("idle");

  const load = () => {
    setStatus("scanning…");
    invoke<string[]>("spike_list_files", { dir: PROXY_DIR })
      .then((paths) => {
        const mp4s = paths.filter((p) => p.toLowerCase().endsWith(".mp4"));
        setProxies(mp4s);
        setStatus(
          mp4s.length === 0
            ? "No proxies found — run scripts/spike-playback.ps1 first"
            : `${mp4s.length} proxies`,
        );
      })
      .catch((e: unknown) => setStatus(`Error: ${String(e)}`));
  };

  useEffect(load, []);

  // Also include source corpus clips at the top
  const sourceClips: { path: string; label: string }[] = [
    {
      path: `${CORPUS}/cards/ILCE-7M4/PRIVATE/M4ROOT/CLIP/C0006.MP4`,
      label: "XAVC S source — C0006",
    },
    {
      path: `${CORPUS}/cards/ILCE-7M4/PRIVATE/M4ROOT/CLIP/C0007.MP4`,
      label: "XAVC S source — C0007",
    },
    {
      path: `${CORPUS}/cards/ILCE-7M4/PRIVATE/M4ROOT/CLIP/C0008.MP4`,
      label: "XAVC S source — C0008",
    },
  ];

  const proxyRows = proxies.map((p) => ({
    path: p,
    label: proxyLabel(p),
  }));

  const allRows = [...sourceClips, ...proxyRows];

  return (
    <section style={{ marginTop: 32 }}>
      <h2 style={{ marginBottom: 4 }}>Proxy / source list</h2>
      <p style={{ fontSize: 12, color: "#888", marginBottom: 8 }}>
        {status} &nbsp;
        <button onClick={load} style={{ fontSize: 11 }}>
          Refresh
        </button>
      </p>

      <table style={{ width: "100%", borderCollapse: "collapse", fontSize: 13 }}>
        <thead>
          <tr style={{ color: "#888", textAlign: "left", borderBottom: "1px solid #333" }}>
            <th style={{ padding: "4px 8px", fontWeight: "normal" }}>File</th>
            <th style={{ padding: "4px 8px", fontWeight: "normal" }}>Source type</th>
          </tr>
        </thead>
        <tbody>
          {allRows.map(({ path, label }) => {
            const isActive = path === activePath;
            const filename = path.split("/").pop() ?? path;
            return (
              <tr
                key={path}
                onClick={() => onSelect(path)}
                style={{
                  cursor: "pointer",
                  background: isActive ? "#1e3a5f" : "transparent",
                  borderBottom: "1px solid #222",
                }}
                onMouseEnter={(e) => {
                  if (!isActive) (e.currentTarget as HTMLElement).style.background = "#1a1a1a";
                }}
                onMouseLeave={(e) => {
                  if (!isActive) (e.currentTarget as HTMLElement).style.background = "transparent";
                }}
              >
                <td style={{ padding: "5px 8px", fontFamily: "monospace", fontSize: 12 }}>
                  {isActive ? "▶ " : ""}
                  {filename}
                </td>
                <td style={{ padding: "5px 8px", color: "#aaa" }}>{label}</td>
              </tr>
            );
          })}
          {allRows.length === 0 && (
            <tr>
              <td colSpan={2} style={{ padding: "8px", color: "#666" }}>
                No files — run <code>scripts/spike-playback.ps1</code> to generate proxies.
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </section>
  );
}

// ── Item 5: Thumbnail grid scroll ───────────────────────────────────────────

function GridTest() {
  const [files, setFiles] = useState<string[]>([]);
  const [status, setStatus] = useState("idle");
  const [fps, setFps] = useState<number | null>(null);
  const parentRef = useRef<HTMLDivElement>(null);

  const COLS = 5;
  const ITEM_H = 140;

  const rows = Math.ceil(files.length / COLS);

  const virtualizer = useVirtualizer({
    count: rows,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ITEM_H,
    overscan: 5,
  });

  useEffect(() => {
    setStatus("loading…");
    invoke<string[]>("spike_list_files", { dir: LIBRARY_SAMPLE })
      .then((paths) => {
        setFiles(paths);
        setStatus(`${paths.length} files loaded`);
      })
      .catch((e: unknown) => setStatus(`Error: ${String(e)}`));
  }, []);

  const frameRef = useRef<number | null>(null);
  const lastTs = useRef<number>(0);
  const frameTimes = useRef<number[]>([]);

  const measureFps = () => {
    const measure = (ts: number) => {
      if (lastTs.current) {
        frameTimes.current.push(ts - lastTs.current);
        if (frameTimes.current.length > 60) frameTimes.current.shift();
        const avg = frameTimes.current.reduce((a, b) => a + b, 0) / frameTimes.current.length;
        setFps(Math.round(1000 / avg));
      }
      lastTs.current = ts;
      frameRef.current = requestAnimationFrame(measure);
    };
    frameRef.current = requestAnimationFrame(measure);
  };

  const stopFps = () => {
    if (frameRef.current) cancelAnimationFrame(frameRef.current);
  };

  return (
    <section style={{ marginTop: 32 }}>
      <h2>Item 5 — Thumbnail grid ({files.length} items)</h2>
      <p style={{ fontSize: 12 }}>
        Status: {status} &nbsp;|&nbsp; FPS: {fps !== null ? <strong>{fps}</strong> : "—"}
      </p>
      <div style={{ marginBottom: 8 }}>
        <button onClick={measureFps}>Start FPS meter</button>{" "}
        <button onClick={stopFps}>Stop</button>
      </div>

      <div
        ref={parentRef}
        style={{ height: 600, overflow: "auto", border: "1px solid #333", background: "#1a1a1a" }}
      >
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((vrow) => {
            const start = vrow.index * COLS;
            const rowFiles = files.slice(start, start + COLS);
            return (
              <div
                key={vrow.key}
                style={{
                  position: "absolute",
                  top: vrow.start,
                  left: 0,
                  right: 0,
                  height: ITEM_H,
                  display: "flex",
                  gap: 4,
                  padding: "4px",
                }}
              >
                {rowFiles.map((f) => (
                  <div
                    key={f}
                    style={{ flex: 1, background: "#333", overflow: "hidden", borderRadius: 2 }}
                  >
                    <img
                      src={convertFileSrc(f)}
                      loading="lazy"
                      decoding="async"
                      style={{ width: "100%", height: "100%", objectFit: "cover" }}
                      alt=""
                    />
                  </div>
                ))}
              </div>
            );
          })}
        </div>
      </div>
    </section>
  );
}

// ── Root ─────────────────────────────────────────────────────────────────────

export default function SpikePlayback() {
  const [activePath, setActivePath] = useState(CLIP_PATH);

  return (
    <div
      style={{
        fontFamily: "system-ui, sans-serif",
        maxWidth: 960,
        margin: "0 auto",
        padding: 16,
        color: "#eee",
        background: "#0d0d0d",
        minHeight: "100vh",
      }}
    >
      <h1 style={{ fontSize: 18, marginBottom: 4 }}>Spike: Playback Gate 1</h1>
      <p style={{ fontSize: 12, color: "#888", marginBottom: 24 }}>
        Temporary spike page — not a product feature.
      </p>

      <VideoPlayer activePath={activePath} onPathChange={setActivePath} />
      <ProxyList activePath={activePath} onSelect={setActivePath} />
      <GridTest />
    </div>
  );
}
