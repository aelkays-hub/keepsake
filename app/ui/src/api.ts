// The only way the UI talks to Keepsake: typed Tauri commands and events.
// Types come from src/bindings (generated from Rust by ts-rs).

import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { AiInstallProgress } from "./bindings/AiInstallProgress";
import type { AircraftStatus } from "./bindings/AircraftStatus";
import type { AiFolderStatus } from "./bindings/AiFolderStatus";
import type { AiRangeStatus } from "./bindings/AiRangeStatus";
import type { AiStatus } from "./bindings/AiStatus";
import type { AiTagDto } from "./bindings/AiTagDto";
import type { AiTagsDto } from "./bindings/AiTagsDto";
import type { FaceDto } from "./bindings/FaceDto";
import type { FacesStatus } from "./bindings/FacesStatus";
import type { PersonDto } from "./bindings/PersonDto";
import type { PhotoFacesDto } from "./bindings/PhotoFacesDto";
import type { SuggestDto } from "./bindings/SuggestDto";
import type { AiAdviceDto } from "./bindings/AiAdviceDto";
import type { AircraftConfidenceDto } from "./bindings/AircraftConfidenceDto";
import type { AircraftLookupDto } from "./bindings/AircraftLookupDto";
import type { AircraftNeighbourDto } from "./bindings/AircraftNeighbourDto";
import type { SamePlaneDto } from "./bindings/SamePlaneDto";
import type { AircraftReviewDto } from "./bindings/AircraftReviewDto";
import type { AircraftReviewItemDto } from "./bindings/AircraftReviewItemDto";
import type { ScoredMarkDto } from "./bindings/ScoredMarkDto";
import type { AskingDto } from "./bindings/AskingDto";
import type { EditAdviceDto } from "./bindings/EditAdviceDto";
import type { EditSuggestionDto } from "./bindings/EditSuggestionDto";
import type { MeasuredDto } from "./bindings/MeasuredDto";
import type { AppInfo } from "./bindings/AppInfo";
import type { AssetMembers } from "./bindings/AssetMembers";
import type { BackupOverview } from "./bindings/BackupOverview";
import type { Bucket } from "./bindings/Bucket";
import type { CalendarLevelDto } from "./bindings/CalendarLevelDto";
import type { CardNotice } from "./bindings/CardNotice";
import type { CardScan } from "./bindings/CardScan";
import type { CloudInfo } from "./bindings/CloudInfo";
import type { CloudModelDto } from "./bindings/CloudModelDto";
import type { CloudProviderDto } from "./bindings/CloudProviderDto";
import type { CloudStarted } from "./bindings/CloudStarted";
import type { CloudStatusDto } from "./bindings/CloudStatusDto";
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
import type { OriginalPlace } from "./bindings/OriginalPlace";
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
  AiFolderStatus,
  AiRangeStatus,
  AiStatus,
  AiTagDto,
  AiTagsDto,
  AppInfo,
  AircraftStatus,
  FaceDto,
  FacesStatus,
  PersonDto,
  PhotoFacesDto,
  SuggestDto,
  AiAdviceDto,
  AircraftConfidenceDto,
  AircraftLookupDto,
  AircraftNeighbourDto,
  SamePlaneDto,
  AircraftReviewDto,
  AircraftReviewItemDto,
  ScoredMarkDto,
  AskingDto,
  EditAdviceDto,
  EditSuggestionDto,
  MeasuredDto,
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
  OriginalPlace,
  Preset,
  Query,
  RenderSummary,
  ReportDrive,
  ReportGroup,
  Row,
  Selection,
  SelectionSummary,
  CloudInfo,
  CloudModelDto,
  CloudProviderDto,
  CloudStarted,
  CloudStatusDto,
  SettingsInfo,
  SettingsSaved,
  SettingsUpdate,
  Sidebar,
  SortSpec,
  TimeShiftPreview,
  VideoInfo,
  ViewInfo,
};

/** The two separate AI tasks for a date range. */
/** `aircraft`: Read aircraft (ADR-041) for the photos tagged Airplane,
 * Helicopter or Airport in a folder or on dates. */
export type AiTask = "duplicates" | "tags" | "aircraft";

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
  /** Open in another application: `app` from `openChoices` ("" = system default), or the default.
   * Returns the line to show ("Opened with Photoshop", or what to do when
   * the program was already open and may not show the photo). */
  openExternal: (id: string, member: MemberDto, app: string | null = null) =>
    invoke<string>("open_external", { id, member, app }),
  /** Every metadata tag of an item's file (ExifTool; read only). */
  metadataAll: (id: string, member: MemberDto) =>
    invoke<FileMetadata>("metadata_all", { id, member }),
  openChoices: (id: string, member: MemberDto) =>
    invoke<OpenChoice[]>("open_choices", { id, member }),
  /** Where an item's original is, for "Show in Explorer / Finder". */
  originalPlace: (id: string) => invoke<OriginalPlace>("original_place", { id }),
  /** Open the folder holding the original, with the file selected. */
  showInFolder: (id: string) => invoke<null>("show_in_folder", { id }),

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
  /** Read aircraft (ADR-041): on downloads the text models first. */
  aircraftStatus: () => invoke<AircraftStatus>("aircraft_status"),
  aircraftEnable: (enabled: boolean) => invoke<void>("aircraft_enable", { enabled }),
  aircraftInstallCancel: () => invoke<void>("aircraft_install_cancel"),
  aircraftRemoveModels: () => invoke<boolean>("aircraft_remove_models"),
  /** Read the aircraft in the picked photos, on this computer; then how
   * far it has got, and stopping it. */
  aircraftRead: (selection: Selection) => invoke<CloudStarted>("aircraft_read", { selection }),
  aircraftReadStatus: (ids: string[]) => invoke<CloudStatusDto>("aircraft_read_status", { ids }),
  aircraftReadStop: (ids: string[]) => invoke<void>("aircraft_read_stop", { ids }),
  /** The review window: the photos a folder (`volume`, `folder`) or dates
   * (`from`, `to`) are read for. */
  aircraftTargetIds: (t: { volume?: string; folder?: string; from?: string; to?: string }) =>
    invoke<string[]>("aircraft_target_ids", {
      volume: t.volume ?? null,
      folder: t.folder ?? null,
      from: t.from ?? null,
      to: t.to ?? null,
    }),
  /** What was read in `ids` so far, offered at `confidence` (the saved one
   * when `null`). Thumbnails and neighbours only for `shown` (all when
   * `null`). */
  aircraftReview: (
    ids: string[],
    confidence: AircraftConfidenceDto | null = null,
    shown: string[] | null = null,
  ) => invoke<AircraftReviewDto>("aircraft_review", { ids, confidence, shown }),
  aircraftSetConfidence: (confidence: AircraftConfidenceDto) =>
    invoke<AircraftConfidenceDto>("aircraft_set_confidence", { confidence }),
  /** Accept a photo's read: these keywords are added (none: dismissed),
   * to the photo and to the `neighbours` ticked (`null`: not shown, so the
   * alike ones). */
  aircraftAccept: (id: string, keywords: string[], neighbours: string[] | null = []) =>
    invoke<string[]>("aircraft_accept", { id, keywords, neighbours }),
  /** The other photos of an accepted registration that lack its airline
   * or type. */
  aircraftSamePlane: (id: string, keywords: string[]) =>
    invoke<SamePlaneDto[]>("aircraft_same_plane", { id, keywords }),
  aircraftAddKeywords: (ids: string[], keywords: string[]) =>
    invoke<number>("aircraft_add_keywords", { ids, keywords }),
  /** Look registrations up online (adsbdb.com). */
  aircraftSetLookup: (enabled: boolean) => invoke<void>("aircraft_set_lookup", { enabled }),
  aircraftDismiss: (id: string) => invoke<void>("aircraft_dismiss", { id }),
  /** Recognise faces (ADR-040): on downloads the face models first. */
  facesStatus: () => invoke<FacesStatus>("faces_status"),
  facesEnable: (enabled: boolean) => invoke<void>("faces_enable", { enabled }),
  facesInstallCancel: () => invoke<void>("faces_install_cancel"),
  /** Search every photo not searched yet; returns how many were queued. */
  facesScanAll: () => invoke<number>("faces_scan_all"),
  facesStop: () => invoke<number>("faces_stop"),
  /** Forget every face, person and decision; returns how many faces. */
  facesDeleteAll: () => invoke<number>("faces_delete_all"),
  facesRemoveModels: () => invoke<boolean>("faces_remove_models"),
  /** Edit suggestions (ADR-042): kept, or measured now (`again`: measured
   * again). `waiting`: the preview is being made first (ask again). */
  editSuggestions: (id: string, again = false) =>
    invoke<EditAdviceDto>("edit_suggestions", { id, again }),
  /** Ask an AI for this photo's edit suggestions (in the background). */
  editSuggestionsAsk: (id: string, providerId: string) =>
    invoke<EditAdviceDto>("edit_suggestions_ask", { id, providerId }),
  editSuggestionsStop: (id: string) => invoke<void>("edit_suggestions_stop", { id }),
  /** Name and find (ADR-040). A photo not searched yet is searched now:
   * `searched` is false until it has been (ask again). */
  facesOfPhoto: (id: string, search = true) =>
    invoke<PhotoFacesDto>("faces_of_photo", { id, search }),
  facesPeople: () => invoke<PersonDto[]>("faces_people"),
  facesName: (face: number, name: string) => invoke<PersonDto>("faces_name", { face, name }),
  /** Search every photo not searched yet; returns how many were queued. */
  facesFind: () => invoke<number>("faces_find"),
  /** `matchPct`: a new cut-off (stricter or looser), kept for next time. */
  facesSuggest: (person: number, matchPct: number | null = null) =>
    invoke<SuggestDto>("faces_suggest", { person, matchPct }),
  /** A person's confirmed faces, least like the others first. */
  facesCheck: (person: number) => invoke<FaceDto[]>("faces_check", { person }),
  facesReview: (person: number) => invoke<FaceDto[]>("faces_review", { person }),
  /** Returns [confirmed, rejected]. */
  facesDecide: (person: number, confirm: number[], reject: number[]) =>
    invoke<[number, number]>("faces_decide", { person, confirm, reject }),
  /** The viewer's face menu: not this person, or not a face (hidden). */
  facesNotPerson: (face: number) => invoke<boolean>("faces_not_person", { face }),
  facesNotFace: (face: number) => invoke<void>("faces_not_face", { face }),
  /** Days with duplicates, from `from` to `to` (`YYYY-MM-DD`; null: no bound). */
  aiDuplicates: (from: string | null = null, to: string | null = null) =>
    invoke<DuplicateSummaryDto>("ai_duplicates", { from, to }),
  /** Find duplicates (the RAW photos) or tag the photos of these dates:
   * queued ahead of other work. */
  aiAnalyzeRange: (from: string, to: string, what: AiTask) =>
    invoke<AiRangeStatus>("ai_analyze_range", { from, to, what }),
  aiRangeStatus: (from: string, to: string, what: AiTask) =>
    invoke<AiRangeStatus>("ai_range_status", { from, to, what }),
  /** Find duplicates or tag the photos of a folder and the folders in it
   * (`""`: the whole drive). */
  aiAnalyzeFolder: (volume: string, folder: string, what: AiTask) =>
    invoke<AiFolderStatus>("ai_analyze_folder", { volume, folder, what }),
  aiFolderStatus: (volume: string, folder: string, what: AiTask) =>
    invoke<AiFolderStatus>("ai_folder_status", { volume, folder, what }),
  /** The same for the photos with no date (the calendar's "No date"). */
  aiAnalyzeUndated: (what: AiTask) => invoke<AiFolderStatus>("ai_analyze_undated", { what }),
  aiUndatedStatus: (what: AiTask) => invoke<AiFolderStatus>("ai_undated_status", { what }),
  aiUndatedStop: (what: AiTask) => invoke<void>("ai_undated_stop", { what }),
  /** Tag with AI (ADR-036, ADR-038): each provider's key and model, the
   * provider offered first, and the photos sent to one. */
  aiCloudInfo: () => invoke<CloudInfo>("ai_cloud_info"),
  /** `null`: remove the key. */
  aiCloudSetKey: (providerId: string, key: string | null) =>
    invoke<CloudInfo>("ai_cloud_set_key", { providerId, key }),
  aiCloudSetModel: (model: string) => invoke<CloudInfo>("ai_cloud_set_model", { model }),
  aiCloudSetDefault: (providerId: string) =>
    invoke<CloudInfo>("ai_cloud_set_default", { providerId }),
  aiCloudTag: (selection: Selection, providerId: string) =>
    invoke<CloudStarted>("ai_cloud_tag", { selection, providerId }),
  aiCloudStatus: (ids: string[], providerId: string) =>
    invoke<CloudStatusDto>("ai_cloud_status", { ids, providerId }),
  aiCloudStop: (ids: string[], providerId: string) =>
    invoke<void>("ai_cloud_stop", { ids, providerId }),
  /** Stop a date range's work: queued jobs are dropped; what was done stays. */
  aiRangeStop: (from: string, to: string, what: AiTask) =>
    invoke<void>("ai_range_stop", { from, to, what }),
  /** Stop: its queued work is dropped; what was done stays. */
  aiFolderStop: (volume: string, folder: string, what: AiTask) =>
    invoke<void>("ai_folder_stop", { volume, folder, what }),
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
  facesInstall: (f: (e: AiInstallProgress) => void): Promise<UnlistenFn> =>
    listen<AiInstallProgress>("faces://install", (e) => f(e.payload)),
  facesStatus: (f: () => void): Promise<UnlistenFn> => listen("faces://status", () => f()),
  aircraftInstall: (f: (e: AiInstallProgress) => void): Promise<UnlistenFn> =>
    listen<AiInstallProgress>("aircraft://install", (e) => f(e.payload)),
  aircraftStatus: (f: () => void): Promise<UnlistenFn> => listen("aircraft://status", () => f()),
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
