// The only way the UI talks to Keepsake: typed Tauri commands and events.
// Types come from src/bindings (generated from Rust by ts-rs).

import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { AiInstallProgress } from "./bindings/AiInstallProgress";
import type { AiRangeStatus } from "./bindings/AiRangeStatus";
import type { AiStatus } from "./bindings/AiStatus";
import type { AiTagDto } from "./bindings/AiTagDto";
import type { AiTagsDto } from "./bindings/AiTagsDto";
import type { AppInfo } from "./bindings/AppInfo";
import type { AssetMembers } from "./bindings/AssetMembers";
import type { BackupOverview } from "./bindings/BackupOverview";
import type { Bucket } from "./bindings/Bucket";
import type { CalendarLevelDto } from "./bindings/CalendarLevelDto";
import type { CardNotice } from "./bindings/CardNotice";
import type { CardScan } from "./bindings/CardScan";
import type { CrashNotice } from "./bindings/CrashNotice";
import type { Details } from "./bindings/Details";
import type { DuplicateGroupDto } from "./bindings/DuplicateGroupDto";
import type { DuplicateMemberDto } from "./bindings/DuplicateMemberDto";
import type { DuplicateSummaryDto } from "./bindings/DuplicateSummaryDto";
import type { EditProgress } from "./bindings/EditProgress";
import type { EditResult } from "./bindings/EditResult";
import type { Facets } from "./bindings/Facets";
import type { FileMetadata } from "./bindings/FileMetadata";
import type { FolderDone } from "./bindings/FolderDone";
import type { FolderSaved } from "./bindings/FolderSaved";
import type { FolderNode } from "./bindings/FolderNode";
import type { ImportCheck } from "./bindings/ImportCheck";
import type { ImportPrefs } from "./bindings/ImportPrefs";
import type { ImportProgress } from "./bindings/ImportProgress";
import type { ImportRequest } from "./bindings/ImportRequest";
import type { ImportResult } from "./bindings/ImportResult";
import type { LibraryReport } from "./bindings/LibraryReport";
import type { MaintDone } from "./bindings/MaintDone";
import type { MaintProgress } from "./bindings/MaintProgress";
import type { MemberDto } from "./bindings/MemberDto";
import type { OpenApp } from "./bindings/OpenApp";
import type { OpenChoice } from "./bindings/OpenChoice";
import type { Preset } from "./bindings/Preset";
import type { Query } from "./bindings/Query";
import type { RenderSummary } from "./bindings/RenderSummary";
import type { ReportDrive } from "./bindings/ReportDrive";
import type { ReportGroup } from "./bindings/ReportGroup";
import type { Row } from "./bindings/Row";
import type { Selection } from "./bindings/Selection";
import type { SelectionSummary } from "./bindings/SelectionSummary";
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
  AiInstallProgress,
  AiRangeStatus,
  AiStatus,
  AiTagDto,
  AiTagsDto,
  AppInfo,
  AssetMembers,
  BackupOverview,
  Bucket,
  CalendarLevelDto,
  CardNotice,
  CardScan,
  CrashNotice,
  Details,
  DuplicateGroupDto,
  DuplicateMemberDto,
  DuplicateSummaryDto,
  EditProgress,
  Facets,
  FileMetadata,
  FolderDone,
  FolderSaved,
  FolderNode,
  ImportCheck,
  ImportPrefs,
  ImportProgress,
  ImportRequest,
  ImportResult,
  LibraryReport,
  MaintDone,
  MaintProgress,
  MemberDto,
  OpenApp,
  OpenChoice,
  Preset,
  Query,
  RenderSummary,
  ReportDrive,
  ReportGroup,
  Row,
  Selection,
  SelectionSummary,
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
  folderTree: (volume: string, parent: string | null, query: Query) =>
    invoke<FolderNode[]>("folder_tree", { volume, parent, query }),
  facets: () => invoke<Facets>("facets"),
  sidebar: () => invoke<Sidebar>("sidebar"),
  details: (id: string) => invoke<Details>("details", { id }),

  editRating: (selection: Selection, rating: number) =>
    invoke<EditResult>("edit_rating", { selection, rating }),
  editLabel: (selection: Selection, label: string | null) =>
    invoke<EditResult>("edit_label", { selection, label }),
  editComment: (selection: Selection, comment: string | null) =>
    invoke<EditResult>("edit_comment", { selection, comment }),
  selectionSummary: (selection: Selection) =>
    invoke<SelectionSummary>("selection_summary", { selection }),
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
  /** Open in another application: `app` from `openChoices` ("" = system default), or the default. */
  openExternal: (id: string, member: MemberDto, app: string | null = null) =>
    invoke<string>("open_external", { id, member, app }),
  /** Every metadata tag of an item's file (ExifTool; read only). */
  metadataAll: (id: string, member: MemberDto) =>
    invoke<FileMetadata>("metadata_all", { id, member }),
  openChoices: (id: string, member: MemberDto) =>
    invoke<OpenChoice[]>("open_choices", { id, member }),

  timeShiftPreview: (selection: Selection, reference: string, correct: string) =>
    invoke<TimeShiftPreview>("time_shift_preview", { selection, reference, correct }),
  timeShiftApply: (selection: Selection, offsetS: number) =>
    invoke<EditResult>("time_shift_apply", { selection, offsetS }),

  backupOverview: () => invoke<BackupOverview>("backup_overview"),
  libraryReport: () => invoke<LibraryReport>("library_report"),
  libraryReportCsv: (path: string) => invoke<string>("library_report_csv", { path }),
  backupRun: (dest: string, label: string | null) => invoke<void>("backup_run", { dest, label }),
  verifyRun: (volume: string) => invoke<void>("verify_run", { volume }),
  maintCancel: () => invoke<void>("maint_cancel"),
  folderAdd: (path: string) => invoke<void>("folder_add", { path }),
  /** Scan again one folder of a volume (`path` on it; `null` the whole volume). */
  folderRescan: (volume: string, path: string | null) =>
    invoke<void>("folder_rescan", { volume, path }),

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

  aiStatus: () => invoke<AiStatus>("ai_status"),
  /** Turn AI on (downloads the model first if needed) or off. */
  aiEnable: (enabled: boolean, gpu: boolean | null = null) =>
    invoke<void>("ai_enable", { enabled, gpu }),
  aiInstallCancel: () => invoke<void>("ai_install_cancel"),
  aiRemoveModel: () => invoke<boolean>("ai_remove_model"),
  /** Days with duplicates, from `from` to `to` (`YYYY-MM-DD`; null: no bound). */
  aiDuplicates: (from: string | null = null, to: string | null = null) =>
    invoke<DuplicateSummaryDto>("ai_duplicates", { from, to }),
  /** Find duplicates and tags for these dates: queued ahead of other work. */
  aiAnalyzeRange: (from: string, to: string) =>
    invoke<AiRangeStatus>("ai_analyze_range", { from, to }),
  aiRangeStatus: (from: string, to: string) =>
    invoke<AiRangeStatus>("ai_range_status", { from, to }),
  aiDuplicateDay: (date: string) => invoke<DuplicateGroupDto[]>("ai_duplicate_day", { date }),
  /** Keep `keeper`; the rest of `group` become "not kept" (nothing is deleted). */
  aiPickKeeper: (keeper: string, group: string[]) =>
    invoke<void>("ai_pick_keeper", { keeper, group }),
  /** `keeper`, `not_kept`, `not_duplicate`, or null to clear. */
  aiSetChoice: (ids: string[], choice: string | null) =>
    invoke<void>("ai_set_choice", { ids, choice }),
  aiTags: (id: string) => invoke<AiTagsDto>("ai_tags", { id }),
  /** Remove an AI tag for good, or (`keep`) make it your own keyword. */
  aiTagDecide: (id: string, tag: string, keep: boolean) =>
    invoke<void>("ai_tag_decide", { id, tag, keep }),
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
  folderDone: (f: (e: FolderDone) => void): Promise<UnlistenFn> =>
    listen<FolderDone>("folder://done", (e) => f(e.payload)),
  folderSaved: (f: (e: FolderSaved) => void): Promise<UnlistenFn> =>
    listen<FolderSaved>("folder://saved", (e) => f(e.payload)),
  aiInstall: (f: (e: AiInstallProgress) => void): Promise<UnlistenFn> =>
    listen<AiInstallProgress>("ai://install", (e) => f(e.payload)),
  aiStatus: (f: () => void): Promise<UnlistenFn> => listen("ai://status", () => f()),
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

/** Ask the native dialog where to save a CSV file (null if cancelled). */
export async function pickSaveCsv(title: string, defaultPath: string): Promise<string | null> {
  const { save } = await import("@tauri-apps/plugin-dialog");
  const r = await save({ title, defaultPath, filters: [{ name: "CSV", extensions: ["csv"] }] });
  return typeof r === "string" ? r : null;
}
