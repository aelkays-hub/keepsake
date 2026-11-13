// The full-screen viewer for the focused item.
//
// Photos: the preview fits the window. The zoom bar (Fit, -, a slider up to
// 400%, +, 100%), the + and - keys and the mouse wheel zoom; Z or a click
// switch between Fit and 100%. Zoom shows a full-size picture made on demand
// (a RAW is decoded by LibRaw). Drag or arrows pan while zoomed. R toggles
// JPEG | RAW for pairs.
// Video: the proxy with a scrub bar, Space play/pause, , and . step one
// frame, J and L jump a second, and the timecode of the frame on screen.
// Arrows (when not zoomed) move through the selection, or the whole view
// when one item is selected. F: full screen. Esc: close. 0–5 and 6–9 rate
// and label the item shown; Ctrl+O opens it in another application. I (or
// the Info button) shows the item's metadata in a panel on the right; the
// picture's area shrinks to make room, and the choice is remembered.

import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";

import {
  type FaceDto,
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
import { ContextMenu } from "./ContextMenu";
import { OpenInMenu } from "./OpenInMenu";
import { ItemMenu } from "./ItemMenu";
import { isMenuClick, isMenuKey } from "./useItemMenu";
import { setViewerInfo, useViewerInfo } from "../viewerInfo";
import { ViewerInfo } from "./ViewerInfo";

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

/** The highest zoom, in percent of the picture's own pixels. */
const MAX_ZOOM = 400;
/** One step of + and −. */
const STEP = 1.25;
/** Slider positions: 0 is Fit, the end is the highest zoom (a log scale). */
const SLIDER = 1000;
/** Time on a photo before its full size is made, ready for zooming. */
const PREFETCH_MS = 500;

const clamp01 = (x: number) => Math.max(0, Math.min(1, x));

/** The full size guessed from a preview (fitted to 2560 px, never enlarged). */
function guessFull(w: number, h: number): [number, number] {
  const long = Math.max(w, h);
  const f = long >= 2500 ? 6000 / long : 1;
  return [w * f, h * f];
}

/** Where a picture of `size` CSS px sits along a window of `box`: centred while
 * smaller, otherwise never leaving a gap at either side. */
const place = (t: number, size: number, box: number) =>
  size <= box ? (box - size) / 2 : Math.min(0, Math.max(box - size, t));

/** The zoom ("fit" or a percentage) and the picture point (0–1) at the window's centre. */
interface View {
  pct: number | null;
  fx: number;
  fy: number;
}
const FIT: View = { pct: null, fx: 0.5, fy: 0.5 };

function PhotoStage({
  d,
  members,
  member,
  onKeyRef,
  overlay,
}: {
  /** Drawn over the picture, at its size and zoom (the face boxes); gets
   * the zoom, so lines and labels can keep their size. */
  overlay?: (scale: number) => React.ReactNode;
  d: Details;
  members: AssetMembers | null;
  member: MemberDto;
  onKeyRef: React.MutableRefObject<KeyFn>;
}) {
  const [view, setView] = useState<View>(FIT);
  /** Full-size pictures, decoded and ready: path and natural size. */
  const [full, setFull] = useState<Record<string, { path: string; w: number; h: number }>>({});
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [prefetch, setPrefetch] = useState(false);
  /** Natural size of the preview. */
  const [pre, setPre] = useState<{ w: number; h: number } | null>(null);
  const [box, setBox] = useState({ w: 0, h: 0 });
  const stage = useRef<HTMLDivElement>(null);
  const imgRef = useRef<HTMLImageElement>(null);
  const drag = useRef<{ x: number; y: number; tx: number; ty: number; moved: boolean } | null>(
    null,
  );
  useLayoutEffect(() => {
    const el = stage.current;
    if (!el) return;
    const measure = () => setBox({ w: el.clientWidth, h: el.clientHeight });
    const ro = new ResizeObserver(measure);
    ro.observe(el);
    measure();
    return () => ro.disconnect();
  }, []);
  // A while on a photo: make its full size in the background, so zoom is quick.
  useEffect(() => {
    if (!d.online) return;
    const t = setTimeout(() => setPrefetch(true), PREFETCH_MS);
    return () => clearTimeout(t);
  }, [d.online]);
  const asked = view.pct !== null || member === "raw" || (members?.rawOnly ?? false);
  const needFull = asked || prefetch;
  const key = `${d.id}:${member}`;
  useEffect(() => {
    if (!needFull || full[key] || !d.online) return;
    let live = true;
    setLoading(true);
    setError(null);
    api
      .fullImage(d.id, member)
      .then(async (p) => {
        // Decoded off the main thread before it is shown: no stall.
        const im = new Image();
        im.src = fileUrl(p) ?? "";
        await im.decode().catch(() => undefined);
        if (live)
          setFull((f) => ({ ...f, [key]: { path: p, w: im.naturalWidth, h: im.naturalHeight } }));
      })
      .catch((e) => live && setError(String(e)))
      .finally(() => live && setLoading(false));
    return () => {
      live = false;
    };
  }, [needFull, key, d.id, d.online, member, full]);
  const made = full[key];
  // Once made, the full size is shown at Fit too (sharper on big screens).
  const shown = made ? fileUrl(made.path) : fileUrl(d.preview);
  const dpr = window.devicePixelRatio || 1;
  // The picture zoom percentages are of: the full size; the preview when its
  // drive is away; until the full size is here, a guess from the preview.
  const fitKnown = !!made || (!d.online && pre !== null);
  const base: [number, number] | null = made
    ? [made.w, made.h]
    : pre
      ? d.online
        ? guessFull(pre.w, pre.h)
        : [pre.w, pre.h]
      : null;
  // Fit never enlarges: at most 100%.
  const fitPct =
    base && box.w > 0 && box.h > 0
      ? Math.min(100, Math.min(box.w / base[0], box.h / base[1]) * dpr * 100)
      : 100;
  const hi = Math.max(MAX_ZOOM, fitPct * 2);
  const pct = view.pct ?? fitPct;
  // On screen: the picture's size and place, in CSS px.
  const sw = base ? ((base[0] / dpr) * pct) / 100 : 0;
  const sh = base ? ((base[1] / dpr) * pct) / 100 : 0;
  const tx = place(box.w / 2 - view.fx * sw, sw, box.w);
  const ty = place(box.h / 2 - view.fy * sh, sh, box.h);
  const geom = { base, sw, sh, tx, ty, box, fitPct, hi, pct, dpr, fitKnown };
  const g = useRef(geom);
  g.current = geom;

  /** Zoom to `to`, keeping the picture point at `at` (in the window; default its centre) in place. */
  const zoomTo = useCallback((to: "fit" | number, at?: { x: number; y: number }) => {
    const c = g.current;
    if (to === "fit" || (c.fitKnown && to <= c.fitPct * 1.001) || !c.base) {
      setView(to === "fit" || c.base ? FIT : { ...FIT, pct: to });
      return;
    }
    const np = Math.min(c.hi, to);
    const p = at ?? { x: c.box.w / 2, y: c.box.h / 2 };
    const ux = c.sw ? clamp01((p.x - c.tx) / c.sw) : 0.5;
    const uy = c.sh ? clamp01((p.y - c.ty) / c.sh) : 0.5;
    const nsw = ((c.base[0] / c.dpr) * np) / 100;
    const nsh = ((c.base[1] / c.dpr) * np) / 100;
    const ntx = place(p.x - ux * nsw, nsw, c.box.w);
    const nty = place(p.y - uy * nsh, nsh, c.box.h);
    setView({ pct: np, fx: (c.box.w / 2 - ntx) / nsw, fy: (c.box.h / 2 - nty) / nsh });
  }, []);
  const zoomBy = useCallback(
    (f: number, at?: { x: number; y: number }) => zoomTo(g.current.pct * f, at),
    [zoomTo],
  );
  /** Move the picture to (x, y) in the window. */
  const moveTo = useCallback((x: number, y: number) => {
    const c = g.current;
    if (!c.sw || !c.sh) return;
    const nx = place(x, c.sw, c.box.w);
    const ny = place(y, c.sh, c.box.h);
    setView((v) =>
      v.pct === null ? v : { ...v, fx: (c.box.w / 2 - nx) / c.sw, fy: (c.box.h / 2 - ny) / c.sh },
    );
  }, []);

  // While zooming or panning the picture is kept on its own GPU layer, and
  // drawn sharp again once it stops.
  useEffect(() => {
    const img = imgRef.current;
    if (!img) return;
    img.classList.add("moving");
    const t = setTimeout(() => img.classList.remove("moving"), 200);
    return () => clearTimeout(t);
  }, [view]);

  // The wheel zooms around the pointer, one step per frame however fast it
  // turns (a native listener, to stop the page scrolling).
  useEffect(() => {
    const el = stage.current;
    if (!el) return;
    let acc = 0;
    let at = { x: 0, y: 0 };
    let frame = 0;
    const on = (e: WheelEvent) => {
      e.preventDefault();
      const r = el.getBoundingClientRect();
      acc += e.deltaY;
      at = { x: e.clientX - r.left, y: e.clientY - r.top };
      if (!frame)
        frame = requestAnimationFrame(() => {
          frame = 0;
          const f = Math.exp(-acc * 0.0015);
          acc = 0;
          zoomBy(f, at);
        });
    };
    el.addEventListener("wheel", on, { passive: false });
    return () => {
      el.removeEventListener("wheel", on);
      cancelAnimationFrame(frame);
    };
  }, [zoomBy]);

  const zoomed = view.pct !== null;
  onKeyRef.current = (e) => {
    if (e.key === "z" || e.key === "Z") {
      zoomTo(zoomed ? "fit" : 100);
      return true;
    }
    if (e.key === "+" || e.key === "=") {
      zoomBy(STEP);
      return true;
    }
    if (e.key === "-") {
      zoomBy(1 / STEP);
      return true;
    }
    if (zoomed) {
      const by = e.shiftKey ? 400 : 100;
      const pan: Record<string, [number, number]> = {
        ArrowLeft: [by, 0],
        ArrowRight: [-by, 0],
        ArrowUp: [0, by],
        ArrowDown: [0, -by],
      };
      const p = pan[e.key];
      if (p) {
        moveTo(tx + p[0], ty + p[1]);
        return true;
      }
    }
    return false;
  };
  const span = Math.log(hi / fitPct);
  const pos = !zoomed ? 0 : Math.round((SLIDER * Math.log(pct / fitPct)) / span);
  const fromPos = (p: number) => (p <= 0 ? "fit" : fitPct * Math.exp((p / SLIDER) * span));
  const level = !zoomed
    ? fitKnown
      ? `Fit (${Math.round(fitPct)}%)`
      : "Fit"
    : `${Math.round(pct)}%`;
  const imgStyle: React.CSSProperties | undefined = base
    ? {
        position: "absolute",
        left: 0,
        top: 0,
        maxWidth: "none",
        maxHeight: "none",
        width: base[0] / dpr,
        height: base[1] / dpr,
        transformOrigin: "0 0",
        transform: `translate(${tx}px, ${ty}px) scale(${pct / 100})`,
      }
    : undefined;
  return (
    <div className="viewer-photo">
      <div
        ref={stage}
        className={`viewer-stage${zoomed ? " zoomed" : ""}`}
        data-testid="viewer-stage"
        data-zoom={zoomed ? String(Math.round(pct)) : "fit"}
        onMouseDown={(e) => {
          if (e.button !== 0) return;
          drag.current = { x: e.clientX, y: e.clientY, tx, ty, moved: false };
        }}
        onMouseMove={(e) => {
          const m = drag.current;
          if (!m) return;
          if (Math.abs(e.clientX - m.x) + Math.abs(e.clientY - m.y) > 4) m.moved = true;
          if (zoomed && m.moved) moveTo(m.tx + e.clientX - m.x, m.ty + e.clientY - m.y);
        }}
        onMouseUp={(e) => {
          const m = drag.current;
          drag.current = null;
          if (m && !m.moved) {
            const r = e.currentTarget.getBoundingClientRect();
            zoomTo(zoomed ? "fit" : 100, { x: e.clientX - r.left, y: e.clientY - r.top });
          }
        }}
        onMouseLeave={() => (drag.current = null)}
      >
        {shown && (
          <img
            ref={imgRef}
            src={shown}
            alt={d.name ?? ""}
            data-testid="viewer-image"
            data-member={member}
            data-full={made ? "1" : "0"}
            draggable={false}
            decoding="async"
            onLoad={(e) => {
              if (!made) {
                const img = e.currentTarget;
                setPre({ w: img.naturalWidth, h: img.naturalHeight });
              }
            }}
            style={imgStyle}
          />
        )}
        {shown && overlay && imgStyle && (
          <div className="face-layer" style={imgStyle} data-testid="face-layer">
            {overlay(pct / 100)}
          </div>
        )}
        {((loading && asked) || error || !d.online) && (
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
      <div className="viewer-controls viewer-zoom" role="group" aria-label="Zoom">
        <button
          onClick={() => zoomTo("fit")}
          aria-pressed={!zoomed}
          title="Fit the window (Z)"
          data-testid="zoom-fit"
        >
          Fit
        </button>
        <button
          onClick={() => zoomBy(1 / STEP)}
          disabled={!zoomed}
          aria-label="Zoom out"
          title="Zoom out (−)"
          data-testid="zoom-out"
        >
          −
        </button>
        <input
          type="range"
          className="zoom-slider"
          aria-label="Zoom"
          aria-valuetext={level}
          min={0}
          max={SLIDER}
          step={1}
          value={Math.max(0, Math.min(SLIDER, pos))}
          onChange={(e) => zoomTo(fromPos(Number(e.target.value)))}
          data-testid="zoom-slider"
        />
        <button
          onClick={() => zoomBy(STEP)}
          disabled={zoomed && pct >= hi}
          aria-label="Zoom in"
          title="Zoom in (+)"
          data-testid="zoom-in"
        >
          +
        </button>
        <button
          onClick={() => zoomTo(100)}
          aria-pressed={zoomed && Math.round(pct) === 100}
          title="Actual pixels: one picture pixel per screen pixel (Z)"
          data-testid="zoom-100"
        >
          100%
        </button>
        <span className="muted zoom-level" data-testid="zoom-level" aria-live="polite">
          {level}
        </span>
      </div>
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
  /** Faces drawn over the photo (P), and the menu of one of them. */
  const [showFaces, setShowFaces] = useState(false);
  const [faces, setFaces] = useState<FaceDto[]>([]);
  const [faceMenu, setFaceMenu] = useState<{ x: number; y: number; face: FaceDto } | null>(null);

  /** The item menu (right-click, Menu key, Shift+F10), where it opens. */
  const [menuAt, setMenuAt] = useState<{ x: number; y: number } | null>(null);
  const main = useRef<HTMLDivElement>(null);
  const stageKey = useRef<KeyFn>(() => false);
  const info = useViewerInfo();
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

  // The photo's faces, while they are shown (never starts a search).
  useEffect(() => {
    setFaces([]);
    if (!showFaces || !row || row.kind === "video") return;
    let live = true;
    api
      .facesOfPhoto(row.id, false)
      .then((r) => live && setFaces(r.faces))
      .catch((e) => {
        if (!live) return;
        setNote(String(e));
        setShowFaces(false);
      });
    return () => {
      live = false;
    };
  }, [showFaces, row, s.revision]);

  // A video without its proxy, or a photo without its preview (not made in
  // the background while the cache is full): have it made first, and look
  // again.
  useEffect(() => {
    if (!d || !d.online) return;
    const video = d.kind === "video";
    if (video ? d.proxy : d.preview) return;
    void api.renderSetVisible([d.id]).catch(() => undefined);
    const t = setInterval(() => {
      api
        .details(d.id)
        .then((x) => (video ? x.proxy : x.preview) && setD(x))
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

  const step = (dir: "next" | "prev" | "first" | "last") =>
    index === null ? null : sel.viewerStep(s.sel, count, index, dir);
  const go = (dir: "next" | "prev" | "first" | "last") => {
    const to = step(dir);
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
      .then((line) => setNote(line))
      .catch((e) => setNote(String(e)));
  };

  const onKey = (e: React.KeyboardEvent) => {
    const k = e.nativeEvent;
    // A slider (zoom, video position) takes its own arrow keys.
    const t = e.target as HTMLElement;
    if (
      t instanceof HTMLInputElement &&
      t.type === "range" &&
      /^(Arrow|Home$|End$|Page)/.test(e.key)
    )
      return;
    if (isMenuKey(e)) {
      const b = main.current?.getBoundingClientRect();
      if (row) setMenuAt({ x: b ? b.left + b.width / 2 : 0, y: b ? b.top + b.height / 2 : 0 });
    } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "o") {
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
    } else if (e.key === "i" || e.key === "I") {
      setViewerInfo(!info);
    } else if ((e.key === "p" || e.key === "P") && row?.kind !== "video") {
      setShowFaces((v) => !v);
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
        {row && only && (
          <button
            onClick={() =>
              dispatch({
                t: "aiJob",
                job: { cloud: only, count: 1, title: row.name ?? "this photo", what: "tags" },
              })
            }
            title="Send this photo to Claude, ChatGPT or Gemini to be tagged (asks first)"
            data-testid="viewer-cloud"
          >
            Tag with AI
          </button>
        )}
        {row && only && (
          <button
            onClick={() =>
              dispatch({
                t: "aircraftReview",
                job: { selection: only, count: 1, title: row.name ?? "this photo" },
              })
            }
            title="Read the registration, airline and type of the aircraft in this photo (on this computer)"
            data-testid="viewer-aircraft"
          >
            Read aircraft
          </button>
        )}
        {row && row.kind !== "video" && (
          <button
            onClick={() =>
              dispatch({ t: "nameFind", photo: { photo: row.id, title: row.name ?? "this photo" } })
            }
            title="Name a person in this photo and find the other photos they are in (on this computer)"
            data-testid="viewer-faces"
          >
            Name and find
          </button>
        )}
        {row && row.kind !== "video" && (
          <button
            onClick={() =>
              dispatch({
                t: "editAdvice",
                photo: { photo: row.id, title: row.name ?? "this photo" },
              })
            }
            title="Measure this photo and suggest edits for Photoshop and Topaz Photo"
            data-testid="viewer-edit-advice"
          >
            Edit suggestions
          </button>
        )}
        {row && row.kind !== "video" && (
          <button
            onClick={() => setShowFaces((v) => !v)}
            aria-pressed={showFaces}
            title="Show the faces and who they are (P)"
            data-testid="viewer-show-faces"
          >
            Faces
          </button>
        )}
        <OpenInMenu id={row?.id ?? null} member={member} onOpened={setNote} />
        <button
          onClick={() => void toggleFullscreen()}
          title="Full screen (F)"
          aria-label="Full screen"
        >
          ⛶
        </button>
        <button
          onClick={() => setViewerInfo(!info)}
          aria-pressed={info}
          aria-label="Information"
          title="Show or hide the information panel (I)"
          data-testid="viewer-info-toggle"
        >
          ⓘ Info
        </button>
      </header>
      <div className={`viewer-body${info ? " with-info" : ""}`}>
        <div
          className="viewer-main"
          ref={main}
          onContextMenu={(e) => {
            e.preventDefault();
            if (!row || !isMenuClick(e)) return;
            const b = main.current?.getBoundingClientRect();
            setMenuAt(
              e.clientX === 0 && e.clientY === 0 && b
                ? { x: b.left + b.width / 2, y: b.top + b.height / 2 }
                : { x: e.clientX, y: e.clientY },
            );
          }}
        >
          {d && row?.id === d.id ? (
            d.kind === "video" ? (
              <VideoStage key={d.id} d={d} onKeyRef={stageKey} />
            ) : (
              <PhotoStage
                key={d.id}
                d={d}
                members={members}
                member={member}
                onKeyRef={stageKey}
                overlay={
                  showFaces && faces.length > 0
                    ? (scale) =>
                        faces.map((f) => (
                          <button
                            key={f.id}
                            className={`face-box ${f.state}`}
                            style={{
                              left: `${f.x * 100}%`,
                              top: `${f.y * 100}%`,
                              width: `${f.w * 100}%`,
                              height: `${f.h * 100}%`,
                              borderWidth: 2 / scale,
                            }}
                            aria-label={
                              f.person
                                ? `${f.person}${f.state === "suggested" ? " (suggested)" : ""}`
                                : "A face with no name"
                            }
                            // Not a click on the photo (that zooms).
                            onMouseDown={(e) => e.stopPropagation()}
                            onMouseUp={(e) => e.stopPropagation()}
                            onClick={(e) => {
                              e.stopPropagation();
                              setFaceMenu({ x: e.clientX, y: e.clientY, face: f });
                            }}
                            onContextMenu={(e) => {
                              e.preventDefault();
                              e.stopPropagation();
                              setFaceMenu({ x: e.clientX, y: e.clientY, face: f });
                            }}
                            data-testid="face-box"
                          >
                            {f.person && (
                              <span className="face-label" style={{ fontSize: 12 / scale }}>
                                {f.person}
                                {f.state === "suggested" ? "?" : ""}
                              </span>
                            )}
                          </button>
                        ))
                    : undefined
                }
              />
            )
          ) : (
            <div className="viewer-stage" />
          )}
          {/* Previous and next on the picture's edges, without leaving the viewer.
              aria-disabled, not disabled, at the ends: a disabled button drops
              keyboard focus out of the viewer. */}
          <button
            className="viewer-step prev"
            onClick={() => go("prev")}
            aria-disabled={step("prev") === null}
            aria-label="Previous item"
            title="Previous (←)"
            data-testid="viewer-prev"
          >
            ‹
          </button>
          <button
            className="viewer-step next"
            onClick={() => go("next")}
            aria-disabled={step("next") === null}
            aria-label="Next item"
            title="Next (→)"
            data-testid="viewer-next"
          >
            ›
          </button>
        </div>
        {faceMenu && row && (
          <ContextMenu
            x={faceMenu.x}
            y={faceMenu.y}
            label={faceMenu.face.person ?? "A face with no name"}
            testId="face-menu"
            onClose={() => setFaceMenu(null)}
            items={[
              {
                label: faceMenu.face.person ? "Name…" : "Name and find…",
                testId: "face-menu-name",
                run: () =>
                  dispatch({
                    t: "nameFind",
                    photo: {
                      photo: row.id,
                      title: row.name ?? "this photo",
                      face: faceMenu.face.id,
                    },
                  }),
              },
              ...(faceMenu.face.person
                ? [
                    {
                      label: `Not ${faceMenu.face.person}`,
                      title: "This is not that person: never suggested for them again",
                      testId: "face-menu-not-person",
                      run: () =>
                        api.facesNotPerson(faceMenu.face.id).then(() => dispatch({ t: "edited" })),
                    },
                  ]
                : []),
              {
                label: "Not a face",
                title: "Hide it for good",
                testId: "face-menu-not-face",
                run: () => api.facesNotFace(faceMenu.face.id).then(() => dispatch({ t: "edited" })),
              },
            ]}
          />
        )}
        {info && d && row?.id === d.id && (
          <ViewerInfo d={d} member={member} hasPair={members?.hasPair ?? false} />
        )}
      </div>
      <footer className="viewer-foot">
        <span className="muted">
          ← → previous / next · {d?.kind === "video" ? "" : "Z fit/100% · + − zoom · "}
          {d?.kind === "video" ? "Space play · , . frame · J L second · " : ""}
          {members?.hasPair ? "R JPEG/RAW · " : ""}I info · P faces · F full screen · Esc close
        </span>
        {note && (
          <span role="status" data-testid="viewer-note">
            {note}
          </span>
        )}
      </footer>
      {menuAt && row && (
        <ItemMenu
          key={row.id}
          x={menuAt.x}
          y={menuAt.y}
          id={row.id}
          name={row.name ?? "Item"}
          onNote={setNote}
          onClose={() => {
            setMenuAt(null);
            root.current?.focus();
          }}
        />
      )}
    </div>
  );
}
