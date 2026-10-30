// An in-memory stand-in for the Rust backend, for browser tests (Playwright
// "mocked" project, which also runs on macOS where WKWebView cannot be
// automated) and `pnpm dev:mock`. It answers the same commands with the same
// types, over synthetic data: 300,000 items by default (`?items=N`).
//
// Loaded only when VITE_KEEPSAKE_MOCK=1; never part of the app build.

import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";

import type { Bucket } from "../bindings/Bucket";
import type { CalendarLevelDto } from "../bindings/CalendarLevelDto";
import type { CardItem } from "../bindings/CardItem";
import type { HistoryRun } from "../bindings/HistoryRun";
import type { Details } from "../bindings/Details";
import type { ImportRequest } from "../bindings/ImportRequest";
import type { ItemKind } from "../bindings/ItemKind";
import type { Preset } from "../bindings/Preset";
import type { Query } from "../bindings/Query";
import type { Row } from "../bindings/Row";
import type { Selection } from "../bindings/Selection";
import type { SettingsInfo } from "../bindings/SettingsInfo";
import type { SettingsUpdate } from "../bindings/SettingsUpdate";
import type { SortSpec } from "../bindings/SortSpec";

const CAMERAS = ["ILCE-7M4", "ILCE-6000", "ZV-E10", "ILCE-1"];
const LENSES = [
  "FE 24-70mm F2.8 GM II",
  "E 16-50mm F3.5-5.6 OSS",
  "FE 35mm F1.8",
  "FE 70-200mm F2.8 GM OSS II",
];
const START = Date.UTC(2015, 0, 1, 8);
const SPAN = 10 * 365 * 24 * 3600 * 1000;

const svg = (h: number) =>
  "data:image/svg+xml;utf8," +
  encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" width="256" height="171"><defs><linearGradient id="g" x2="1" y2="1"><stop offset="0" stop-color="hsl(${h},60%,55%)"/><stop offset="1" stop-color="hsl(${(h + 60) % 360},60%,35%)"/></linearGradient></defs><rect width="256" height="171" fill="url(#g)"/></svg>`,
  );
const THUMBS = Array.from({ length: 16 }, (_, i) => svg(i * 22));
/** The 1024 px thumbnails: the same picture, marked so tests can tell. */
const BIG_THUMBS = THUMBS.map((t) => `${t}#large`);

/** A 4000 × 3000 "full-size" picture with a grid, to see zoom and pan. */
const fullSvg = (h: number, raw: boolean) =>
  "data:image/svg+xml;utf8," +
  encodeURIComponent(
    `<svg xmlns="http://www.w3.org/2000/svg" width="4000" height="3000"><defs><pattern id="p" width="200" height="200" patternUnits="userSpaceOnUse"><rect width="200" height="200" fill="hsl(${h},50%,${raw ? 30 : 50}%)"/><path d="M0 0H200V200" fill="none" stroke="white" stroke-width="4"/></pattern></defs><rect width="4000" height="3000" fill="url(#p)"/><text x="2000" y="1500" font-size="200" text-anchor="middle" fill="white">${raw ? "RAW" : "JPEG"} 100%</text></svg>`,
  );

interface Item {
  i: number;
  kind: ItemKind;
  t: number | null;
  camera: string;
  lens: string;
  size: number;
  durationMs: number | null;
  name: string;
  online: boolean;
  event: string | null;
}

export interface MockState {
  items: number;
  calls: { cmd: string; args: unknown }[];
  rating: Map<number, number>;
  label: Map<number, string | null>;
  comment: Map<number, string | null>;
  keywords: Map<number, string[]>;
  presets: Preset[];
  imported: Set<string>;
  /** Seconds added to capture times by time shifts. */
  shift: Map<number, number>;
  history: HistoryRun[];
  /** Folders added in place. */
  folders: { id: string; label: string; mount: string; count: number }[];
  settings: SettingsInfo;
  opened: { id: string; member: string }[];
  uiErrors: { message: string; detail: string | null }[];
  crashSeen: boolean;
}

declare global {
  interface Window {
    __mock?: MockState;
  }
}

function item(i: number, n: number): Item {
  const r = (i * 2654435761) % 1000;
  const kind: ItemKind = r < 150 ? "video" : r > 850 ? "raw" : "photo";
  return {
    i,
    kind,
    t: i % 500 === 499 ? null : START + Math.floor((SPAN * i) / n) + ((r * 3600) % 3600000),
    camera: CAMERAS[i % 4],
    lens: LENSES[(i >> 2) % 4],
    size: kind === "video" ? 50e6 + r * 1e6 : kind === "raw" ? 24e6 + r * 1e4 : 4e6 + r * 1e4,
    durationMs: kind === "video" ? 2000 + r * 600 : null,
    name: `${kind === "video" ? "C" : "DSC"}${String(i % 100000).padStart(5, "0")}.${kind === "video" ? "MP4" : kind === "raw" ? "ARW" : "JPG"}`,
    online: i % 4 !== 3,
    event: i % 97 === 0 ? "ev1" : null,
  };
}

const iso = (t: number | null) => (t === null ? null : new Date(t).toISOString().slice(0, 19));
const day = (t: number | null) => (t === null ? null : new Date(t).toISOString().slice(0, 10));

export function installMock(): void {
  const params = new URLSearchParams(window.location.search);
  const n = Number(params.get("items") ?? 300000);
  const noLibrary = params.get("library") === "none";
  const st: MockState = {
    items: n,
    calls: [],
    rating: new Map(),
    label: new Map(),
    comment: new Map(),
    keywords: new Map(),
    presets: [
      {
        name: "Two drives",
        destinations: ["F:\\Photos", "G:\\Photos"],
        template: "{yyyy}/{yyyy}-{mm}-{dd}/{orig}",
        event: null,
      },
    ],
    imported: new Set(),
    shift: new Map(),
    history: [],
    folders: [],
    settings: {
      library: "D:\\Keepsake",
      cacheDir: "C:\\Users\\me\\AppData\\Local\\keepsake\\cache\\renditions",
      cacheDirCustom: false,
      cacheMaxBytes: 20 * 1024 ** 3,
      cacheBytes: 3e9,
      workers: null,
      workersDefault: 4,
      proxyWorkers: 1,
      pauseOnBattery: true,
      openApps: [],
      import: {
        // `?autoImport=1`: on, to E:\Pictures; `=Z`: on, to a drive not connected.
        auto: params.has("autoImport"),
        destination: params.has("autoImport")
          ? `${params.get("autoImport") === "Z" ? "Z" : "E"}:\\Pictures`
          : null,
        template: "{yyyy}/{month}/{yyyy}-{mm}-{dd}/{orig}",
      },
      configFile: "C:\\Users\\me\\AppData\\Roaming\\keepsake\\config.toml",
    },
    opened: [],
    uiErrors: [],
    crashSeen: false,
  };
  // `?crash=1`: the app closed unexpectedly last time.
  const crashed = params.get("crash") === "1";
  // `?broken=details`: the backend returns malformed details (tests the
  // error screen with a real render failure).
  const broken = params.get("broken");
  window.__mock = st;
  let library: string | null = noLibrary ? null : "D:\\Keepsake";
  const all = Array.from({ length: n }, (_, i) => item(i, n));
  const views = new Map<number, number[]>();
  let nextView = 0;

  const rating = (it: Item) =>
    st.rating.get(it.i) ?? ((it.i * 7) % 10 > 6 ? ((it.i * 7) % 10) - 4 : 0);
  const label = (it: Item) =>
    st.label.has(it.i) ? (st.label.get(it.i) ?? null) : it.i % 13 === 0 ? "Red" : null;
  const time = (it: Item) => (it.t === null ? null : it.t + (st.shift.get(it.i) ?? 0) * 1000);
  const row = (it: Item): Row => ({
    id: `a${it.i}`,
    name: it.name,
    captured: iso(time(it)),
    camera: it.camera,
    lens: it.lens,
    kind: it.kind,
    size: it.size,
    durationMs: it.durationMs,
    rating: rating(it),
    label: label(it),
    thumb: THUMBS[it.i % 16],
    thumbLarge: BIG_THUMBS[it.i % 16],
    online: it.online,
  });
  /** The folder an item is in, on its volume: `2024/2024-06`, or `Undated`. */
  const folderOf = (it: Item) => {
    const d = day(it.t);
    return d ? `${d.slice(0, 4)}/${d.slice(0, 7)}` : "Undated";
  };
  const matches = (q: Query) => (it: Item) => {
    const d = day(it.t);
    if (q.undated && it.t !== null) return false;
    if (q.from && (!d || d < q.from)) return false;
    if (q.to && (!d || d > q.to)) return false;
    if (q.camera && !it.camera.toLowerCase().includes(q.camera.toLowerCase())) return false;
    if (q.lens && !it.lens.toLowerCase().includes(q.lens.toLowerCase())) return false;
    if (q.kind && it.kind !== q.kind && !(q.kind === "photo" && it.kind === "raw")) return false;
    if (q.minRating && rating(it) < q.minRating) return false;
    if (q.label && label(it)?.toLowerCase() !== q.label.toLowerCase()) return false;
    if (q.event && it.event !== q.event) return false;
    if (q.volume && (q.volume === "vOff") === it.online) return false;
    if (q.folder && !`${folderOf(it)}/`.startsWith(`${q.folder.replace(/^\/+|\/+$/g, "")}/`))
      return false;
    if (q.text) {
      const words = q.text.toLowerCase().split(/\W+/).filter(Boolean);
      const hay =
        `${it.name} ${st.comment.get(it.i) ?? ""} ${(st.keywords.get(it.i) ?? []).join(" ")}`.toLowerCase();
      if (!words.every((w) => hay.split(/[^a-z0-9]+/).some((h) => h.startsWith(w)))) return false;
    }
    return true;
  };
  const key =
    (s: SortSpec) =>
    (it: Item): string | number => {
      switch (s.field) {
        case "name":
          return it.name.toLowerCase();
        case "camera":
          return it.camera;
        case "lens":
          return it.lens;
        case "type":
          return it.kind;
        case "size":
          return it.size;
        case "duration":
          return it.durationMs ?? -1;
        case "rating":
          return rating(it);
        default:
          return it.t ?? (s.descending ? -1 : Number.MAX_SAFE_INTEGER);
      }
    };
  const resolve = (sel: Selection): number[] => {
    const out = sel.ids.map((id) => Number(id.slice(1)));
    if (sel.view !== null) {
      const v = views.get(sel.view) ?? [];
      for (const [a, b] of sel.ranges) out.push(...v.slice(a, b));
    }
    return out;
  };
  const cardItems: CardItem[] = Array.from({ length: 12 }, (_, i) => ({
    key: `dcim/100msdcf/dsc0${1000 + i}.jpg`,
    kind: i % 5 === 4 ? "video" : "photo",
    files: [
      i % 5 === 4
        ? `PRIVATE/M4ROOT/CLIP/C${String(i).padStart(4, "0")}.MP4`
        : `DCIM/100MSDCF/DSC0${1000 + i}.JPG`,
    ],
    size: 8e6,
    day: i < 7 ? "2024-06-15" : "2024-06-16",
    new: i !== 0,
  }));

  mockIPC(
    (cmd, args) => {
      const a = (args ?? {}) as Record<string, unknown>;
      st.calls.push({ cmd, args: a });
      switch (cmd) {
        case "app_info":
          return { version: "0.1.0-mock", library, uiTest: true };
        case "library_create":
        case "library_open":
          library = String(a.path);
          return { version: "0.1.0-mock", library, uiTest: true };
        case "view_open": {
          const q = a.query as Query;
          const s = a.sort as SortSpec;
          const k = key(s);
          const ids = all.filter(matches(q)).sort((x, y) => {
            const kx = k(x);
            const ky = k(y);
            const c = kx < ky ? -1 : kx > ky ? 1 : x.i - y.i;
            return s.descending ? -c : c;
          });
          nextView += 1;
          views.set(
            nextView,
            ids.map((x) => x.i),
          );
          return { view: nextView, count: ids.length };
        }
        case "view_rows": {
          const v = views.get(a.view as number) ?? [];
          const off = a.offset as number;
          return v.slice(off, off + (a.limit as number)).map((i) => row(all[i]));
        }
        case "view_index_of": {
          const v = views.get(a.view as number) ?? [];
          const p = v.indexOf(Number(String(a.id).slice(1)));
          return p < 0 ? null : p;
        }
        case "calendar": {
          const q = a.query as Query;
          const lv = a.level as CalendarLevelDto;
          const len = lv.level === "years" ? 4 : lv.level === "months" ? 7 : 10;
          const prefix =
            lv.level === "months"
              ? `${lv.year}-`
              : lv.level === "days"
                ? `${lv.year}-${String(lv.month).padStart(2, "0")}-`
                : "";
          const m = new Map<string, Item[]>();
          let undated = 0;
          for (const it of all) {
            if (!matches(q)(it)) continue;
            const d = day(it.t);
            if (!d) {
              undated += 1;
              continue;
            }
            if (!d.startsWith(prefix)) continue;
            const k = d.slice(0, len);
            if (!m.has(k)) m.set(k, []);
            m.get(k)?.push(it);
          }
          const out: Bucket[] = [...m.entries()].sort().map(([k, list]) => ({
            key: k,
            count: list.length,
            coverId: `a${list[0].i}`,
            cover: THUMBS[list[0].i % 16],
          }));
          if (lv.level === "years" && undated)
            out.push({ key: "undated", count: undated, coverId: null, cover: null });
          return out;
        }
        case "folder_tree": {
          const q = { ...(a.query as Query), volume: null, folder: undefined };
          const parent = (a.parent as string | null) ?? "";
          const on = a.volume !== "vOff";
          const by = new Map<string, { count: number; deeper: boolean }>();
          for (const it of all) {
            if (it.online !== on || !matches(q)(it)) continue;
            const p = folderOf(it);
            const rest = parent
              ? p.startsWith(`${parent}/`)
                ? p.slice(parent.length + 1)
                : null
              : p;
            if (rest === null) continue;
            const [name, ...below] = rest.split("/");
            const e = by.get(name) ?? { count: 0, deeper: false };
            e.count += 1;
            e.deeper ||= below.length > 0;
            by.set(name, e);
          }
          return [...by.entries()]
            .sort(([x], [y]) => x.localeCompare(y))
            .map(([name, e]) => ({
              name,
              path: parent ? `${parent}/${name}` : name,
              count: e.count,
              hasChildren: e.deeper,
            }));
        }
        case "facets":
          return {
            cameras: CAMERAS.map((c) => ({ value: c, count: Math.round(n / 4) })),
            lenses: LENSES.map((c) => ({ value: c, count: Math.round(n / 4) })),
          };
        case "sidebar":
          return {
            events: [{ id: "ev1", title: "Lake 2024", count: Math.ceil(n / 97) }],
            volumes: [
              {
                id: "vOn",
                label: "Photos A",
                online: true,
                mount: "F:\\Photos",
                count: Math.ceil((n * 3) / 4),
              },
              {
                id: "vOff",
                label: "Archive (unplugged)",
                online: false,
                mount: null,
                count: Math.floor(n / 4),
              },
              ...st.folders.map((f) => ({ ...f, online: true })),
            ].filter((v) => v.count > 0),
            imports: [
              {
                id: "s1",
                startedMs: Date.UTC(2024, 5, 16, 18),
                status: "completed",
                source: "A7 IV card",
                safeToFormat: true,
                count: 34,
              },
            ],
          };
        case "details": {
          const it = all[Number(String(a.id).slice(1))];
          const d: Details = {
            id: `a${it.i}`,
            name: it.name,
            kind: it.kind,
            captured: iso(time(it)),
            captureSource: "exif",
            confidence: "high",
            cameraMake: "SONY",
            cameraModel: it.camera,
            lens: it.lens,
            durationMs: it.durationMs,
            rating: rating(it),
            label: label(it),
            comment: st.comment.get(it.i) ?? null,
            keywords: st.keywords.get(it.i) ?? [],
            gps: null,
            event: it.event ? "Lake 2024" : null,
            files: [
              {
                path: `F:\\Photos\\${it.name}`,
                volume: it.online ? "Photos A" : "Archive",
                role: "original",
                size: it.size,
                online: it.online,
              },
              // Even RAWs have a JPEG beside them (as asset_members says).
              ...(it.kind === "raw" && it.i % 2 === 0
                ? [
                    {
                      path: `F:\\Photos\\${it.name.replace(/\.\w+$/, ".JPG")}`,
                      volume: it.online ? "Photos A" : "Archive",
                      role: "raw_pair",
                      size: 8e6,
                      online: it.online,
                    },
                  ]
                : []),
            ],
            preview: THUMBS[it.i % 16],
            // A tiny synthetic WebM (Playwright's Chromium has no H.264).
            proxy: it.kind === "video" ? `${window.location.origin}/e2e/assets/clip.webm` : null,
            online: it.online,
          };
          if (broken === "details") return { ...d, files: null };
          return d;
        }
        case "edit_rating":
          for (const i of resolve(a.selection as Selection)) st.rating.set(i, a.rating as number);
          return { changed: 1, background: false };
        case "edit_label":
          for (const i of resolve(a.selection as Selection))
            st.label.set(i, (a.label as string | null) ?? null);
          return { changed: 1, background: false };
        case "edit_comment":
          st.comment.set(Number(String(a.id).slice(1)), (a.comment as string | null) ?? null);
          return { changed: 1, background: false };
        case "edit_keywords":
          for (const i of resolve(a.selection as Selection)) {
            const cur = (st.keywords.get(i) ?? []).filter(
              (k) => !(a.remove as string[]).includes(k),
            );
            for (const k of a.add as string[]) if (!cur.includes(k)) cur.push(k);
            st.keywords.set(i, cur);
          }
          return { changed: 1, background: false };
        case "keywords":
          return [...new Set([...st.keywords.values()].flat())].sort();
        case "render_status":
          return {
            queued: 0,
            running: 0,
            done: n,
            failed: 0,
            paused: null,
            cacheBytes: 3e9,
            cacheMax: 21474836480,
            keptOffline: 1e9,
          };
        case "render_set_visible":
          return 0;
        case "import_scan":
          return {
            mount: String(a.mount),
            label: "A7 IV card",
            card: "c1",
            known: true,
            preset: null,
            items: cardItems.map((c) => ({ ...c, new: c.new && !st.imported.has(c.key) })),
          };
        case "card_thumb":
          return THUMBS[(a.files as string[])[0].length % 16];
        case "presets_list":
          return st.presets;
        case "preset_save":
          st.presets = [
            ...st.presets.filter((p) => p.name !== (a.preset as Preset).name),
            a.preset as Preset,
          ];
          return null;
        case "import_check": {
          const r = a.request as ImportRequest;
          const needed = (a.sizes as number[]).reduce((x, y) => x + y, 0);
          if (r.destinations.length === 0)
            return { ok: false, problem: "choose a destination", space: [], example: null };
          const gone = r.destinations.find((d) => d.startsWith("Z:"));
          if (gone)
            return {
              ok: false,
              problem: `destination ${gone} does not exist (is the drive connected?)`,
              space: [],
              example: null,
            };
          if (r.destinations.length === 2 && r.destinations[0][0] === r.destinations[1][0])
            return {
              ok: false,
              problem: "both destinations are on the same disk; two copies must be on two disks",
              space: [],
              example: null,
            };
          if (needed === 0)
            return { ok: false, problem: "nothing selected", space: [], example: null };
          return {
            ok: true,
            problem: null,
            space: r.destinations.map((d) => ({ destination: d, needed, free: 900e9 })),
            example: "2024/2024-06-15/DSC01001.JPG",
          };
        }
        case "import_start": {
          const r = a.request as ImportRequest;
          const total = r.keys.length * 8e6;
          let done = 0;
          const tick = setInterval(() => {
            done = Math.min(total, done + total / 5);
            void emit("import://progress", {
              session: "s2",
              filesDone: Math.round((done / total) * r.keys.length),
              bytesDone: done,
              bytesTotal: total,
              current: "DCIM/100MSDCF/DSC01003.JPG",
              phase: null,
            });
            if (done >= total) {
              clearInterval(tick);
              r.keys.forEach((k) => st.imported.add(k));
              const two = r.destinations.length === 2;
              void emit("import://done", {
                session: "s2",
                safeToFormat: two,
                notSafe: two ? [] : ["only one destination"],
                catalogued: r.keys.length,
                failed: [],
                cancelled: false,
                error: null,
              });
            }
          }, 120);
          return null;
        }
        case "import_cancel":
          return null;
        case "preset_delete": {
          const before = st.presets.length;
          st.presets = st.presets.filter((p) => p.name !== a.name);
          return st.presets.length < before;
        }
        case "full_image": {
          const it = all[Number(String(a.id).slice(1))];
          if (!it.online) throw new Error("the drive with this item is not connected");
          return fullSvg((it.i * 22) % 360, a.member === "raw");
        }
        case "asset_members": {
          const it = all[Number(String(a.id).slice(1))];
          // Even RAWs have a JPEG beside them; odd ones are RAW alone.
          return {
            hasPair: it.kind === "raw" && it.i % 2 === 0,
            rawOnly: it.kind === "raw" && it.i % 2 === 1,
            online: it.online,
          };
        }
        case "video_info":
          return {
            fpsNum: 25,
            fpsDen: 1,
            startTimecode: "10:00:00:00",
            durationMs: 2000,
            width: 160,
            height: 90,
          };
        case "metadata_all": {
          const it = all[Number(String(a.id).slice(1))];
          if (!it.online) throw new Error("the drive with this item is not connected");
          const raw = a.member === "raw";
          const file = raw ? it.name.replace(/\.\w+$/, ".ARW") : it.name;
          return {
            file,
            groups: [
              {
                group: "IFD0",
                tags: [
                  { name: "Make", value: "SONY" },
                  { name: "Model", value: it.camera },
                ],
              },
              {
                group: "ExifIFD",
                tags: [
                  { name: "ExposureTime", value: "1/250" },
                  { name: "FNumber", value: "2.8" },
                  { name: "ISO", value: raw ? "400" : "200" },
                  { name: "LensModel", value: it.lens },
                ],
              },
              { group: "Sony", tags: [{ name: "SonyModelID", value: it.camera }] },
              { group: "XMP-dc", tags: [{ name: "Subject", value: "Lake, Summer" }] },
              { group: "File", tags: [{ name: "FileSize", value: "24 MB" }] },
            ],
          };
        }
        case "open_choices":
        case "open_external": {
          const it = all[Number(String(a.id).slice(1))];
          const ext =
            a.member === "raw" && it.kind !== "video"
              ? "arw"
              : (it.name.split(".").pop() ?? "").toLowerCase();
          const apps = [
            ...new Set(st.settings.openApps.filter((x) => x.ext === ext).map((x) => x.app)),
          ];
          const name = (app: string) =>
            (app.split(/[\\/]/).pop() ?? app).replace(/\.[^.]+$/, "") || app;
          if (cmd === "open_choices")
            return [
              ...apps.map((app, i) => ({ app, name: name(app), default: i === 0 })),
              { app: "", name: "System default", default: apps.length === 0 },
            ];
          const app = (a.app as string | null | undefined) ?? null;
          st.opened.push({ id: String(a.id), member: String(a.member) });
          if (app === null) return apps.length ? name(apps[0]) : "the system default";
          if (app === "") return "the system default";
          if (!apps.includes(app))
            throw new Error(`${name(app)} is not set up in Settings for this file type`);
          return name(app);
        }
        case "time_shift_preview": {
          const ids = resolve(a.selection as Selection);
          const ref = all[Number(String(a.reference).slice(1))];
          const correct = Date.parse(`${String(a.correct)}Z`);
          const rt = time(ref);
          if (rt === null) throw new Error("the reference photo has no capture time");
          if (Number.isNaN(correct)) throw new Error(`not a date and time: ${String(a.correct)}`);
          const off = Math.floor(correct / 1000) - Math.floor(rt / 1000);
          const dated = ids.map((i) => all[i]).filter((x) => x.t !== null);
          dated.sort((x, y) => (time(x) ?? 0) - (time(y) ?? 0));
          const fmtT = (t: number | null) => (iso(t) ?? "").replace("T", " ");
          return {
            offsetS: off,
            total: dated.length,
            undated: ids.length - dated.length,
            samples: dated.slice(0, 10).map((x) => ({
              id: `a${x.i}`,
              name: x.name,
              before: fmtT(time(x)),
              after: fmtT((time(x) ?? 0) + off * 1000),
            })),
            referenceBefore: fmtT(rt),
          };
        }
        case "time_shift_apply": {
          const ids = resolve(a.selection as Selection).filter((i) => all[i].t !== null);
          for (const i of ids) st.shift.set(i, (st.shift.get(i) ?? 0) + (a.offsetS as number));
          return { changed: ids.length, background: false };
        }
        case "backup_overview": {
          const dests = new Map<string, HistoryRun>();
          for (const r of st.history)
            if (r.kind === "backup" && r.volume && !dests.has(r.volume)) dests.set(r.volume, r);
          return {
            destinations: [...dests.values()].map((r) => ({
              volume: r.volume ?? "",
              label: r.label,
              path: r.destination,
              online: true,
              lastBackupMs: r.finishedMs,
              lastVerifiedMs:
                st.history.find((v) => v.kind === "verify" && v.volume === r.volume && v.ok)
                  ?.finishedMs ?? null,
            })),
            runs: st.history,
            busy: false,
          };
        }
        case "backup_run":
        case "verify_run": {
          const kind = cmd === "backup_run" ? "backup" : "verify";
          const prev = st.history.find(
            (r) => r.kind === "backup" && (r.volume === a.volume || r.destination === a.dest),
          );
          const dest = kind === "backup" ? String(a.dest) : (prev?.destination ?? "");
          const label =
            kind === "backup" ? String(a.label ?? dest.split(/[\\/]/).pop()) : (prev?.label ?? "");
          const volume = prev?.volume ?? `vb${st.history.length}`;
          const total = 120e6;
          let done = 0;
          const started = Date.now();
          const tick = setInterval(() => {
            done = Math.min(total, done + total / 4);
            void emit("maint://progress", {
              kind,
              bytesDone: done,
              bytesTotal: total,
              filesDone: Math.round((done / total) * 30),
              current: "2024/2024-06-15/DSC01003.JPG",
            });
            if (done >= total) {
              clearInterval(tick);
              const run: HistoryRun = {
                kind,
                startedMs: started,
                finishedMs: Date.now(),
                destination: dest,
                volume,
                label,
                files: 30,
                bytes: kind === "backup" ? total : 0,
                problems: 0,
                cancelled: false,
                ok: true,
              };
              st.history = [run, ...st.history];
              void emit("maint://done", { kind, run, problems: [], error: null });
            }
          }, 100);
          return null;
        }
        case "folder_add": {
          const mount = String(a.path ?? "").trim();
          if (!mount) throw new Error("choose a folder");
          if (mount.toLowerCase().includes("missing")) throw new Error(`${mount} is not a folder`);
          const prev = st.folders.find((f) => f.mount === mount);
          const label = prev?.label ?? mount.split(/[\\/]/).filter(Boolean).pop() ?? mount;
          const id = prev?.id ?? `vf${st.folders.length}`;
          const total = 250e6;
          let bytes = 0;
          const tick = setInterval(() => {
            bytes = Math.min(total, bytes + total / 4);
            void emit("maint://progress", {
              kind: "folder",
              bytesDone: bytes,
              bytesTotal: total,
              filesDone: Math.round((bytes / total) * 40),
              current: "January/DSC00002.ARW",
            });
            if (bytes >= total) {
              clearInterval(tick);
              if (!prev) st.folders.push({ id, label, mount, count: 38 });
              void emit("folder://done", {
                folder: mount,
                volume: id,
                label,
                items: 38,
                files: 40,
                withSidecars: 12,
                otherFiles: 2,
                problems: [],
                cancelled: false,
                error: null,
              });
            }
          }, 100);
          return null;
        }
        case "maint_cancel":
          return null;
        case "settings_get":
          return st.settings;
        case "settings_set": {
          const u = a.update as SettingsUpdate;
          if (u.cacheMaxBytes < 256 * 1024 ** 2)
            throw new Error("the cache limit must be at least 256 MB");
          const dest = u.import.destination?.trim() || null;
          if (u.import.auto && !dest)
            throw new Error("choose where cards are copied to import them automatically");
          if (
            /\{(?!yyyy|mm|dd|hh|month|event|camera|lens|seq|orig\})[^}]*\}/.test(u.import.template)
          )
            throw new Error("folder layout: unknown template token");
          const old = st.settings.cacheDir;
          st.settings = {
            ...st.settings,
            cacheDir: u.cacheDir ?? st.settings.cacheDir,
            cacheDirCustom: u.cacheDir !== null,
            cacheMaxBytes: u.cacheMaxBytes,
            workers: u.workers,
            proxyWorkers: u.proxyWorkers,
            pauseOnBattery: u.pauseOnBattery,
            // One row per type and app, in order (a row may list several types).
            openApps: u.openApps
              .flatMap((x) =>
                x.ext.split(/[,;]/).map((e) => ({
                  ext: e.trim().replace(/^\./, "").toLowerCase(),
                  app: x.app.trim(),
                })),
              )
              .filter((x) => x.ext && x.app)
              .filter((x, i, all) => all.findIndex((y) => y.ext === x.ext && y.app === x.app) === i)
              .sort((p, q) => (p.ext < q.ext ? -1 : p.ext > q.ext ? 1 : 0)),
            import: { auto: u.import.auto, destination: dest, template: u.import.template.trim() },
          };
          return { settings: st.settings, oldCache: old !== st.settings.cacheDir ? old : null };
        }
        case "cache_remove_old":
          return 2.5e9;
        case "diagnostic_report":
          return [
            "Keepsake diagnostic report",
            "==========================",
            "Program: keepsake-app 0.1.0-mock",
            "Tools:",
            "  exiftool   13.59  [~\\AppData\\Local\\Keepsake\\exiftool\\exiftool.exe]",
            `Library: D:\\Keepsake`,
            `  ${n} assets`,
          ].join("\n");
        case "open_log_folder":
          return "~\\AppData\\Local\\keepsake\\logs";
        case "log_ui_error":
          st.uiErrors.push({
            message: String(a.message),
            detail: (a.detail as string | null) ?? null,
          });
          return null;
        case "crash_notice":
          return crashed && !st.crashSeen
            ? { file: "crash-20261025-221500-4242.txt", summary: "panicked at src/render.rs:88" }
            : null;
        case "ui_ready":
          return 0;
        case "crash_dismiss":
          st.crashSeen = true;
          return null;
        default:
          throw new Error(`mock: unknown command ${cmd}`);
      }
    },
    { shouldMockEvents: true },
  );
}
