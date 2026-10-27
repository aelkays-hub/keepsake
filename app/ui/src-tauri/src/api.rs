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
        pub session: Option<String>,
        #[serde(default)]
        pub undated: bool,
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
    }

    pub struct VolumeItem {
        pub id: String,
        pub label: String,
        pub online: bool,
        pub mount: Option<String>,
        #[ts(type = "number")]
        pub count: u64,
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

    /// `maint://progress`: a backup or verify is running.
    pub struct MaintProgress {
        /// `backup` or `verify`.
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
        /// Where `config.toml` is.
        pub config_file: String,
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
    }

    pub struct SettingsSaved {
        pub settings: SettingsInfo,
        /// The cache moved: the previous folder, whose files can be removed.
        pub old_cache: Option<String>,
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
