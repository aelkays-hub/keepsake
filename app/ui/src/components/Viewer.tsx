// The full-screen viewer for the focused item.
//
// Photos: the preview fits the window; Z, a click or the mouse wheel zooms
// to 100% from a full-size picture made on demand (a RAW is decoded by
// LibRaw). Drag or arrows pan while zoomed. R toggles JPEG | RAW for pairs.
// Video: the proxy with a scrub bar, Space play/pause, , and . step one
// frame, J and L jump a second, and the timecode of the frame on screen.
// Arrows (when not zoomed) move through the selection, or the whole view
// when one item is selected. F: full screen. Esc: close. 0–5 and 6–9 rate
// and label the item shown; Ctrl+O opens it in another application.

import { useCallback, useEffect, useRef, useState } from "react";

import {
  api,
  fileUrl,
  type AssetMembers,
  type Details,
  type MemberDto,
  type Row,
  type VideoInfo,
} from "../api";
import { useFocusTrap } from "../a11y";
import * as fmt from "../format";
import { LABEL_KEYS } from "../keys";
import * as sel from "../selection";
import { useUi } from "../state";

async function toggleFullscreen() {
  try {
    const { getCurrentWindow } = await import("@tauri-apps/api/window");
    const w = getCurrentWindow();
    await w.setFullscreen(!(await w.isFullscreen()));
  } catch {
    // Not in the app (browser tests): the page's own full screen.
    if (document.fullscreenElement) await document.exitFullscreen().catch(() => undefined);
    else await document.documentElement.requestFullscreen().catch(() => undefined);
  }
}

/** Seconds per frame. */
const frameS = (v: VideoInfo | null) => (v ? v.fpsDen / v.fpsNum : 1 / 30);

function VideoStage({ d, onKeyRef }: { d: Details; onKeyRef: React.MutableRefObject<KeyFn> }) {
  const video = useRef<HTMLVideoElement>(null);
  const [info, setInfo] = useState<VideoInfo | null>(null);
  const [t, setT] = useState(0);
  const [dur, setDur] = useState((d.durationMs ?? 0) / 1000);
  const [playing, setPlaying] = useState(false);
  useEffect(() => {
    let live = true;
    api
      .videoInfo(d.id)
      .then((v) => live && setInfo(v))
      .catch(() => undefined);
    return () => {
      live = false;
    };
  }, [d.id]);
  const seek = useCallback((to: number) => {
    const v = video.current;
    if (!v) return;
    v.currentTime = Math.max(0, Math.min(v.duration || to, to));
    setT(v.currentTime);
  }, []);
  const step = useCallback(
    (frames: number) => {
      const v = video.current;
      if (!v) return;
      v.pause();
      const f = frameS(info);
      // Land in the middle of the target frame: no rounding surprises.
      const n = fmt.frameAt(
        info ? { num: info.fpsNum, den: info.fpsDen } : { num: 30, den: 1 },
        v.currentTime,
      );
      seek((n + frames + 0.5) * f);
    },
    [info, seek],
  );
  const play = useCallback(() => {
    const v = video.current;
    if (!v) return;
    if (v.paused) void v.play().catch(() => undefined);
    else v.pause();
  }, []);
  onKeyRef.current = (e) => {
    switch (e.key) {
      case " ":
      case "k":
      case "K":
        play();
        return true;
      case ",":
        step(-1);
        return true;
      case ".":
        step(1);
        return true;
      case "j":
      case "J":
        seek((video.current?.currentTime ?? 0) - 1);
        return true;
      case "l":
      case "L":
        seek((video.current?.currentTime ?? 0) + 1);
        return true;
    }
    return false;
  };
  const rate = info ? { num: info.fpsNum, den: info.fpsDen } : { num: 30, den: 1 };
  const src = fileUrl(d.proxy);
  return (
    <div className="viewer-video">
      <div className="viewer-stage">
        {src ? (
          <video
            ref={video}
            src={src}
            data-testid="viewer-video"
            onClick={play}
            onTimeUpdate={(e) => setT(e.currentTarget.currentTime)}
            onSeeked={(e) => setT(e.currentTarget.currentTime)}
            onLoadedMetadata={(e) => setDur(e.currentTarget.duration)}
            onPlay={() => setPlaying(true)}
            onPause={() => setPlaying(false)}
          />
        ) : (
          <div className="viewer-wait">
            {d.preview && <img src={fileUrl(d.preview) ?? undefined} alt="" />}
            <p>
              {d.online ? "Making a playable copy…" : "The drive with this clip is not connected."}
            </p>
          </div>
        )}
      </div>
      <div className="viewer-controls" aria-label="Playback">
        <button onClick={play} disabled={!src} aria-label={playing ? "Pause" : "Play"}>
          {playing ? "❚❚" : "▶"}
        </button>
        <button
          onClick={() => step(-1)}
          disabled={!src}
          aria-label="Previous frame"
          title="Previous frame (,)"
        >
          ‹|
        </button>
        <button
          onClick={() => step(1)}
          disabled={!src}
          aria-label="Next frame"
          title="Next frame (.)"
        >
          |›
        </button>
        <input
          type="range"
          className="scrub"
          aria-label="Position"
          min={0}
          max={dur || 0}
          step="any"
          value={Math.min(t, dur || 0)}
          disabled={!src}
          onChange={(e) => seek(Number(e.target.value))}
        />
        <span className="timecode" data-testid="timecode" title="Timecode">
          {fmt.timecode(info?.startTimecode ?? null, rate, t)}
        </span>
        <span className="muted" data-testid="frame">
          frame {fmt.frameAt(rate, t)}
        </span>
        <span className="muted">
          {fmt.duration(t * 1000)} / {fmt.duration(dur * 1000)}
        </span>
      </div>
    </div>
  );
}

type KeyFn = (e: KeyboardEvent) => boolean;

function PhotoStage({
  d,
  members,
  member,
  onKeyRef,
}: {
  d: Details;
  members: AssetMembers | null;
  member: MemberDto;
  onKeyRef: React.MutableRefObject<KeyFn>;
}) {
  const [zoom, setZoom] = useState(false);
  const [full, setFull] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const stage = useRef<HTMLDivElement>(null);
  const focal = useRef<[number, number]>([0.5, 0.5]);
  const drag = useRef<{ x: number; y: number; sx: number; sy: number; moved: boolean } | null>(
    null,
  );
  // A RAW member has no preview of its own: it is always the full decode.
  const needFull = zoom || member === "raw" || (members?.rawOnly ?? false);
  const key = `${d.id}:${member}`;
  useEffect(() => {
    if (!needFull || full[key] || !d.online) return;
    let live = true;
    setLoading(true);
    setError(null);
    api
      .fullImage(d.id, member)
      .then((p) => live && setFull((f) => ({ ...f, [key]: p })))
      .catch((e) => live && setError(String(e)))
      .finally(() => live && setLoading(false));
    return () => {
      live = false;
    };
  }, [needFull, key, d.id, d.online, member, full]);
  const shown = needFull && full[key] ? fileUrl(full[key]) : fileUrl(d.preview);
  const zoomed = zoom && !!full[key];
  const zoomAt = (clientX: number, clientY: number, on: boolean) => {
    const img = stage.current?.querySelector("img");
    if (on && img) {
      const r = img.getBoundingClientRect();
      focal.current = [
        Math.max(0, Math.min(1, (clientX - r.left) / r.width)),
        Math.max(0, Math.min(1, (clientY - r.top) / r.height)),
      ];
    }
    setZoom(on);
  };
  const recentre = () => {
    const s = stage.current;
    if (!s || !zoomed) return;
    s.scrollLeft = focal.current[0] * s.scrollWidth - s.clientWidth / 2;
    s.scrollTop = focal.current[1] * s.scrollHeight - s.clientHeight / 2;
  };
  onKeyRef.current = (e) => {
    const s = stage.current;
    if (e.key === "z" || e.key === "Z") {
      focal.current = [0.5, 0.5];
      setZoom((z) => !z);
      return true;
    }
    if (zoomed && s) {
      const by = e.shiftKey ? 400 : 100;
      const pan: Record<string, [number, number]> = {
        ArrowLeft: [-by, 0],
        ArrowRight: [by, 0],
        ArrowUp: [0, -by],
        ArrowDown: [0, by],
      };
      const p = pan[e.key];
      if (p) {
        s.scrollBy(p[0], p[1]);
        return true;
      }
    }
    return false;
  };
  return (
    <div
      ref={stage}
      className={`viewer-stage${zoomed ? " zoomed" : ""}`}
      data-testid="viewer-stage"
      data-zoom={zoomed ? "100" : "fit"}
      onWheel={(e) => {
        if (e.deltaY < 0 && !zoom) zoomAt(e.clientX, e.clientY, true);
        else if (e.deltaY > 0 && zoom) setZoom(false);
      }}
      onMouseDown={(e) => {
        const s = stage.current;
        if (!s || e.button !== 0) return;
        drag.current = {
          x: e.clientX,
          y: e.clientY,
          sx: s.scrollLeft,
          sy: s.scrollTop,
          moved: false,
        };
      }}
      onMouseMove={(e) => {
        const g = drag.current;
        const s = stage.current;
        if (!g || !s) return;
        if (Math.abs(e.clientX - g.x) + Math.abs(e.clientY - g.y) > 4) g.moved = true;
        if (zoomed && g.moved) {
          s.scrollLeft = g.sx - (e.clientX - g.x);
          s.scrollTop = g.sy - (e.clientY - g.y);
        }
      }}
      onMouseUp={(e) => {
        const g = drag.current;
        drag.current = null;
        if (g && !g.moved) zoomAt(e.clientX, e.clientY, !zoom);
      }}
    >
      {shown && (
        <img
          src={shown}
          alt={d.name ?? ""}
          data-testid="viewer-image"
          data-member={member}
          data-full={needFull && full[key] ? "1" : "0"}
          draggable={false}
          onLoad={(e) => {
            const img = e.currentTarget;
            if (zoomed) {
              // 100%: one picture pixel per screen pixel.
              img.style.width = `${img.naturalWidth / window.devicePixelRatio}px`;
              requestAnimationFrame(recentre);
            } else img.style.width = "";
          }}
          style={zoomed ? undefined : { width: "" }}
        />
      )}
      {(loading || error || !d.online) && (
        <p className="viewer-note" role="status">
          {error
            ? error
            : !d.online
              ? "The drive with this photo is not connected: showing the preview."
              : member === "raw" || members?.rawOnly
                ? "Decoding the RAW…"
                : "Loading full size…"}
        </p>
      )}
    </div>
  );
}

export function Viewer() {
  const { s, dispatch, view } = useUi();
  const index = s.sel.focus;
  const count = view?.count ?? 0;
  const [row, setRow] = useState<Row | null>(null);
  const [d, setD] = useState<Details | null>(null);
  const [members, setMembers] = useState<AssetMembers | null>(null);
  const [member, setMember] = useState<MemberDto>("primary");
  const [note, setNote] = useState<string | null>(null);
  const stageKey = useRef<KeyFn>(() => false);
  const root = useRef<HTMLDivElement>(null);
  useFocusTrap(root);
  const close = useCallback(() => dispatch({ t: "viewer", open: false }), [dispatch]);

  useEffect(() => {
    if (index === null || !view) {
      close();
      return;
    }
    let live = true;
    api
      .viewRows(view.view, index, 1)
      .then(async ([r]) => {
        if (!live || !r) return;
        setRow(r);
        const [det, m] = await Promise.all([api.details(r.id), api.assetMembers(r.id)]);
        if (!live) return;
        setD(det);
        setMembers(m);
        setMember("primary");
      })
      .catch((e) => live && setNote(String(e)));
    return () => {
      live = false;
    };
  }, [index, view, s.revision, close]);

  // A video without its proxy yet: have it made first, and look again.
  useEffect(() => {
    if (!d || d.kind !== "video" || d.proxy || !d.online) return;
    void api.renderSetVisible([d.id]).catch(() => undefined);
    const t = setInterval(() => {
      api
        .details(d.id)
        .then((x) => x.proxy && setD(x))
        .catch(() => undefined);
    }, 2000);
    return () => clearInterval(t);
  }, [d]);

  // Keyboard focus comes back to where the viewer was opened from.
  useEffect(() => {
    const back = document.activeElement as HTMLElement | null;
    root.current?.focus();
    return () => back?.focus();
  }, []);

  const go = (dir: "next" | "prev" | "first" | "last") => {
    if (index === null) return;
    const to = sel.viewerStep(s.sel, count, index, dir);
    if (to === null) return;
    // Through a selection: it stays. Through the view: select what is shown.
    dispatch({ t: "sel", sel: sel.size(s.sel) > 1 ? { ...s.sel, focus: to } : sel.only(to) });
  };
  const only = row ? { ids: [row.id], view: null, ranges: [] as [number, number][] } : null;
  const edited = () => dispatch({ t: "edited" });
  const openElsewhere = () => {
    if (!row) return;
    api
      .openExternal(row.id, member)
      .then((app) => setNote(`Opened with ${app}`))
      .catch((e) => setNote(String(e)));
  };

  const onKey = (e: React.KeyboardEvent) => {
    const k = e.nativeEvent;
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "o") {
      openElsewhere();
    } else if (e.ctrlKey || e.metaKey || e.altKey) {
      return;
    } else if (stageKey.current(k)) {
      // handled by the photo or video
    } else if (e.key === "Escape") {
      close();
    } else if (e.key === "ArrowRight" || e.key === "ArrowDown" || e.key === "PageDown") {
      go("next");
    } else if (e.key === "ArrowLeft" || e.key === "ArrowUp" || e.key === "PageUp") {
      go("prev");
    } else if (e.key === "Home") {
      go("first");
    } else if (e.key === "End") {
      go("last");
    } else if (e.key === "f" || e.key === "F") {
      void toggleFullscreen();
    } else if ((e.key === "r" || e.key === "R") && members?.hasPair) {
      setMember((m) => (m === "raw" ? "primary" : "raw"));
    } else if (/^[0-5]$/.test(e.key) && only) {
      api.editRating(only, Number(e.key)).then(edited).catch(console.error);
    } else if (e.key in LABEL_KEYS && only) {
      const l = LABEL_KEYS[e.key];
      api
        .editLabel(only, row?.label === l ? null : l)
        .then(edited)
        .catch(console.error);
    } else return;
    e.preventDefault();
    e.stopPropagation();
  };

  const position =
    index === null
      ? ""
      : sel.size(s.sel) > 1
        ? `${fmt.count(index + 1)} of ${fmt.count(count)} · ${fmt.plural(sel.size(s.sel), "selected", "selected")}`
        : `${fmt.count(index + 1)} of ${fmt.count(count)}`;
  return (
    <div
      ref={root}
      className="viewer"
      role="dialog"
      aria-modal="true"
      aria-label="Viewer"
      tabIndex={-1}
      onKeyDown={onKey}
      data-testid="viewer"
    >
      <header className="viewer-bar">
        <button onClick={close} aria-label="Close viewer" title="Close (Esc)">
          ✕
        </button>
        <span className="viewer-name" data-testid="viewer-name">
          {row?.name ?? ""}
        </span>
        <span className="muted">{fmt.dateTime(row?.captured)}</span>
        <span className="muted" data-testid="viewer-position">
          {position}
        </span>
        <span className="spacer" />
        {members?.hasPair && (
          <div className="segmented" role="tablist" aria-label="Show">
            {(["primary", "raw"] as const).map((m) => (
              <button
                key={m}
                role="tab"
                aria-selected={member === m}
                onClick={() => setMember(m)}
                title="Toggle with R"
              >
                {m === "primary" ? "JPEG" : "RAW"}
              </button>
            ))}
          </div>
        )}
        {row && <span className="stars-text">{"★".repeat(row.rating)}</span>}
        <button
          onClick={openElsewhere}
          disabled={!row}
          title="Open in another application (Ctrl+O)"
        >
          Open in…
        </button>
        <button
          onClick={() => void toggleFullscreen()}
          title="Full screen (F)"
          aria-label="Full screen"
        >
          ⛶
        </button>
      </header>
      {d && row?.id === d.id ? (
        d.kind === "video" ? (
          <VideoStage key={d.id} d={d} onKeyRef={stageKey} />
        ) : (
          <PhotoStage key={d.id} d={d} members={members} member={member} onKeyRef={stageKey} />
        )
      ) : (
        <div className="viewer-stage" />
      )}
      <footer className="viewer-foot">
        <button onClick={() => go("prev")} aria-label="Previous item">
          ‹
        </button>
        <span className="muted">
          ← → move · Z zoom · {d?.kind === "video" ? "Space play · , . frame · J L second · " : ""}
          {members?.hasPair ? "R JPEG/RAW · " : ""}F full screen · Esc close
        </span>
        {note && (
          <span role="status" data-testid="viewer-note">
            {note}
          </span>
        )}
        <span className="spacer" />
        <button onClick={() => go("next")} aria-label="Next item">
          ›
        </button>
      </footer>
    </div>
  );
}
