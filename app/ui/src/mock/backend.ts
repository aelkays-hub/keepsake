// An in-memory stand-in for the Rust backend, for browser tests (Playwright
// "mocked" project, which also runs on macOS where WKWebView cannot be
// automated) and `pnpm dev:mock`. It answers the same commands with the same
// types, over synthetic data: 300,000 items by default (`?items=N`).
//
// Loaded only when VITE_KEEPSAKE_MOCK=1; never part of the app build.

import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";

import type { AiAdviceDto } from "../bindings/AiAdviceDto";
import type { Bucket } from "../bindings/Bucket";
import type { CalendarLevelDto } from "../bindings/CalendarLevelDto";
import type { CardItem } from "../bindings/CardItem";
import type { HistoryRun } from "../bindings/HistoryRun";
import type { Details } from "../bindings/Details";
import type { ImportRequest } from "../bindings/ImportRequest";
import type { ItemKind } from "../bindings/ItemKind";
import type { LibraryReport } from "../bindings/LibraryReport";
import type { Preset } from "../bindings/Preset";
import type { Query } from "../bindings/Query";
import type { ReportGroup } from "../bindings/ReportGroup";
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
  folders: { id: string; label: string; mount: string; count: number; unfingerprinted: number }[];
  settings: SettingsInfo;
  opened: { id: string; member: string }[];
  uiErrors: { message: string; detail: string | null }[];
  crashSeen: boolean;
  /** On-device AI (`?ai=off` starts with it off). */
  ai: { enabled: boolean; gpu: boolean; modelInstalled: boolean };
  /** Recognise faces (`?faces=on`: on, models installed). `left`: status
   * polls until a search of all photos is done. */
  faces: { enabled: boolean; installed: boolean; scanned: number; people: number; left: number };
  /** Read aircraft (`?aircraft=on`: on, models installed), and the photos
   * asked for: status polls left. */
  aircraft: { enabled: boolean; installed: boolean; asked: Map<string, number> };
  /** Name and find: the faces found so far (by id), the photos asked
   * about once (searched the next time they are asked about), people. */
  faceRows: Map<number, MockFace>;
  /** The face cut-off, in percent. */
  facesMatch: number;
  facesAsked: Set<number>;
  people: { id: number; name: string }[];
  /** Duplicate decisions: `keeper`, `not_kept`, `not_duplicate`. */
  choice: Map<number, string>;
  /** AI tags removed by the user. */
  aiRejected: Map<number, string[]>;
  /** Tag with AI: the keywords each provider gave an item (ADR-039). */
  aiSources: Map<number, Map<string, string[]>>;
  /** Date ranges asked for: status polls left until they are analysed. */
  aiRanges: Map<string, number>;
  /** Tag with AI (`?claude=key`, `?openai=key`, `?gemini=key` start with
   * that provider's key saved; `=bad`: with one it refuses). The provider
   * offered first, each one's model, and the photos asked for (by provider):
   * status polls left. */
  cloud: {
    keys: Record<string, string | null>;
    models: Record<string, string>;
    def: string;
    asked: Map<string, number>;
  };
  /** Edit suggestions (ADR-042), by item: when measured, the AI's answer,
   * and a question on its way (status polls left; `bad`: it fails).
   * `?suggestWait=1`: the first look waits for the preview. */
  suggest: Map<
    number,
    {
      madeMs: number;
      ai: AiAdviceDto | null;
      asking: { provider: string; left: number; failed: string | null } | null;
    }
  >;
  suggestWait: number;
}

const AI_LABELS = ["People", "Beach", "Dog", "Food", "Sunset", "Mountain"];

/** Tag with AI: the providers, as the backend describes them. */
/** A face in the mock: photo `i` (not a video) has one or two, each of
 * one of five pretend people (`who`). */
interface MockFace {
  id: number;
  photo: number;
  who: number;
  k: number;
  state: "unknown" | "suggested" | "confirmed" | "hidden";
  person: number | null;
  likeness: number | null;
  rejected: Set<number>;
}

/** What each provider tags a photo with in the mock (they differ a little,
 * so the inspector's list per AI can be checked). */
/** Read aircraft (ADR-041): who gives the keywords, what every photo
 * "shows", and the size of the text models. */
const AIRCRAFT = { id: "aircraft", name: "Aircraft reader" };
const AIRCRAFT_TAGS = [
  "Aircraft|Registration|C-FJZS",
  "Aircraft|Airline|Air Canada",
  "Aircraft|Type|Boeing 777-300ER",
];
const AIRCRAFT_BYTES = 95861692;
/** The mock's aircraft photos (those "tagged Airplane"): every fifth item
 * that is not a video. */
const isAircraft = (it: { i: number; kind: string }) => it.i % 5 === 0 && it.kind !== "video";

const CLOUD_TAGS: Record<string, string[]> = {
  claude: ["Beach", "AI|Sand Castle"],
  openai: ["Beach", "Sky"],
  gemini: ["Beach", "AI|Sand Castle"],
};

const CLOUD = [
  {
    id: "claude",
    name: "Claude",
    company: "Anthropic",
    prefix: "sk-ant-",
    key: /^sk-ant-[A-Za-z0-9_-]{13,}$/,
    hint: " (they start with sk-ant-)",
    models: [
      { id: "claude-opus-5-5", name: "Claude Opus 5.5", centsPerPhoto: 2 },
      { id: "claude-fable-5-1", name: "Claude Fable 5.1", centsPerPhoto: 6 },
      { id: "claude-haiku-5-5", name: "Claude Haiku 5.5", centsPerPhoto: 0.1 },
    ],
  },
  {
    id: "openai",
    name: "ChatGPT",
    company: "OpenAI",
    prefix: "sk-",
    key: /^sk-(?!ant-)[A-Za-z0-9_-]{17,}$/,
    hint: " (they start with sk-)",
    models: [
      { id: "gpt-6.1-sol", name: "GPT-6.1 Sol", centsPerPhoto: 1 },
      { id: "gpt-6-astra", name: "GPT-6 Astra", centsPerPhoto: 5 },
      { id: "gpt-6-luna", name: "GPT-6 Luna", centsPerPhoto: 0.05 },
    ],
  },
  {
    id: "gemini",
    name: "Gemini",
    company: "Google",
    prefix: "AIza",
    key: /^(?!sk-)[A-Za-z0-9._-]{20,256}$/,
    hint: "",
    models: [
      { id: "gemini-3.6-flash", name: "Gemini 3.6 Flash", centsPerPhoto: 0.8 },
      { id: "gemini-4-argon", name: "Gemini 4 Argon", centsPerPhoto: 2 },
      { id: "gemini-3.5-flash-lite", name: "Gemini 3.5 Flash-Lite", centsPerPhoto: 0.2 },
    ],
  },
];
/** Groups of duplicates among the first items, all shown on item 0's day. */
const DUP_GROUPS = [
  [0, 1, 2],
  [3, 4],
  [10, 11],
];

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

/** Tools → Report over the mock items: a RAW item is a RAW+JPEG pair (one
 * photo, two files, the JPEG 6 MB). Same ordering rules as the backend. */
function mockReport(all: Item[]): LibraryReport {
  type Acc = Map<string | null, ReportGroup>;
  const blank = (g: string | null, split: boolean): ReportGroup => ({
    group: g,
    items: 0,
    photos: split ? 0 : null,
    videos: split ? 0 : null,
    files: 0,
    bytes: 0,
  });
  // `photo` null: a breakdown without the photo/video split.
  const bump = (m: Acc, g: string | null, photo: boolean | null, files: number, bytes: number) => {
    let r = m.get(g);
    if (!r) m.set(g, (r = blank(g, photo !== null)));
    r.items++;
    if (photo === true && r.photos !== null) r.photos++;
    if (photo === false && r.videos !== null) r.videos++;
    r.files += files;
    r.bytes += bytes;
  };
  const [total, years, months, days, cameras, lenses, types, drives]: Acc[] = Array.from(
    { length: 8 },
    () => new Map(),
  );
  for (const it of all) {
    const photo = it.kind !== "video";
    const files = it.kind === "raw" ? 2 : 1;
    const bytes = it.size + (it.kind === "raw" ? 6e6 : 0);
    const d = day(it.t);
    bump(total, null, photo, files, bytes);
    bump(years, d && d.slice(0, 4), photo, files, bytes);
    bump(months, d && d.slice(0, 7), photo, files, bytes);
    bump(days, d, photo, files, bytes);
    bump(cameras, it.camera, photo, files, bytes);
    bump(lenses, it.lens, photo, files, bytes);
    bump(types, it.kind === "video" ? "MP4" : it.kind === "raw" ? "ARW" : "JPG", null, 1, it.size);
    if (it.kind === "raw") bump(types, "JPG", null, 1, 6e6);
    bump(drives, it.online ? "vOn" : "vOff", null, files, bytes);
  }
  const byDate = (m: Acc) =>
    [...m.values()].sort((a, b) =>
      a.group === null ? 1 : b.group === null ? -1 : b.group.localeCompare(a.group),
    );
  const byCount = (m: Acc) =>
    [...m.values()].sort((a, b) =>
      a.group === null
        ? 1
        : b.group === null
          ? -1
          : b.items - a.items || a.group.localeCompare(b.group),
    );
  const names: Record<string, string> = { vOn: "Photos A", vOff: "Archive (unplugged)" };
  return {
    totals: total.get(null) ?? blank(null, true),
    years: byDate(years),
    months: byDate(months),
    days: byDate(days),
    cameras: byCount(cameras),
    lenses: byCount(lenses),
    fileTypes: byCount(types),
    drives: [...drives.values()]
      .map((r) => ({
        volume: r.group ?? "",
        online: r.group === "vOn",
        row: { ...r, group: names[r.group ?? ""] ?? r.group },
      }))
      .sort((a, b) => (a.row.group ?? "").localeCompare(b.row.group ?? "")),
  };
}

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
    ai: {
      enabled: params.get("ai") !== "off",
      gpu: true,
      modelInstalled: params.get("ai") !== "off",
    },
    faces: {
      enabled: params.get("faces") === "on",
      installed: params.get("faces") === "on",
      scanned: 0,
      people: 0,
      left: 0,
    },
    aircraft: {
      enabled: params.get("aircraft") === "on",
      installed: params.get("aircraft") === "on",
      asked: new Map(),
    },
    faceRows: new Map(),
    facesMatch: 55,
    facesAsked: new Set(),
    people: [],
    choice: new Map(),
    aiRejected: new Map(),
    aiSources: new Map(),
    aiRanges: new Map(),
    cloud: {
      keys: Object.fromEntries(
        CLOUD.map((c) => [c.id, params.get(c.id) ? `${c.prefix}mock-${params.get(c.id)}` : null]),
      ),
      models: Object.fromEntries(CLOUD.map((c) => [c.id, c.models[0].id])),
      def: params.get("ai-default") ?? "claude",
      asked: new Map(),
    },
    suggest: new Map(),
    suggestWait: params.get("suggestWait") === "1" ? 1 : 0,
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
      openApps:
        params.get("apps") === "ps"
          ? ["jpg", "arw"].flatMap((ext) => [
              { ext, app: "C:\\Program Files\\Adobe\\Adobe Photoshop 2026\\Photoshop.exe" },
              { ext, app: "C:\\Program Files\\Topaz Labs LLC\\Topaz Photo\\Topaz Photo.exe" },
            ])
          : [],
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
  // `?folderStall=N`: the first folder scan fails after N batches (as when
  // a network drive drops); the next scan carries on.
  let folderStall = Number(params.get("folderStall") ?? 0);
  // `?noPreview=1`: photo previews are not made yet (a full cache); an
  // item put on screen with render_set_visible gets one.
  const noPreview = params.get("noPreview") === "1";
  const previewMade = new Set<string>();
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
  /** An item's file extensions, as the report counts them. */
  const typesOf = (it: Item) =>
    it.kind === "video" ? ["MP4"] : it.kind === "raw" ? ["ARW", "JPG"] : ["JPG"];
  /** An item's AI tags: two of AI_LABELS, less the ones removed. */
  const aiTagsOf = (i: number) =>
    st.ai.enabled
      ? [AI_LABELS[i % 6], AI_LABELS[(i + 2) % 6]].filter(
          (t) => !(st.aiRejected.get(i) ?? []).includes(t),
        )
      : [];
  /** Its keywords and AI tags. */
  const tagsOf = (i: number) => [...new Set([...(st.keywords.get(i) ?? []), ...aiTagsOf(i)])];
  const matches = (q: Query) => (it: Item) => {
    const d = day(it.t);
    if (q.undated && it.t !== null) return false;
    if (q.from && (!d || d < q.from)) return false;
    if (q.to && (!d || d > q.to)) return false;
    if (q.camera && !it.camera.toLowerCase().includes(q.camera.toLowerCase())) return false;
    if (q.lens && !it.lens.toLowerCase().includes(q.lens.toLowerCase())) return false;
    if (q.cameraIs !== undefined && it.camera !== q.cameraIs) return false;
    if (q.lensIs !== undefined && it.lens !== q.lensIs) return false;
    if (q.fileType !== undefined && !typesOf(it).includes(q.fileType.toUpperCase())) return false;
    if (q.kind && it.kind !== q.kind && !(q.kind === "photo" && it.kind === "raw")) return false;
    if (q.minRating && rating(it) < q.minRating) return false;
    if (q.hideNotKept && st.choice.get(it.i) === "not_kept") return false;
    if (q.tag && !tagsOf(it.i).some((t) => t === q.tag || t.startsWith(`${q.tag}|`))) return false;
    for (const want of q.tags ?? [])
      if (!tagsOf(it.i).some((t) => t === want || t.startsWith(`${want}|`))) return false;
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
        case "app_info": {
          const reply = { version: "0.1.0-mock", library, uiTest: true };
          // ?opening=<ms>: the library takes that long to open.
          const ms = Number(params.get("opening") ?? 0);
          return ms > 0 ? new Promise((r) => setTimeout(() => r(reply), ms)) : reply;
        }
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
            tags: (() => {
              const counts = new Map<string, number>();
              for (const it of all)
                for (const t of tagsOf(it.i)) counts.set(t, (counts.get(t) ?? 0) + 1);
              return [...counts.entries()]
                .sort((x, y) => y[1] - x[1] || x[0].localeCompare(y[0]))
                .map(([value, count]) => ({ value, count }));
            })(),
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
                unfingerprinted: 0,
              },
              {
                id: "vOff",
                label: "Archive (unplugged)",
                online: false,
                mount: null,
                count: Math.floor(n / 4),
                unfingerprinted: 0,
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
            preview: noPreview && !previewMade.has(`a${it.i}`) ? null : THUMBS[it.i % 16],
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
          for (const i of resolve(a.selection as Selection))
            st.comment.set(i, (a.comment as string | null) || null);
          return { changed: 1, background: false };
        case "selection_summary": {
          const ids = [...new Set(resolve(a.selection as Selection))].filter((i) => all[i]);
          const one = <T>(vals: T[]): { v: T | null; mixed: boolean } =>
            vals.length && vals.every((x) => x === vals[0])
              ? { v: vals[0], mixed: false }
              : { v: null, mixed: vals.length > 0 };
          const r = one(ids.map((i) => rating(all[i])));
          const l = one(ids.map((i) => label(all[i])));
          const c = one(ids.map((i) => st.comment.get(i) ?? null));
          const counts = new Map<string, number>();
          for (const i of ids)
            for (const k of st.keywords.get(i) ?? []) counts.set(k, (counts.get(k) ?? 0) + 1);
          return {
            count: ids.length,
            rating: r.v,
            label: l.v,
            labelMixed: l.mixed,
            comment: c.v,
            commentMixed: c.mixed,
            keywords: [...counts.entries()]
              .sort(([x], [y]) => x.localeCompare(y))
              .map(([keyword, count]) => ({ keyword, count })),
          };
        }
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
          for (const id of a.ids as string[]) previewMade.add(id);
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
                  { name: "FocalLength", value: "35.0 mm" },
                ],
              },
              { group: "Sony", tags: [{ name: "SonyModelID", value: it.camera }] },
              { group: "XMP-dc", tags: [{ name: "Subject", value: "Lake, Summer" }] },
              { group: "File", tags: [{ name: "FileSize", value: "24 MB" }] },
              {
                group: "Composite",
                tags: [{ name: "ImageSize", value: raw ? "6048x4024" : "4000x2667" }],
              },
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
        case "original_place": {
          const it = all[Number(String(a.id).slice(1))];
          return it.online
            ? { online: true, path: `F:\\Photos\\${it.name}`, problem: null, manager: "Explorer" }
            : {
                online: false,
                path: null,
                problem: "Drive not connected (Archive)",
                manager: "Explorer",
              };
        }
        case "show_in_folder": {
          const it = all[Number(String(a.id).slice(1))];
          if (!it.online) throw new Error("Drive not connected (Archive)");
          return null;
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
        case "library_report":
          return mockReport(all);
        case "plugin:dialog|save":
          return "C:\\Users\\me\\Documents\\Keepsake report.csv";
        case "library_report_csv": {
          const path = String(a.path ?? "").trim();
          if (!path) throw new Error("choose where to save the report");
          return /\.[^\\/.]+$/.test(path) ? path : `${path}.csv`;
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
        case "folder_rescan": {
          // ?rescanBusy=1: another scan is running.
          if (params.get("rescanBusy") === "1")
            throw new Error("a backup, verification or folder scan is already running");
          const where = a.path ? `${String(a.volume)}/${String(a.path)}` : String(a.volume);
          let n = 0;
          const tick = setInterval(() => {
            n += 1;
            void emit("maint://progress", {
              kind: "folder",
              bytesDone: n * 25e6,
              bytesTotal: 100e6,
              filesDone: n * 3,
              current: `${String(a.path ?? "")}/DSC0000${n}.JPG`,
            });
            if (n < 4) return;
            clearInterval(tick);
            void emit("folder://done", {
              folder: where,
              volume: String(a.volume),
              label: "Photos A",
              items: 12,
              files: 12,
              withSidecars: 0,
              otherFiles: 0,
              problems: [],
              cancelled: false,
              error: null,
              scannedAgain: true,
            });
          }, 300);
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
          // Saved as it is read: in the library at once, then every batch.
          const f = prev ?? { id, label, mount, count: 0, unfingerprinted: 0 };
          if (!prev) st.folders.push(f);
          void emit("folder://saved", { folder: mount, items: f.count, files: f.count });
          let bytes = (f.count / 38) * total;
          let batches = 0;
          const tick = setInterval(() => {
            bytes = Math.min(total, bytes + total / 4);
            f.count = Math.min(38, f.count + 10);
            // Only the start and end of each file is read; full fingerprints come later.
            f.unfingerprinted = Math.round((f.count * 40) / 38);
            batches += 1;
            void emit("maint://progress", {
              kind: "folder",
              bytesDone: bytes,
              bytesTotal: total,
              filesDone: Math.round((bytes / total) * 40),
              current: "January/DSC00002.ARW",
            });
            void emit("folder://saved", { folder: mount, items: f.count, files: f.count });
            if (folderStall > 0 && batches >= folderStall) {
              folderStall = 0;
              clearInterval(tick);
              void emit("folder://done", {
                folder: mount,
                volume: null,
                label: null,
                items: 0,
                files: 0,
                withSidecars: 0,
                otherFiles: 0,
                problems: [],
                cancelled: false,
                error: "the network drive stopped answering",
                scannedAgain: false,
              });
              return;
            }
            if (bytes >= total) {
              clearInterval(tick);
              f.count = 38;
              f.unfingerprinted = 40;
              // The background job fingerprints 10 files every half second.
              const fp = setInterval(() => {
                f.unfingerprinted = Math.max(0, f.unfingerprinted - 10);
                if (f.unfingerprinted === 0) clearInterval(fp);
              }, 500);
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
                scannedAgain: false,
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
        case "ai_status":
          return {
            enabled: st.ai.enabled,
            gpu: st.ai.gpu,
            modelInstalled: st.ai.modelInstalled,
            modelBytes: 371807752,
            runtimeFound: true,
            accel: st.ai.enabled ? "graphics card (DirectML)" : null,
            problem: null,
            installing: null,
            analysed: st.ai.enabled ? n - 1000 : 0,
            assets: n,
            pending: st.ai.enabled ? 1000 : 0,
            failed: 0,
          };
        case "ai_enable":
          st.ai.enabled = Boolean(a.enabled);
          if (a.gpu !== null && a.gpu !== undefined) st.ai.gpu = Boolean(a.gpu);
          if (st.ai.enabled && !st.ai.modelInstalled) {
            st.ai.modelInstalled = true;
            void emit("ai://install", {
              done: 371807752,
              total: 371807752,
              finished: true,
              error: null,
            });
          }
          return null;
        case "ai_install_cancel":
          return null;
        case "faces_of_photo":
        case "faces_people":
        case "faces_name":
        case "faces_find":
        case "faces_suggest":
        case "faces_review":
        case "faces_decide":
        case "faces_check":
        case "faces_not_person":
        case "faces_not_face": {
          if (
            !(st.faces.enabled && st.ai.enabled) &&
            (cmd === "faces_of_photo" || cmd === "faces_find")
          )
            throw "Turn on Recognise faces first (Settings).";
          // Every photo's faces, made when first needed.
          const facesOf = (i: number): MockFace[] => {
            if (all[i]?.kind === "video" || i % 3 === 2) return [];
            const have = [...st.faceRows.values()].filter((f) => f.photo === i);
            if (have.length) return have;
            const made = Array.from({ length: i % 3 === 0 ? 2 : 1 }, (_, k) => {
              const f: MockFace = {
                id: i * 10 + k + 1,
                photo: i,
                who: (i + k) % 5,
                k,
                state: "unknown",
                person: null,
                likeness: null,
                rejected: new Set(),
              };
              st.faceRows.set(f.id, f);
              return f;
            });
            return made;
          };
          const dto = (f: MockFace) => ({
            id: f.id,
            photo: `a${f.photo}`,
            thumb: THUMBS[f.photo % 16],
            pw: 320,
            ph: 240,
            x: 0.15 + f.k * 0.45,
            y: 0.2,
            w: 0.3,
            h: 0.45,
            person:
              f.person === null ? null : (st.people.find((p) => p.id === f.person)?.name ?? null),
            personId: f.person,
            state: f.state,
            likeness: f.likeness,
            date: day(all[f.photo]?.t ?? null),
          });
          const personDto = (p: { id: number; name: string }) => ({
            id: p.id,
            name: p.name,
            confirmed: [...st.faceRows.values()].filter(
              (f) => f.person === p.id && f.state === "confirmed",
            ).length,
          });
          if (cmd === "faces_of_photo") {
            const i = Number(String(a.id).slice(1));
            // Searched the second time it is asked about (not when only
            // looking: the Inspector, the viewer).
            const searched = st.facesAsked.has(i);
            if (a.search !== false) st.facesAsked.add(i);
            const shown = searched ? facesOf(i).filter((f) => f.state !== "hidden") : [];
            return { searched, faces: shown.map(dto) };
          }
          if (cmd === "faces_not_person" || cmd === "faces_not_face") {
            const f = st.faceRows.get(Number(a.face));
            if (!f) return cmd === "faces_not_person" ? false : null;
            const had = f.person;
            if (cmd === "faces_not_person" && had !== null) f.rejected.add(had);
            f.person = null;
            f.likeness = null;
            f.state = cmd === "faces_not_face" ? "hidden" : "unknown";
            return cmd === "faces_not_person" ? had !== null : null;
          }
          if (cmd === "faces_people") return st.people.map(personDto);
          if (cmd === "faces_name") {
            const f = st.faceRows.get(Number(a.face));
            const name = String(a.name).trim();
            if (!f || !name) throw "no such face";
            let p = st.people.find((x) => x.name.toLowerCase() === name.toLowerCase());
            if (!p) {
              p = { id: st.people.length + 1, name };
              st.people.push(p);
              st.faces.people = st.people.length;
            }
            f.person = p.id;
            f.state = "confirmed";
            f.likeness = null;
            f.rejected.delete(p.id);
            return personDto(p);
          }
          if (cmd === "faces_find") {
            const photos = all.filter((it) => it.kind !== "video").length;
            const left = photos - st.faces.scanned;
            if (left > 0) st.faces.left = 2;
            return left;
          }
          const person = Number(a.person);
          if (cmd === "faces_check") {
            // Least alike first.
            return [...st.faceRows.values()]
              .filter((f) => f.person === person && f.state === "confirmed")
              .map((f) => ({ ...dto(f), likeness: 30 + ((f.photo * 13) % 70) }))
              .sort((x, y) => x.likeness - y.likeness);
          }
          if (cmd === "faces_suggest") {
            if (typeof a.matchPct === "number") st.facesMatch = a.matchPct;
            all.forEach((_, i) => facesOf(i));
            const mine = [...st.faceRows.values()].filter(
              (f) => f.person === person && f.state === "confirmed",
            );
            if (!mine.length) throw "name one of this person's faces first";
            const who = mine[0].who;
            const taken = new Set(
              [...st.faceRows.values()].filter((f) => f.person === person).map((f) => f.photo),
            );
            let added = 0;
            for (const f of st.faceRows.values()) {
              if (f.state !== "unknown" || f.who !== who || f.rejected.has(person)) continue;
              if (taken.has(f.photo)) continue;
              taken.add(f.photo);
              f.state = "suggested";
              f.person = person;
              f.likeness = 40 + ((f.photo * 7) % 55);
              added += 1;
            }
            const waiting = [...st.faceRows.values()].filter(
              (f) => f.person === person && f.state === "suggested",
            ).length;
            return {
              added,
              waiting,
              confirmed: mine.length,
              fromYear: 2012,
              toYear: 2020,
              matchPct: st.facesMatch,
            };
          }
          if (cmd === "faces_review")
            return [...st.faceRows.values()]
              .filter((f) => f.person === person && f.state === "suggested")
              .sort((x, y) => (y.likeness ?? 0) - (x.likeness ?? 0))
              .map(dto);
          // faces_decide
          let c = 0;
          let r = 0;
          for (const id of a.confirm as number[]) {
            const f = st.faceRows.get(id);
            if (f?.person === person) {
              f.state = "confirmed";
              f.likeness = null;
              c += 1;
            }
          }
          for (const id of a.reject as number[]) {
            const f = st.faceRows.get(id);
            if (f?.person === person) {
              f.rejected.add(person);
              f.state = "unknown";
              f.person = null;
              f.likeness = null;
              r += 1;
            }
          }
          return [c, r];
        }
        case "aircraft_status":
          return {
            enabled: st.aircraft.enabled,
            modelsInstalled: st.aircraft.installed,
            modelsBytes: AIRCRAFT_BYTES,
            runtimeFound: true,
            looks: st.ai.modelInstalled,
            accel: st.aircraft.enabled ? "graphics card (DirectML)" : null,
            problem: null,
            installing: null,
          };
        case "aircraft_enable":
          st.aircraft.enabled = Boolean(a.enabled);
          if (st.aircraft.enabled && !st.aircraft.installed) {
            st.aircraft.installed = true;
            void emit("aircraft://install", {
              done: AIRCRAFT_BYTES,
              total: AIRCRAFT_BYTES,
              finished: true,
              error: null,
            });
          }
          return null;
        case "aircraft_install_cancel":
          return null;
        case "aircraft_remove_models":
          st.aircraft.enabled = false;
          st.aircraft.installed = false;
          return true;
        case "aircraft_read":
        case "aircraft_read_status":
        case "aircraft_read_stop": {
          let ids = (a.ids ?? []) as string[];
          if (cmd === "aircraft_read") {
            if (!st.aircraft.enabled) throw "Turn on Read aircraft in Settings first.";
            ids = [...new Set(resolve(a.selection as Selection))].map((i) => `a${i}`);
            st.aircraft.asked.set(ids.join(","), 2);
          }
          const key = ids.join(",");
          const left = st.aircraft.asked.get(key) ?? 0;
          if (cmd === "aircraft_read_stop") {
            // Stopped before any was read (−1): none is counted as read.
            st.aircraft.asked.set(key, left > 0 ? -1 : 0);
            return null;
          }
          if (cmd === "aircraft_read_status" && left > 0) {
            st.aircraft.asked.set(key, left - 1);
            if (left === 1)
              for (const id of ids) {
                const i = Number(id.slice(1));
                const cur = st.keywords.get(i) ?? [];
                st.keywords.set(i, [...new Set([...cur, ...AIRCRAFT_TAGS])]);
                const by = st.aiSources.get(i) ?? new Map<string, string[]>();
                by.set(AIRCRAFT.id, [...AIRCRAFT_TAGS].sort());
                st.aiSources.set(i, by);
              }
          }
          const items = ids.length;
          const pending = left > 0 ? items : 0;
          const status = {
            items,
            analysed: pending || left < 0 ? 0 : items,
            pending,
            failed: 0,
            problem: null,
          };
          return cmd === "aircraft_read" ? { provider: AIRCRAFT.id, ids, status } : status;
        }
        case "faces_status": {
          const f = st.faces;
          const photos = all.filter((it) => it.kind !== "video").length;
          if (f.left > 0) {
            f.left -= 1;
            if (f.left === 0) f.scanned = photos;
          }
          // About 3 photos in 5 have faces, 1.5 faces each.
          const withFaces = Math.round(f.scanned * 0.6);
          return {
            enabled: f.enabled,
            aiEnabled: st.ai.enabled,
            modelsInstalled: f.installed,
            modelsBytes: 38928942,
            runtimeFound: true,
            accel: f.enabled && st.ai.enabled ? "graphics card (DirectML)" : null,
            problem: null,
            installing: null,
            photos,
            scanned: f.scanned,
            withFaces,
            faces: Math.round(withFaces * 1.5),
            people: f.people,
            pending: f.left > 0 ? photos - f.scanned : 0,
            failed: 0,
          };
        }
        case "faces_enable":
          if (a.enabled && !st.ai.enabled) throw "Turn on On-device AI first (just above).";
          st.faces.enabled = Boolean(a.enabled);
          if (st.faces.enabled && !st.faces.installed) {
            st.faces.installed = true;
            void emit("faces://install", {
              done: 38928942,
              total: 38928942,
              finished: true,
              error: null,
            });
          }
          return null;
        case "faces_install_cancel":
          return null;
        case "faces_scan_all": {
          if (!(st.faces.enabled && st.ai.enabled)) throw "Turn on Recognise faces first.";
          const photos = all.filter((it) => it.kind !== "video").length;
          st.faces.left = 2;
          return photos - st.faces.scanned;
        }
        case "faces_stop": {
          const was = st.faces.left > 0;
          st.faces.left = 0;
          return was ? 1 : 0;
        }
        case "faces_delete_all": {
          const n = Math.round(Math.round(st.faces.scanned * 0.6) * 1.5);
          st.faces.scanned = 0;
          st.faces.people = 0;
          st.faces.left = 0;
          return n;
        }
        case "faces_remove_models":
          st.faces.enabled = false;
          st.faces.installed = false;
          return true;
        case "ai_remove_model":
          st.ai.enabled = false;
          st.ai.modelInstalled = false;
          return true;
        case "ai_duplicates":
        case "ai_duplicate_day": {
          const groups = DUP_GROUPS.map((g) =>
            g.filter((i) => st.choice.get(i) !== "not_duplicate"),
          )
            .filter((g) => g.length >= 2)
            .map((g) => {
              const kept = g.find((i) => st.choice.get(i) === "keeper");
              const keeper = kept ?? g[0];
              return {
                // All on the first item's day (a mock simplification).
                date: day(time(all[0])) ?? "",
                keeper: `a${keeper}`,
                decided: kept !== undefined || g.every((i) => st.choice.has(i)),
                members: g.map((i) => ({
                  id: `a${i}`,
                  name: all[i].name,
                  video: all[i].kind === "video",
                  captureMs: time(all[i]),
                  rating: rating(all[i]),
                  choice: st.choice.get(i) ?? null,
                  likeness: i === keeper ? 100 : 96,
                  exact: false,
                  thumb: THUMBS[i % THUMBS.length],
                })),
              };
            });
          if (cmd === "ai_duplicate_day") return groups.filter((g) => g.date === a.date);
          if (!st.ai.enabled) return { days: [], groups: 0, items: 0, undecided: 0 };
          const inRange = (d: string) =>
            (!a.from || d >= String(a.from)) && (!a.to || d <= String(a.to));
          const dates = [...new Set(groups.map((g) => g.date))].filter(inRange).sort().reverse();
          const days = dates.map((date) => {
            const of = groups.filter((g) => g.date === date);
            return {
              date,
              groups: of.length,
              items: of.reduce((k, g) => k + g.members.length, 0),
              undecided: of.filter((g) => !g.decided).length,
            };
          });
          const shown = groups.filter((g) => inRange(g.date));
          return {
            days,
            groups: shown.length,
            items: shown.reduce((k, g) => k + g.members.length, 0),
            undecided: shown.filter((g) => !g.decided).length,
          };
        }
        case "ai_analyze_range":
        case "ai_range_status": {
          const key = `${a.what}|${a.from}|${a.to}`;
          if (cmd === "ai_analyze_range") {
            if (a.what === "aircraft") {
              if (!st.aircraft.enabled) throw "Turn on Read aircraft in Settings first.";
            } else if (!st.ai.enabled)
              throw "Turn on On-device AI in Settings first (it downloads the model once).";
            if (!st.aiRanges.has(key)) st.aiRanges.set(key, 2);
          }
          const items = all.filter((it) => {
            const d = day(time(it));
            return (
              d !== null &&
              d >= String(a.from) &&
              d <= String(a.to) &&
              (a.what !== "aircraft" || isAircraft(it))
            );
          }).length;
          const left = st.aiRanges.get(key);
          if (left === undefined) return { from: a.from, to: a.to, items, analysed: 0, pending: 0 };
          if (cmd === "ai_range_status" && left > 0) st.aiRanges.set(key, left - 1);
          const pending = left > 0 ? Math.ceil((items * left) / 3) : 0;
          const reply = { from: a.from, to: a.to, items, analysed: items - pending, pending };
          // ?queueing=<ms>: queueing the work takes that long (a busy drive).
          const ms = cmd === "ai_analyze_range" ? Number(params.get("queueing") ?? 0) : 0;
          return ms > 0 ? new Promise((r) => setTimeout(() => r(reply), ms)) : reply;
        }
        case "edit_suggestions":
        case "edit_suggestions_ask":
        case "edit_suggestions_stop": {
          const i = Number(String(a.id).slice(1));
          const it = all[i];
          if (!it) throw `no item ${String(a.id)}`;
          if (it.kind === "video") throw "Edit suggestions are for photos.";
          if (cmd === "edit_suggestions_stop") {
            const cur = st.suggest.get(i);
            if (cur?.asking && cur.asking.left > 0) cur.asking = null;
            return null;
          }
          if (cmd === "edit_suggestions_ask") {
            const p = CLOUD.find((x) => x.id === a.providerId);
            if (!p) throw `unknown AI ${String(a.providerId)}`;
            if (!st.cloud.keys[p.id])
              throw `Add your ${p.name} API key in Settings first (Settings → Tag with AI).`;
            const cur = st.suggest.get(i) ?? { madeMs: Date.now(), ai: null, asking: null };
            cur.asking = { provider: p.id, left: 2, failed: null };
            st.suggest.set(i, cur);
          } else if (st.suggestWait > 0) {
            st.suggestWait--;
            return {
              id: String(a.id),
              waiting: true,
              measured: [],
              summary: "",
              suggestions: [],
              madeMs: 0,
              ai: null,
              asking: null,
            };
          }
          let cur = st.suggest.get(i);
          if (!cur || a.again) {
            cur = { madeMs: Date.now(), ai: null, asking: cur?.asking ?? null };
            st.suggest.set(i, cur);
          }
          // The question moves on one step each look.
          const q = cur.asking;
          if (q && cmd === "edit_suggestions" && q.left > 0) {
            q.left--;
            const p = CLOUD.find((x) => x.id === q.provider)!;
            if (q.left === 0) {
              if (st.cloud.keys[p.id]?.endsWith("bad")) {
                q.failed = `${p.name} did not accept the API key: check it in Settings`;
              } else {
                cur.ai = {
                  provider: p.id,
                  providerName: p.name,
                  model: st.cloud.models[p.id],
                  summary:
                    "A lovely evening scene; the sky is a little blown and the foreground dark.",
                  suggestions: [
                    {
                      title: "Recover the sky",
                      why: "The clouds at the top are nearly white.",
                      photoshop: "Camera Raw > Masking > Select Sky, then Highlights -50",
                      topaz: "",
                    },
                    {
                      title: "Remove the bin on the right",
                      why: "It pulls the eye away from the people.",
                      photoshop: "Remove tool (J): paint over the bin",
                      topaz: "Remove: brush over the bin",
                    },
                  ],
                  madeMs: Date.now(),
                };
                cur.asking = null;
              }
            }
          }
          // Every mock photo measures dark and grainy.
          const dark = true;
          const shown = cur.asking;
          const shownP = shown && CLOUD.find((x) => x.id === shown.provider)!;
          return {
            id: String(a.id),
            waiting: false,
            measured: [
              {
                label: "Brightness",
                value: dark ? "Dark (middle tone 18%)" : "Fine (middle tone 44%)",
              },
              { label: "Highlights", value: "0% blown out" },
              { label: "Shadows", value: "0.4% pure black" },
              { label: "Contrast", value: "Fine" },
              { label: "Colour", value: "Greys look neutral" },
              { label: "Noise", value: dark ? "Shows" : "Low" },
              { label: "Sharpness", value: "Fine" },
              { label: "Level", value: "No tilt found" },
            ],
            summary: dark
              ? "Keepsake measured the photo and found 2 things to look at."
              : "Nothing stands out: exposure, contrast, colour, noise and sharpness all measure fine.",
            suggestions: dark
              ? [
                  {
                    title: "Brighten the photo",
                    why: "Half of it is darker than 18% grey and nothing is blown out, so there is room to lift it (unless it is meant to be dark).",
                    photoshop:
                      "Camera Raw > Light: Exposure +1.8, then Shadows +20 if the dark parts are still heavy.",
                    topaz:
                      "Adjust Lighting: start at Strength 30 and raise it until the subject looks right.",
                  },
                  {
                    title: "Reduce the noise",
                    why: "The smooth areas are grainy even in the small preview; at full size it will show more.",
                    photoshop:
                      "Camera Raw > Detail: Denoise (for a raw file) at 40, or Manual Noise Reduction: Luminance 25, Color 25.",
                    topaz:
                      "Denoise: the Standard model on Auto; lower Strength if fine detail turns waxy.",
                  },
                ]
              : [],
            madeMs: cur.madeMs,
            ai: cur.ai,
            asking:
              shown && shownP
                ? {
                    provider: shownP.id,
                    providerName: shownP.name,
                    pending: shown.failed === null,
                    problem: shown.failed,
                  }
                : null,
          };
        }
        case "ai_cloud_info":
        case "ai_cloud_set_key":
        case "ai_cloud_set_model":
        case "ai_cloud_set_default": {
          const c = st.cloud;
          if (cmd === "ai_cloud_set_key") {
            const p = CLOUD.find((x) => x.id === a.providerId);
            if (!p) throw `unknown AI provider ${String(a.providerId)}`;
            const k = typeof a.key === "string" ? a.key.trim() : "";
            if (k && !p.key.test(k)) throw `That does not look like a ${p.name} API key${p.hint}.`;
            c.keys[p.id] = k || null;
          }
          if (cmd === "ai_cloud_set_model") {
            const p = CLOUD.find((x) => x.models.some((m) => m.id === a.model));
            if (!p) throw `unknown model ${String(a.model)}`;
            c.models[p.id] = String(a.model);
          }
          if (cmd === "ai_cloud_set_default") c.def = String(a.providerId);
          return {
            defaultProvider: c.def,
            providers: CLOUD.map((p) => ({
              id: p.id,
              name: p.name,
              company: p.company,
              hasKey: c.keys[p.id] !== null,
              model: c.models[p.id],
              models: p.models,
            })),
          };
        }
        case "ai_cloud_tag":
        case "ai_cloud_status":
        case "ai_cloud_stop": {
          const p = CLOUD.find((x) => x.id === a.providerId);
          if (!p) throw `unknown AI provider ${String(a.providerId)}`;
          let ids = (a.ids ?? []) as string[];
          if (cmd === "ai_cloud_tag") {
            if (!st.cloud.keys[p.id]) throw `Add your ${p.name} API key in Settings first.`;
            ids = [...new Set(resolve(a.selection as Selection))].map((i) => `a${i}`);
            st.cloud.asked.set(`${p.id}|${ids.join(",")}`, 2);
          }
          const key = `${p.id}|${ids.join(",")}`;
          const left = st.cloud.asked.get(key) ?? 0;
          if (cmd === "ai_cloud_stop") {
            st.cloud.asked.set(key, 0);
            return null;
          }
          const bad = st.cloud.keys[p.id]?.endsWith("bad") ?? false;
          if (cmd === "ai_cloud_status" && left > 0) {
            st.cloud.asked.set(key, left - 1);
            if (left === 1 && !bad)
              for (const id of ids) {
                const i = Number(id.slice(1));
                const cur = st.keywords.get(i) ?? [];
                const given = CLOUD_TAGS[p.id] ?? [];
                st.keywords.set(i, [...new Set([...cur, ...given])]);
                const by = st.aiSources.get(i) ?? new Map<string, string[]>();
                by.set(p.id, [...new Set([...(by.get(p.id) ?? []), ...given])].sort());
                st.aiSources.set(i, by);
              }
          }
          const items = ids.length;
          const pending = left > 0 ? items : 0;
          const failed = !pending && bad ? items : 0;
          const status = {
            items,
            analysed: pending || bad ? 0 : items,
            pending,
            failed,
            problem: bad ? `${p.name} did not accept the API key: check it in Settings` : null,
          };
          return cmd === "ai_cloud_tag" ? { provider: p.id, ids, status } : status;
        }
        case "ai_range_stop": {
          st.aiRanges.set(`${a.what}|${a.from}|${a.to}`, 0);
          return null;
        }
        case "ai_analyze_folder":
        case "ai_folder_status":
        case "ai_folder_stop": {
          const key = `${a.what}|folder|${a.volume}|${a.folder}`;
          if (cmd === "ai_analyze_folder") {
            if (a.what === "aircraft") {
              if (!st.aircraft.enabled) throw "Turn on Read aircraft in Settings first.";
            } else if (!st.ai.enabled)
              throw "Turn on On-device AI in Settings first (it downloads the model once).";
            st.aiRanges.set(key, 3);
          }
          const q = { volume: a.volume, folder: a.folder || undefined } as unknown as Query;
          const items = all
            .filter(matches(q))
            .filter((it) =>
              a.what === "aircraft" ? isAircraft(it) : a.what === "tags" || it.kind === "raw",
            ).length;
          const left = st.aiRanges.get(key) ?? 0;
          if (cmd === "ai_folder_stop") {
            // What was done stays: a third, say.
            st.aiRanges.set(key, -Math.ceil(items / 3));
            return null;
          }
          if (left < 0) return { items, analysed: -left, pending: 0 };
          if (cmd === "ai_folder_status" && left > 0) st.aiRanges.set(key, left - 1);
          const pending = left > 0 ? Math.ceil((items * left) / 4) : 0;
          const reply = { items, analysed: items - pending, pending };
          const ms = cmd === "ai_analyze_folder" ? Number(params.get("queueing") ?? 0) : 0;
          return ms > 0 ? new Promise((r) => setTimeout(() => r(reply), ms)) : reply;
        }
        case "ai_analyze_undated":
        case "ai_undated_status":
        case "ai_undated_stop": {
          const key = `${a.what}|undated`;
          if (cmd === "ai_analyze_undated") {
            if (!st.ai.enabled)
              throw "Turn on On-device AI in Settings first (it downloads the model once).";
            st.aiRanges.set(key, 3);
          }
          const items = all
            .filter((it) => it.t === null)
            .filter((it) => a.what === "tags" || it.kind === "raw").length;
          const left = st.aiRanges.get(key) ?? 0;
          if (cmd === "ai_undated_stop") {
            st.aiRanges.set(key, -Math.ceil(items / 3));
            return null;
          }
          if (left < 0) return { items, analysed: -left, pending: 0 };
          if (cmd === "ai_undated_status" && left > 0) st.aiRanges.set(key, left - 1);
          const pending = left > 0 ? Math.ceil((items * left) / 4) : 0;
          return { items, analysed: items - pending, pending };
        }
        case "ai_pick_keeper": {
          const keeper = Number(String(a.keeper).slice(1));
          for (const id of a.group as string[]) {
            const i = Number(id.slice(1));
            st.choice.set(i, i === keeper ? "keeper" : "not_kept");
          }
          return null;
        }
        case "ai_set_choice":
          for (const id of a.ids as string[]) {
            const i = Number(id.slice(1));
            if (a.choice === null) st.choice.delete(i);
            else st.choice.set(i, String(a.choice));
          }
          return null;
        case "ai_tags": {
          const i = Number(String(a.id).slice(1));
          // Per AI, the keywords it gave that the item still has.
          const kw = st.keywords.get(i) ?? [];
          const by = st.aiSources.get(i);
          const byAi = [...CLOUD, AIRCRAFT].flatMap((p) => {
            const tags = (by?.get(p.id) ?? []).filter((t) => kw.includes(t));
            return tags.length ? [{ provider: p.id, name: p.name, tags }] : [];
          });
          if (!st.ai.enabled) return { enabled: false, analysed: false, tags: [], byAi };
          const gone = st.aiRejected.get(i) ?? [];
          const mine = st.keywords.get(i) ?? [];
          const tags = [AI_LABELS[i % 6], AI_LABELS[(i + 2) % 6]]
            .filter((t) => !gone.includes(t) && !mine.includes(t))
            .map((tag, k) => ({ tag, confidence: k === 0 ? 61 : 23 }));
          return { enabled: true, analysed: true, tags, byAi };
        }
        case "ai_tag_decide": {
          const i = Number(String(a.id).slice(1));
          const tag = String(a.tag);
          if (a.keep) st.keywords.set(i, [...(st.keywords.get(i) ?? []), tag]);
          else st.aiRejected.set(i, [...(st.aiRejected.get(i) ?? []), tag]);
          return null;
        }
        default:
          throw new Error(`mock: unknown command ${cmd}`);
      }
    },
    { shouldMockEvents: true },
  );
}
