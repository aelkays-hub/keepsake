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
import type { RouteSuggestionDto } from "../bindings/RouteSuggestionDto";
import type { CalendarLevelDto } from "../bindings/CalendarLevelDto";
import type { CardItem } from "../bindings/CardItem";
import type { HistoryRun } from "../bindings/HistoryRun";
import type { Details } from "../bindings/Details";
import type { ImportRequest } from "../bindings/ImportRequest";
import type { ItemKind } from "../bindings/ItemKind";
import type { AircraftReport } from "../bindings/AircraftReport";
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

/** A small THIRD-PARTY-NOTICES.txt in the real file's format (make-notices.py). */
const MOCK_NOTICES = (() => {
  const rule = `\n\n${"=".repeat(76)}\n\n`;
  const sub = `\n\n${"-".repeat(76)}\n\n`;
  return (
    [
      "THIRD-PARTY NOTICES FOR KEEPSAKE\n\nKeepsake includes the software listed below.",
      "1. PROGRAMS SHIPPED BESIDE KEEPSAKE\n\n" +
        "FFmpeg and ffprobe (separate programs, not linked)\n  Licence: GNU LGPL 3.0.\n\n" +
        "ExifTool 13.59 by Phil Harvey (separate program, not linked)",
      "2. RUST CRATES BUILT INTO KEEPSAKE (2)\n\n" +
        "rusqlite 0.40.0  —  MIT  https://github.com/rusqlite/rusqlite\n" +
        "    Licence text: [aaaaaaaaaaaa]\n" +
        "ts-rs 12.0.1  —  MIT  https://github.com/Aleph-Alpha/ts-rs\n" +
        "    Licence text: [bbbbbbbbbbbb] (standard text of its licence; the package carries no licence file)",
      "3. JAVASCRIPT PACKAGES IN KEEPSAKE'S WINDOW (1)\n\n" +
        "react 18.3.1  —  MIT  https://react.dev/\n    Licence text: [cccccccccccc]",
      [
        "4. LICENCE TEXTS\n\nEach text is followed by the packages it comes with.",
        "[aaaaaaaaaaaa] used by: rusqlite 0.40.0\n\nMIT License\n\nCopyright (c) 2014 The rusqlite developers",
        "[bbbbbbbbbbbb] used by: ts-rs 12.0.1\n\nMIT License\n\nCopyright (c) the authors of ts-rs (see its repository)",
        "[cccccccccccc] used by: react 18.3.1\n\nMIT License\n\nCopyright (c) Meta Platforms, Inc. and affiliates.",
      ].join(sub),
    ].join(rule) + "\n"
  );
})();

export interface MockState {
  items: number;
  /** Upload to Flickr (ADR-046): `?flickr=app` (key saved) or
   * `?flickr=signed` (signed in); `?flickrFail=a1` fails that photo. */
  flickr: {
    hasApp: boolean;
    signedIn: boolean;
    awaiting: boolean;
    uploads: Map<string, { url: string; privacy: string; date: string }[]>;
    /** Queued photos: how often their status was asked. */
    queue: Map<string, { polls: number; privacy: string }>;
    fail: string | null;
    /** Report → Flickr: reads started, and looks since the last. */
    accountReads: number;
    accountPolls: number;
  };
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
  aircraft: { enabled: boolean; installed: boolean; finder: boolean; asked: Map<string, number> };
  /** Read aircraft's review window: each photo's read (`left`: reviews
   * until it is read; −1: stopped before) and review state, and the
   * sliders. */
  /** `left` reads to go (−1: stopped); `next`: when the next one is done. */
  aircraftReads: Map<number, { left: number; review: string; next?: number }>;
  aircraftConf: {
    registrationPct: number;
    airlinePct: number;
    typePct: number;
    sameShotS: number;
  };
  /** Look registrations up online (`?lookup=on`). */
  aircraftLookup: boolean;
  /** OpenSky's aircraft database is on this computer (`?builtdb=on`; one
   * kept before the transponder codes were: `?builtdb=old`). */
  aircraftBuiltDb: boolean;
  aircraftBuiltCodes: boolean;
  /** When each registration was built, as kept (ADR-044). */
  aircraftBuilt: Map<string, { built: string; source: string }>;
  /** Add route (ADR-044): on (`?route=on`), the OpenSky client saved
   * (`?opensky=saved`), and how often each photo was asked about. */
  route: {
    enabled: boolean;
    saved: boolean;
    eastern: boolean;
    asked: Map<string, number>;
    /** Time zones set per item; codes typed per registration; with
     * `?nocode=on` no aircraft's code is known until typed. */
    zone: Map<string, number>;
    codes: Map<string, string>;
    noCode: boolean;
    /** `auto` (the default), `opensky` or `flightradar`
     * (`?routesource=flightradar`); the Flightradar24 token saved
     * (`?fr24=saved`); how far back its plan goes (`?fr24days=5`). */
    source: string;
    fr24Days: number;
    fr24: boolean;
    /** Look up aircraft on Flightradar24: asked so far, running. */
    lookedUp: number;
    lookingUp: boolean;
    /** Look new registrations up on their own. */
    fr24Auto: boolean;
    /** National registers downloaded (`?registers=tc,faa`). */
    registers: Set<string>;
    /** Fill in route keywords: how often each photo was looked at. */
    fill: Map<string, number>;
    /** Flight numbers chosen, per photo. */
    chosen: Map<string, string>;
  };
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
const AIRCRAFT_BYTES = 88030804 + 7830888 + 35858002;
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
/** How long the mock takes to read a photo's aircraft. */
const READ_MS = 1000;

/** Report → Aircraft: a small fixed fleet. */
function mockAircraftReport(): AircraftReport {
  const row = (
    kind: string,
    name: string,
    photos: number,
    registrations: number,
    airlines: string[],
    types: string[],
    first: string | null,
    last: string | null,
    built: string | null = null,
  ) => ({
    name,
    path: `Aircraft|${kind}|${name}`,
    photos,
    registrations,
    airlines,
    types,
    first,
    last,
    built,
  });
  return {
    totals: {
      photos: 120,
      shots: 110,
      withMarks: 40,
      withAirline: 38,
      withType: 30,
      withRegistration: 25,
      airlines: 3,
      types: 3,
      registrations: 4,
      waitingReview: 7,
    },
    airlines: [
      row(
        "Airline",
        "Air Canada",
        20,
        2,
        [],
        ["Boeing 787-9", "Boeing 777-300ER"],
        "2018-01-02",
        "2024-06-01",
      ),
      row("Airline", "KLM", 12, 1, [], ["Boeing 747"], "2019-01-06", "2019-01-06"),
      row("Airline", "Interjet", 6, 1, [], [], "2019-01-06", "2019-01-06"),
    ],
    types: [
      row("Type", "Boeing 787-9", 14, 2, ["Air Canada"], [], "2018-01-02", "2024-06-01"),
      row("Type", "Boeing 747", 12, 1, ["KLM"], [], "2019-01-06", "2019-01-06"),
      row("Type", "Boeing 777-300ER", 4, 1, ["Air Canada"], [], "2020-02-02", "2020-02-02"),
    ],
    registrations: [
      row(
        "Registration",
        "PH-BFA",
        12,
        0,
        ["KLM"],
        ["Boeing 747"],
        "2019-01-06",
        "2019-01-06",
        "1989",
      ),
      row(
        "Registration",
        "C-FVLU",
        10,
        0,
        ["Air Canada"],
        ["Boeing 787-9"],
        "2018-01-02",
        "2024-06-01",
        "2016-03-10",
      ),
      row(
        "Registration",
        "C-FITU",
        4,
        0,
        ["Air Canada"],
        ["Boeing 777-300ER"],
        "2020-02-02",
        "2020-02-02",
        "2008",
      ),
      row("Registration", "XA-MBA", 6, 0, ["Interjet"], [], "2019-01-06", "2019-01-06"),
    ],
    years: [
      { year: "2018", photos: 30, withMarks: 10 },
      { year: "2019", photos: 60, withMarks: 25 },
      { year: "2024", photos: 25, withMarks: 5 },
      { year: null, photos: 5, withMarks: 0 },
    ],
    byAirline: [
      {
        name: "Air Canada",
        photos: 20,
        children: [
          {
            name: "Boeing 787-9",
            photos: 14,
            children: [
              { name: "C-FVLU", photos: 10, children: [] },
              { name: null, photos: 4, children: [] },
            ],
          },
          {
            name: "Boeing 777-300ER",
            photos: 4,
            children: [{ name: "C-FITU", photos: 4, children: [] }],
          },
          { name: null, photos: 2, children: [{ name: null, photos: 2, children: [] }] },
        ],
      },
      {
        name: "KLM",
        photos: 12,
        children: [
          {
            name: "Boeing 747",
            photos: 12,
            children: [{ name: "PH-BFA", photos: 12, children: [] }],
          },
        ],
      },
      {
        name: "Interjet",
        photos: 6,
        children: [
          { name: null, photos: 6, children: [{ name: "XA-MBA", photos: 6, children: [] }] },
        ],
      },
    ],
  };
}

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
    flickr: {
      hasApp: ["app", "signed"].includes(params.get("flickr") ?? ""),
      signedIn: params.get("flickr") === "signed",
      awaiting: false,
      uploads: new Map(
        params.get("flickrDone")
          ? [
              [
                String(params.get("flickrDone")),
                [
                  {
                    url: "https://www.flickr.com/photos/1%40N00/1/",
                    privacy: "public",
                    date: "2026-11-01",
                  },
                ],
              ],
            ]
          : [],
      ),
      queue: new Map(),
      fail: params.get("flickrFail"),
      accountReads: 0,
      accountPolls: 0,
    },
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
      finder: params.get("finder") !== "off",
      asked: new Map(),
    },
    aircraftReads: new Map(),
    aircraftConf: { registrationPct: 80, airlinePct: 85, typePct: 70, sameShotS: 10 },
    aircraftLookup: params.get("lookup") === "on",
    aircraftBuiltDb: ["on", "old"].includes(params.get("builtdb") ?? ""),
    aircraftBuiltCodes: params.get("builtdb") === "on",
    route: {
      enabled: params.get("route") === "on",
      eastern: false,
      zone: new Map(),
      codes: new Map(),
      noCode: params.get("nocode") === "on",
      source: params.get("routesource") ?? "auto",
      fr24Days: Number(params.get("fr24days") ?? 30),
      fr24: params.get("fr24") === "saved",
      lookedUp: 0,
      fr24Auto: true,
      registers: new Set((params.get("registers") ?? "").split(",").filter(Boolean)),
      fill: new Map(),
      chosen: new Map(),
      lookingUp: false,
      saved: params.get("opensky") === "saved",
      asked: new Map(),
    },
    aircraftBuilt: new Map(
      params.get("builtdb") === "on" ? [["C-FJZS", { built: "2008", source: "opensky" }]] : [],
    ),
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
          const reply = { version: "0.11.1-mock", library, uiTest: true };
          // ?opening=<ms>: the library takes that long to open.
          const ms = Number(params.get("opening") ?? 0);
          return ms > 0 ? new Promise((r) => setTimeout(() => r(reply), ms)) : reply;
        }
        case "third_party_notices":
          return MOCK_NOTICES;
        case "about_open":
          return null;
        case "library_create":
        case "library_open":
          library = String(a.path);
          return { version: "0.11.1-mock", library, uiTest: true };
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
          const line = (n: string) =>
            // ?topazOpen=1: Topaz is already running (it ignores files then).
            /topaz/i.test(n) && params.get("topazOpen") === "1"
              ? `${n} was already open, so it may not show this photo: close ${n} and choose Open in again.`
              : `Opened with ${n}`;
          if (app === null) return line(apps.length ? name(apps[0]) : "the system default");
          if (app === "") return line("the system default");
          if (!apps.includes(app))
            throw new Error(`${name(app)} is not set up in Settings for this file type`);
          return line(name(app));
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
        case "aircraft_report":
          return mockAircraftReport();
        case "flight_report":
          return {
            withoutRoute: 12,
            flights: (
              [
                [
                  "2026-06-27",
                  "12:56",
                  "BG305",
                  "YYZ",
                  "Canada",
                  "DAC",
                  "Bangladesh",
                  "Biman Bangladesh",
                  "S2-AJY",
                  "Boeing 787-9",
                  14,
                ],
                [
                  "2026-06-07",
                  "20:00",
                  null,
                  "CMN",
                  "Morocco",
                  "YYZ",
                  "Canada",
                  "Royal Air Maroc",
                  "CN-RGY",
                  "Boeing 787-9",
                  6,
                ],
                [
                  "2026-05-31",
                  "08:31",
                  "RV2000",
                  "YYZ",
                  "Canada",
                  null,
                  null,
                  "Air Canada Rouge",
                  "C-GBIJ",
                  "Airbus A321",
                  3,
                ],
              ] as const
            ).map(
              ([
                date,
                time,
                flight,
                from,
                fromCountry,
                to,
                toCountry,
                airline,
                registration,
                aircraftType,
                photos,
              ]) => ({
                date,
                time,
                flight,
                from,
                fromCountry,
                to,
                toCountry,
                airline,
                registration,
                aircraftType,
                photos,
                firstPhoto: "A7402789.ARW",
                tags: [`Aircraft|Registration|${registration}`],
              }),
            ),
          };
        case "flight_report_csv": {
          const path = String(a.path ?? "").trim();
          if (!path) throw new Error("choose where to save the report");
          return /\.[^\\/.]+$/.test(path) ? path : `${path}.csv`;
        }
        case "aircraft_report_csv": {
          const path = String(a.path ?? "").trim();
          if (!path) throw new Error("choose where to save the report");
          return /\.[^\\/.]+$/.test(path) ? path : `${path}.csv`;
        }
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
            "Program: keepsake-app 0.11.1-mock",
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
            finderInstalled: st.aircraft.installed && st.aircraft.finder,
            finderBytes: 35858002,
            modelsBytes: AIRCRAFT_BYTES,
            runtimeFound: true,
            looks: st.ai.modelInstalled,
            accel: st.aircraft.enabled ? "graphics card (DirectML)" : null,
            problem: null,
            installing: null,
            confidence: st.aircraftConf,
            lookup: st.aircraftLookup,
            builtInstalled: st.aircraftBuiltDb,
            builtCodes: st.aircraftBuiltCodes,
            builtBytes: 94509542,
            builtInstalling: null,
          };
        case "aircraft_built_install":
          st.aircraftBuiltDb = true;
          st.aircraftBuiltCodes = true;
          if (!st.aircraftBuilt.has("C-FJZS"))
            st.aircraftBuilt.set("C-FJZS", { built: "2008", source: "opensky" });
          void emit("aircraft://built-install", {
            done: 0,
            total: 94509542,
            finished: true,
            error: null,
          });
          return null;
        case "aircraft_built_cancel":
          return null;
        case "aircraft_route_status":
          return {
            enabled: st.route.enabled,
            credentials:
              st.route.source === "flightradar"
                ? st.route.fr24
                : st.route.source === "opensky"
                  ? st.route.saved
                  : st.route.saved || st.route.fr24,
            source: st.route.source,
            fr24Days: st.route.fr24Days,
            opensky: st.route.saved,
            flightradar: st.route.fr24,
            lookup: st.aircraftLookup,
            eastern: st.route.eastern,
          };
        case "aircraft_set_route":
          st.route.enabled = Boolean(a.enabled);
          void emit("aircraft://status", null);
          return null;
        case "aircraft_set_route_source":
          st.route.source = ["opensky", "flightradar"].includes(String(a.source))
            ? String(a.source)
            : "auto";
          void emit("aircraft://status", null);
          return null;
        case "flightradar_set_token":
          st.route.fr24 = Boolean(String(a.token ?? "").trim());
          void emit("aircraft://status", null);
          return null;
        case "fr24_aircraft": {
          // Known for C-FJZS once looked up (`?fr24=saved` and a run).
          const reg = String(a.registration);
          if (st.route.lookedUp === 0) return null;
          return reg === "C-FJZS"
            ? {
                registration: reg,
                found: true,
                aircraftType: "Boeing 787-9",
                icaoType: "B789",
                airline: "Air Canada",
                livery: "ACA",
                hex: "C05E2B",
                seen: "2026-11-19",
                keptUntil: "2026-12-20",
              }
            : null;
        }
        case "fr24_aircraft_status": {
          // 40 registrations; each look at the status while running asks 10.
          if (st.route.lookingUp) {
            st.route.lookedUp = Math.min(40, st.route.lookedUp + 10);
            if (st.route.lookedUp >= 40) st.route.lookingUp = false;
          }
          return {
            token: st.route.fr24,
            registrations: 40,
            asked: st.route.lookedUp,
            found: Math.floor(st.route.lookedUp * 0.75),
            new: 40 - st.route.lookedUp,
            auto: st.route.fr24Auto,
            running: st.route.lookingUp,
            problem: null,
          };
        }
        case "fr24_set_auto":
          st.route.fr24Auto = Boolean(a.on);
          return null;
        case "aircraft_registers":
          return [
            ["tc", "Transport Canada", 4_600_000, 34919, "Thu, 19 Nov 2026 16:44:22 GMT"],
            ["faa", "FAA", 73_000_000, 319530, "Fri, 20 Nov 2026 05:58:34 GMT"],
          ].map(([id, name, bytes, rows, date]) => {
            const on = st.route.registers.has(id as string);
            return {
              id,
              name,
              bytes,
              downloaded: on,
              fileModified: on ? date : null,
              imported: on ? "2026-11-20 12:00 UTC" : null,
              rows: on ? rows : 0,
              running: false,
              done: 0,
              importing: false,
            };
          });
        case "aircraft_register_update": {
          // Downloaded and imported at once; a second time: unchanged.
          const id = String(a.id);
          const had = st.route.registers.has(id);
          st.route.registers.add(id);
          void emit("aircraft://register", {
            id,
            done: 0,
            total: 0,
            importing: false,
            finished: true,
            message: had ? "Already up to date." : "34919 aircraft imported.",
            error: null,
          });
          return null;
        }
        case "aircraft_register_cancel":
          return null;
        case "aircraft_register_remove":
          st.route.registers.delete(String(a.id));
          return null;
        case "aircraft_register": {
          const reg = String(a.registration);
          if (!st.route.registers.has("tc") || reg !== "C-FJZS") return null;
          return {
            registration: "C-FJZS",
            register: "Transport Canada",
            aircraftType: "Boeing 777-300ER",
            maker: "Boeing",
            model: "777-333ER",
            serial: "62400",
            built: "2016",
            modeS: "C01A61",
            owner: "Air Canada",
            airline: "Air Canada",
            current: true,
          };
        }
        case "fr24_aircraft_start":
          if (!st.route.fr24) throw "Add your Flightradar24 API token first.";
          st.route.lookingUp = true;
          return true;
        case "fr24_aircraft_stop":
          st.route.lookingUp = false;
          return null;
        case "flightradar_test":
          if (!st.route.fr24)
            throw "No Flightradar24 API token: add it in Settings → Read aircraft";
          return "Flightradar24 accepted the API token.";
        case "aircraft_set_route_eastern":
          st.route.eastern = Boolean(a.eastern);
          void emit("aircraft://status", null);
          return null;
        case "aircraft_route_set_code": {
          const reg = String(a.registration);
          if (a.code) st.route.codes.set(reg, String(a.code));
          else st.route.codes.delete(reg);
          return null;
        }
        case "time_zone_apply": {
          const ids = resolve(a.selection as Selection);
          for (const i of ids) st.route.zone.set(`a${i}`, Number(a.offsetMin));
          return { changed: ids.length, background: false };
        }
        case "aircraft_route_again": {
          const ids = (a.ids ?? []) as string[];
          for (const id of ids) st.route.asked.set(id, 0);
          return ids.length;
        }
        case "opensky_set_credentials": {
          const id = String(a.clientId ?? "").trim();
          const secret = String(a.secret ?? "").trim();
          if (Boolean(id) !== Boolean(secret)) throw "Fill in both the client ID and the secret.";
          st.route.saved = Boolean(id);
          void emit("aircraft://status", null);
          return null;
        }
        case "opensky_test":
          if (!st.route.saved)
            throw "No OpenSky client ID and secret: add them in Settings → Read aircraft";
          return "OpenSky accepted the client ID and secret.";
        case "flickr_status":
        case "flickr_set_app":
        case "flickr_sign_in_finish":
        case "flickr_sign_out": {
          const f = st.flickr;
          if (cmd === "flickr_set_app") {
            const k = String(a.key ?? "").trim();
            const sec = String(a.secret ?? "").trim();
            if (Boolean(k) !== Boolean(sec)) throw "Fill in both the key and the secret.";
            f.hasApp = Boolean(k);
            f.signedIn = false;
            f.awaiting = false;
          }
          if (cmd === "flickr_sign_in_finish") {
            if (!f.awaiting) throw "Choose Sign in to Flickr first.";
            if (String(a.code ?? "").trim() !== "123-456-789")
              throw "Flickr did not accept the sign-in (token_rejected). Check the API key and secret, and sign in again.";
            f.signedIn = true;
            f.awaiting = false;
          }
          if (cmd === "flickr_sign_out") f.signedIn = false;
          return {
            hasApp: f.hasApp,
            signedIn: f.signedIn,
            username: f.signedIn ? "keepsake-tester" : null,
            awaitingCode: f.awaiting,
            uploaded: f.uploads.size,
          };
        }
        case "flickr_sign_in_start":
          if (!st.flickr.hasApp) throw "Add your Flickr API key and secret first.";
          st.flickr.awaiting = true;
          return "https://www.flickr.com/services/oauth/authorize?oauth_token=72157-mock&perms=write";
        case "flickr_test":
          if (!st.flickr.signedIn) throw "Sign in to Flickr first.";
          return "Flickr says you are signed in as keepsake-tester.";
        case "flickr_open":
          return null;
        case "flickr_prepare": {
          const ids = [...new Set(resolve(a.selection as Selection))];
          const camera = Boolean(a.camera);
          const machine = Boolean(a.machine);
          const parents = Boolean(a.parents);
          return {
            items: ids.map((i) => {
              const id = `a${i}`;
              const it = all[i];
              const kw = [...(st.keywords.get(i) ?? []), "Aircraft|Registration|C-FEJC"];
              const tags: string[] = [];
              for (const k of kw) {
                const segs = k.split("|");
                if (parents) tags.push(...segs.slice(0, -1));
                tags.push(segs[segs.length - 1]);
                if (machine && segs.length >= 3)
                  tags.push(
                    `${segs[0].toLowerCase()}:${segs[1].toLowerCase().replace(/ /g, "_")}=${segs.slice(2).join(" ")}`,
                  );
              }
              if (camera) tags.push("Sony", "Sony ILCE-1", "FE 200-600mm F5.6-6.3 G OSS");
              return {
                id,
                name: `${id}.ARW`,
                thumb: null,
                title: id,
                description: "",
                tags: [...new Set(tags)],
                video: it?.kind === "video",
                uploads: st.flickr.uploads.get(id) ?? [],
              };
            }),
            status: {
              hasApp: st.flickr.hasApp,
              signedIn: st.flickr.signedIn,
              username: st.flickr.signedIn ? "keepsake-tester" : null,
              awaitingCode: false,
              uploaded: st.flickr.uploads.size,
            },
          };
        }
        case "flickr_account_read":
        case "flickr_account": {
          // The account: four photos over two years; read on the second look.
          const f = st.flickr;
          if (!f.signedIn) {
            if (cmd === "flickr_account_read")
              throw "Sign in to Flickr in Settings → Flickr first.";
          } else if (cmd === "flickr_account_read" && (a.again || f.accountReads === 0)) {
            f.accountReads += 1;
            f.accountPolls = 0;
          } else if (cmd === "flickr_account") f.accountPolls += 1;
          const done = f.accountReads > 0 && f.accountPolls > 0;
          return {
            signedIn: f.signedIn,
            username: f.signedIn ? "keepsake-tester" : null,
            reading: f.signedIn && !done,
            read: done ? 4 : 0,
            total: 4,
            error: null,
            readAt: done ? "14:02" : null,
            photos: done ? 4 : 0,
            years: done
              ? [
                  {
                    year: 2026,
                    count: 3,
                    months: [
                      {
                        month: "2026-11",
                        count: 3,
                        days: [
                          { date: "2026-11-21", count: 2 },
                          { date: "2026-11-02", count: 1 },
                        ],
                      },
                    ],
                  },
                  {
                    year: 2019,
                    count: 1,
                    months: [
                      { month: "2019-07", count: 1, days: [{ date: "2019-07-04", count: 1 }] },
                    ],
                  },
                ]
              : [],
          };
        }
        case "flickr_account_day": {
          const photo = (id: string, title: string, uploaded: string, keepsake: boolean) => ({
            id,
            title,
            thumb: null,
            url: `https://www.flickr.com/photos/1%40N00/${id}/`,
            privacy: "public",
            uploaded,
            taken: null,
            keepsake,
          });
          const byDay: Record<string, ReturnType<typeof photo>[]> = {
            "2026-11-21": [
              photo("501", "C-FEJC landing", "2026-11-21 14:01", true),
              photo("502", "Sunset", "2026-11-21 09:30", false),
            ],
            "2026-11-02": [photo("401", "", "2026-11-02 08:00", false)],
            "2019-07-04": [photo("101", "Fireworks", "2019-07-04 22:10", false)],
          };
          return byDay[String(a.date)] ?? [];
        }
        case "flickr_upload": {
          if (!st.flickr.signedIn) throw "Sign in to Flickr in Settings → Flickr first.";
          const items = (a.items as { id: string }[]) ?? [];
          for (const it of items)
            st.flickr.queue.set(it.id, { polls: 0, privacy: String(a.privacy) });
          return items.length;
        }
        case "flickr_upload_stop": {
          for (const id of (a.ids as string[]) ?? []) st.flickr.queue.delete(id);
          return null;
        }
        case "flickr_upload_status": {
          // Uploading on the first look, done (or failed) on the second.
          const ids = (a.ids as string[]) ?? [];
          let pending = 0;
          const items = ids.map((id) => {
            const q = st.flickr.queue.get(id);
            if (!q) {
              const done = st.flickr.uploads.get(id)?.[0];
              return done
                ? { id, state: "done", url: done.url, error: null }
                : { id, state: "none", url: null, error: null };
            }
            q.polls += 1;
            if (q.polls < 2) {
              pending += 1;
              return { id, state: "uploading", url: null, error: null };
            }
            st.flickr.queue.delete(id);
            if (id === st.flickr.fail)
              return {
                id,
                state: "failed",
                url: null,
                error:
                  "Flickr said: User exceeded upload limit (error 6). Your Flickr account's upload limit is reached (a free account keeps 1,000 photos and videos).",
              };
            const url = `https://www.flickr.com/photos/1%40N00/${id.slice(1)}/`;
            st.flickr.uploads.set(id, [
              { url, privacy: q.privacy, date: "2026-11-21" },
              ...(st.flickr.uploads.get(id) ?? []),
            ]);
            return { id, state: "done", url, error: null };
          });
          return { pending, items };
        }
        case "aircraft_route_fill": {
          const ids = [...new Set(resolve(a.selection as Selection))].map((i) => `a${i}`);
          for (const id of ids) st.route.fill.set(id, 0);
          return {
            pending: ids.length,
            items: ids.map((id, i) => ({
              id,
              name: `${id}.ARW`,
              state: "waiting",
              added: [],
              had:
                i === 0
                  ? ["Aircraft|Flight|AT210"]
                  : i === 1
                    ? ["Aircraft|From|YYZ", "Aircraft|To|IND"]
                    : [],
              message: null,
              choices: [],
            })),
          };
        }
        case "aircraft_route_fill_choose": {
          st.route.chosen.set(String(a.id), String(a.flight));
          return `Aircraft|Flight|${String(a.flight)}`;
        }
        case "aircraft_route_fill_status": {
          // The first photo gets its airports; the others have no route
          // keyword. Done on the second look.
          const ids = (a.ids as string[]) ?? [];
          let pending = 0;
          const items = ids.map((id, i) => {
            const n = (st.route.fill.get(id) ?? 0) + 1;
            st.route.fill.set(id, n);
            const name = `${id}.ARW`;
            const base = { id, name, had: [], choices: [] as string[] };
            if (n < 2) {
              pending += 1;
              return { ...base, state: "waiting", added: [], message: null };
            }
            if (i === 0)
              return {
                ...base,
                state: "done",
                added: [
                  "Aircraft|Flight|AT210",
                  "Aircraft|From|CMN",
                  "Aircraft|From country|Morocco",
                  "Aircraft|To|YYZ",
                  "Aircraft|To country|Canada",
                ],
                message: null,
              };
            if (i === 1)
              return {
                ...base,
                state: "nothing",
                added: [],
                message: "QK893 and QK895 fly YYZ → IND: choose the one photographed",
                choices: ["QK893", "QK895"],
              };
            return {
              ...base,
              state: "nothing",
              added: [],
              message:
                "no route keyword: add Aircraft|Flight|…, Aircraft|From|… or Aircraft|To|… first",
            };
          });
          return { pending, items };
        }
        case "aircraft_route": {
          if (!st.route.enabled)
            throw "Turn on Find the route online in Settings → Read aircraft first.";
          const ids = [...new Set(resolve(a.selection as Selection))].map((i) => `a${i}`);
          for (const id of ids) st.route.asked.set(id, 0);
          return {
            provider: st.route.source === "flightradar" ? "Flightradar24" : "OpenSky",
            ids,
            status: {
              items: ids.length,
              analysed: 0,
              pending: ids.length,
              failed: 0,
              problem: null,
            },
          };
        }
        case "aircraft_route_stop":
          for (const id of (a.ids ?? []) as string[]) st.route.asked.set(id, -1);
          return null;
        case "aircraft_route_review": {
          // Each photo is "asked about" on the first look and answered on
          // the next. By the item's number: a departure (YYZ → LHR), two
          // flights as near, no flight near, nothing known; videos and
          // photos not of aircraft have no registration.
          const choice = (
            from: [string, string, string] | null,
            to: [string, string, string] | null,
            callsign: string,
            flight: string,
            at: string,
            gapMin: number,
          ) => ({
            callsign,
            flight,
            from: from?.[0] ?? null,
            fromIcao: from?.[1] ?? null,
            fromCountry: from?.[2] ?? null,
            to: to?.[0] ?? null,
            toIcao: to?.[1] ?? null,
            toCountry: to?.[2] ?? null,
            departed: "2026-05-31T12:16:00Z",
            arrived: "2026-05-31T19:25:00Z",
            at,
            gapMin,
            inferred: false,
            byCallsign: false,
            keywords: [
              ...(from ? [`Aircraft|From|${from[0]}`, `Aircraft|From country|${from[2]}`] : []),
              ...(to ? [`Aircraft|To|${to[0]}`, `Aircraft|To country|${to[2]}`] : []),
              `Aircraft|Flight|${flight}`,
            ],
          });
          const YYZ: [string, string, string] = ["YYZ", "CYYZ", "Canada"];
          const LHR: [string, string, string] = ["LHR", "EGLL", "United Kingdom"];
          const YUL: [string, string, string] = ["YUL", "CYUL", "Canada"];
          let pending = 0;
          const items = ((a.ids ?? []) as string[]).map((id) => {
            const i = Number(id.slice(1));
            const it = all[i];
            const kws = st.keywords.get(i) ?? [];
            const reg =
              kws.find((k) => k.startsWith("Aircraft|Registration|"))?.slice(22) ??
              (it && isAircraft(it) ? "C-FJZS" : null);
            const n = st.route.asked.get(id) ?? 1;
            if (n >= 0) st.route.asked.set(id, n + 1);
            const offset = st.route.zone.get(id) ?? (st.route.eastern ? -240 : -300);
            const code = (reg && st.route.codes.get(reg)) ?? (st.route.noCode ? null : "C04826");
            // Automatic: by the photo's date; here every other photo is
            // within the Flightradar24 plan, when a token is saved.
            const source =
              st.route.source !== "auto"
                ? st.route.source
                : st.route.fr24 && (i % 2 === 1 || !st.route.saved)
                  ? "flightradar"
                  : "opensky";
            const base = {
              id,
              name: it?.name ?? null,
              thumb: null,
              registration: reg,
              // Marked UTC−05:00; read as Eastern daylight time, an hour
              // earlier; a time zone set keeps the clock time (07:31).
              taken: new Date(Date.UTC(2026, 4, 31, 7, 31, 11) - offset * 60_000)
                .toISOString()
                .replace(".000", ""),
              local: "2026-05-31T07:31:11",
              offsetMin: offset,
              zoneSet: st.route.zone.has(id),
              code: reg ? code : null,
              codeTyped: reg !== null && st.route.codes.has(reg),
              source: reg ? source : null,
              error: null,
              choices: [] as ReturnType<typeof choice>[],
              has: kws.filter((k) =>
                /^Aircraft\|(From|To|From country|To country|Flight)\|/.test(k),
              ),
            };
            if (!reg) return { ...base, state: "no-registration" };
            if (!code && source !== "flightradar") return { ...base, state: "no-transponder" };
            if (n === 0) {
              pending += 1;
              return { ...base, state: "waiting" };
            }
            switch ((i / 5) % 4) {
              case 0:
                return {
                  ...base,
                  state: "found",
                  choices: [choice(YYZ, LHR, "ACA856", "AC856", "departure", 15)],
                };
              case 1:
                return {
                  ...base,
                  state: "several",
                  choices: [
                    choice(null, YYZ, "ACA443", "AC443", "arrival", 30),
                    choice(YYZ, YUL, "ACA412", "AC412", "departure", 30),
                  ],
                };
              case 2:
                return { ...base, state: "none-near" };
              default:
                return { ...base, state: "nothing" };
            }
          });
          return { items, pending, failed: 0 };
        }
        case "aircraft_route_accept": {
          const keep = (a.keywords ?? []) as string[];
          if (keep.some((k) => !/^Aircraft\|(From|To|From country|To country|Flight)\|/.test(k)))
            throw `${keep.find((k) => !k.startsWith("Aircraft|From"))} is not a route keyword`;
          const i = Number(String(a.id).slice(1));
          const cur = st.keywords.get(i) ?? [];
          const next = [...new Set([...cur, ...keep])];
          st.keywords.set(i, next);
          return next.length > cur.length ? 1 : 0;
        }
        case "aircraft_route_suggest": {
          // The routes of the other photos with this flight number, most
          // photos first, then the most recent (as keepsake-core's suggest).
          const sold: Record<string, string> = { ACA: "AC", DLH: "LH", ROU: "RV" };
          const key = (f: string) => {
            const c = f.replace(/[^0-9a-z]/gi, "").toUpperCase();
            const m = /^([A-Z]{3})(\d{1,4})$/.exec(c);
            return m && sold[m[1]] ? `${sold[m[1]]}${Number(m[2])}` : c;
          };
          const countries: Record<string, string> = {
            YYZ: "Canada",
            YUL: "Canada",
            YVR: "Canada",
            LHR: "United Kingdom",
          };
          const want = key(String(a.flight ?? ""));
          const self = Number(String(a.id).slice(1));
          const out: RouteSuggestionDto[] = [];
          const at: number[] = [];
          if (!want) return out;
          for (const [i, kws] of st.keywords) {
            if (i === self) continue;
            const get = (kind: string) =>
              kws.find((k) => k.startsWith(`Aircraft|${kind}|`))?.slice(kind.length + 10) ?? null;
            const flight = get("Flight");
            const from = get("From");
            const to = get("To");
            if (!flight || key(flight) !== want || (!from && !to)) continue;
            const fromCountry = get("From country") ?? (from ? (countries[from] ?? null) : null);
            const toCountry = get("To country") ?? (to ? (countries[to] ?? null) : null);
            const t = all[i] ? (time(all[i]) ?? 0) : 0;
            const j = out.findIndex((s) => s.from === from && s.to === to);
            if (j >= 0) {
              out[j].photos += 1;
              at[j] = Math.max(at[j], t);
              continue;
            }
            const keywords = [
              ...(from ? [`Aircraft|From|${from}`] : []),
              ...(fromCountry ? [`Aircraft|From country|${fromCountry}`] : []),
              ...(to ? [`Aircraft|To|${to}`] : []),
              ...(toCountry ? [`Aircraft|To country|${toCountry}`] : []),
              `Aircraft|Flight|${flight}`,
            ];
            out.push({
              from,
              fromCountry,
              to,
              toCountry,
              flight,
              photos: 1,
              latest: null,
              keywords,
            });
            at.push(t);
          }
          const order = out.map((s, j) => ({ s: { ...s, latest: day(at[j]) }, t: at[j] }));
          order.sort((x, y) => y.s.photos - x.s.photos || y.t - x.t);
          return order.map((o) => o.s);
        }
        case "aircraft_built_remove": {
          const had = st.aircraftBuiltDb;
          st.aircraftBuiltDb = false;
          return had;
        }
        case "aircraft_built":
          return ((a.registrations ?? []) as string[]).flatMap((r) => {
            const k = st.aircraftBuilt.get(r);
            return k ? [{ registration: r, ...k }] : [];
          });
        case "aircraft_set_built": {
          const reg = String(a.registration ?? "");
          const typed = a.built == null ? null : String(a.built).trim();
          if (typed && !/^\d{4}(-\d{2}-\d{2})?$/.test(typed))
            throw `${typed} is not a year (2014) or a date (2014-03-05)`;
          const before = st.aircraftBuilt.get(reg);
          if (typed) st.aircraftBuilt.set(reg, { built: typed, source: "typed" });
          else st.aircraftBuilt.delete(reg);
          let changed = 0;
          for (const [i, kws] of st.keywords) {
            if (!kws.includes(`Aircraft|Registration|${reg}`)) continue;
            const old = before ? `Aircraft|Built|${before.built.slice(0, 4)}` : null;
            const next = kws.filter(
              (k) => !(k.startsWith("Aircraft|Built|") && (typed !== null || k === old)),
            );
            if (typed) next.push(`Aircraft|Built|${typed.slice(0, 4)}`);
            if (next.join() !== kws.join()) {
              st.keywords.set(i, next);
              changed += 1;
            }
          }
          return changed;
        }
        case "aircraft_enable":
          st.aircraft.enabled = Boolean(a.enabled);
          if (st.aircraft.enabled && (!st.aircraft.installed || !st.aircraft.finder)) {
            st.aircraft.installed = true;
            st.aircraft.finder = true;
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
            // Read again: waits for review again.
            for (const id of ids)
              st.aircraftReads.set(Number(id.slice(1)), { left: 1, review: "waiting" });
          }
          const key = ids.join(",");
          const left = st.aircraft.asked.get(key) ?? 0;
          if (cmd === "aircraft_read_stop") {
            // Stopped before any was read (−1): none is counted as read.
            st.aircraft.asked.set(key, left > 0 ? -1 : 0);
            for (const id of ids) {
              const r = st.aircraftReads.get(Number(id.slice(1)));
              if (r && r.left > 0) r.left = -1;
            }
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
        case "aircraft_target_ids": {
          const pick = all.filter((it) => {
            if (!isAircraft(it)) return false;
            if (a.volume)
              return matches({
                volume: a.volume,
                folder: a.folder || undefined,
              } as unknown as Query)(it);
            const d = day(time(it));
            return d !== null && d >= String(a.from) && d <= String(a.to);
          });
          for (const it of pick)
            if (!st.aircraftReads.has(it.i))
              st.aircraftReads.set(it.i, { left: 1, review: "waiting" });
          return pick.map((it) => `a${it.i}`);
        }
        case "aircraft_review": {
          const conf =
            (a.confidence as typeof st.aircraftConf | null | undefined) ?? st.aircraftConf;
          const ids = (a.ids ?? []) as string[];
          let pending = 0;
          const items = [];
          for (const id of ids) {
            const i = Number(id.slice(1));
            const r = st.aircraftReads.get(i);
            if (!r || r.left < 0) continue;
            if (r.left > 0) {
              // A read takes a moment (about a second from when the window
              // first asks), not a number of requests: the window asks
              // again at once when its sliders and photos on screen settle.
              const now = Date.now();
              r.next ??= now + READ_MS;
              if (now >= r.next) {
                r.left -= 1;
                r.next = now + READ_MS;
              }
              if (r.left > 0) {
                pending += 1;
                continue;
              }
            }
            const registrations = [{ name: "C-FJZS", pct: 96 }];
            const airlines = [{ name: "Air Canada", pct: 82 + (i % 3) * 5 }];
            const types = [{ name: "Boeing 777-300ER", pct: 66 }];
            const offered = [
              ...registrations
                .filter((m) => m.pct >= conf.registrationPct)
                .map((m) => `Aircraft|Registration|${m.name}`),
              ...airlines
                .filter((m) => m.pct >= conf.airlinePct)
                .map((m) => `Aircraft|Airline|${m.name}`),
            ];
            const painted = types.filter((m) => m.pct >= conf.typePct);
            // With the lookup on, the registry says a 777-200LR of Air Canada.
            const looked = st.aircraftLookup && conf.registrationPct <= 96;
            const fromLookup = looked
              ? [
                  "Aircraft|Airline|Air Canada",
                  "Aircraft|Type|Boeing 777-200LR",
                  // The build year, from OpenSky's database (ADR-044).
                  ...(st.aircraftBuiltDb ? ["Aircraft|Built|2008"] : []),
                ]
              : [];
            offered.push(
              ...(painted.length
                ? painted.map((m) => `Aircraft|Type|${m.name}`)
                : looked
                  ? []
                  : ["Aircraft|Type|Boeing 777"]),
            );
            for (const k of fromLookup) if (!offered.includes(k)) offered.push(k);
            const unticked = looked
              ? painted.map((m) => `Aircraft|Type|${m.name}`).filter((k) => !fromLookup.includes(k))
              : [];
            // The next photo was taken 2 s later and is alike; the one
            // before, 5 s earlier, is not.
            const neighbours = [i + 1, i - 1]
              .filter((j) => all[j] && all[j].kind !== "video")
              .map((j) => ({
                id: `a${j}`,
                name: all[j].name,
                thumb: null,
                seconds: j > i ? 2 : -5,
                similarityPct: j > i ? 96 : 61,
                alike: j > i,
              }));
            items.push({
              id,
              name: all[i]?.name ?? null,
              thumb: null,
              review: r.review,
              fullSize: true,
              registrations,
              airlines,
              types,
              looksLike: "Boeing 777",
              offered,
              unticked,
              fromLookup,
              lookups: looked
                ? [
                    {
                      registration: "C-FJZS",
                      known: true,
                      aircraftType: "Boeing 777-200LR",
                      airline: "Air Canada",
                      owner: "Air Canada",
                      country: "Canada",
                      built: st.aircraftBuiltDb ? "2008" : null,
                    },
                  ]
                : [],
              neighbours: r.review === "waiting" ? neighbours : [],
            });
          }
          return {
            items,
            confidence: conf,
            total: ids.length,
            read: items.length,
            pending,
            failed: 0,
            problem: null,
          };
        }
        case "aircraft_set_lookup":
          st.aircraftLookup = Boolean(a.enabled);
          return null;
        case "aircraft_same_plane": {
          // Two other photos of C-FJZS lack the type.
          const keep = (a.keywords ?? []) as string[];
          const type = keep.find((k) => k.startsWith("Aircraft|Type|"));
          if (!keep.includes("Aircraft|Registration|C-FJZS") || !type) return [];
          return [900, 901]
            .filter((j) => !(st.keywords.get(j) ?? []).includes(type))
            .map((j) => ({
              id: `a${j}`,
              name: all[j]?.name ?? null,
              thumb: null,
              missing: [type],
            }));
        }
        case "aircraft_add_keywords": {
          const keep = (a.keywords ?? []) as string[];
          for (const id of (a.ids ?? []) as string[]) {
            const j = Number(id.slice(1));
            st.keywords.set(j, [...new Set([...(st.keywords.get(j) ?? []), ...keep])]);
          }
          return ((a.ids ?? []) as string[]).length;
        }
        case "aircraft_set_confidence":
          st.aircraftConf = a.confidence as typeof st.aircraftConf;
          return st.aircraftConf;
        case "aircraft_accept":
        case "aircraft_dismiss": {
          const i = Number(String(a.id).slice(1));
          const r = st.aircraftReads.get(i);
          if (!r) throw `${String(a.id)} has not been read`;
          const keep = cmd === "aircraft_accept" ? ((a.keywords ?? []) as string[]) : [];
          if (keep.some((k) => !k.startsWith("Aircraft|")))
            throw "only aircraft keywords can be accepted";
          r.review = keep.length ? "accepted" : "dismissed";
          const near = keep.length ? ((a.neighbours ?? []) as string[]) : [];
          const accepted: string[] = [];
          for (const nid of near) {
            const j = Number(nid.slice(1));
            st.keywords.set(j, [...new Set([...(st.keywords.get(j) ?? []), ...keep])]);
            // Accepted with it: not asked about again, also when read later.
            // (One already accepted or dismissed keeps its review.)
            const nr = st.aircraftReads.get(j);
            if (!nr) st.aircraftReads.set(j, { left: 0, review: "accepted" });
            else if (nr.review === "waiting") nr.review = "accepted";
            else continue;
            accepted.push(nid);
          }
          if (keep.length) {
            const cur = st.keywords.get(i) ?? [];
            st.keywords.set(i, [...new Set([...cur, ...keep])]);
            const by = st.aiSources.get(i) ?? new Map<string, string[]>();
            by.set(AIRCRAFT.id, [...new Set([...(by.get(AIRCRAFT.id) ?? []), ...keep])].sort());
            st.aiSources.set(i, by);
          }
          return cmd === "aircraft_accept" ? accepted : null;
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
          if (a.what === "aircraft")
            for (const r of st.aircraftReads.values()) if (r.left > 0) r.left = -1;
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
            if (a.what === "aircraft")
              for (const r of st.aircraftReads.values()) if (r.left > 0) r.left = -1;
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
