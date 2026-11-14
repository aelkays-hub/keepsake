//! The app ↔ UI contract: every type a command takes or returns, or an event
//! carries. `ts-rs` generates `app/ui/src/bindings/*.ts` from these (run
//! `cargo test -p keepsake-app`); the UI imports only those types, and CI
//! fails if they are out of date.
//!
//! Ids travel as strings (UUIDs), sizes and times as numbers (safe up to
//! 2^53, plenty for bytes and milliseconds).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Where generated TypeScript goes, relative to this crate.
macro_rules! dto {
    ($($item:item)*) => {
        $(
            #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
            #[serde(rename_all = "camelCase")]
            #[ts(export, export_to = "../../src/bindings/")]
            $item
        )*
    };
}

dto! {
    /// Basic facts for the window title, first-run screen and tests.
    pub struct AppInfo {
        pub version: String,
        /// The open library's folder, if any.
        pub library: Option<String>,
        /// Running under the UI tests (`KEEPSAKE_UI_TEST=1`).
        pub ui_test: bool,
    }

    /// Filters for grid, details and calendar. Every field narrows.
    #[derive(Default)]
    pub struct Query {
        pub text: Option<String>,
        /// First local capture day, `YYYY-MM-DD`.
        pub from: Option<String>,
        pub to: Option<String>,
        pub camera: Option<String>,
        pub lens: Option<String>,
        pub kind: Option<KindFilter>,
        pub min_rating: Option<u8>,
        pub label: Option<String>,
        pub event: Option<String>,
        pub volume: Option<String>,
        /// With `volume`: this folder of it and below (`2025/May`).
        #[serde(default)]
        #[ts(optional)]
        pub folder: Option<String>,
        pub session: Option<String>,
        #[serde(default)]
        pub undated: bool,
        /// Camera model is exactly this; `""`: none (a report row).
        #[serde(default)]
        #[ts(optional)]
        pub camera_is: Option<String>,
        /// Lens model is exactly this; `""`: none.
        #[serde(default)]
        #[ts(optional)]
        pub lens_is: Option<String>,
        /// Has a file of this extension (`JPG`); `""`: one without.
        #[serde(default)]
        #[ts(optional)]
        pub file_type: Option<String>,
        /// Leave out items marked "not kept" among duplicates.
        #[serde(default)]
        #[ts(optional)]
        pub hide_not_kept: Option<bool>,
        /// Has this tag (keyword or AI tag) or one below it (`Places|Lake`).
        #[serde(default)]
        #[ts(optional)]
        pub tag: Option<String>,
        /// Has every one of these tags (the search bar's tag chips).
        #[serde(default)]
        #[ts(optional)]
        pub tags: Option<Vec<String>>,
    }

    /// On-device AI at a glance (Settings, Duplicates).
    pub struct AiStatus {
        /// Turned on in the settings.
        pub enabled: bool,
        pub gpu: bool,
        /// The model is on this computer.
        pub model_installed: bool,
        #[ts(type = "number")]
        pub model_bytes: u64,
        /// ONNX Runtime is present.
        pub runtime_found: bool,
        /// Where it runs (`graphics card (DirectML)`, …) once loaded.
        pub accel: Option<String>,
        /// Why it is not running, if it is not.
        pub problem: Option<String>,
        /// Downloading the model: bytes so far.
        #[ts(type = "number | null")]
        pub installing: Option<u64>,
        #[ts(type = "number")]
        pub analysed: u64,
        #[ts(type = "number")]
        pub assets: u64,
        #[ts(type = "number")]
        pub pending: u64,
        #[ts(type = "number")]
        pub failed: u64,
    }

    /// Read aircraft at a glance (Settings; ADR-041).
    pub struct AircraftStatus {
        /// Turned on in the settings.
        pub enabled: bool,
        /// Both text models are on this computer.
        pub models_installed: bool,
        #[ts(type = "number")]
        pub models_bytes: u64,
        /// ONNX Runtime is present.
        pub runtime_found: bool,
        /// On-device AI's model is installed, so the type can be guessed
        /// from the look when it is not painted on the aircraft.
        pub looks: bool,
        /// Where the text detector runs, once loaded.
        pub accel: Option<String>,
        /// Why the aircraft worker is not running, if it is not.
        pub problem: Option<String>,
        /// Downloading the models: bytes so far.
        #[ts(type = "number | null")]
        pub installing: Option<u64>,
        /// The review window's confidence, as last set.
        pub confidence: AircraftConfidenceDto,
        /// Registrations are looked up online (adsbdb.com).
        pub lookup: bool,
    }

    /// Recognise faces at a glance (Settings; ADR-040).
    pub struct FacesStatus {
        /// Turned on in the settings.
        pub enabled: bool,
        /// On-device AI is on (faces need it).
        pub ai_enabled: bool,
        /// Both face models are on this computer.
        pub models_installed: bool,
        #[ts(type = "number")]
        pub models_bytes: u64,
        /// ONNX Runtime is present.
        pub runtime_found: bool,
        /// Where the face models run, once loaded.
        pub accel: Option<String>,
        /// Why the face worker is not running, if it is not.
        pub problem: Option<String>,
        /// Downloading the models: bytes so far.
        #[ts(type = "number | null")]
        pub installing: Option<u64>,
        /// Photos in the library, searched, with faces; faces found; people
        /// named; searches still to do, and that failed.
        #[ts(type = "number")]
        pub photos: u64,
        #[ts(type = "number")]
        pub scanned: u64,
        #[ts(type = "number")]
        pub with_faces: u64,
        #[ts(type = "number")]
        pub faces: u64,
        #[ts(type = "number")]
        pub people: u64,
        #[ts(type = "number")]
        pub pending: u64,
        #[ts(type = "number")]
        pub failed: u64,
    }

    /// A face in a photo (Name and find; ADR-040).
    pub struct FaceDto {
        #[ts(type = "number")]
        pub id: i64,
        /// The photo's asset id.
        pub photo: String,
        /// The photo's thumbnail (the face is cut out of it), if made.
        pub thumb: Option<String>,
        /// The picture the face was found in (the thumbnail), in pixels:
        /// to cut the face out at the right shape.
        pub pw: u32,
        pub ph: u32,
        /// Its box, in 0–1 units of the picture.
        pub x: f32,
        pub y: f32,
        pub w: f32,
        pub h: f32,
        /// Whose face it is (confirmed or suggested), if anyone's.
        pub person: Option<String>,
        #[ts(type = "number | null")]
        pub person_id: Option<i64>,
        /// `unknown`, `suggested` or `confirmed`.
        pub state: String,
        /// A suggestion's likeness to the person, 0–100.
        pub likeness: Option<u8>,
        /// When the photo was taken, `YYYY-MM-DD` (UTC), if known.
        pub date: Option<String>,
    }

    /// The faces in one photo, and whether it has been searched yet.
    pub struct PhotoFacesDto {
        pub searched: bool,
        pub faces: Vec<FaceDto>,
    }

    /// A named person.
    pub struct PersonDto {
        #[ts(type = "number")]
        pub id: i64,
        pub name: String,
        /// Faces confirmed as theirs.
        #[ts(type = "number")]
        pub confirmed: u64,
    }

    /// What finding a person's faces did (Find, Find more).
    pub struct SuggestDto {
        /// Suggested this time.
        #[ts(type = "number")]
        pub added: u64,
        /// Waiting for an answer, in all.
        #[ts(type = "number")]
        pub waiting: u64,
        #[ts(type = "number")]
        pub confirmed: u64,
        /// The years looked at (none: every year).
        pub from_year: Option<i32>,
        pub to_year: Option<i32>,
        /// The cut-off used, in percent (the stricter/looser control).
        pub match_pct: u8,
    }

    /// How far "Find duplicates" or "Tag photos" has got for a date range.
    pub struct AiRangeStatus {
        /// `YYYY-MM-DD`, inclusive.
        pub from: String,
        pub to: String,
        #[ts(type = "number")]
        pub items: u64,
        #[ts(type = "number")]
        pub analysed: u64,
        #[ts(type = "number")]
        pub pending: u64,
    }

    /// Tag with AI (ADR-036, ADR-038): one model the user can choose.
    pub struct CloudModelDto {
        /// The API id, `gpt-6.1-sol`.
        pub id: String,
        /// "GPT-6.1 Sol".
        pub name: String,
        /// About what one photo costs, in US cents.
        pub cents_per_photo: f64,
    }

    /// One AI provider: whether a key is saved, and the model chosen.
    pub struct CloudProviderDto {
        /// `claude`, `openai` or `gemini`.
        pub id: String,
        /// "ChatGPT".
        pub name: String,
        /// Who runs it and charges for it: "OpenAI".
        pub company: String,
        pub has_key: bool,
        /// The chosen model's id.
        pub model: String,
        pub models: Vec<CloudModelDto>,
    }

    /// Tag with AI: every provider, and the one offered first.
    pub struct CloudInfo {
        /// The id of the provider offered first (the one used last), or
        /// `all`: every AI with a key.
        pub default_provider: String,
        pub providers: Vec<CloudProviderDto>,
    }

    /// Tag with AI started: the provider, the photos it covers (to follow
    /// and stop it) and how far it has got.
    pub struct CloudStarted {
        pub provider: String,
        pub ids: Vec<String>,
        pub status: CloudStatusDto,
    }

    /// How far tagging some photos with one provider has got.
    pub struct CloudStatusDto {
        #[ts(type = "number")]
        pub items: u64,
        /// Tagged.
        #[ts(type = "number")]
        pub analysed: u64,
        /// Still to send.
        #[ts(type = "number")]
        pub pending: u64,
        /// Not tagged, for good.
        #[ts(type = "number")]
        pub failed: u64,
        /// The latest problem (a failed photo's, or one being retried).
        pub problem: Option<String>,
    }

    /// How far "Find duplicates" or "Tag photos" has got for a folder.
    pub struct AiFolderStatus {
        /// Items it covers (RAW photos for duplicates, all for tags).
        #[ts(type = "number")]
        pub items: u64,
        /// Of those, done.
        #[ts(type = "number")]
        pub analysed: u64,
        /// Jobs still to run.
        #[ts(type = "number")]
        pub pending: u64,
    }

    /// A day with duplicates.
    pub struct DuplicateDayDto {
        /// `YYYY-MM-DD`.
        pub date: String,
        #[ts(type = "number")]
        pub groups: u64,
        #[ts(type = "number")]
        pub items: u64,
        #[ts(type = "number")]
        pub undecided: u64,
    }

    pub struct DuplicateSummaryDto {
        pub days: Vec<DuplicateDayDto>,
        #[ts(type = "number")]
        pub groups: u64,
        #[ts(type = "number")]
        pub items: u64,
        #[ts(type = "number")]
        pub undecided: u64,
    }

    /// One item of a group of duplicates.
    pub struct DuplicateMemberDto {
        pub id: String,
        pub name: String,
        pub video: bool,
        #[ts(type = "number | null")]
        pub capture_ms: Option<i64>,
        pub rating: u8,
        /// `keeper`, `not_kept`, `not_duplicate`, or null.
        pub choice: Option<String>,
        /// Likeness to the keeper, 0–100.
        pub likeness: u8,
        /// The same picture as the keeper (re-saved or resized copy).
        pub exact: bool,
        /// Its 256 px thumbnail, if made.
        pub thumb: Option<String>,
    }

    pub struct DuplicateGroupDto {
        pub date: String,
        pub keeper: String,
        pub decided: bool,
        pub members: Vec<DuplicateMemberDto>,
    }

    /// An item's AI tags, and why there may be none.
    pub struct AiTagsDto {
        /// On-device AI is turned on.
        pub enabled: bool,
        /// The item has been tagged ("Tag photos").
        pub analysed: bool,
        /// Most likely first.
        pub tags: Vec<AiTagDto>,
        /// The keywords each AI gave it with Tag with AI (ADR-039): one
        /// entry per AI, Claude, ChatGPT, Gemini in that order. A keyword
        /// two AIs gave is in both.
        pub by_ai: Vec<AiTagSourceDto>,
    }

    /// The keywords one AI gave an item (Tag with AI).
    pub struct AiTagSourceDto {
        /// `claude`, `openai` or `gemini`.
        pub provider: String,
        /// For people: "ChatGPT".
        pub name: String,
        /// Lightroom-style paths, sorted.
        pub tags: Vec<String>,
    }

    /// An AI tag on an item.
    pub struct AiTagDto {
        /// Lightroom-style path.
        pub tag: String,
        /// 0–100.
        pub confidence: u8,
    }

    /// The model download (`ai://install`).
    pub struct AiInstallProgress {
        #[ts(type = "number")]
        pub done: u64,
        #[ts(type = "number")]
        pub total: u64,
        /// Finished: null when it worked, else why not.
        pub finished: bool,
        pub error: Option<String>,
    }

    /// A folder in the sidebar's folder tree.
    pub struct FolderNode {
        /// Its own name (`May`).
        pub name: String,
        /// Its path on the volume (`2025/May`), for `Query.folder`.
        pub path: String,
        #[ts(type = "number")]
        pub count: u64,
        pub has_children: bool,
    }

    pub struct SortSpec {
        pub field: SortKey,
        pub descending: bool,
    }

    /// A view: a filtered, sorted snapshot of the library, kept by the app.
    pub struct ViewInfo {
        pub view: u32,
        #[ts(type = "number")]
        pub count: u64,
    }

    /// One grid tile or details row.
    pub struct Row {
        pub id: String,
        pub name: Option<String>,
        /// Local capture time `YYYY-MM-DDTHH:MM:SS` (camera clock).
        pub captured: Option<String>,
        pub camera: Option<String>,
        pub lens: Option<String>,
        pub kind: ItemKind,
        #[ts(type = "number | null")]
        pub size: Option<u64>,
        #[ts(type = "number | null")]
        pub duration_ms: Option<i64>,
        pub rating: u8,
        pub label: Option<String>,
        /// Path of the 256 px thumbnail, if made (use `convertFileSrc`).
        pub thumb: Option<String>,
        /// Path of the 1024 px thumbnail, if made: for large grid tiles.
        pub thumb_large: Option<String>,
        /// A volume holding the original is mounted.
        pub online: bool,
    }

    /// A calendar cell.
    pub struct Bucket {
        /// `2024`, `2024-06`, `2024-06-15` or `undated`.
        pub key: String,
        #[ts(type = "number")]
        pub count: u64,
        pub cover_id: Option<String>,
        pub cover: Option<String>,
    }

    pub struct FacetItem {
        pub value: String,
        #[ts(type = "number")]
        pub count: u64,
    }

    pub struct Facets {
        pub cameras: Vec<FacetItem>,
        pub lenses: Vec<FacetItem>,
        /// Tags (keywords and AI tags), most used first.
        pub tags: Vec<FacetItem>,
    }

    pub struct VolumeItem {
        pub id: String,
        pub label: String,
        pub online: bool,
        pub mount: Option<String>,
        #[ts(type = "number")]
        pub count: u64,
        /// Files added in place that are still being read in full for their
        /// fingerprint, in the background.
        #[ts(type = "number")]
        pub unfingerprinted: u64,
    }

    pub struct EventItem {
        pub id: String,
        pub title: String,
        #[ts(type = "number")]
        pub count: u64,
    }

    pub struct ImportItem {
        pub id: String,
        #[ts(type = "number")]
        pub started_ms: i64,
        pub status: String,
        pub source: String,
        pub safe_to_format: bool,
        #[ts(type = "number")]
        pub count: u64,
    }

    pub struct Sidebar {
        pub events: Vec<EventItem>,
        pub volumes: Vec<VolumeItem>,
        pub imports: Vec<ImportItem>,
    }

    pub struct FileItem {
        /// Full path if the volume is mounted, else the path on the volume.
        pub path: String,
        pub volume: String,
        pub role: String,
        #[ts(type = "number")]
        pub size: u64,
        pub online: bool,
    }

    /// Everything the inspector shows.
    pub struct Details {
        pub id: String,
        pub name: Option<String>,
        pub kind: ItemKind,
        pub captured: Option<String>,
        pub capture_source: Option<String>,
        pub confidence: Option<String>,
        pub camera_make: Option<String>,
        pub camera_model: Option<String>,
        pub lens: Option<String>,
        #[ts(type = "number | null")]
        pub duration_ms: Option<i64>,
        pub rating: u8,
        pub label: Option<String>,
        pub comment: Option<String>,
        /// Lightroom-style paths (`Places|Lake`).
        pub keywords: Vec<String>,
        pub gps: Option<GpsItem>,
        pub event: Option<String>,
        pub files: Vec<FileItem>,
        pub preview: Option<String>,
        pub proxy: Option<String>,
        pub online: bool,
    }

    pub struct GpsItem {
        pub lat: f64,
        pub lon: f64,
    }

    /// Which assets an edit applies to: explicit ids, and/or index ranges
    /// `[from, to)` of a view.
    #[derive(Default)]
    pub struct Selection {
        #[serde(default)]
        pub ids: Vec<String>,
        pub view: Option<u32>,
        #[serde(default)]
        #[ts(type = "Array<[number, number]>")]
        pub ranges: Vec<(u64, u64)>,
    }

    /// What the selected items have in common (the inspector with several
    /// selected). A field is `null` with `…Mixed` set when they differ.
    pub struct SelectionSummary {
        #[ts(type = "number")]
        pub count: u64,
        /// `null` when the ratings differ.
        pub rating: Option<u8>,
        pub label: Option<String>,
        pub label_mixed: bool,
        pub comment: Option<String>,
        pub comment_mixed: bool,
        /// Every keyword on any of them; on all of them when `count` equals
        /// the summary's `count`.
        pub keywords: Vec<KeywordCount>,
    }

    pub struct KeywordCount {
        /// Lightroom-style path (`Places|Lake`).
        pub keyword: String,
        #[ts(type = "number")]
        pub count: u64,
    }

    /// Result of an edit. Large selections run in the background and report
    /// with `edit://progress` and `edit://done` events.
    pub struct EditResult {
        #[ts(type = "number")]
        pub changed: u64,
        pub background: bool,
    }

    pub struct EditProgress {
        #[ts(type = "number")]
        pub done: u64,
        #[ts(type = "number")]
        pub total: u64,
        pub finished: bool,
        pub errors: Vec<String>,
    }

    pub struct RenderSummary {
        #[ts(type = "number")]
        pub queued: u64,
        #[ts(type = "number")]
        pub running: u64,
        #[ts(type = "number")]
        pub done: u64,
        #[ts(type = "number")]
        pub failed: u64,
        pub paused: Option<String>,
        #[ts(type = "number")]
        pub cache_bytes: u64,
        #[ts(type = "number")]
        pub cache_max: u64,
        #[ts(type = "number")]
        pub kept_offline: u64,
    }

    /// `render://thumbs`: thumbnails that just became available.
    pub struct ThumbsReady {
        pub ids: Vec<String>,
        /// Thumbnails of any item may have changed (cache names were
        /// repaired): reload every row on screen.
        pub all: bool,
    }

    /// `device://card`: a Sony card was inserted (or removed).
    pub struct CardNotice {
        pub card: String,
        pub label: String,
        pub mount: String,
        pub known: bool,
        #[ts(type = "number")]
        pub items: u64,
        #[ts(type = "number")]
        pub new_items: u64,
        pub preset: Option<String>,
        pub removed: bool,
    }

    /// One asset on a card.
    pub struct CardItem {
        pub key: String,
        pub kind: ItemKind,
        /// Relative to the card; the original first.
        pub files: Vec<String>,
        #[ts(type = "number")]
        pub size: u64,
        /// Local day `YYYY-MM-DD` (from the file time).
        pub day: Option<String>,
        /// Not in the library yet.
        pub new: bool,
    }

    pub struct CardScan {
        pub mount: String,
        pub label: String,
        pub card: Option<String>,
        pub known: bool,
        pub preset: Option<String>,
        pub items: Vec<CardItem>,
    }

    pub struct Preset {
        pub name: String,
        pub destinations: Vec<String>,
        pub template: String,
        pub event: Option<String>,
    }

    pub struct ImportRequest {
        pub mount: String,
        pub label: String,
        pub destinations: Vec<String>,
        pub template: String,
        pub event: Option<String>,
        /// `CardItem.key`s to import.
        pub keys: Vec<String>,
    }

    pub struct DestinationSpace {
        pub destination: String,
        #[ts(type = "number")]
        pub needed: u64,
        #[ts(type = "number | null")]
        pub free: Option<u64>,
    }

    /// What an import would do (no writes).
    pub struct ImportCheck {
        pub ok: bool,
        /// Why it cannot run (same disk, missing folder, bad template…).
        pub problem: Option<String>,
        pub space: Vec<DestinationSpace>,
        /// Destination name of the first selected item, as an example.
        pub example: Option<String>,
    }

    /// `import://progress`.
    pub struct ImportProgress {
        pub session: String,
        #[ts(type = "number")]
        pub files_done: u64,
        #[ts(type = "number")]
        pub bytes_done: u64,
        #[ts(type = "number")]
        pub bytes_total: u64,
        pub current: Option<String>,
        pub phase: Option<String>,
    }

    /// `import://done`.
    pub struct ImportResult {
        pub session: Option<String>,
        pub safe_to_format: bool,
        pub not_safe: Vec<String>,
        #[ts(type = "number")]
        pub catalogued: u64,
        pub failed: Vec<String>,
        pub cancelled: bool,
        pub error: Option<String>,
    }
}

/// Type filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum KindFilter {
    Photo,
    Video,
    Raw,
}

/// What an item is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum ItemKind {
    Photo,
    Video,
    Raw,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum SortKey {
    Date,
    Name,
    Camera,
    Lens,
    Type,
    Size,
    Duration,
    Rating,
}

/// Calendar level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "level", rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum CalendarLevelDto {
    Years,
    Months { year: i32 },
    Days { year: i32, month: u32 },
}

dto! {
    /// What the viewer can toggle between for an asset.
    pub struct AssetMembers {
        /// RAW + JPEG (or HEIF): the viewer offers a JPEG | RAW toggle.
        pub has_pair: bool,
        /// The asset is a RAW alone.
        pub raw_only: bool,
        pub online: bool,
    }

    /// Frame rate, start timecode and size of a video, for the viewer.
    pub struct VideoInfo {
        pub fps_num: u32,
        pub fps_den: u32,
        /// `HH:MM:SS:FF`, `;` before the frames when drop-frame.
        pub start_timecode: Option<String>,
        #[ts(type = "number | null")]
        pub duration_ms: Option<i64>,
        pub width: u32,
        pub height: u32,
    }

    /// One asset before and after a time shift.
    pub struct ShiftSample {
        pub id: String,
        pub name: Option<String>,
        pub before: String,
        pub after: String,
    }

    /// What a time shift would do (no writes).
    pub struct TimeShiftPreview {
        /// Seconds to add to every selected capture time.
        #[ts(type = "number")]
        pub offset_s: i64,
        /// Assets that will be shifted.
        #[ts(type = "number")]
        pub total: u64,
        /// Selected assets with no capture time (left alone).
        #[ts(type = "number")]
        pub undated: u64,
        /// The first few, in capture order.
        pub samples: Vec<ShiftSample>,
        /// The reference photo's current time.
        pub reference_before: String,
    }

    /// One backup or verify run.
    pub struct HistoryRun {
        /// `backup` or `verify`.
        pub kind: String,
        #[ts(type = "number")]
        pub started_ms: i64,
        #[ts(type = "number")]
        pub finished_ms: i64,
        pub destination: Option<String>,
        pub volume: Option<String>,
        pub label: String,
        #[ts(type = "number")]
        pub files: u64,
        #[ts(type = "number")]
        pub bytes: u64,
        #[ts(type = "number")]
        pub problems: u64,
        pub cancelled: bool,
        pub ok: bool,
    }

    /// A folder or drive backed up to before.
    pub struct BackupDestination {
        pub volume: String,
        pub label: String,
        pub path: Option<String>,
        pub online: bool,
        #[ts(type = "number | null")]
        pub last_backup_ms: Option<i64>,
        /// Last verify with no problems.
        #[ts(type = "number | null")]
        pub last_verified_ms: Option<i64>,
    }

    pub struct BackupOverview {
        pub destinations: Vec<BackupDestination>,
        /// Newest first.
        pub runs: Vec<HistoryRun>,
        /// A backup or verify is running.
        pub busy: bool,
    }

    /// One group of a library report breakdown (Tools → Report).
    pub struct ReportGroup {
        /// `2024`, `2024-06`, `2024-06-01`, a camera, lens or extension;
        /// null when unknown (shown as `Unknown`).
        pub group: Option<String>,
        /// Photos plus videos.
        #[ts(type = "number")]
        pub items: u64,
        /// Null in the file type and drive breakdowns (not split there).
        #[ts(type = "number | null")]
        pub photos: Option<u64>,
        #[ts(type = "number | null")]
        pub videos: Option<u64>,
        /// Every file of the items (both halves of a RAW+JPEG pair), one
        /// copy of each.
        #[ts(type = "number")]
        pub files: u64,
        #[ts(type = "number")]
        pub bytes: u64,
    }

    /// Every copy on one drive.
    pub struct ReportDrive {
        pub volume: String,
        pub online: bool,
        /// `group` is the drive's name.
        pub row: ReportGroup,
    }

    /// Tools → Report: totals and every breakdown.
    pub struct LibraryReport {
        pub totals: ReportGroup,
        /// Newest first, unknown last.
        pub years: Vec<ReportGroup>,
        pub months: Vec<ReportGroup>,
        pub days: Vec<ReportGroup>,
        /// Most items first, unknown last.
        pub cameras: Vec<ReportGroup>,
        pub lenses: Vec<ReportGroup>,
        pub file_types: Vec<ReportGroup>,
        pub drives: Vec<ReportDrive>,
    }

    /// `maint://progress`: a backup, verify or folder scan is running.
    pub struct MaintProgress {
        /// `backup`, `verify` or `folder`.
        pub kind: String,
        #[ts(type = "number")]
        pub bytes_done: u64,
        #[ts(type = "number")]
        pub bytes_total: u64,
        #[ts(type = "number")]
        pub files_done: u64,
        pub current: Option<String>,
    }

    /// `maint://done`.
    pub struct MaintDone {
        pub kind: String,
        /// The run as recorded in the history (absent on an error).
        pub run: Option<HistoryRun>,
        /// Up to 20 problems, one line each.
        pub problems: Vec<String>,
        pub error: Option<String>,
    }

    /// `folder://done`: adding (or scanning again) a folder in place finished.
    pub struct FolderDone {
        /// The folder as chosen.
        pub folder: String,
        /// Its volume and name in the library (absent on an error).
        pub volume: Option<String>,
        pub label: Option<String>,
        #[ts(type = "number")]
        pub items: u64,
        #[ts(type = "number")]
        pub files: u64,
        /// Items that took ratings or labels from Lightroom or Keepsake sidecars.
        #[ts(type = "number")]
        pub with_sidecars: u64,
        /// Files that are not photos or videos, left out.
        #[ts(type = "number")]
        pub other_files: u64,
        /// Up to 20 problems, one line each.
        pub problems: Vec<String>,
        pub cancelled: bool,
        pub error: Option<String>,
        /// A folder already in the library, or one folder of it, scanned
        /// again from the sidebar (not added).
        pub scanned_again: bool,
    }

    /// `folder://saved`: a folder being added has more of its items in the
    /// library. Sent once it is in the library (with what an earlier scan
    /// saved), then after every saved batch, so the views can update.
    pub struct FolderSaved {
        /// The folder as chosen.
        pub folder: String,
        /// Its items and files in the library so far.
        #[ts(type = "number")]
        pub items: u64,
        #[ts(type = "number")]
        pub files: u64,
    }

    /// An application in the Open in menu for one item.
    pub struct OpenChoice {
        /// The program, as set in Settings; empty for the system default.
        pub app: String,
        /// Its name for people (`Photoshop`, `Topaz Photo`, `System default`).
        pub name: String,
        /// The one Ctrl+O uses.
        pub default: bool,
    }

    /// Where an item's original file is, for Show in Explorer / Finder.
    pub struct OriginalPlace {
        /// The file is on a connected drive: it can be shown.
        pub online: bool,
        /// Its absolute path, while its drive is connected.
        pub path: Option<String>,
        /// Why it cannot be shown ("Drive not connected (Card A)"), if not.
        pub problem: Option<String>,
        /// The file manager's name for the menu: "Explorer", "Finder".
        pub manager: String,
    }

    /// One metadata tag: ExifTool's name (`ExposureTime`) and its value as text.
    pub struct MetaTagDto {
        pub name: String,
        pub value: String,
    }

    /// The tags of one group (`IFD0`, `ExifIFD`, `GPS`, `Sony`, `XMP-xmp`, `File`…).
    pub struct MetaGroupDto {
        pub group: String,
        pub tags: Vec<MetaTagDto>,
    }

    /// Every metadata tag of an item's file (the inspector's "All metadata").
    pub struct FileMetadata {
        /// The file read: its name only.
        pub file: String,
        pub groups: Vec<MetaGroupDto>,
    }

    /// An "Open in" application for one file type.
    pub struct OpenApp {
        /// Lower-case extension without the dot (`arw`).
        pub ext: String,
        pub app: String,
    }

    /// The settings screen.
    pub struct SettingsInfo {
        pub library: Option<String>,
        /// The cache folder in use.
        pub cache_dir: String,
        /// Set by the user (else the system cache folder).
        pub cache_dir_custom: bool,
        #[ts(type = "number")]
        pub cache_max_bytes: u64,
        #[ts(type = "number")]
        pub cache_bytes: u64,
        /// `None`: automatic.
        pub workers: Option<u32>,
        pub workers_default: u32,
        pub proxy_workers: u32,
        pub pause_on_battery: bool,
        pub open_apps: Vec<OpenApp>,
        /// When a card is inserted: import it by itself.
        pub import: ImportPrefs,
        /// Where `config.toml` is.
        pub config_file: String,
    }

    /// What happens when a camera card is inserted.
    pub struct ImportPrefs {
        /// Import new items by itself (else the import window asks).
        pub auto: bool,
        /// Where they are copied.
        pub destination: Option<String>,
        /// Folder layout (`{yyyy}/{month}/{yyyy}-{mm}-{dd}/{orig}`).
        pub template: String,
    }

    /// Changes from the settings screen (every field is the new value).
    pub struct SettingsUpdate {
        /// `None`: the system cache folder.
        pub cache_dir: Option<String>,
        #[ts(type = "number")]
        pub cache_max_bytes: u64,
        pub workers: Option<u32>,
        pub proxy_workers: u32,
        pub pause_on_battery: bool,
        pub open_apps: Vec<OpenApp>,
        pub import: ImportPrefs,
    }

    pub struct SettingsSaved {
        pub settings: SettingsInfo,
        /// The cache moved: the previous folder, whose files can be removed.
        pub old_cache: Option<String>,
    }
}

dto! {
    /// Read aircraft's review window (ADR-041): how sure, in percent, a
    /// line must be for each kind of mark to be offered.
    pub struct AircraftConfidenceDto {
        pub registration_pct: u8,
        pub airline_pct: u8,
        pub type_pct: u8,
        /// The same-shot window, seconds either side (1–60).
        pub same_shot_s: u8,
    }

    /// A possible mark and how surely it was read (percent).
    pub struct ScoredMarkDto {
        pub name: String,
        pub pct: u8,
    }

    /// One read photo in the review window.
    pub struct AircraftReviewItemDto {
        pub id: String,
        pub name: Option<String>,
        /// Its thumbnail file, when made.
        pub thumb: Option<String>,
        /// `waiting`, `accepted` or `dismissed`.
        pub review: String,
        /// Read from the full-size picture (else from the thumbnail).
        pub full_size: bool,
        /// Every possible mark, most surely read first.
        pub registrations: Vec<ScoredMarkDto>,
        pub airlines: Vec<ScoredMarkDto>,
        pub types: Vec<ScoredMarkDto>,
        /// The type the picture looks like, when one stands out.
        pub looks_like: Option<String>,
        /// The keywords offered at the window's confidence (`A|B|C`).
        pub offered: Vec<String>,
        /// Of those, the ones not ticked at first (a painted type the
        /// online lookup disagrees with).
        pub unticked: Vec<String>,
        /// Of those, the ones the online lookup gave.
        pub from_lookup: Vec<String>,
        /// What the lookup said about its registrations.
        pub lookups: Vec<AircraftLookupDto>,
        /// Photos taken by the same camera and lens within the same-shot
        /// window; `None`: not looked for (the photo is not on screen).
        pub neighbours: Option<Vec<AircraftNeighbourDto>>,
    }

    /// What the online lookup (adsbdb.com) said about a registration.
    pub struct AircraftLookupDto {
        pub registration: String,
        /// The service knows it.
        pub known: bool,
        pub aircraft_type: Option<String>,
        pub airline: Option<String>,
        pub owner: Option<String>,
        pub country: Option<String>,
    }

    /// A photo taken just before or after a read one.
    pub struct AircraftNeighbourDto {
        pub id: String,
        pub name: Option<String>,
        pub thumb: Option<String>,
        /// Seconds after the read photo (before: below zero).
        pub seconds: f32,
        /// How alike, in percent, when both were analysed by On-device AI.
        pub similarity_pct: Option<u8>,
        /// Alike enough to be the same shot: ticked at first.
        pub alike: bool,
    }

    /// Another photo of the same registration, and the keywords it lacks.
    pub struct SamePlaneDto {
        pub id: String,
        pub name: Option<String>,
        pub thumb: Option<String>,
        pub missing: Vec<String>,
    }

    /// The review window's photos: those read so far, and how reading the
    /// rest is getting on.
    pub struct AircraftReviewDto {
        pub items: Vec<AircraftReviewItemDto>,
        pub confidence: AircraftConfidenceDto,
        #[ts(type = "number")]
        pub total: u64,
        /// Read (with this version).
        #[ts(type = "number")]
        pub read: u64,
        /// Still to read.
        #[ts(type = "number")]
        pub pending: u64,
        /// Could not be read.
        #[ts(type = "number")]
        pub failed: u64,
        pub problem: Option<String>,
    }
}

dto! {
    /// Edit suggestions (ADR-042): one edit, and how to do it in each
    /// program (empty: that program has no tool for it).
    pub struct EditSuggestionDto {
        pub title: String,
        pub why: String,
        pub photoshop: String,
        pub topaz: String,
    }

    /// One line of what Keepsake measured: "Highlights", "3% blown out".
    pub struct MeasuredDto {
        pub label: String,
        pub value: String,
    }

    /// What an AI advised.
    pub struct AiAdviceDto {
        /// The provider's id (`claude`) and name (`Claude`).
        pub provider: String,
        pub provider_name: String,
        pub model: String,
        pub summary: String,
        pub suggestions: Vec<EditSuggestionDto>,
        #[ts(type = "number")]
        pub made_ms: i64,
    }

    /// A question to an AI on its way, or one that failed.
    pub struct AskingDto {
        pub provider: String,
        pub provider_name: String,
        /// Still to be answered; else it failed for good.
        pub pending: bool,
        pub problem: Option<String>,
    }

    /// A photo's edit suggestions. `waiting`: its preview is being made
    /// first (ask again shortly); nothing else is filled in.
    pub struct EditAdviceDto {
        pub id: String,
        pub waiting: bool,
        pub measured: Vec<MeasuredDto>,
        /// One sentence on what the measurements found.
        pub summary: String,
        pub suggestions: Vec<EditSuggestionDto>,
        #[ts(type = "number")]
        pub made_ms: i64,
        pub ai: Option<AiAdviceDto>,
        pub asking: Option<AskingDto>,
    }
}

dto! {
    /// The app crashed last time: shown once at start-up.
    pub struct CrashNotice {
        /// The crash file's name in the log folder.
        pub file: String,
        /// Where it happened, in a line.
        pub summary: String,
    }
}

/// Which member of an asset to show or open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../src/bindings/")]
pub enum MemberDto {
    Primary,
    Raw,
}
