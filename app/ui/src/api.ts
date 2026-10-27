// The only way the UI talks to Keepsake: typed Tauri commands and events.
// Types come from src/bindings (generated from Rust by ts-rs).

import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { AppInfo } from "./bindings/AppInfo";
import type { AssetMembers } from "./bindings/AssetMembers";
import type { BackupOverview } from "./bindings/BackupOverview";
import type { Bucket } from "./bindings/Bucket";
import type { CalendarLevelDto } from "./bindings/CalendarLevelDto";
import type { CardNotice } from "./bindings/CardNotice";
import type { CardScan } from "./bindings/CardScan";
import type { CrashNotice } from "./bindings/CrashNotice";
import type { Details } from "./bindings/Details";
import type { EditProgress } from "./bindings/EditProgress";
import type { EditResult } from "./bindings/EditResult";
import type { Facets } from "./bindings/Facets";
import type { ImportCheck } from "./bindings/ImportCheck";
import type { ImportProgress } from "./bindings/ImportProgress";
import type { ImportRequest } from "./bindings/ImportRequest";
import type { ImportResult } from "./bindings/ImportResult";
import type { MaintDone } from "./bindings/MaintDone";
import type { MaintProgress } from "./bindings/MaintProgress";
import type { MemberDto } from "./bindings/MemberDto";
import type { OpenApp } from "./bindings/OpenApp";
import type { Preset } from "./bindings/Preset";
import type { Query } from "./bindings/Query";
import type { RenderSummary } from "./bindings/RenderSummary";
import type { Row } from "./bindings/Row";
import type { Selection } from "./bindings/Selection";
import type { SettingsInfo } from "./bindings/SettingsInfo";
import type { SettingsSaved } from "./bindings/SettingsSaved";
import type { SettingsUpdate } from "./bindings/SettingsUpdate";
import type { Sidebar } from "./bindings/Sidebar";
import type { SortSpec } from "./bindings/SortSpec";
import type { ThumbsReady } from "./bindings/ThumbsReady";
import type { TimeShiftPreview } from "./bindings/TimeShiftPreview";
import type { VideoInfo } from "./bindings/VideoInfo";
import type { ViewInfo } from "./bindings/ViewInfo";

export type {
  AppInfo,
  AssetMembers,
  BackupOverview,
  Bucket,
  CalendarLevelDto,
  CardNotice,
  CardScan,
  CrashNotice,
  Details,
  EditProgress,
  Facets,
  ImportCheck,
  ImportProgress,
  ImportRequest,
  ImportResult,
  MaintDone,
  MaintProgress,
  MemberDto,
  OpenApp,
  Preset,
  Query,
  RenderSummary,
  Row,
  Selection,
  SettingsInfo,
  SettingsSaved,
  SettingsUpdate,
  Sidebar,
  SortSpec,
  TimeShiftPreview,
  VideoInfo,
  ViewInfo,
};

export const emptyQuery: Query = {
  text: null,
  from: null,
  to: null,
  camera: null,
  lens: null,
  kind: null,
  minRating: null,
  label: null,
  event: null,
  volume: null,
  session: null,
  undated: false,
};

/** A file path from the app as an URL the webview can load. */
export function fileUrl(path: string | null | undefined): string | null {
  if (!path) return null;
  // The mock backend (browser tests) hands out data URLs.
  if (path.startsWith("data:") || path.startsWith("http")) return path;
  return convertFileSrc(path);
}

export const api = {
  appInfo: () => invoke<AppInfo>("app_info"),
  libraryCreate: (path: string) => invoke<AppInfo>("library_create", { path }),
  libraryOpen: (path: string) => invoke<AppInfo>("library_open", { path }),

  viewOpen: (query: Query, sort: SortSpec) => invoke<ViewInfo>("view_open", { query, sort }),
  viewRows: (view: number, offset: number, limit: number) =>
    invoke<Row[]>("view_rows", { view, offset, limit }),
  viewIndexOf: (view: number, id: string) => invoke<number | null>("view_index_of", { view, id }),
  calendar: (query: Query, level: CalendarLevelDto) =>
    invoke<Bucket[]>("calendar", { query, level }),
  facets: () => invoke<Facets>("facets"),
  sidebar: () => invoke<Sidebar>("sidebar"),
  details: (id: string) => invoke<Details>("details", { id }),

  editRating: (selection: Selection, rating: number) =>
    invoke<EditResult>("edit_rating", { selection, rating }),
  editLabel: (selection: Selection, label: string | null) =>
    invoke<EditResult>("edit_label", { selection, label }),
  editComment: (id: string, comment: string | null) =>
    invoke<EditResult>("edit_comment", { id, comment }),
  editKeywords: (selection: Selection, add: string[], remove: string[]) =>
    invoke<EditResult>("edit_keywords", { selection, add, remove }),
  keywords: () => invoke<string[]>("keywords"),

  renderStatus: () => invoke<RenderSummary>("render_status"),
  renderSetVisible: (ids: string[]) => invoke<number>("render_set_visible", { ids }),

  importScan: (mount: string) => invoke<CardScan>("import_scan", { mount }),
  cardThumb: (mount: string, files: string[]) =>
    invoke<string | null>("card_thumb", { mount, files }),
  presetsList: () => invoke<Preset[]>("presets_list"),
  presetSave: (preset: Preset) => invoke<void>("preset_save", { preset }),
  importCheck: (
    request: ImportRequest,
    sizes: number[],
    first: string | null,
    firstMs: number | null,
  ) => invoke<ImportCheck>("import_check", { request, sizes, first, firstMs }),
  importStart: (request: ImportRequest) => invoke<void>("import_start", { request }),
  importCancel: () => invoke<void>("import_cancel"),
  presetDelete: (name: string) => invoke<boolean>("preset_delete", { name }),

  fullImage: (id: string, member: MemberDto) => invoke<string>("full_image", { id, member }),
  assetMembers: (id: string) => invoke<AssetMembers>("asset_members", { id }),
  videoInfo: (id: string) => invoke<VideoInfo>("video_info", { id }),
  openExternal: (id: string, member: MemberDto) => invoke<string>("open_external", { id, member }),

  timeShiftPreview: (selection: Selection, reference: string, correct: string) =>
    invoke<TimeShiftPreview>("time_shift_preview", { selection, reference, correct }),
  timeShiftApply: (selection: Selection, offsetS: number) =>
    invoke<EditResult>("time_shift_apply", { selection, offsetS }),

  backupOverview: () => invoke<BackupOverview>("backup_overview"),
  backupRun: (dest: string, label: string | null) => invoke<void>("backup_run", { dest, label }),
  verifyRun: (volume: string) => invoke<void>("verify_run", { volume }),
  maintCancel: () => invoke<void>("maint_cancel"),

  settingsGet: () => invoke<SettingsInfo>("settings_get"),
  settingsSet: (update: SettingsUpdate) => invoke<SettingsSaved>("settings_set", { update }),
  cacheRemoveOld: (path: string) => invoke<number>("cache_remove_old", { path }),

  diagnosticReport: () => invoke<string>("diagnostic_report"),
  openLogFolder: () => invoke<string>("open_log_folder"),
  logUiError: (message: string, detail: string | null) =>
    invoke<void>("log_ui_error", { message, detail }),
  crashNotice: () => invoke<CrashNotice | null>("crash_notice"),
  crashDismiss: () => invoke<void>("crash_dismiss"),
  uiReady: (what: string) => invoke<number>("ui_ready", { what }),
};

export const events = {
  thumbs: (f: (e: ThumbsReady) => void): Promise<UnlistenFn> =>
    listen<ThumbsReady>("render://thumbs", (e) => f(e.payload)),
  card: (f: (e: CardNotice) => void): Promise<UnlistenFn> =>
    listen<CardNotice>("device://card", (e) => f(e.payload)),
  edit: (f: (e: EditProgress) => void): Promise<UnlistenFn> =>
    listen<EditProgress>("edit://progress", (e) => f(e.payload)),
  importProgress: (f: (e: ImportProgress) => void): Promise<UnlistenFn> =>
    listen<ImportProgress>("import://progress", (e) => f(e.payload)),
  importDone: (f: (e: ImportResult) => void): Promise<UnlistenFn> =>
    listen<ImportResult>("import://done", (e) => f(e.payload)),
  maintProgress: (f: (e: MaintProgress) => void): Promise<UnlistenFn> =>
    listen<MaintProgress>("maint://progress", (e) => f(e.payload)),
  maintDone: (f: (e: MaintDone) => void): Promise<UnlistenFn> =>
    listen<MaintDone>("maint://done", (e) => f(e.payload)),
};

/** Ask the native dialog for a file, e.g. an application (null if cancelled). */
export async function pickFile(title: string): Promise<string | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const r = await open({ directory: false, multiple: false, title });
  return typeof r === "string" ? r : null;
}

/** Ask the native dialog for a folder (null if cancelled). */
export async function pickFolder(title: string): Promise<string | null> {
  const { open } = await import("@tauri-apps/plugin-dialog");
  const r = await open({ directory: true, multiple: false, title });
  return typeof r === "string" ? r : null;
}
